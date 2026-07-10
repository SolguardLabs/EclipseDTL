use crate::amount::{Amount, BasisPoints};
use crate::error::{EclipseError, Result};
use crate::ids::{AssetId, OperatorId, RouteId};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum PolicyDecision {
    Accept,
    Warn(String),
    Reject(String),
}

impl PolicyDecision {
    pub fn is_accept(&self) -> bool {
        matches!(self, PolicyDecision::Accept | PolicyDecision::Warn(_))
    }

    pub fn reason(&self) -> Option<&str> {
        match self {
            PolicyDecision::Accept => None,
            PolicyDecision::Warn(reason) | PolicyDecision::Reject(reason) => Some(reason.as_str()),
        }
    }

    pub fn merge(self, other: PolicyDecision) -> PolicyDecision {
        match (self, other) {
            (PolicyDecision::Reject(left), PolicyDecision::Reject(right)) => {
                PolicyDecision::Reject(format!("{left}; {right}"))
            }
            (PolicyDecision::Reject(reason), _) | (_, PolicyDecision::Reject(reason)) => {
                PolicyDecision::Reject(reason)
            }
            (PolicyDecision::Warn(left), PolicyDecision::Warn(right)) => {
                PolicyDecision::Warn(format!("{left}; {right}"))
            }
            (PolicyDecision::Warn(reason), _) | (_, PolicyDecision::Warn(reason)) => {
                PolicyDecision::Warn(reason)
            }
            (PolicyDecision::Accept, PolicyDecision::Accept) => PolicyDecision::Accept,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RoutePolicy {
    pub route: RouteId,
    pub max_notional: Amount,
    pub min_guarantee_bps: BasisPoints,
    pub max_fee_bps: BasisPoints,
    pub soft_fee_bps: BasisPoints,
    pub enabled: bool,
}

impl RoutePolicy {
    pub fn new(route: RouteId) -> Self {
        Self {
            route,
            max_notional: Amount(u128::MAX / 8),
            min_guarantee_bps: BasisPoints(500),
            max_fee_bps: BasisPoints(1_000),
            soft_fee_bps: BasisPoints(300),
            enabled: true,
        }
    }

    pub fn evaluate(
        &self,
        notional: Amount,
        guarantee_bps: BasisPoints,
        fee_bps: BasisPoints,
    ) -> PolicyDecision {
        if !self.enabled {
            return PolicyDecision::Reject("route policy disabled".to_owned());
        }
        if notional > self.max_notional {
            return PolicyDecision::Reject("route notional limit".to_owned());
        }
        if guarantee_bps.raw() < self.min_guarantee_bps.raw() {
            return PolicyDecision::Reject("route guarantee floor".to_owned());
        }
        if fee_bps.raw() > self.max_fee_bps.raw() {
            return PolicyDecision::Reject("route fee ceiling".to_owned());
        }
        if fee_bps.raw() > self.soft_fee_bps.raw() {
            return PolicyDecision::Warn("route fee above soft target".to_owned());
        }
        PolicyDecision::Accept
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AssetPolicy {
    pub asset: AssetId,
    pub max_vault_draw: Amount,
    pub min_vault_reserve: Amount,
    pub settlement_fee_bps: BasisPoints,
    pub enabled: bool,
}

impl AssetPolicy {
    pub fn new(asset: AssetId) -> Self {
        Self {
            asset,
            max_vault_draw: Amount(u128::MAX / 8),
            min_vault_reserve: Amount::ZERO,
            settlement_fee_bps: BasisPoints::ZERO,
            enabled: true,
        }
    }

    pub fn evaluate_draw(&self, vault_available: Amount, draw: Amount) -> PolicyDecision {
        if !self.enabled {
            return PolicyDecision::Reject("asset policy disabled".to_owned());
        }
        if draw > self.max_vault_draw {
            return PolicyDecision::Reject("asset draw ceiling".to_owned());
        }
        if vault_available < draw {
            return PolicyDecision::Reject("asset vault liquidity".to_owned());
        }
        let remaining = vault_available.saturating_sub(draw);
        if remaining < self.min_vault_reserve {
            return PolicyDecision::Warn("asset reserve below target".to_owned());
        }
        PolicyDecision::Accept
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OperatorPolicy {
    pub operator: OperatorId,
    pub max_active_routes: usize,
    pub max_external_commitment: Amount,
    pub min_reliability_bps: BasisPoints,
    pub preferred: bool,
}

impl OperatorPolicy {
    pub fn new(operator: OperatorId) -> Self {
        Self {
            operator,
            max_active_routes: 16,
            max_external_commitment: Amount(u128::MAX / 8),
            min_reliability_bps: BasisPoints(7_500),
            preferred: false,
        }
    }

    pub fn evaluate(
        &self,
        active_routes: usize,
        external_commitment: Amount,
        reliability_bps: BasisPoints,
    ) -> PolicyDecision {
        if active_routes > self.max_active_routes {
            return PolicyDecision::Reject("operator route concentration".to_owned());
        }
        if external_commitment > self.max_external_commitment {
            return PolicyDecision::Reject("operator external commitment".to_owned());
        }
        if reliability_bps.raw() < self.min_reliability_bps.raw() {
            return PolicyDecision::Reject("operator reliability floor".to_owned());
        }
        if self.preferred {
            return PolicyDecision::Warn("preferred operator path".to_owned());
        }
        PolicyDecision::Accept
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PolicyBook {
    route_policies: BTreeMap<RouteId, RoutePolicy>,
    asset_policies: BTreeMap<AssetId, AssetPolicy>,
    operator_policies: BTreeMap<OperatorId, OperatorPolicy>,
}

impl PolicyBook {
    pub fn new() -> Self {
        Self {
            route_policies: BTreeMap::new(),
            asset_policies: BTreeMap::new(),
            operator_policies: BTreeMap::new(),
        }
    }

    pub fn set_route_policy(&mut self, policy: RoutePolicy) {
        self.route_policies.insert(policy.route.clone(), policy);
    }

    pub fn set_asset_policy(&mut self, policy: AssetPolicy) {
        self.asset_policies.insert(policy.asset.clone(), policy);
    }

    pub fn set_operator_policy(&mut self, policy: OperatorPolicy) {
        self.operator_policies
            .insert(policy.operator.clone(), policy);
    }

    pub fn route_policy(&self, route: &RouteId) -> RoutePolicy {
        self.route_policies
            .get(route)
            .cloned()
            .unwrap_or_else(|| RoutePolicy::new(route.clone()))
    }

    pub fn asset_policy(&self, asset: &AssetId) -> AssetPolicy {
        self.asset_policies
            .get(asset)
            .cloned()
            .unwrap_or_else(|| AssetPolicy::new(asset.clone()))
    }

    pub fn operator_policy(&self, operator: &OperatorId) -> OperatorPolicy {
        self.operator_policies
            .get(operator)
            .cloned()
            .unwrap_or_else(|| OperatorPolicy::new(operator.clone()))
    }

    pub fn disable_route(&mut self, route: &RouteId) {
        let mut policy = self.route_policy(route);
        policy.enabled = false;
        self.set_route_policy(policy);
    }

    pub fn disable_asset(&mut self, asset: &AssetId) {
        let mut policy = self.asset_policy(asset);
        policy.enabled = false;
        self.set_asset_policy(policy);
    }

    pub fn route_count(&self) -> usize {
        self.route_policies.len()
    }

    pub fn asset_count(&self) -> usize {
        self.asset_policies.len()
    }

    pub fn operator_count(&self) -> usize {
        self.operator_policies.len()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PolicyTrace {
    pub decisions: Vec<PolicyDecision>,
}

impl PolicyTrace {
    pub fn new() -> Self {
        Self {
            decisions: Vec::new(),
        }
    }

    pub fn push(&mut self, decision: PolicyDecision) {
        self.decisions.push(decision);
    }

    pub fn final_decision(&self) -> PolicyDecision {
        self.decisions
            .iter()
            .cloned()
            .fold(PolicyDecision::Accept, |acc, decision| acc.merge(decision))
    }

    pub fn ensure_accept(&self) -> Result<()> {
        match self.final_decision() {
            PolicyDecision::Accept | PolicyDecision::Warn(_) => Ok(()),
            PolicyDecision::Reject(reason) => Err(EclipseError::BidRejected(reason)),
        }
    }
}

impl Default for PolicyTrace {
    fn default() -> Self {
        Self::new()
    }
}
