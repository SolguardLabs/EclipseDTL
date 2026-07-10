use crate::amount::Amount;
use crate::auction::BidTicket;
use crate::error::{EclipseError, Result};
use crate::ids::{AccountId, AssetId, BatchId, BidId};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum BatchStatus {
    Open,
    Selected,
    Settled,
    Cancelled,
    Failed,
}

impl BatchStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            BatchStatus::Open => "open",
            BatchStatus::Selected => "selected",
            BatchStatus::Settled => "settled",
            BatchStatus::Cancelled => "cancelled",
            BatchStatus::Failed => "failed",
        }
    }

    pub fn is_terminal(&self) -> bool {
        matches!(
            self,
            BatchStatus::Settled | BatchStatus::Cancelled | BatchStatus::Failed
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BatchOrder {
    pub id: BatchId,
    pub payer: AccountId,
    pub recipient: AccountId,
    pub source: AssetId,
    pub target: AssetId,
    pub amount_in: Amount,
    pub min_out: Amount,
    pub opened_at: u64,
    pub deadline: u64,
    pub allow_fallback: bool,
    pub memo: String,
}

impl BatchOrder {
    pub fn new(
        id: BatchId,
        payer: AccountId,
        recipient: AccountId,
        source: AssetId,
        target: AssetId,
        amount_in: Amount,
        min_out: Amount,
        opened_at: u64,
        deadline: u64,
    ) -> Self {
        Self {
            id,
            payer,
            recipient,
            source,
            target,
            amount_in,
            min_out,
            opened_at,
            deadline,
            allow_fallback: false,
            memo: String::new(),
        }
    }

    pub fn with_fallback(mut self, allow_fallback: bool) -> Self {
        self.allow_fallback = allow_fallback;
        self
    }

    pub fn with_memo(mut self, memo: impl Into<String>) -> Self {
        self.memo = memo.into();
        self
    }

    pub fn is_live_at(&self, now: u64) -> bool {
        now <= self.deadline
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BatchRecord {
    pub order: BatchOrder,
    pub status: BatchStatus,
    pub selected_bid: Option<BidId>,
    pub fallback_bid: Option<BidId>,
    pub closed_at: Option<u64>,
    pub fail_reason: Option<String>,
}

impl BatchRecord {
    pub fn new(order: BatchOrder) -> Self {
        Self {
            order,
            status: BatchStatus::Open,
            selected_bid: None,
            fallback_bid: None,
            closed_at: None,
            fail_reason: None,
        }
    }

    pub fn ensure_open(&self) -> Result<()> {
        if self.status.is_terminal() {
            return Err(EclipseError::BatchClosed(self.order.id.to_string()));
        }
        Ok(())
    }

    pub fn select(&mut self, ticket: &BidTicket) -> Result<()> {
        self.ensure_open()?;
        if ticket.batch() != &self.order.id {
            return Err(EclipseError::InvalidScenario(format!(
                "bid {} belongs to batch {}",
                ticket.id(),
                ticket.batch()
            )));
        }
        self.selected_bid = Some(ticket.id().clone());
        self.status = BatchStatus::Selected;
        Ok(())
    }

    pub fn set_fallback(&mut self, ticket: &BidTicket) -> Result<()> {
        self.ensure_open()?;
        self.fallback_bid = Some(ticket.id().clone());
        self.status = BatchStatus::Selected;
        Ok(())
    }

    pub fn close_settled(&mut self, now: u64) {
        self.status = BatchStatus::Settled;
        self.closed_at = Some(now);
    }

    pub fn close_failed(&mut self, now: u64, reason: impl Into<String>) {
        self.status = BatchStatus::Failed;
        self.closed_at = Some(now);
        self.fail_reason = Some(reason.into());
    }

    pub fn cancel(&mut self, now: u64, reason: impl Into<String>) {
        self.status = BatchStatus::Cancelled;
        self.closed_at = Some(now);
        self.fail_reason = Some(reason.into());
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct BatchBook {
    batches: BTreeMap<BatchId, BatchRecord>,
}

impl BatchBook {
    pub fn new() -> Self {
        Self {
            batches: BTreeMap::new(),
        }
    }

    pub fn insert(&mut self, order: BatchOrder) -> Result<()> {
        if self.batches.contains_key(&order.id) {
            return Err(EclipseError::DuplicateId(order.id.to_string()));
        }
        self.batches
            .insert(order.id.clone(), BatchRecord::new(order));
        Ok(())
    }

    pub fn get(&self, id: &BatchId) -> Result<&BatchRecord> {
        self.batches
            .get(id)
            .ok_or_else(|| EclipseError::BatchNotFound(id.to_string()))
    }

    pub fn get_mut(&mut self, id: &BatchId) -> Result<&mut BatchRecord> {
        self.batches
            .get_mut(id)
            .ok_or_else(|| EclipseError::BatchNotFound(id.to_string()))
    }

    pub fn order(&self, id: &BatchId) -> Result<&BatchOrder> {
        Ok(&self.get(id)?.order)
    }

    pub fn select(&mut self, id: &BatchId, ticket: &BidTicket) -> Result<()> {
        self.get_mut(id)?.select(ticket)
    }

    pub fn set_fallback(&mut self, id: &BatchId, ticket: &BidTicket) -> Result<()> {
        self.get_mut(id)?.set_fallback(ticket)
    }

    pub fn close_settled(&mut self, id: &BatchId, now: u64) -> Result<()> {
        self.get_mut(id)?.close_settled(now);
        Ok(())
    }

    pub fn close_failed(
        &mut self,
        id: &BatchId,
        now: u64,
        reason: impl Into<String>,
    ) -> Result<()> {
        self.get_mut(id)?.close_failed(now, reason);
        Ok(())
    }

    pub fn cancel(&mut self, id: &BatchId, now: u64, reason: impl Into<String>) -> Result<()> {
        self.get_mut(id)?.cancel(now, reason);
        Ok(())
    }

    pub fn views(&self) -> Vec<BatchView> {
        self.batches.values().map(BatchView::from).collect()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BatchView {
    pub id: BatchId,
    pub payer: AccountId,
    pub recipient: AccountId,
    pub source: AssetId,
    pub target: AssetId,
    pub amount_in: u128,
    pub min_out: u128,
    pub status: String,
    pub selected_bid: Option<BidId>,
    pub fallback_bid: Option<BidId>,
    pub opened_at: u64,
    pub deadline: u64,
    pub closed_at: Option<u64>,
    pub fail_reason: Option<String>,
}

impl From<&BatchRecord> for BatchView {
    fn from(value: &BatchRecord) -> Self {
        Self {
            id: value.order.id.clone(),
            payer: value.order.payer.clone(),
            recipient: value.order.recipient.clone(),
            source: value.order.source.clone(),
            target: value.order.target.clone(),
            amount_in: value.order.amount_in.raw(),
            min_out: value.order.min_out.raw(),
            status: value.status.as_str().to_owned(),
            selected_bid: value.selected_bid.clone(),
            fallback_bid: value.fallback_bid.clone(),
            opened_at: value.order.opened_at,
            deadline: value.order.deadline,
            closed_at: value.closed_at,
            fail_reason: value.fail_reason.clone(),
        }
    }
}
