use crate::accounts::AccountBook;
use crate::amount::{Amount, BasisPoints, Ratio};
use crate::asset::AssetBook;
use crate::error::{EclipseError, Result};
use crate::ids::{AccountId, AssetId, BatchId, OperatorId, RouteId};
use crate::operators::{OperatorBook, OperatorProfile};
use crate::routes::{RouteBook, RoutePlan};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RiskConfig {
    pub min_guarantee_bps: BasisPoints,
    pub max_fee_bps: BasisPoints,
    pub max_route_hops: usize,
    pub min_operator_reliability_bps: BasisPoints,
    pub vault_account: AccountId,
    pub admission_clock_skew: u64,
}

impl Default for RiskConfig {
    fn default() -> Self {
        Self {
            min_guarantee_bps: BasisPoints(500),
            max_fee_bps: BasisPoints(750),
            max_route_hops: 4,
            min_operator_reliability_bps: BasisPoints(7_500),
            vault_account: AccountId::new("vault"),
            admission_clock_skew: 30,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LiquiditySnapshot {
    pub batch: BatchId,
    pub route: RouteId,
    pub operator: OperatorId,
    pub target_asset: AssetId,
    pub vault_available: u128,
    pub operator_available_guarantee: u128,
    pub required_guarantee: u128,
    pub projected_gross_out: u128,
    pub projected_net_out: u128,
    pub timestamp: u64,
}

impl LiquiditySnapshot {
    pub fn guarantee_margin(&self) -> i128 {
        self.operator_available_guarantee as i128 - self.required_guarantee as i128
    }

    pub fn vault_margin(&self) -> i128 {
        self.vault_available as i128 - self.projected_gross_out as i128
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RiskAssessment {
    pub admitted: bool,
    pub reason: String,
    pub snapshot: LiquiditySnapshot,
    pub score_penalty: i128,
}

impl RiskAssessment {
    pub fn admitted(snapshot: LiquiditySnapshot, score_penalty: i128) -> Self {
        Self {
            admitted: true,
            reason: "admitted".to_owned(),
            snapshot,
            score_penalty,
        }
    }

    pub fn rejected(snapshot: LiquiditySnapshot, reason: impl Into<String>) -> Self {
        Self {
            admitted: false,
            reason: reason.into(),
            snapshot,
            score_penalty: i128::MIN / 4,
        }
    }
}

#[derive(Debug, Clone)]
pub struct RiskEngine {
    pub config: RiskConfig,
}

impl RiskEngine {
    pub fn new(config: RiskConfig) -> Self {
        Self { config }
    }

    pub fn default_with_vault(vault_account: AccountId) -> Self {
        Self {
            config: RiskConfig {
                vault_account,
                ..RiskConfig::default()
            },
        }
    }

    pub fn required_guarantee(
        &self,
        gross_out: Amount,
        route: &RoutePlan,
        bid_guarantee_bps: BasisPoints,
    ) -> Result<Amount> {
        let floor = self
            .config
            .min_guarantee_bps
            .raw()
            .max(route.guarantee_bps_floor.raw());
        let selected = floor.max(bid_guarantee_bps.raw());
        BasisPoints::new(selected)?.checked_amount_ceil(gross_out)
    }

    pub fn assess_bid(
        &self,
        batch: &BatchId,
        route_id: &RouteId,
        operator_id: &OperatorId,
        amount_in: Amount,
        price: Ratio,
        fee_bps: BasisPoints,
        guarantee_bps: BasisPoints,
        now: u64,
        assets: &AssetBook,
        accounts: &AccountBook,
        operators: &OperatorBook,
        routes: &RouteBook,
    ) -> Result<RiskAssessment> {
        let route = routes.get(route_id)?;
        let operator = operators.get(operator_id)?;
        route.ensure_assets(assets)?;
        route.ensure_enabled()?;
        self.ensure_operator(operator)?;
        route.quote(amount_in)?;
        let gross_out = price.apply_floor(amount_in)?;
        let fee = fee_bps.checked_amount_floor(gross_out)?;
        let net_out = gross_out.checked_sub(fee)?;
        let required = self.required_guarantee(gross_out, route, guarantee_bps)?;
        let available_guarantee = operator.available_guarantee()?;
        let vault_available = accounts.balance(&self.config.vault_account, &route.target)?;
        let snapshot = LiquiditySnapshot {
            batch: batch.clone(),
            route: route_id.clone(),
            operator: operator_id.clone(),
            target_asset: route.target.clone(),
            vault_available: vault_available.raw(),
            operator_available_guarantee: available_guarantee.raw(),
            required_guarantee: required.raw(),
            projected_gross_out: gross_out.raw(),
            projected_net_out: net_out.raw(),
            timestamp: now,
        };
        if route.hops() > self.config.max_route_hops {
            return Ok(RiskAssessment::rejected(snapshot, "route hop limit"));
        }
        if fee_bps.raw() > self.config.max_fee_bps.raw() {
            return Ok(RiskAssessment::rejected(snapshot, "fee limit"));
        }
        if operator.reliability_bps.raw() < self.config.min_operator_reliability_bps.raw() {
            return Ok(RiskAssessment::rejected(snapshot, "operator reliability"));
        }
        if gross_out < route.liquidity_floor {
            return Ok(RiskAssessment::rejected(snapshot, "route liquidity floor"));
        }
        if vault_available < gross_out {
            return Ok(RiskAssessment::rejected(snapshot, "vault liquidity"));
        }
        if available_guarantee < required {
            return Ok(RiskAssessment::rejected(snapshot, "operator guarantee"));
        }
        let penalty = self.score_penalty(route, operator, fee_bps, guarantee_bps);
        Ok(RiskAssessment::admitted(snapshot, penalty))
    }

    fn ensure_operator(&self, operator: &OperatorProfile) -> Result<()> {
        if operator.can_bid() {
            Ok(())
        } else {
            Err(EclipseError::OperatorUnavailable(operator.id.to_string()))
        }
    }

    fn score_penalty(
        &self,
        route: &RoutePlan,
        operator: &OperatorProfile,
        fee_bps: BasisPoints,
        guarantee_bps: BasisPoints,
    ) -> i128 {
        let fee_penalty = fee_bps.raw() as i128 * 8;
        let hop_penalty = route.hops() as i128 * 10;
        let reliability_bonus = operator.reliability_bps.raw() as i128 / 25;
        let guarantee_bonus = guarantee_bps.raw() as i128 / 20;
        fee_penalty + hop_penalty - reliability_bonus - guarantee_bonus
    }

    pub fn preflight_settlement(
        &self,
        route: &RoutePlan,
        accounts: &AccountBook,
        gross_out: Amount,
        min_out: Amount,
        net_out: Amount,
    ) -> Result<()> {
        if net_out < min_out {
            return Err(EclipseError::SettlementFloor {
                net: net_out.raw(),
                minimum: min_out.raw(),
            });
        }
        let vault_available = accounts.balance(&self.config.vault_account, &route.target)?;
        if vault_available < gross_out {
            return Err(EclipseError::InsufficientBalance {
                account: self.config.vault_account.to_string(),
                asset: route.target.to_string(),
                available: vault_available.raw(),
                needed: gross_out.raw(),
            });
        }
        Ok(())
    }
}
