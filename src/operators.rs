use crate::amount::{Amount, BasisPoints};
use crate::error::{EclipseError, Result};
use crate::ids::{AccountId, AssetId, OperatorId, RouteId};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum OperatorStatus {
    Active,
    Paused,
    Draining,
    Suspended,
}

impl OperatorStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            OperatorStatus::Active => "active",
            OperatorStatus::Paused => "paused",
            OperatorStatus::Draining => "draining",
            OperatorStatus::Suspended => "suspended",
        }
    }

    pub fn can_bid(&self) -> bool {
        matches!(self, OperatorStatus::Active)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GuaranteeState {
    pub pledged: Amount,
    pub locked: Amount,
    pub pending_release: Amount,
    pub slash_accumulator: Amount,
    pub reserve_floor_bps: BasisPoints,
}

impl Default for GuaranteeState {
    fn default() -> Self {
        Self {
            pledged: Amount::ZERO,
            locked: Amount::ZERO,
            pending_release: Amount::ZERO,
            slash_accumulator: Amount::ZERO,
            reserve_floor_bps: BasisPoints::ZERO,
        }
    }
}

impl GuaranteeState {
    pub fn pledge(&mut self, amount: Amount) -> Result<()> {
        self.pledged = self.pledged.checked_add(amount)?;
        Ok(())
    }

    pub fn available(&self) -> Result<Amount> {
        let used = self.locked.checked_add(self.pending_release)?;
        if self.pledged < used {
            return Ok(Amount::ZERO);
        }
        let free = self.pledged.checked_sub(used)?;
        let floor = self.reserve_floor_bps.checked_amount_floor(self.pledged)?;
        if free <= floor {
            Ok(Amount::ZERO)
        } else {
            free.checked_sub(floor)
        }
    }

    pub fn attach(&mut self, amount: Amount) -> Result<Amount> {
        let available = self.available()?;
        let applied = available.min(amount);
        self.locked = self.locked.checked_add(applied)?;
        Ok(applied)
    }

    pub fn release(&mut self, amount: Amount) -> Result<()> {
        if self.locked < amount {
            return Err(EclipseError::AmountUnderflow);
        }
        self.locked = self.locked.checked_sub(amount)?;
        self.pending_release = self.pending_release.checked_add(amount)?;
        Ok(())
    }

    pub fn mature_release(&mut self, amount: Amount) -> Result<()> {
        if self.pending_release < amount {
            return Err(EclipseError::AmountUnderflow);
        }
        self.pending_release = self.pending_release.checked_sub(amount)?;
        Ok(())
    }

    pub fn slash(&mut self, amount: Amount) -> Result<Amount> {
        let slashable = self.pledged.min(amount);
        self.pledged = self.pledged.checked_sub(slashable)?;
        self.slash_accumulator = self.slash_accumulator.checked_add(slashable)?;
        if self.locked > self.pledged {
            self.locked = self.pledged;
        }
        Ok(slashable)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct ExposureCell {
    pub committed: Amount,
    pub settled: Amount,
    pub last_batch: Option<String>,
}

impl ExposureCell {
    pub fn add_commitment(&mut self, amount: Amount, batch: impl Into<String>) -> Result<()> {
        self.committed = self.committed.checked_add(amount)?;
        self.last_batch = Some(batch.into());
        Ok(())
    }

    pub fn settle(&mut self, amount: Amount) -> Result<()> {
        if self.committed < amount {
            self.committed = Amount::ZERO;
        } else {
            self.committed = self.committed.checked_sub(amount)?;
        }
        self.settled = self.settled.checked_add(amount)?;
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct ExposureState {
    by_route: BTreeMap<RouteId, ExposureCell>,
    by_asset: BTreeMap<AssetId, Amount>,
    pub external_commitment: Amount,
}

impl ExposureState {
    pub fn add_route_commitment(
        &mut self,
        route: RouteId,
        target_asset: AssetId,
        amount: Amount,
        batch: impl Into<String>,
    ) -> Result<()> {
        self.by_route
            .entry(route)
            .or_default()
            .add_commitment(amount, batch)?;
        let next = self
            .by_asset
            .get(&target_asset)
            .copied()
            .unwrap_or_default()
            .checked_add(amount)?;
        self.by_asset.insert(target_asset, next);
        Ok(())
    }

    pub fn add_external(&mut self, amount: Amount) -> Result<()> {
        self.external_commitment = self.external_commitment.checked_add(amount)?;
        Ok(())
    }

    pub fn settle_route(
        &mut self,
        route: &RouteId,
        target_asset: &AssetId,
        amount: Amount,
    ) -> Result<()> {
        if let Some(cell) = self.by_route.get_mut(route) {
            cell.settle(amount)?;
        }
        if let Some(asset_amount) = self.by_asset.get_mut(target_asset) {
            *asset_amount = asset_amount.saturating_sub(amount);
        }
        Ok(())
    }

    pub fn route_exposure(&self, route: &RouteId) -> Amount {
        self.by_route
            .get(route)
            .map(|cell| cell.committed)
            .unwrap_or_default()
    }

    pub fn asset_exposure(&self, asset: &AssetId) -> Amount {
        self.by_asset.get(asset).copied().unwrap_or_default()
    }

    pub fn total_route_exposure(&self) -> Result<Amount> {
        self.by_route
            .values()
            .map(|cell| cell.committed)
            .try_fold(Amount::ZERO, |acc, amount| acc.checked_add(amount))
    }

    pub fn total_exposure(&self) -> Result<Amount> {
        self.total_route_exposure()?
            .checked_add(self.external_commitment)
    }

    pub fn route_views(&self) -> Vec<RouteExposureView> {
        self.by_route
            .iter()
            .map(|(route, cell)| RouteExposureView {
                route: route.clone(),
                committed: cell.committed.raw(),
                settled: cell.settled.raw(),
                last_batch: cell.last_batch.clone(),
            })
            .collect()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OperatorProfile {
    pub id: OperatorId,
    pub display_name: String,
    pub lane: String,
    pub status: OperatorStatus,
    pub guarantee: GuaranteeState,
    pub exposure: ExposureState,
    pub fee_account: AccountId,
    pub fee_floor_bps: BasisPoints,
    pub reliability_bps: BasisPoints,
    pub max_open_batches: u32,
    pub tags: Vec<String>,
}

impl OperatorProfile {
    pub fn new(
        id: OperatorId,
        display_name: impl Into<String>,
        lane: impl Into<String>,
        fee_account: AccountId,
    ) -> Self {
        Self {
            id,
            display_name: display_name.into(),
            lane: lane.into(),
            status: OperatorStatus::Active,
            guarantee: GuaranteeState::default(),
            exposure: ExposureState::default(),
            fee_account,
            fee_floor_bps: BasisPoints::ZERO,
            reliability_bps: BasisPoints::ONE_HUNDRED_PERCENT,
            max_open_batches: 64,
            tags: Vec::new(),
        }
    }

    pub fn can_bid(&self) -> bool {
        self.status.can_bid()
    }

    pub fn available_guarantee(&self) -> Result<Amount> {
        self.guarantee.available()
    }

    pub fn pledge(&mut self, amount: Amount) -> Result<()> {
        self.guarantee.pledge(amount)
    }

    pub fn attach_guarantee(
        &mut self,
        route: RouteId,
        target_asset: AssetId,
        amount: Amount,
        batch: impl Into<String>,
    ) -> Result<GuaranteeAttachment> {
        let attached = self.guarantee.attach(amount)?;
        self.exposure
            .add_route_commitment(route.clone(), target_asset, amount, batch)?;
        Ok(GuaranteeAttachment {
            requested: amount,
            attached,
            route,
        })
    }

    pub fn allocate_external_commitment(&mut self, amount: Amount) -> Result<()> {
        let attached = self.guarantee.attach(amount)?;
        self.exposure.add_external(attached)?;
        Ok(())
    }

    pub fn settle_route(
        &mut self,
        route: &RouteId,
        target_asset: &AssetId,
        amount: Amount,
    ) -> Result<()> {
        self.exposure.settle_route(route, target_asset, amount)
    }

    pub fn set_status(&mut self, status: OperatorStatus) {
        self.status = status;
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GuaranteeAttachment {
    pub requested: Amount,
    pub attached: Amount,
    pub route: RouteId,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct OperatorBook {
    operators: BTreeMap<OperatorId, OperatorProfile>,
}

impl OperatorBook {
    pub fn new() -> Self {
        Self {
            operators: BTreeMap::new(),
        }
    }

    pub fn insert(&mut self, operator: OperatorProfile) -> Result<()> {
        if self.operators.contains_key(&operator.id) {
            return Err(EclipseError::DuplicateId(operator.id.to_string()));
        }
        self.operators.insert(operator.id.clone(), operator);
        Ok(())
    }

    pub fn get(&self, id: &OperatorId) -> Result<&OperatorProfile> {
        self.operators
            .get(id)
            .ok_or_else(|| EclipseError::OperatorNotFound(id.to_string()))
    }

    pub fn get_mut(&mut self, id: &OperatorId) -> Result<&mut OperatorProfile> {
        self.operators
            .get_mut(id)
            .ok_or_else(|| EclipseError::OperatorNotFound(id.to_string()))
    }

    pub fn pledge(&mut self, id: &OperatorId, amount: Amount) -> Result<()> {
        self.get_mut(id)?.pledge(amount)
    }

    pub fn set_status(&mut self, id: &OperatorId, status: OperatorStatus) -> Result<()> {
        self.get_mut(id)?.set_status(status);
        Ok(())
    }

    pub fn available_guarantee(&self, id: &OperatorId) -> Result<Amount> {
        self.get(id)?.available_guarantee()
    }

    pub fn attach_guarantee(
        &mut self,
        id: &OperatorId,
        route: RouteId,
        target_asset: AssetId,
        amount: Amount,
        batch: impl Into<String>,
    ) -> Result<GuaranteeAttachment> {
        self.get_mut(id)?
            .attach_guarantee(route, target_asset, amount, batch)
    }

    pub fn allocate_external_commitment(&mut self, id: &OperatorId, amount: Amount) -> Result<()> {
        self.get_mut(id)?.allocate_external_commitment(amount)
    }

    pub fn list(&self) -> Vec<&OperatorProfile> {
        self.operators.values().collect()
    }

    pub fn views(&self) -> Vec<OperatorView> {
        self.operators.values().map(OperatorView::from).collect()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RouteExposureView {
    pub route: RouteId,
    pub committed: u128,
    pub settled: u128,
    pub last_batch: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OperatorView {
    pub id: OperatorId,
    pub display_name: String,
    pub lane: String,
    pub status: String,
    pub fee_account: AccountId,
    pub pledged: u128,
    pub locked: u128,
    pub available: u128,
    pub external_commitment: u128,
    pub route_exposure: Vec<RouteExposureView>,
    pub fee_floor_bps: u32,
    pub reliability_bps: u32,
}

impl From<&OperatorProfile> for OperatorView {
    fn from(value: &OperatorProfile) -> Self {
        Self {
            id: value.id.clone(),
            display_name: value.display_name.clone(),
            lane: value.lane.clone(),
            status: value.status.as_str().to_owned(),
            fee_account: value.fee_account.clone(),
            pledged: value.guarantee.pledged.raw(),
            locked: value.guarantee.locked.raw(),
            available: value.available_guarantee().unwrap_or_default().raw(),
            external_commitment: value.exposure.external_commitment.raw(),
            route_exposure: value.exposure.route_views(),
            fee_floor_bps: value.fee_floor_bps.raw(),
            reliability_bps: value.reliability_bps.raw(),
        }
    }
}
