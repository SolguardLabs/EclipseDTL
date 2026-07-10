use crate::amount::Amount;
use crate::error::{EclipseError, Result};
use crate::ids::{AccountId, AssetId};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum AccountRole {
    User,
    Vault,
    OperatorFee,
    Treasury,
    Insurance,
}

impl AccountRole {
    pub fn as_str(&self) -> &'static str {
        match self {
            AccountRole::User => "user",
            AccountRole::Vault => "vault",
            AccountRole::OperatorFee => "operator_fee",
            AccountRole::Treasury => "treasury",
            AccountRole::Insurance => "insurance",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct BalanceCell {
    pub available: Amount,
    pub reserved: Amount,
    pub pending_credit: Amount,
    pub cumulative_debit: Amount,
    pub cumulative_credit: Amount,
}

impl BalanceCell {
    pub fn new(available: Amount) -> Self {
        Self {
            available,
            reserved: Amount::ZERO,
            pending_credit: Amount::ZERO,
            cumulative_debit: Amount::ZERO,
            cumulative_credit: available,
        }
    }

    pub fn available(&self) -> Amount {
        self.available
    }

    pub fn total(&self) -> Result<Amount> {
        self.available
            .checked_add(self.reserved)?
            .checked_add(self.pending_credit)
    }

    pub fn credit(&mut self, amount: Amount) -> Result<()> {
        self.available = self.available.checked_add(amount)?;
        self.cumulative_credit = self.cumulative_credit.checked_add(amount)?;
        Ok(())
    }

    pub fn debit(&mut self, amount: Amount) -> Result<()> {
        if self.available < amount {
            return Err(EclipseError::AmountUnderflow);
        }
        self.available = self.available.checked_sub(amount)?;
        self.cumulative_debit = self.cumulative_debit.checked_add(amount)?;
        Ok(())
    }

    pub fn reserve(&mut self, amount: Amount) -> Result<()> {
        if self.available < amount {
            return Err(EclipseError::AmountUnderflow);
        }
        self.available = self.available.checked_sub(amount)?;
        self.reserved = self.reserved.checked_add(amount)?;
        Ok(())
    }

    pub fn release(&mut self, amount: Amount) -> Result<()> {
        if self.reserved < amount {
            return Err(EclipseError::AmountUnderflow);
        }
        self.reserved = self.reserved.checked_sub(amount)?;
        self.available = self.available.checked_add(amount)?;
        Ok(())
    }

    pub fn settle_reserved(&mut self, amount: Amount) -> Result<()> {
        if self.reserved < amount {
            return Err(EclipseError::AmountUnderflow);
        }
        self.reserved = self.reserved.checked_sub(amount)?;
        self.cumulative_debit = self.cumulative_debit.checked_add(amount)?;
        Ok(())
    }

    pub fn queue_credit(&mut self, amount: Amount) -> Result<()> {
        self.pending_credit = self.pending_credit.checked_add(amount)?;
        Ok(())
    }

    pub fn mature_credit(&mut self, amount: Amount) -> Result<()> {
        if self.pending_credit < amount {
            return Err(EclipseError::AmountUnderflow);
        }
        self.pending_credit = self.pending_credit.checked_sub(amount)?;
        self.available = self.available.checked_add(amount)?;
        self.cumulative_credit = self.cumulative_credit.checked_add(amount)?;
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AccountState {
    pub id: AccountId,
    pub role: AccountRole,
    pub label: String,
    balances: BTreeMap<AssetId, BalanceCell>,
    pub frozen: bool,
}

impl AccountState {
    pub fn new(id: AccountId, role: AccountRole, label: impl Into<String>) -> Self {
        Self {
            id,
            role,
            label: label.into(),
            balances: BTreeMap::new(),
            frozen: false,
        }
    }

    pub fn balance_cell(&self, asset: &AssetId) -> BalanceCell {
        self.balances.get(asset).cloned().unwrap_or_default()
    }

    pub fn balance(&self, asset: &AssetId) -> Amount {
        self.balance_cell(asset).available()
    }

    pub fn balances(&self) -> &BTreeMap<AssetId, BalanceCell> {
        &self.balances
    }

    pub fn ensure_active(&self) -> Result<()> {
        if self.frozen {
            Err(EclipseError::InvalidScenario(format!(
                "account {} is frozen",
                self.id
            )))
        } else {
            Ok(())
        }
    }

    pub fn credit(&mut self, asset: AssetId, amount: Amount) -> Result<()> {
        self.ensure_active()?;
        self.balances.entry(asset).or_default().credit(amount)
    }

    pub fn debit(&mut self, asset: &AssetId, amount: Amount) -> Result<()> {
        self.ensure_active()?;
        let cell = self.balances.entry(asset.clone()).or_default();
        if cell.available < amount {
            return Err(EclipseError::InsufficientBalance {
                account: self.id.to_string(),
                asset: asset.to_string(),
                available: cell.available.raw(),
                needed: amount.raw(),
            });
        }
        cell.debit(amount)
    }

    pub fn reserve(&mut self, asset: &AssetId, amount: Amount) -> Result<()> {
        self.ensure_active()?;
        let cell = self.balances.entry(asset.clone()).or_default();
        if cell.available < amount {
            return Err(EclipseError::InsufficientBalance {
                account: self.id.to_string(),
                asset: asset.to_string(),
                available: cell.available.raw(),
                needed: amount.raw(),
            });
        }
        cell.reserve(amount)
    }

    pub fn release(&mut self, asset: &AssetId, amount: Amount) -> Result<()> {
        self.ensure_active()?;
        self.balances
            .entry(asset.clone())
            .or_default()
            .release(amount)
    }

    pub fn queue_credit(&mut self, asset: AssetId, amount: Amount) -> Result<()> {
        self.ensure_active()?;
        self.balances.entry(asset).or_default().queue_credit(amount)
    }

    pub fn mature_credit(&mut self, asset: &AssetId, amount: Amount) -> Result<()> {
        self.ensure_active()?;
        self.balances
            .entry(asset.clone())
            .or_default()
            .mature_credit(amount)
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AccountBook {
    accounts: BTreeMap<AccountId, AccountState>,
}

impl AccountBook {
    pub fn new() -> Self {
        Self {
            accounts: BTreeMap::new(),
        }
    }

    pub fn insert(&mut self, account: AccountState) -> Result<()> {
        if self.accounts.contains_key(&account.id) {
            return Err(EclipseError::DuplicateId(account.id.to_string()));
        }
        self.accounts.insert(account.id.clone(), account);
        Ok(())
    }

    pub fn create(
        &mut self,
        id: AccountId,
        role: AccountRole,
        label: impl Into<String>,
    ) -> Result<()> {
        self.insert(AccountState::new(id, role, label))
    }

    pub fn get(&self, id: &AccountId) -> Result<&AccountState> {
        self.accounts
            .get(id)
            .ok_or_else(|| EclipseError::AccountNotFound(id.to_string()))
    }

    pub fn get_mut(&mut self, id: &AccountId) -> Result<&mut AccountState> {
        self.accounts
            .get_mut(id)
            .ok_or_else(|| EclipseError::AccountNotFound(id.to_string()))
    }

    pub fn ensure(&self, id: &AccountId) -> Result<()> {
        self.get(id).map(|_| ())
    }

    pub fn deposit(&mut self, account: &AccountId, asset: AssetId, amount: Amount) -> Result<()> {
        self.get_mut(account)?.credit(asset, amount)
    }

    pub fn withdraw(&mut self, account: &AccountId, asset: &AssetId, amount: Amount) -> Result<()> {
        self.get_mut(account)?.debit(asset, amount)
    }

    pub fn reserve(&mut self, account: &AccountId, asset: &AssetId, amount: Amount) -> Result<()> {
        self.get_mut(account)?.reserve(asset, amount)
    }

    pub fn release(&mut self, account: &AccountId, asset: &AssetId, amount: Amount) -> Result<()> {
        self.get_mut(account)?.release(asset, amount)
    }

    pub fn transfer(
        &mut self,
        from: &AccountId,
        to: &AccountId,
        asset: &AssetId,
        amount: Amount,
    ) -> Result<()> {
        if from == to || amount.is_zero() {
            return Ok(());
        }
        self.ensure(from)?;
        self.ensure(to)?;
        self.withdraw(from, asset, amount)?;
        self.deposit(to, asset.clone(), amount)?;
        Ok(())
    }

    pub fn balance(&self, account: &AccountId, asset: &AssetId) -> Result<Amount> {
        Ok(self.get(account)?.balance(asset))
    }

    pub fn contains(&self, account: &AccountId) -> bool {
        self.accounts.contains_key(account)
    }

    pub fn all_accounts(&self) -> Vec<&AccountState> {
        self.accounts.values().collect()
    }

    pub fn views(&self) -> Vec<AccountView> {
        self.accounts.values().map(AccountView::from).collect()
    }

    pub fn total_available(&self, asset: &AssetId) -> Result<Amount> {
        self.accounts
            .values()
            .map(|account| account.balance(asset))
            .try_fold(Amount::ZERO, |acc, amount| acc.checked_add(amount))
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BalanceView {
    pub asset: AssetId,
    pub available: u128,
    pub reserved: u128,
    pub pending_credit: u128,
    pub cumulative_debit: u128,
    pub cumulative_credit: u128,
}

impl BalanceView {
    pub fn from_cell(asset: AssetId, cell: &BalanceCell) -> Self {
        Self {
            asset,
            available: cell.available.raw(),
            reserved: cell.reserved.raw(),
            pending_credit: cell.pending_credit.raw(),
            cumulative_debit: cell.cumulative_debit.raw(),
            cumulative_credit: cell.cumulative_credit.raw(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AccountView {
    pub id: AccountId,
    pub role: String,
    pub label: String,
    pub frozen: bool,
    pub balances: Vec<BalanceView>,
}

impl From<&AccountState> for AccountView {
    fn from(value: &AccountState) -> Self {
        Self {
            id: value.id.clone(),
            role: value.role.as_str().to_owned(),
            label: value.label.clone(),
            frozen: value.frozen,
            balances: value
                .balances
                .iter()
                .map(|(asset, cell)| BalanceView::from_cell(asset.clone(), cell))
                .collect(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TransferPlan {
    pub from: AccountId,
    pub to: AccountId,
    pub asset: AssetId,
    pub amount: Amount,
    pub memo: String,
}

impl TransferPlan {
    pub fn new(
        from: AccountId,
        to: AccountId,
        asset: AssetId,
        amount: Amount,
        memo: impl Into<String>,
    ) -> Self {
        Self {
            from,
            to,
            asset,
            amount,
            memo: memo.into(),
        }
    }

    pub fn apply(&self, accounts: &mut AccountBook) -> Result<()> {
        accounts.transfer(&self.from, &self.to, &self.asset, self.amount)
    }
}
