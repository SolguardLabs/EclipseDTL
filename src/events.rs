use crate::ids::{AccountId, AssetId, BatchId, BidId, OperatorId, RouteId};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Event {
    AssetRegistered {
        asset: AssetId,
        symbol: String,
    },
    AccountCreated {
        account: AccountId,
        role: String,
    },
    Deposit {
        account: AccountId,
        asset: AssetId,
        amount: u128,
    },
    Withdrawal {
        account: AccountId,
        asset: AssetId,
        amount: u128,
    },
    OperatorRegistered {
        operator: OperatorId,
        lane: String,
    },
    GuaranteePledged {
        operator: OperatorId,
        amount: u128,
    },
    OperatorCommitment {
        operator: OperatorId,
        route: RouteId,
        amount: u128,
        label: String,
    },
    RouteRegistered {
        route: RouteId,
        source: AssetId,
        target: AssetId,
    },
    BatchOpened {
        batch: BatchId,
        source: AssetId,
        target: AssetId,
        amount_in: u128,
        min_out: u128,
    },
    BidSubmitted {
        bid: BidId,
        batch: BatchId,
        route: RouteId,
        operator: OperatorId,
        expected_net: u128,
    },
    BidSelected {
        batch: BatchId,
        bid: BidId,
        operator: OperatorId,
        score: i128,
    },
    SettlementCompleted {
        batch: BatchId,
        bid: BidId,
        operator: OperatorId,
        route: RouteId,
        gross_out: u128,
        net_out: u128,
        operator_fee: u128,
    },
    SettlementFallback {
        batch: BatchId,
        from_bid: BidId,
        to_bid: BidId,
        reason: String,
    },
    BatchClosed {
        batch: BatchId,
        status: String,
    },
    Snapshot {
        name: String,
    },
}

impl Event {
    pub fn topic(&self) -> &'static str {
        match self {
            Event::AssetRegistered { .. } => "asset",
            Event::AccountCreated { .. } => "account",
            Event::Deposit { .. } | Event::Withdrawal { .. } => "balance",
            Event::OperatorRegistered { .. }
            | Event::GuaranteePledged { .. }
            | Event::OperatorCommitment { .. } => "operator",
            Event::RouteRegistered { .. } => "route",
            Event::BatchOpened { .. } | Event::BatchClosed { .. } => "batch",
            Event::BidSubmitted { .. } | Event::BidSelected { .. } => "auction",
            Event::SettlementCompleted { .. } | Event::SettlementFallback { .. } => "settlement",
            Event::Snapshot { .. } => "snapshot",
        }
    }

    pub fn batch_id(&self) -> Option<&BatchId> {
        match self {
            Event::BatchOpened { batch, .. }
            | Event::BidSubmitted { batch, .. }
            | Event::BidSelected { batch, .. }
            | Event::SettlementCompleted { batch, .. }
            | Event::SettlementFallback { batch, .. }
            | Event::BatchClosed { batch, .. } => Some(batch),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct EventLog {
    events: Vec<Event>,
}

impl EventLog {
    pub fn new() -> Self {
        Self { events: Vec::new() }
    }

    pub fn push(&mut self, event: Event) {
        self.events.push(event);
    }

    pub fn extend<I>(&mut self, events: I)
    where
        I: IntoIterator<Item = Event>,
    {
        self.events.extend(events);
    }

    pub fn iter(&self) -> impl Iterator<Item = &Event> {
        self.events.iter()
    }

    pub fn into_vec(self) -> Vec<Event> {
        self.events
    }

    pub fn to_vec(&self) -> Vec<Event> {
        self.events.clone()
    }

    pub fn len(&self) -> usize {
        self.events.len()
    }

    pub fn is_empty(&self) -> bool {
        self.events.is_empty()
    }

    pub fn by_topic(&self, topic: &str) -> Vec<&Event> {
        self.events
            .iter()
            .filter(|event| event.topic() == topic)
            .collect()
    }

    pub fn by_batch(&self, batch: &BatchId) -> Vec<&Event> {
        self.events
            .iter()
            .filter(|event| event.batch_id() == Some(batch))
            .collect()
    }
}
