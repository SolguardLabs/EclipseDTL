use crate::amount::{Amount, BasisPoints};
use crate::error::{EclipseError, Result};
use crate::ids::AssetId;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum AssetClass {
    Stable,
    Native,
    LiquidStaking,
    Treasury,
    Synthetic,
    InternalCredit,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AssetMetadata {
    pub issuer: String,
    pub settlement_domain: String,
    pub external_symbol: String,
    pub notes: String,
}

impl Default for AssetMetadata {
    fn default() -> Self {
        Self {
            issuer: "Eclipse Clearing".to_owned(),
            settlement_domain: "internal".to_owned(),
            external_symbol: String::new(),
            notes: String::new(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Asset {
    pub id: AssetId,
    pub symbol: String,
    pub decimals: u8,
    pub class: AssetClass,
    pub enabled: bool,
    pub transfer_fee_bps: BasisPoints,
    pub risk_weight_bps: BasisPoints,
    pub min_settlement_unit: Amount,
    pub metadata: AssetMetadata,
}

impl Asset {
    pub fn new(id: AssetId, symbol: impl Into<String>, decimals: u8, class: AssetClass) -> Self {
        Self {
            id,
            symbol: symbol.into(),
            decimals,
            class,
            enabled: true,
            transfer_fee_bps: BasisPoints::ZERO,
            risk_weight_bps: BasisPoints::ONE_HUNDRED_PERCENT,
            min_settlement_unit: Amount::ONE,
            metadata: AssetMetadata::default(),
        }
    }

    pub fn with_transfer_fee(mut self, fee_bps: BasisPoints) -> Self {
        self.transfer_fee_bps = fee_bps;
        self
    }

    pub fn with_risk_weight(mut self, risk_weight_bps: BasisPoints) -> Self {
        self.risk_weight_bps = risk_weight_bps;
        self
    }

    pub fn with_min_settlement_unit(mut self, unit: Amount) -> Self {
        self.min_settlement_unit = unit;
        self
    }

    pub fn with_metadata(mut self, metadata: AssetMetadata) -> Self {
        self.metadata = metadata;
        self
    }

    pub fn disabled(mut self) -> Self {
        self.enabled = false;
        self
    }

    pub fn scale(&self) -> Result<u128> {
        10_u128
            .checked_pow(self.decimals as u32)
            .ok_or(EclipseError::AmountOverflow)
    }

    pub fn ensure_enabled(&self) -> Result<()> {
        if self.enabled {
            Ok(())
        } else {
            Err(EclipseError::AssetDisabled(self.id.to_string()))
        }
    }

    pub fn transfer_fee(&self, amount: Amount) -> Result<Amount> {
        self.transfer_fee_bps.checked_amount_floor(amount)
    }

    pub fn risk_weighted(&self, amount: Amount) -> Result<Amount> {
        self.risk_weight_bps.checked_amount_floor(amount)
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AssetBook {
    assets: BTreeMap<AssetId, Asset>,
}

impl AssetBook {
    pub fn new() -> Self {
        Self {
            assets: BTreeMap::new(),
        }
    }

    pub fn insert(&mut self, asset: Asset) -> Result<()> {
        if self.assets.contains_key(&asset.id) {
            return Err(EclipseError::DuplicateId(asset.id.to_string()));
        }
        self.assets.insert(asset.id.clone(), asset);
        Ok(())
    }

    pub fn upsert(&mut self, asset: Asset) {
        self.assets.insert(asset.id.clone(), asset);
    }

    pub fn get(&self, id: &AssetId) -> Result<&Asset> {
        self.assets
            .get(id)
            .ok_or_else(|| EclipseError::AssetNotFound(id.to_string()))
    }

    pub fn get_mut(&mut self, id: &AssetId) -> Result<&mut Asset> {
        self.assets
            .get_mut(id)
            .ok_or_else(|| EclipseError::AssetNotFound(id.to_string()))
    }

    pub fn ensure_enabled(&self, id: &AssetId) -> Result<&Asset> {
        let asset = self.get(id)?;
        asset.ensure_enabled()?;
        Ok(asset)
    }

    pub fn disable(&mut self, id: &AssetId) -> Result<()> {
        self.get_mut(id)?.enabled = false;
        Ok(())
    }

    pub fn enable(&mut self, id: &AssetId) -> Result<()> {
        self.get_mut(id)?.enabled = true;
        Ok(())
    }

    pub fn contains(&self, id: &AssetId) -> bool {
        self.assets.contains_key(id)
    }

    pub fn list(&self) -> Vec<&Asset> {
        self.assets.values().collect()
    }

    pub fn ids(&self) -> Vec<AssetId> {
        self.assets.keys().cloned().collect()
    }

    pub fn len(&self) -> usize {
        self.assets.len()
    }

    pub fn is_empty(&self) -> bool {
        self.assets.is_empty()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AssetView {
    pub id: AssetId,
    pub symbol: String,
    pub decimals: u8,
    pub class: AssetClass,
    pub enabled: bool,
    pub transfer_fee_bps: u32,
    pub risk_weight_bps: u32,
}

impl From<&Asset> for AssetView {
    fn from(value: &Asset) -> Self {
        Self {
            id: value.id.clone(),
            symbol: value.symbol.clone(),
            decimals: value.decimals,
            class: value.class.clone(),
            enabled: value.enabled,
            transfer_fee_bps: value.transfer_fee_bps.raw(),
            risk_weight_bps: value.risk_weight_bps.raw(),
        }
    }
}
