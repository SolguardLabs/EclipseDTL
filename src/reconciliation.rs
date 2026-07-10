use crate::accounts::{AccountBook, AccountView};
use crate::amount::Amount;
use crate::error::{EclipseError, Result};
use crate::ids::{AccountId, AssetId, BatchId, BidId, OperatorId, RouteId};
use crate::settlement::SettlementReceipt;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ReconciliationSide {
    Debit,
    Credit,
}

impl ReconciliationSide {
    pub fn as_str(&self) -> &'static str {
        match self {
            ReconciliationSide::Debit => "debit",
            ReconciliationSide::Credit => "credit",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReconciliationLine {
    pub batch: BatchId,
    pub bid: BidId,
    pub route: RouteId,
    pub account: AccountId,
    pub asset: AssetId,
    pub side: ReconciliationSide,
    pub amount: Amount,
    pub memo: String,
}

impl ReconciliationLine {
    pub fn debit(
        batch: BatchId,
        bid: BidId,
        route: RouteId,
        account: AccountId,
        asset: AssetId,
        amount: Amount,
        memo: impl Into<String>,
    ) -> Self {
        Self {
            batch,
            bid,
            route,
            account,
            asset,
            side: ReconciliationSide::Debit,
            amount,
            memo: memo.into(),
        }
    }

    pub fn credit(
        batch: BatchId,
        bid: BidId,
        route: RouteId,
        account: AccountId,
        asset: AssetId,
        amount: Amount,
        memo: impl Into<String>,
    ) -> Self {
        Self {
            batch,
            bid,
            route,
            account,
            asset,
            side: ReconciliationSide::Credit,
            amount,
            memo: memo.into(),
        }
    }

    pub fn signed_amount(&self) -> i128 {
        match self.side {
            ReconciliationSide::Debit => -(self.amount.raw() as i128),
            ReconciliationSide::Credit => self.amount.raw() as i128,
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ReconciliationLedger {
    lines: Vec<ReconciliationLine>,
}

impl ReconciliationLedger {
    pub fn new() -> Self {
        Self { lines: Vec::new() }
    }

    pub fn push(&mut self, line: ReconciliationLine) {
        self.lines.push(line);
    }

    pub fn extend<I>(&mut self, lines: I)
    where
        I: IntoIterator<Item = ReconciliationLine>,
    {
        self.lines.extend(lines);
    }

    pub fn from_receipt(receipt: &SettlementReceipt, vault: AccountId) -> Self {
        let mut ledger = Self::new();
        ledger.extend(lines_from_receipt(receipt, vault));
        ledger
    }

    pub fn from_receipts<I>(receipts: I, vault: AccountId) -> Self
    where
        I: IntoIterator<Item = SettlementReceipt>,
    {
        let mut ledger = Self::new();
        for receipt in receipts {
            ledger.extend(lines_from_receipt(&receipt, vault.clone()));
        }
        ledger
    }

    pub fn lines(&self) -> &[ReconciliationLine] {
        &self.lines
    }

    pub fn by_account(&self, account: &AccountId) -> Vec<&ReconciliationLine> {
        self.lines
            .iter()
            .filter(|line| &line.account == account)
            .collect()
    }

    pub fn by_asset(&self, asset: &AssetId) -> Vec<&ReconciliationLine> {
        self.lines
            .iter()
            .filter(|line| &line.asset == asset)
            .collect()
    }

    pub fn by_batch(&self, batch: &BatchId) -> Vec<&ReconciliationLine> {
        self.lines
            .iter()
            .filter(|line| &line.batch == batch)
            .collect()
    }

    pub fn account_asset_delta(&self) -> BTreeMap<(AccountId, AssetId), i128> {
        let mut deltas: BTreeMap<(AccountId, AssetId), i128> = BTreeMap::new();
        for line in &self.lines {
            let key = (line.account.clone(), line.asset.clone());
            let current = deltas.get(&key).copied().unwrap_or(0);
            deltas.insert(key, current.saturating_add(line.signed_amount()));
        }
        deltas
    }

    pub fn asset_conservation(&self) -> BTreeMap<AssetId, i128> {
        let mut deltas: BTreeMap<AssetId, i128> = BTreeMap::new();
        for line in &self.lines {
            let current = deltas.get(&line.asset).copied().unwrap_or(0);
            deltas.insert(
                line.asset.clone(),
                current.saturating_add(line.signed_amount()),
            );
        }
        deltas
    }

    pub fn assert_conserved(&self) -> Result<()> {
        for (asset, delta) in self.asset_conservation() {
            if delta != 0 {
                return Err(EclipseError::InvalidScenario(format!(
                    "asset {} reconciliation delta {}",
                    asset, delta
                )));
            }
        }
        Ok(())
    }
}

pub fn lines_from_receipt(
    receipt: &SettlementReceipt,
    vault: AccountId,
) -> Vec<ReconciliationLine> {
    let batch = receipt.key.batch_id.clone();
    let bid = receipt.key.bid_id.clone();
    let route = receipt.key.route_id.clone();
    let fee_account = AccountId::new(format!("fee-{}", receipt.operator.as_str()));
    let mut lines = vec![
        ReconciliationLine::debit(
            batch.clone(),
            bid.clone(),
            route.clone(),
            receipt.payer.clone(),
            receipt.source.clone(),
            Amount(receipt.amount_in),
            "payer source debit",
        ),
        ReconciliationLine::credit(
            batch.clone(),
            bid.clone(),
            route.clone(),
            vault.clone(),
            receipt.source.clone(),
            Amount(receipt.amount_in),
            "vault source credit",
        ),
        ReconciliationLine::debit(
            batch.clone(),
            bid.clone(),
            route.clone(),
            vault.clone(),
            receipt.target.clone(),
            Amount(receipt.net_out),
            "recipient payout debit",
        ),
        ReconciliationLine::credit(
            batch.clone(),
            bid.clone(),
            route.clone(),
            receipt.recipient.clone(),
            receipt.target.clone(),
            Amount(receipt.net_out),
            "recipient payout credit",
        ),
    ];
    if receipt.operator_fee > 0 {
        lines.push(ReconciliationLine::debit(
            batch.clone(),
            bid.clone(),
            route.clone(),
            vault,
            receipt.target.clone(),
            Amount(receipt.operator_fee),
            "operator fee debit",
        ));
        lines.push(ReconciliationLine::credit(
            batch,
            bid,
            route,
            fee_account,
            receipt.target.clone(),
            Amount(receipt.operator_fee),
            "operator fee credit",
        ));
    }
    lines
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AccountBalanceObservation {
    pub account: AccountId,
    pub asset: AssetId,
    pub available: Amount,
    pub reserved: Amount,
}

impl AccountBalanceObservation {
    pub fn total(&self) -> Result<Amount> {
        self.available.checked_add(self.reserved)
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct BalanceObservationSet {
    observations: Vec<AccountBalanceObservation>,
}

impl BalanceObservationSet {
    pub fn from_account_views(views: &[AccountView]) -> Self {
        let mut set = Self::default();
        for account in views {
            for balance in &account.balances {
                set.observations.push(AccountBalanceObservation {
                    account: account.id.clone(),
                    asset: balance.asset.clone(),
                    available: Amount(balance.available),
                    reserved: Amount(balance.reserved),
                });
            }
        }
        set
    }

    pub fn from_account_book(accounts: &AccountBook) -> Self {
        Self::from_account_views(&accounts.views())
    }

    pub fn observations(&self) -> &[AccountBalanceObservation] {
        &self.observations
    }

    pub fn get(&self, account: &AccountId, asset: &AssetId) -> Amount {
        self.observations
            .iter()
            .find(|obs| &obs.account == account && &obs.asset == asset)
            .map(|obs| obs.available)
            .unwrap_or_default()
    }

    pub fn total_by_asset(&self) -> Result<BTreeMap<AssetId, Amount>> {
        let mut totals: BTreeMap<AssetId, Amount> = BTreeMap::new();
        for observation in &self.observations {
            let current = totals.get(&observation.asset).copied().unwrap_or_default();
            totals.insert(
                observation.asset.clone(),
                current.checked_add(observation.total()?)?,
            );
        }
        Ok(totals)
    }

    pub fn diff(&self, next: &BalanceObservationSet) -> BalanceDiff {
        let mut entries: BTreeMap<(AccountId, AssetId), BalanceDiffEntry> = BTreeMap::new();
        for observation in &self.observations {
            let key = (observation.account.clone(), observation.asset.clone());
            entries.insert(
                key,
                BalanceDiffEntry {
                    account: observation.account.clone(),
                    asset: observation.asset.clone(),
                    before: observation.available.raw(),
                    after: 0,
                    delta: -(observation.available.raw() as i128),
                },
            );
        }
        for observation in &next.observations {
            let key = (observation.account.clone(), observation.asset.clone());
            let entry = entries.entry(key).or_insert_with(|| BalanceDiffEntry {
                account: observation.account.clone(),
                asset: observation.asset.clone(),
                before: 0,
                after: 0,
                delta: 0,
            });
            entry.after = observation.available.raw();
            entry.delta = entry.after.saturating_sub(entry.before) as i128
                - if entry.before > entry.after {
                    (entry.before - entry.after) as i128
                } else {
                    0
                };
            if entry.after < entry.before {
                entry.delta = -((entry.before - entry.after) as i128);
            }
        }
        BalanceDiff {
            entries: entries.into_values().collect(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BalanceDiffEntry {
    pub account: AccountId,
    pub asset: AssetId,
    pub before: u128,
    pub after: u128,
    pub delta: i128,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BalanceDiff {
    pub entries: Vec<BalanceDiffEntry>,
}

impl BalanceDiff {
    pub fn non_zero(&self) -> Vec<&BalanceDiffEntry> {
        self.entries
            .iter()
            .filter(|entry| entry.delta != 0)
            .collect()
    }

    pub fn by_account(&self, account: &AccountId) -> Vec<&BalanceDiffEntry> {
        self.entries
            .iter()
            .filter(|entry| &entry.account == account)
            .collect()
    }

    pub fn by_asset(&self, asset: &AssetId) -> Vec<&BalanceDiffEntry> {
        self.entries
            .iter()
            .filter(|entry| &entry.asset == asset)
            .collect()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OperatorSettlementSummary {
    pub operator: OperatorId,
    pub route: RouteId,
    pub batches: u64,
    pub gross_out: u128,
    pub net_out: u128,
    pub fees: u128,
    pub required_guarantee: u128,
    pub attached_guarantee: u128,
}

impl OperatorSettlementSummary {
    pub fn new(operator: OperatorId, route: RouteId) -> Self {
        Self {
            operator,
            route,
            batches: 0,
            gross_out: 0,
            net_out: 0,
            fees: 0,
            required_guarantee: 0,
            attached_guarantee: 0,
        }
    }

    pub fn record(&mut self, receipt: &SettlementReceipt) {
        self.batches = self.batches.saturating_add(1);
        self.gross_out = self.gross_out.saturating_add(receipt.gross_out);
        self.net_out = self.net_out.saturating_add(receipt.net_out);
        self.fees = self.fees.saturating_add(receipt.operator_fee);
        self.required_guarantee = self
            .required_guarantee
            .saturating_add(receipt.required_guarantee);
        self.attached_guarantee = self
            .attached_guarantee
            .saturating_add(receipt.attached_guarantee);
    }

    pub fn guarantee_margin(&self) -> i128 {
        self.attached_guarantee as i128 - self.required_guarantee as i128
    }
}

pub fn summarize_by_operator_route(
    receipts: &[SettlementReceipt],
) -> Vec<OperatorSettlementSummary> {
    let mut summaries: BTreeMap<(OperatorId, RouteId), OperatorSettlementSummary> = BTreeMap::new();
    for receipt in receipts {
        let key = (receipt.operator.clone(), receipt.key.route_id.clone());
        summaries
            .entry(key.clone())
            .or_insert_with(|| OperatorSettlementSummary::new(key.0, key.1))
            .record(receipt);
    }
    summaries.into_values().collect()
}
