use crate::amount::{Amount, BasisPoints, Ratio};
use crate::asset::AssetBook;
use crate::error::{EclipseError, Result};
use crate::ids::{BatchId, BidId, OperatorId, RouteId};
use crate::operators::OperatorBook;
use crate::risk::{LiquiditySnapshot, RiskAssessment, RiskEngine};
use crate::{accounts::AccountBook, routes::RouteBook};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BidRequest {
    pub id: BidId,
    pub batch: BatchId,
    pub route: RouteId,
    pub operator: OperatorId,
    pub amount_in: Amount,
    pub price: Ratio,
    pub fee_bps: BasisPoints,
    pub guarantee_bps: BasisPoints,
    pub received_at: u64,
    pub expires_at: u64,
    pub metadata: BidMetadata,
}

impl BidRequest {
    pub fn projected_gross(&self) -> Result<Amount> {
        self.price.apply_floor(self.amount_in)
    }

    pub fn projected_net(&self) -> Result<Amount> {
        let gross = self.projected_gross()?;
        let fee = self.fee_bps.checked_amount_floor(gross)?;
        gross.checked_sub(fee)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct BidMetadata {
    pub lane: String,
    pub client_tag: String,
    pub quote_ref: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum BidStatus {
    Received,
    Admitted,
    Rejected,
    Selected,
    Settled,
    Superseded,
}

impl BidStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            BidStatus::Received => "received",
            BidStatus::Admitted => "admitted",
            BidStatus::Rejected => "rejected",
            BidStatus::Selected => "selected",
            BidStatus::Settled => "settled",
            BidStatus::Superseded => "superseded",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ScoreBreakdown {
    pub net_out: u128,
    pub guarantee_bps: u32,
    pub fee_bps: u32,
    pub route_priority: i128,
    pub risk_penalty: i128,
    pub final_score: i128,
}

impl ScoreBreakdown {
    pub fn new(
        net_out: Amount,
        guarantee_bps: BasisPoints,
        fee_bps: BasisPoints,
        route_priority: i128,
        risk_penalty: i128,
    ) -> Self {
        let final_score = net_out.raw() as i128 + route_priority + guarantee_bps.raw() as i128
            - (fee_bps.raw() as i128 * 3)
            - risk_penalty;
        Self {
            net_out: net_out.raw(),
            guarantee_bps: guarantee_bps.raw(),
            fee_bps: fee_bps.raw(),
            route_priority,
            risk_penalty,
            final_score,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BidTicket {
    pub request: BidRequest,
    pub status: BidStatus,
    pub assessment: RiskAssessment,
    pub score: ScoreBreakdown,
    pub reject_reason: Option<String>,
}

impl BidTicket {
    pub fn id(&self) -> &BidId {
        &self.request.id
    }

    pub fn batch(&self) -> &BatchId {
        &self.request.batch
    }

    pub fn route(&self) -> &RouteId {
        &self.request.route
    }

    pub fn operator(&self) -> &OperatorId {
        &self.request.operator
    }

    pub fn is_live_at(&self, timestamp: u64) -> bool {
        timestamp <= self.request.expires_at
            && matches!(
                self.status,
                BidStatus::Admitted | BidStatus::Selected | BidStatus::Received
            )
    }

    pub fn is_eligible(&self, timestamp: u64) -> bool {
        self.assessment.admitted
            && self.is_live_at(timestamp)
            && matches!(self.status, BidStatus::Admitted | BidStatus::Selected)
    }

    pub fn liquidity_snapshot(&self) -> &LiquiditySnapshot {
        &self.assessment.snapshot
    }

    pub fn gross_out(&self) -> Amount {
        Amount(self.assessment.snapshot.projected_gross_out)
    }

    pub fn net_out(&self) -> Amount {
        Amount(self.assessment.snapshot.projected_net_out)
    }

    pub fn required_guarantee(&self) -> Amount {
        Amount(self.assessment.snapshot.required_guarantee)
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AuctionBook {
    tickets: BTreeMap<BidId, BidTicket>,
}

impl AuctionBook {
    pub fn new() -> Self {
        Self {
            tickets: BTreeMap::new(),
        }
    }

    pub fn submit(
        &mut self,
        request: BidRequest,
        now: u64,
        assets: &AssetBook,
        accounts: &AccountBook,
        operators: &OperatorBook,
        routes: &RouteBook,
        risk: &RiskEngine,
    ) -> Result<BidTicket> {
        if self.tickets.contains_key(&request.id) {
            return Err(EclipseError::DuplicateId(request.id.to_string()));
        }
        let mut assessment = risk.assess_bid(
            &request.batch,
            &request.route,
            &request.operator,
            request.amount_in,
            request.price,
            request.fee_bps,
            request.guarantee_bps,
            now,
            assets,
            accounts,
            operators,
            routes,
        )?;
        let route = routes.get(&request.route)?;
        let route_priority = route.class.base_priority();
        let score = ScoreBreakdown::new(
            Amount(assessment.snapshot.projected_net_out),
            request.guarantee_bps,
            request.fee_bps,
            route_priority,
            assessment.score_penalty,
        );
        let status = if assessment.admitted {
            BidStatus::Admitted
        } else {
            BidStatus::Rejected
        };
        if !assessment.admitted && request.expires_at < now {
            assessment.reason = "expired".to_owned();
        }
        let ticket = BidTicket {
            request,
            status,
            reject_reason: if assessment.admitted {
                None
            } else {
                Some(assessment.reason.clone())
            },
            assessment,
            score,
        };
        self.tickets
            .insert(ticket.request.id.clone(), ticket.clone());
        Ok(ticket)
    }

    pub fn get(&self, id: &BidId) -> Result<&BidTicket> {
        self.tickets
            .get(id)
            .ok_or_else(|| EclipseError::BidNotFound(id.to_string()))
    }

    pub fn get_mut(&mut self, id: &BidId) -> Result<&mut BidTicket> {
        self.tickets
            .get_mut(id)
            .ok_or_else(|| EclipseError::BidNotFound(id.to_string()))
    }

    pub fn by_batch(&self, batch: &BatchId) -> Vec<&BidTicket> {
        self.tickets
            .values()
            .filter(|ticket| ticket.batch() == batch)
            .collect()
    }

    pub fn admitted_by_batch(&self, batch: &BatchId, now: u64) -> Vec<&BidTicket> {
        self.by_batch(batch)
            .into_iter()
            .filter(|ticket| ticket.is_eligible(now))
            .collect()
    }

    pub fn select_winner(&mut self, batch: &BatchId, now: u64) -> Result<BidTicket> {
        let winner_id = self
            .tickets
            .values()
            .filter(|ticket| ticket.batch() == batch && ticket.is_eligible(now))
            .max_by(|left, right| {
                left.score
                    .final_score
                    .cmp(&right.score.final_score)
                    .then_with(|| right.request.received_at.cmp(&left.request.received_at))
                    .then_with(|| right.request.id.cmp(&left.request.id))
            })
            .map(|ticket| ticket.request.id.clone())
            .ok_or_else(|| EclipseError::NoEligibleBids(batch.to_string()))?;
        for ticket in self
            .tickets
            .values_mut()
            .filter(|ticket| ticket.batch() == batch)
        {
            if ticket.request.id == winner_id {
                ticket.status = BidStatus::Selected;
            } else if matches!(ticket.status, BidStatus::Admitted | BidStatus::Selected) {
                ticket.status = BidStatus::Admitted;
            }
        }
        self.get(&winner_id).cloned()
    }

    pub fn select_next(
        &mut self,
        batch: &BatchId,
        excluded: &BidId,
        now: u64,
    ) -> Result<BidTicket> {
        let next_id = self
            .tickets
            .values()
            .filter(|ticket| {
                ticket.batch() == batch
                    && ticket.id() != excluded
                    && ticket.assessment.admitted
                    && ticket.is_live_at(now)
            })
            .max_by(|left, right| {
                left.score
                    .final_score
                    .cmp(&right.score.final_score)
                    .then_with(|| right.request.received_at.cmp(&left.request.received_at))
            })
            .map(|ticket| ticket.request.id.clone())
            .ok_or_else(|| EclipseError::NoEligibleBids(batch.to_string()))?;
        self.get_mut(&next_id)?.status = BidStatus::Selected;
        self.get(&next_id).cloned()
    }

    pub fn mark_settled(&mut self, bid: &BidId) -> Result<()> {
        self.get_mut(bid)?.status = BidStatus::Settled;
        Ok(())
    }

    pub fn mark_superseded(&mut self, bid: &BidId) -> Result<()> {
        self.get_mut(bid)?.status = BidStatus::Superseded;
        Ok(())
    }

    pub fn views(&self) -> Vec<BidView> {
        self.tickets.values().map(BidView::from).collect()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BidView {
    pub id: BidId,
    pub batch: BatchId,
    pub route: RouteId,
    pub operator: OperatorId,
    pub status: String,
    pub expected_gross: u128,
    pub expected_net: u128,
    pub required_guarantee: u128,
    pub score: i128,
    pub reject_reason: Option<String>,
}

impl From<&BidTicket> for BidView {
    fn from(value: &BidTicket) -> Self {
        Self {
            id: value.request.id.clone(),
            batch: value.request.batch.clone(),
            route: value.request.route.clone(),
            operator: value.request.operator.clone(),
            status: value.status.as_str().to_owned(),
            expected_gross: value.assessment.snapshot.projected_gross_out,
            expected_net: value.assessment.snapshot.projected_net_out,
            required_guarantee: value.assessment.snapshot.required_guarantee,
            score: value.score.final_score,
            reject_reason: value.reject_reason.clone(),
        }
    }
}
