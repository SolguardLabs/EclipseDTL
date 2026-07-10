use crate::accounts::{AccountBook, AccountRole};
use crate::amount::{Amount, BasisPoints, Ratio};
use crate::asset::{Asset, AssetBook, AssetClass};
use crate::auction::{BidMetadata, BidRequest};
use crate::batch::{BatchBook, BatchOrder};
use crate::error::{EclipseError, Result};
use crate::events::{Event, EventLog};
use crate::ids::{AccountId, AssetId, BatchId, BidId, OperatorId, RouteId};
use crate::operators::{OperatorBook, OperatorProfile, OperatorStatus};
use crate::risk::{RiskConfig, RiskEngine};
use crate::routes::{RouteBook, RouteClass, RouteLeg, RoutePlan};
use crate::settlement::{SettlementConfig, SettlementEngine, SettlementReceipt};
use crate::{auction::AuctionBook, routes::RouteView};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScenarioFile {
    pub name: String,
    #[serde(default)]
    pub vault_account: Option<AccountId>,
    #[serde(default)]
    pub actions: Vec<ScenarioAction>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ScenarioAction {
    RegisterAsset {
        id: AssetId,
        symbol: String,
        decimals: u8,
        class: AssetClass,
        #[serde(default)]
        transfer_fee_bps: Option<u32>,
        #[serde(default)]
        risk_weight_bps: Option<u32>,
    },
    CreateAccount {
        id: AccountId,
        role: AccountRole,
        #[serde(default)]
        label: Option<String>,
    },
    Deposit {
        account: AccountId,
        asset: AssetId,
        amount: u64,
    },
    Transfer {
        from: AccountId,
        to: AccountId,
        asset: AssetId,
        amount: u64,
    },
    RegisterOperator {
        id: OperatorId,
        display_name: String,
        lane: String,
        fee_account: AccountId,
        #[serde(default)]
        reliability_bps: Option<u32>,
        #[serde(default)]
        fee_floor_bps: Option<u32>,
    },
    PledgeGuarantee {
        operator: OperatorId,
        amount: u64,
    },
    SetOperatorStatus {
        operator: OperatorId,
        status: OperatorStatus,
    },
    AllocateCommitment {
        operator: OperatorId,
        amount: u64,
        #[serde(default)]
        label: Option<String>,
    },
    RegisterRoute {
        id: RouteId,
        label: String,
        class: RouteClass,
        source: AssetId,
        target: AssetId,
        price_numerator: u64,
        price_denominator: u64,
        #[serde(default)]
        max_slippage_bps: Option<u32>,
        #[serde(default)]
        min_input: Option<u64>,
        #[serde(default)]
        max_input: Option<u64>,
        #[serde(default)]
        guarantee_bps_floor: Option<u32>,
        #[serde(default)]
        liquidity_floor: Option<u64>,
        #[serde(default)]
        fallback_rank: Option<u32>,
        #[serde(default)]
        venue: Option<String>,
    },
    OpenBatch {
        id: BatchId,
        payer: AccountId,
        recipient: AccountId,
        source: AssetId,
        target: AssetId,
        amount_in: u64,
        min_out: u64,
        opened_at: u64,
        deadline: u64,
        #[serde(default)]
        allow_fallback: Option<bool>,
        #[serde(default)]
        memo: Option<String>,
    },
    SubmitBid {
        id: BidId,
        batch: BatchId,
        route: RouteId,
        operator: OperatorId,
        price_numerator: u64,
        price_denominator: u64,
        fee_bps: u32,
        guarantee_bps: u32,
        received_at: u64,
        expires_at: u64,
        #[serde(default)]
        lane: Option<String>,
        #[serde(default)]
        quote_ref: Option<String>,
    },
    SelectWinner {
        batch: BatchId,
        now: u64,
    },
    SettleBatch {
        batch: BatchId,
        now: u64,
        #[serde(default)]
        allow_fallback: Option<bool>,
    },
    Snapshot {
        name: String,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScenarioReport {
    pub name: String,
    pub events: Vec<Event>,
    pub accounts: Vec<crate::accounts::AccountView>,
    pub operators: Vec<crate::operators::OperatorView>,
    pub routes: Vec<RouteView>,
    pub batches: Vec<crate::batch::BatchView>,
    pub bids: Vec<crate::auction::BidView>,
    pub receipts: Vec<SettlementReceipt>,
    pub snapshots: Vec<NamedSnapshot>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NamedSnapshot {
    pub name: String,
    pub accounts: Vec<crate::accounts::AccountView>,
    pub operators: Vec<crate::operators::OperatorView>,
    pub batches: Vec<crate::batch::BatchView>,
    pub bids: Vec<crate::auction::BidView>,
}

#[derive(Debug)]
pub struct ScenarioRuntime {
    pub name: String,
    pub assets: AssetBook,
    pub accounts: AccountBook,
    pub operators: OperatorBook,
    pub routes: RouteBook,
    pub auction: AuctionBook,
    pub batches: BatchBook,
    pub risk: RiskEngine,
    pub settlement: SettlementEngine,
    pub events: EventLog,
    pub receipts: Vec<SettlementReceipt>,
    pub snapshots: Vec<NamedSnapshot>,
}

impl ScenarioRuntime {
    pub fn new(name: impl Into<String>, vault_account: AccountId) -> Self {
        let risk = RiskEngine::new(RiskConfig {
            vault_account: vault_account.clone(),
            ..RiskConfig::default()
        });
        let settlement = SettlementEngine::new(SettlementConfig {
            vault_account,
            ..SettlementConfig::default()
        });
        Self {
            name: name.into(),
            assets: AssetBook::new(),
            accounts: AccountBook::new(),
            operators: OperatorBook::new(),
            routes: RouteBook::new(),
            auction: AuctionBook::new(),
            batches: BatchBook::new(),
            risk,
            settlement,
            events: EventLog::new(),
            receipts: Vec::new(),
            snapshots: Vec::new(),
        }
    }

    pub fn apply(&mut self, action: ScenarioAction) -> Result<()> {
        match action {
            ScenarioAction::RegisterAsset {
                id,
                symbol,
                decimals,
                class,
                transfer_fee_bps,
                risk_weight_bps,
            } => self.register_asset(
                id,
                symbol,
                decimals,
                class,
                transfer_fee_bps,
                risk_weight_bps,
            ),
            ScenarioAction::CreateAccount { id, role, label } => {
                self.create_account(id, role, label)
            }
            ScenarioAction::Deposit {
                account,
                asset,
                amount,
            } => self.deposit(account, asset, amount),
            ScenarioAction::Transfer {
                from,
                to,
                asset,
                amount,
            } => self.transfer(from, to, asset, amount),
            ScenarioAction::RegisterOperator {
                id,
                display_name,
                lane,
                fee_account,
                reliability_bps,
                fee_floor_bps,
            } => self.register_operator(
                id,
                display_name,
                lane,
                fee_account,
                reliability_bps,
                fee_floor_bps,
            ),
            ScenarioAction::PledgeGuarantee { operator, amount } => self.pledge(operator, amount),
            ScenarioAction::SetOperatorStatus { operator, status } => {
                self.operators.set_status(&operator, status)
            }
            ScenarioAction::AllocateCommitment {
                operator,
                amount,
                label,
            } => self.allocate_commitment(operator, amount, label),
            ScenarioAction::RegisterRoute {
                id,
                label,
                class,
                source,
                target,
                price_numerator,
                price_denominator,
                max_slippage_bps,
                min_input,
                max_input,
                guarantee_bps_floor,
                liquidity_floor,
                fallback_rank,
                venue,
            } => self.register_route(
                id,
                label,
                class,
                source,
                target,
                price_numerator,
                price_denominator,
                max_slippage_bps,
                min_input,
                max_input,
                guarantee_bps_floor,
                liquidity_floor,
                fallback_rank,
                venue,
            ),
            ScenarioAction::OpenBatch {
                id,
                payer,
                recipient,
                source,
                target,
                amount_in,
                min_out,
                opened_at,
                deadline,
                allow_fallback,
                memo,
            } => self.open_batch(
                id,
                payer,
                recipient,
                source,
                target,
                amount_in,
                min_out,
                opened_at,
                deadline,
                allow_fallback,
                memo,
            ),
            ScenarioAction::SubmitBid {
                id,
                batch,
                route,
                operator,
                price_numerator,
                price_denominator,
                fee_bps,
                guarantee_bps,
                received_at,
                expires_at,
                lane,
                quote_ref,
            } => self.submit_bid(
                id,
                batch,
                route,
                operator,
                price_numerator,
                price_denominator,
                fee_bps,
                guarantee_bps,
                received_at,
                expires_at,
                lane,
                quote_ref,
            ),
            ScenarioAction::SelectWinner { batch, now } => self.select_winner(batch, now),
            ScenarioAction::SettleBatch {
                batch,
                now,
                allow_fallback,
            } => self.settle_batch(batch, now, allow_fallback),
            ScenarioAction::Snapshot { name } => self.snapshot(name),
        }
    }

    fn register_asset(
        &mut self,
        id: AssetId,
        symbol: String,
        decimals: u8,
        class: AssetClass,
        transfer_fee_bps: Option<u32>,
        risk_weight_bps: Option<u32>,
    ) -> Result<()> {
        let mut asset = Asset::new(id.clone(), symbol.clone(), decimals, class);
        if let Some(fee) = transfer_fee_bps {
            asset = asset.with_transfer_fee(BasisPoints::new(fee)?);
        }
        if let Some(weight) = risk_weight_bps {
            asset = asset.with_risk_weight(BasisPoints::new(weight)?);
        }
        self.assets.insert(asset)?;
        self.events
            .push(Event::AssetRegistered { asset: id, symbol });
        Ok(())
    }

    fn create_account(
        &mut self,
        id: AccountId,
        role: AccountRole,
        label: Option<String>,
    ) -> Result<()> {
        let label = label.unwrap_or_else(|| id.to_string());
        self.accounts.create(id.clone(), role.clone(), label)?;
        self.events.push(Event::AccountCreated {
            account: id,
            role: role.as_str().to_owned(),
        });
        Ok(())
    }

    fn deposit(&mut self, account: AccountId, asset: AssetId, amount: u64) -> Result<()> {
        self.assets.ensure_enabled(&asset)?;
        self.accounts
            .deposit(&account, asset.clone(), Amount(amount as u128))?;
        self.events.push(Event::Deposit {
            account,
            asset,
            amount: amount as u128,
        });
        Ok(())
    }

    fn transfer(
        &mut self,
        from: AccountId,
        to: AccountId,
        asset: AssetId,
        amount: u64,
    ) -> Result<()> {
        self.assets.ensure_enabled(&asset)?;
        self.accounts
            .transfer(&from, &to, &asset, Amount(amount as u128))?;
        self.events.push(Event::Withdrawal {
            account: from,
            asset: asset.clone(),
            amount: amount as u128,
        });
        self.events.push(Event::Deposit {
            account: to,
            asset,
            amount: amount as u128,
        });
        Ok(())
    }

    fn register_operator(
        &mut self,
        id: OperatorId,
        display_name: String,
        lane: String,
        fee_account: AccountId,
        reliability_bps: Option<u32>,
        fee_floor_bps: Option<u32>,
    ) -> Result<()> {
        self.accounts.ensure(&fee_account)?;
        let mut operator =
            OperatorProfile::new(id.clone(), display_name, lane.clone(), fee_account);
        if let Some(reliability) = reliability_bps {
            operator.reliability_bps = BasisPoints::new(reliability)?;
        }
        if let Some(floor) = fee_floor_bps {
            operator.fee_floor_bps = BasisPoints::new(floor)?;
        }
        self.operators.insert(operator)?;
        self.events
            .push(Event::OperatorRegistered { operator: id, lane });
        Ok(())
    }

    fn pledge(&mut self, operator: OperatorId, amount: u64) -> Result<()> {
        self.operators.pledge(&operator, Amount(amount as u128))?;
        self.events.push(Event::GuaranteePledged {
            operator,
            amount: amount as u128,
        });
        Ok(())
    }

    fn allocate_commitment(
        &mut self,
        operator: OperatorId,
        amount: u64,
        label: Option<String>,
    ) -> Result<()> {
        self.operators
            .allocate_external_commitment(&operator, Amount(amount as u128))?;
        self.events.push(Event::OperatorCommitment {
            operator,
            route: RouteId::new("external"),
            amount: amount as u128,
            label: label.unwrap_or_else(|| "portfolio".to_owned()),
        });
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    fn register_route(
        &mut self,
        id: RouteId,
        label: String,
        class: RouteClass,
        source: AssetId,
        target: AssetId,
        price_numerator: u64,
        price_denominator: u64,
        max_slippage_bps: Option<u32>,
        min_input: Option<u64>,
        max_input: Option<u64>,
        guarantee_bps_floor: Option<u32>,
        liquidity_floor: Option<u64>,
        fallback_rank: Option<u32>,
        venue: Option<String>,
    ) -> Result<()> {
        self.assets.ensure_enabled(&source)?;
        self.assets.ensure_enabled(&target)?;
        let price = Ratio::new(price_numerator as u128, price_denominator as u128)?;
        let slippage = BasisPoints::new(max_slippage_bps.unwrap_or(50))?;
        let leg = RouteLeg::new(
            source.clone(),
            target.clone(),
            price,
            slippage,
            venue.unwrap_or_else(|| "internal".to_owned()),
        );
        let mut route = RoutePlan::new(id.clone(), label, class, source.clone(), target.clone())
            .with_leg(leg)
            .with_limits(
                Amount(min_input.unwrap_or(1) as u128),
                Amount(max_input.map(u128::from).unwrap_or(u128::MAX / 8)),
            )
            .with_guarantee_floor(BasisPoints::new(guarantee_bps_floor.unwrap_or(0))?)
            .with_liquidity_floor(Amount(liquidity_floor.map(u128::from).unwrap_or(0)));
        if let Some(rank) = fallback_rank {
            route = route.with_fallback_rank(rank);
        }
        self.routes.insert(route)?;
        self.events.push(Event::RouteRegistered {
            route: id,
            source,
            target,
        });
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    fn open_batch(
        &mut self,
        id: BatchId,
        payer: AccountId,
        recipient: AccountId,
        source: AssetId,
        target: AssetId,
        amount_in: u64,
        min_out: u64,
        opened_at: u64,
        deadline: u64,
        allow_fallback: Option<bool>,
        memo: Option<String>,
    ) -> Result<()> {
        self.accounts.ensure(&payer)?;
        self.accounts.ensure(&recipient)?;
        self.assets.ensure_enabled(&source)?;
        self.assets.ensure_enabled(&target)?;
        let order = BatchOrder::new(
            id.clone(),
            payer,
            recipient,
            source.clone(),
            target.clone(),
            Amount(amount_in as u128),
            Amount(min_out as u128),
            opened_at,
            deadline,
        )
        .with_fallback(allow_fallback.unwrap_or(false))
        .with_memo(memo.unwrap_or_default());
        self.batches.insert(order)?;
        self.events.push(Event::BatchOpened {
            batch: id,
            source,
            target,
            amount_in: amount_in as u128,
            min_out: min_out as u128,
        });
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    fn submit_bid(
        &mut self,
        id: BidId,
        batch: BatchId,
        route: RouteId,
        operator: OperatorId,
        price_numerator: u64,
        price_denominator: u64,
        fee_bps: u32,
        guarantee_bps: u32,
        received_at: u64,
        expires_at: u64,
        lane: Option<String>,
        quote_ref: Option<String>,
    ) -> Result<()> {
        let order = self.batches.order(&batch)?.clone();
        let request = BidRequest {
            id: id.clone(),
            batch: batch.clone(),
            route: route.clone(),
            operator: operator.clone(),
            amount_in: order.amount_in,
            price: Ratio::new(price_numerator as u128, price_denominator as u128)?,
            fee_bps: BasisPoints::new(fee_bps)?,
            guarantee_bps: BasisPoints::new(guarantee_bps)?,
            received_at,
            expires_at,
            metadata: BidMetadata {
                lane: lane.unwrap_or_default(),
                client_tag: self.name.clone(),
                quote_ref: quote_ref.unwrap_or_default(),
            },
        };
        let ticket = self.auction.submit(
            request,
            received_at,
            &self.assets,
            &self.accounts,
            &self.operators,
            &self.routes,
            &self.risk,
        )?;
        self.events.push(Event::BidSubmitted {
            bid: id,
            batch,
            route,
            operator,
            expected_net: ticket.net_out().raw(),
        });
        Ok(())
    }

    fn select_winner(&mut self, batch: BatchId, now: u64) -> Result<()> {
        let winner = self.auction.select_winner(&batch, now)?;
        self.batches.select(&batch, &winner)?;
        self.events.push(Event::BidSelected {
            batch,
            bid: winner.id().clone(),
            operator: winner.operator().clone(),
            score: winner.score.final_score,
        });
        Ok(())
    }

    fn settle_batch(
        &mut self,
        batch: BatchId,
        now: u64,
        allow_fallback: Option<bool>,
    ) -> Result<()> {
        let receipt = self.settlement.settle_selected(
            &batch,
            now,
            allow_fallback.unwrap_or(false),
            &mut self.accounts,
            &mut self.operators,
            &self.routes,
            &mut self.auction,
            &mut self.batches,
            &self.risk,
            &mut self.events,
        )?;
        self.receipts.push(receipt);
        Ok(())
    }

    fn snapshot(&mut self, name: String) -> Result<()> {
        self.events.push(Event::Snapshot { name: name.clone() });
        self.snapshots.push(NamedSnapshot {
            name,
            accounts: self.accounts.views(),
            operators: self.operators.views(),
            batches: self.batches.views(),
            bids: self.auction.views(),
        });
        Ok(())
    }

    pub fn report(&self) -> ScenarioReport {
        ScenarioReport {
            name: self.name.clone(),
            events: self.events.to_vec(),
            accounts: self.accounts.views(),
            operators: self.operators.views(),
            routes: self.routes.views(),
            batches: self.batches.views(),
            bids: self.auction.views(),
            receipts: self.receipts.clone(),
            snapshots: self.snapshots.clone(),
        }
    }
}

pub fn run_scenario(input: ScenarioFile) -> Result<ScenarioReport> {
    let vault = input
        .vault_account
        .clone()
        .unwrap_or_else(|| AccountId::new("vault"));
    let mut runtime = ScenarioRuntime::new(input.name.clone(), vault);
    for action in input.actions {
        runtime.apply(action)?;
    }
    Ok(runtime.report())
}

pub fn run_scenario_str(input: &str) -> Result<ScenarioReport> {
    let scenario: ScenarioFile = serde_json::from_str(input)?;
    run_scenario(scenario)
}

pub fn run_scenario_file(path: impl AsRef<std::path::Path>) -> Result<ScenarioReport> {
    let raw = std::fs::read_to_string(path)?;
    run_scenario_str(raw.as_str())
}

pub fn report_to_pretty_json(report: &ScenarioReport) -> Result<String> {
    serde_json::to_string_pretty(report).map_err(EclipseError::from)
}
