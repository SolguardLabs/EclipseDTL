use crate::accounts::AccountBook;
use crate::amount::AmountBreakdown;
use crate::auction::{AuctionBook, BidTicket};
use crate::batch::{BatchBook, BatchOrder};
use crate::error::{EclipseError, Result};
use crate::events::{Event, EventLog};
use crate::ids::{AccountId, AssetId, BatchId, BidId, OperatorId, RouteId, SettlementKey};
use crate::operators::{GuaranteeAttachment, OperatorBook};
use crate::risk::RiskEngine;
use crate::routes::{RouteBook, RoutePlan};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SettlementConfig {
    pub vault_account: AccountId,
    pub treasury_account: Option<AccountId>,
    pub insurance_account: Option<AccountId>,
    pub emit_balance_events: bool,
}

impl Default for SettlementConfig {
    fn default() -> Self {
        Self {
            vault_account: AccountId::new("vault"),
            treasury_account: None,
            insurance_account: None,
            emit_balance_events: true,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SettlementReceipt {
    pub key: SettlementKey,
    pub operator: OperatorId,
    pub payer: AccountId,
    pub recipient: AccountId,
    pub source: AssetId,
    pub target: AssetId,
    pub amount_in: u128,
    pub gross_out: u128,
    pub net_out: u128,
    pub operator_fee: u128,
    pub required_guarantee: u128,
    pub attached_guarantee: u128,
    pub fallback: bool,
}

impl SettlementReceipt {
    pub fn guarantee_gap(&self) -> i128 {
        self.attached_guarantee as i128 - self.required_guarantee as i128
    }

    pub fn is_complete(&self) -> bool {
        self.net_out > 0 && self.gross_out >= self.net_out
    }
}

#[derive(Debug, Clone)]
pub struct SettlementEngine {
    pub config: SettlementConfig,
}

impl SettlementEngine {
    pub fn new(config: SettlementConfig) -> Self {
        Self { config }
    }

    pub fn default_with_vault(vault_account: AccountId) -> Self {
        Self {
            config: SettlementConfig {
                vault_account,
                ..SettlementConfig::default()
            },
        }
    }

    pub fn settle_selected(
        &self,
        batch_id: &BatchId,
        now: u64,
        allow_fallback: bool,
        accounts: &mut AccountBook,
        operators: &mut OperatorBook,
        routes: &RouteBook,
        auction: &mut AuctionBook,
        batches: &mut BatchBook,
        risk: &RiskEngine,
        events: &mut EventLog,
    ) -> Result<SettlementReceipt> {
        let selected = {
            let record = batches.get(batch_id)?;
            let selected_bid = record
                .selected_bid
                .clone()
                .ok_or_else(|| EclipseError::BatchNotReady(batch_id.to_string()))?;
            auction.get(&selected_bid)?.clone()
        };
        let order = batches.order(batch_id)?.clone();
        match self.settle_ticket(
            &order, &selected, false, now, accounts, operators, routes, risk,
        ) {
            Ok(receipt) => {
                auction.mark_settled(selected.id())?;
                batches.close_settled(batch_id, now)?;
                self.push_settlement_events(&receipt, events);
                Ok(receipt)
            }
            Err(error) if allow_fallback || order.allow_fallback => {
                let fallback = auction.select_next(batch_id, selected.id(), now)?;
                events.push(Event::SettlementFallback {
                    batch: batch_id.clone(),
                    from_bid: selected.id().clone(),
                    to_bid: fallback.id().clone(),
                    reason: error.to_string(),
                });
                let receipt = self.settle_ticket(
                    &order, &fallback, true, now, accounts, operators, routes, risk,
                )?;
                auction.mark_superseded(selected.id())?;
                auction.mark_settled(fallback.id())?;
                batches.set_fallback(batch_id, &fallback)?;
                batches.close_settled(batch_id, now)?;
                self.push_settlement_events(&receipt, events);
                Ok(receipt)
            }
            Err(error) => {
                batches.close_failed(batch_id, now, error.to_string())?;
                Err(error)
            }
        }
    }

    pub fn settle_ticket(
        &self,
        order: &BatchOrder,
        ticket: &BidTicket,
        fallback: bool,
        _now: u64,
        accounts: &mut AccountBook,
        operators: &mut OperatorBook,
        routes: &RouteBook,
        risk: &RiskEngine,
    ) -> Result<SettlementReceipt> {
        if ticket.batch() != &order.id {
            return Err(EclipseError::InvalidScenario(format!(
                "bid {} is not for batch {}",
                ticket.id(),
                order.id
            )));
        }
        let route = routes.get(ticket.route())?;
        self.ensure_route_matches_order(order, route)?;
        let breakdown = self.calculate_breakdown(ticket)?;
        risk.preflight_settlement(
            route,
            accounts,
            breakdown.gross,
            order.min_out,
            breakdown.net,
        )?;
        self.apply_transfers(order, route, ticket, &breakdown, accounts)?;
        let attachment = self.attach_operator_guarantee(order, route, ticket, operators)?;
        Ok(self.receipt_from_parts(order, route, ticket, &breakdown, attachment, fallback))
    }

    fn ensure_route_matches_order(&self, order: &BatchOrder, route: &RoutePlan) -> Result<()> {
        if route.source != order.source || route.target != order.target {
            return Err(EclipseError::RouteAssetMismatch);
        }
        if !order.is_live_at(order.deadline) {
            return Err(EclipseError::BatchClosed(order.id.to_string()));
        }
        Ok(())
    }

    fn calculate_breakdown(&self, ticket: &BidTicket) -> Result<AmountBreakdown> {
        let gross = ticket.request.price.apply_floor(ticket.request.amount_in)?;
        let fee = ticket.request.fee_bps.checked_amount_floor(gross)?;
        AmountBreakdown::with_fee(gross, fee)
    }

    fn apply_transfers(
        &self,
        order: &BatchOrder,
        route: &RoutePlan,
        ticket: &BidTicket,
        breakdown: &AmountBreakdown,
        accounts: &mut AccountBook,
    ) -> Result<()> {
        let operator = ticket.operator();
        let fee_account = AccountId::new(format!("fee-{}", operator.as_str()));
        accounts.transfer(
            &order.payer,
            &self.config.vault_account,
            &order.source,
            order.amount_in,
        )?;
        accounts.transfer(
            &self.config.vault_account,
            &order.recipient,
            &route.target,
            breakdown.net,
        )?;
        if !breakdown.fee.is_zero() {
            accounts.transfer(
                &self.config.vault_account,
                &fee_account,
                &route.target,
                breakdown.fee,
            )?;
        }
        Ok(())
    }

    fn attach_operator_guarantee(
        &self,
        order: &BatchOrder,
        route: &RoutePlan,
        ticket: &BidTicket,
        operators: &mut OperatorBook,
    ) -> Result<GuaranteeAttachment> {
        operators.attach_guarantee(
            ticket.operator(),
            ticket.route().clone(),
            route.target.clone(),
            ticket.required_guarantee(),
            order.id.to_string(),
        )
    }

    fn receipt_from_parts(
        &self,
        order: &BatchOrder,
        route: &RoutePlan,
        ticket: &BidTicket,
        breakdown: &AmountBreakdown,
        attachment: GuaranteeAttachment,
        fallback: bool,
    ) -> SettlementReceipt {
        SettlementReceipt {
            key: SettlementKey::new(
                order.id.clone(),
                ticket.id().clone(),
                ticket.route().clone(),
            ),
            operator: ticket.operator().clone(),
            payer: order.payer.clone(),
            recipient: order.recipient.clone(),
            source: order.source.clone(),
            target: route.target.clone(),
            amount_in: order.amount_in.raw(),
            gross_out: breakdown.gross.raw(),
            net_out: breakdown.net.raw(),
            operator_fee: breakdown.fee.raw(),
            required_guarantee: attachment.requested.raw(),
            attached_guarantee: attachment.attached.raw(),
            fallback,
        }
    }

    fn push_settlement_events(&self, receipt: &SettlementReceipt, events: &mut EventLog) {
        events.push(Event::SettlementCompleted {
            batch: receipt.key.batch_id.clone(),
            bid: receipt.key.bid_id.clone(),
            operator: receipt.operator.clone(),
            route: receipt.key.route_id.clone(),
            gross_out: receipt.gross_out,
            net_out: receipt.net_out,
            operator_fee: receipt.operator_fee,
        });
        events.push(Event::BatchClosed {
            batch: receipt.key.batch_id.clone(),
            status: "settled".to_owned(),
        });
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SettlementPreview {
    pub batch: BatchId,
    pub bid: BidId,
    pub route: RouteId,
    pub operator: OperatorId,
    pub gross_out: u128,
    pub net_out: u128,
    pub fee: u128,
    pub required_guarantee: u128,
}

impl SettlementPreview {
    pub fn from_ticket(ticket: &BidTicket) -> Result<Self> {
        let gross = ticket.request.price.apply_floor(ticket.request.amount_in)?;
        let fee = ticket.request.fee_bps.checked_amount_floor(gross)?;
        let net = gross.checked_sub(fee)?;
        Ok(Self {
            batch: ticket.batch().clone(),
            bid: ticket.id().clone(),
            route: ticket.route().clone(),
            operator: ticket.operator().clone(),
            gross_out: gross.raw(),
            net_out: net.raw(),
            fee: fee.raw(),
            required_guarantee: ticket.required_guarantee().raw(),
        })
    }
}
