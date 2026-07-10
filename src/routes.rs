use crate::amount::{Amount, BasisPoints, Ratio};
use crate::asset::AssetBook;
use crate::error::{EclipseError, Result};
use crate::ids::{AssetId, RouteId};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum RouteClass {
    DirectInternal,
    InternalNetting,
    MarketMaker,
    FallbackPool,
    TreasuryBridge,
}

impl RouteClass {
    pub fn as_str(&self) -> &'static str {
        match self {
            RouteClass::DirectInternal => "direct_internal",
            RouteClass::InternalNetting => "internal_netting",
            RouteClass::MarketMaker => "market_maker",
            RouteClass::FallbackPool => "fallback_pool",
            RouteClass::TreasuryBridge => "treasury_bridge",
        }
    }

    pub fn base_priority(&self) -> i128 {
        match self {
            RouteClass::DirectInternal => 40,
            RouteClass::InternalNetting => 35,
            RouteClass::MarketMaker => 25,
            RouteClass::FallbackPool => 10,
            RouteClass::TreasuryBridge => 5,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RouteLeg {
    pub source: AssetId,
    pub target: AssetId,
    pub price: Ratio,
    pub max_slippage_bps: BasisPoints,
    pub venue: String,
}

impl RouteLeg {
    pub fn new(
        source: AssetId,
        target: AssetId,
        price: Ratio,
        max_slippage_bps: BasisPoints,
        venue: impl Into<String>,
    ) -> Self {
        Self {
            source,
            target,
            price,
            max_slippage_bps,
            venue: venue.into(),
        }
    }

    pub fn quote(&self, amount_in: Amount) -> Result<Amount> {
        self.price.apply_floor(amount_in)
    }

    pub fn slippage_floor(&self, amount: Amount) -> Result<Amount> {
        let haircut = self.max_slippage_bps.checked_amount_floor(amount)?;
        amount.checked_sub(haircut)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RoutePlan {
    pub id: RouteId,
    pub label: String,
    pub class: RouteClass,
    pub source: AssetId,
    pub target: AssetId,
    pub legs: Vec<RouteLeg>,
    pub enabled: bool,
    pub min_input: Amount,
    pub max_input: Amount,
    pub guarantee_bps_floor: BasisPoints,
    pub operator_reliability_floor_bps: BasisPoints,
    pub liquidity_floor: Amount,
    pub fallback_rank: u32,
}

impl RoutePlan {
    pub fn new(
        id: RouteId,
        label: impl Into<String>,
        class: RouteClass,
        source: AssetId,
        target: AssetId,
    ) -> Self {
        Self {
            id,
            label: label.into(),
            class,
            source,
            target,
            legs: Vec::new(),
            enabled: true,
            min_input: Amount::ONE,
            max_input: Amount(u128::MAX / 4),
            guarantee_bps_floor: BasisPoints::ZERO,
            operator_reliability_floor_bps: BasisPoints::ZERO,
            liquidity_floor: Amount::ZERO,
            fallback_rank: 100,
        }
    }

    pub fn with_leg(mut self, leg: RouteLeg) -> Self {
        self.legs.push(leg);
        self
    }

    pub fn with_limits(mut self, min_input: Amount, max_input: Amount) -> Self {
        self.min_input = min_input;
        self.max_input = max_input;
        self
    }

    pub fn with_guarantee_floor(mut self, guarantee_bps_floor: BasisPoints) -> Self {
        self.guarantee_bps_floor = guarantee_bps_floor;
        self
    }

    pub fn with_liquidity_floor(mut self, liquidity_floor: Amount) -> Self {
        self.liquidity_floor = liquidity_floor;
        self
    }

    pub fn with_fallback_rank(mut self, fallback_rank: u32) -> Self {
        self.fallback_rank = fallback_rank;
        self
    }

    pub fn ensure_enabled(&self) -> Result<()> {
        if self.enabled {
            Ok(())
        } else {
            Err(EclipseError::RouteDisabled(self.id.to_string()))
        }
    }

    pub fn ensure_assets(&self, assets: &AssetBook) -> Result<()> {
        assets.ensure_enabled(&self.source)?;
        assets.ensure_enabled(&self.target)?;
        for leg in &self.legs {
            assets.ensure_enabled(&leg.source)?;
            assets.ensure_enabled(&leg.target)?;
        }
        Ok(())
    }

    pub fn quote(&self, amount_in: Amount) -> Result<Amount> {
        self.ensure_enabled()?;
        if amount_in < self.min_input || amount_in > self.max_input {
            return Err(EclipseError::BidRejected(format!(
                "amount {} outside route limits",
                amount_in
            )));
        }
        if self.legs.is_empty() {
            return Ok(amount_in);
        }
        self.legs
            .iter()
            .try_fold(amount_in, |amount, leg| leg.quote(amount))
    }

    pub fn conservative_quote(&self, amount_in: Amount) -> Result<Amount> {
        if self.legs.is_empty() {
            return Ok(amount_in);
        }
        self.legs.iter().try_fold(amount_in, |amount, leg| {
            let quoted = leg.quote(amount)?;
            leg.slippage_floor(quoted)
        })
    }

    pub fn first_venue(&self) -> Option<&str> {
        self.legs.first().map(|leg| leg.venue.as_str())
    }

    pub fn last_venue(&self) -> Option<&str> {
        self.legs.last().map(|leg| leg.venue.as_str())
    }

    pub fn hops(&self) -> usize {
        self.legs.len()
    }

    pub fn matches_pair(&self, source: &AssetId, target: &AssetId) -> bool {
        &self.source == source && &self.target == target
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct RouteBook {
    routes: BTreeMap<RouteId, RoutePlan>,
}

impl RouteBook {
    pub fn new() -> Self {
        Self {
            routes: BTreeMap::new(),
        }
    }

    pub fn insert(&mut self, route: RoutePlan) -> Result<()> {
        if self.routes.contains_key(&route.id) {
            return Err(EclipseError::DuplicateId(route.id.to_string()));
        }
        self.routes.insert(route.id.clone(), route);
        Ok(())
    }

    pub fn get(&self, id: &RouteId) -> Result<&RoutePlan> {
        self.routes
            .get(id)
            .ok_or_else(|| EclipseError::RouteNotFound(id.to_string()))
    }

    pub fn get_mut(&mut self, id: &RouteId) -> Result<&mut RoutePlan> {
        self.routes
            .get_mut(id)
            .ok_or_else(|| EclipseError::RouteNotFound(id.to_string()))
    }

    pub fn enable(&mut self, id: &RouteId) -> Result<()> {
        self.get_mut(id)?.enabled = true;
        Ok(())
    }

    pub fn disable(&mut self, id: &RouteId) -> Result<()> {
        self.get_mut(id)?.enabled = false;
        Ok(())
    }

    pub fn candidates(&self, source: &AssetId, target: &AssetId) -> Vec<&RoutePlan> {
        self.routes
            .values()
            .filter(|route| route.enabled && route.matches_pair(source, target))
            .collect()
    }

    pub fn fallback_candidates(&self, source: &AssetId, target: &AssetId) -> Vec<&RoutePlan> {
        let mut candidates = self.candidates(source, target);
        candidates.sort_by_key(|route| route.fallback_rank);
        candidates
    }

    pub fn list(&self) -> Vec<&RoutePlan> {
        self.routes.values().collect()
    }

    pub fn views(&self) -> Vec<RouteView> {
        self.routes.values().map(RouteView::from).collect()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RouteQuote {
    pub route: RouteId,
    pub amount_in: u128,
    pub gross_out: u128,
    pub conservative_out: u128,
    pub hops: usize,
}

impl RouteQuote {
    pub fn from_route(route: &RoutePlan, amount_in: Amount) -> Result<Self> {
        Ok(Self {
            route: route.id.clone(),
            amount_in: amount_in.raw(),
            gross_out: route.quote(amount_in)?.raw(),
            conservative_out: route.conservative_quote(amount_in)?.raw(),
            hops: route.hops(),
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RouteView {
    pub id: RouteId,
    pub label: String,
    pub class: String,
    pub source: AssetId,
    pub target: AssetId,
    pub enabled: bool,
    pub min_input: u128,
    pub max_input: u128,
    pub guarantee_bps_floor: u32,
    pub liquidity_floor: u128,
    pub fallback_rank: u32,
    pub hops: usize,
}

impl From<&RoutePlan> for RouteView {
    fn from(value: &RoutePlan) -> Self {
        Self {
            id: value.id.clone(),
            label: value.label.clone(),
            class: value.class.as_str().to_owned(),
            source: value.source.clone(),
            target: value.target.clone(),
            enabled: value.enabled,
            min_input: value.min_input.raw(),
            max_input: value.max_input.raw(),
            guarantee_bps_floor: value.guarantee_bps_floor.raw(),
            liquidity_floor: value.liquidity_floor.raw(),
            fallback_rank: value.fallback_rank,
            hops: value.hops(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RouteEnvelope {
    pub route: RouteId,
    pub amount_in: Amount,
    pub expected_out: Amount,
    pub settlement_window_start: u64,
    pub settlement_window_end: u64,
}

impl RouteEnvelope {
    pub fn new(
        route: RouteId,
        amount_in: Amount,
        expected_out: Amount,
        settlement_window_start: u64,
        settlement_window_end: u64,
    ) -> Self {
        Self {
            route,
            amount_in,
            expected_out,
            settlement_window_start,
            settlement_window_end,
        }
    }

    pub fn is_live_at(&self, timestamp: u64) -> bool {
        timestamp >= self.settlement_window_start && timestamp <= self.settlement_window_end
    }
}
