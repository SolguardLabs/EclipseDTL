use crate::amount::{checked_sum, Amount, BasisPoints};
use crate::error::{EclipseError, Result};
use crate::ids::{AssetId, OperatorId, RouteId};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PortfolioBucket {
    pub name: String,
    pub asset: AssetId,
    pub free: Amount,
    pub reserved: Amount,
    pub haircut_bps: BasisPoints,
}

impl PortfolioBucket {
    pub fn new(name: impl Into<String>, asset: AssetId, free: Amount) -> Self {
        Self {
            name: name.into(),
            asset,
            free,
            reserved: Amount::ZERO,
            haircut_bps: BasisPoints::ZERO,
        }
    }

    pub fn effective_free(&self) -> Result<Amount> {
        let haircut = self.haircut_bps.checked_amount_floor(self.free)?;
        self.free.checked_sub(haircut)
    }

    pub fn reserve(&mut self, amount: Amount) -> Result<()> {
        let effective = self.effective_free()?;
        if effective < amount {
            return Err(EclipseError::AmountUnderflow);
        }
        self.free = self.free.checked_sub(amount)?;
        self.reserved = self.reserved.checked_add(amount)?;
        Ok(())
    }

    pub fn release(&mut self, amount: Amount) -> Result<()> {
        if self.reserved < amount {
            return Err(EclipseError::AmountUnderflow);
        }
        self.reserved = self.reserved.checked_sub(amount)?;
        self.free = self.free.checked_add(amount)?;
        Ok(())
    }

    pub fn consume_reserved(&mut self, amount: Amount) -> Result<()> {
        if self.reserved < amount {
            return Err(EclipseError::AmountUnderflow);
        }
        self.reserved = self.reserved.checked_sub(amount)?;
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RoutePortfolio {
    pub route: RouteId,
    pub buckets: Vec<PortfolioBucket>,
    pub soft_limit: Amount,
    pub hard_limit: Amount,
}

impl RoutePortfolio {
    pub fn new(route: RouteId) -> Self {
        Self {
            route,
            buckets: Vec::new(),
            soft_limit: Amount(u128::MAX / 16),
            hard_limit: Amount(u128::MAX / 8),
        }
    }

    pub fn add_bucket(&mut self, bucket: PortfolioBucket) {
        self.buckets.push(bucket);
    }

    pub fn total_free(&self) -> Result<Amount> {
        checked_sum(self.buckets.iter().map(|bucket| bucket.free))
    }

    pub fn total_reserved(&self) -> Result<Amount> {
        checked_sum(self.buckets.iter().map(|bucket| bucket.reserved))
    }

    pub fn total_effective_free(&self) -> Result<Amount> {
        self.buckets
            .iter()
            .map(PortfolioBucket::effective_free)
            .try_fold(Amount::ZERO, |acc, amount| acc.checked_add(amount?))
    }

    pub fn reserve_from_first_fit(&mut self, amount: Amount) -> Result<String> {
        for bucket in &mut self.buckets {
            if bucket.effective_free()? >= amount {
                bucket.reserve(amount)?;
                return Ok(bucket.name.clone());
            }
        }
        Err(EclipseError::AmountUnderflow)
    }

    pub fn utilization_bps(&self) -> Result<BasisPoints> {
        let reserved = self.total_reserved()?;
        if self.hard_limit.is_zero() {
            return Ok(BasisPoints::ZERO);
        }
        let raw = reserved
            .raw()
            .saturating_mul(10_000)
            .checked_div(self.hard_limit.raw())
            .unwrap_or(10_000)
            .min(10_000);
        BasisPoints::new(raw as u32)
    }

    pub fn is_above_soft_limit(&self) -> Result<bool> {
        Ok(self.total_reserved()? > self.soft_limit)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OperatorPortfolio {
    pub operator: OperatorId,
    routes: BTreeMap<RouteId, RoutePortfolio>,
}

impl OperatorPortfolio {
    pub fn new(operator: OperatorId) -> Self {
        Self {
            operator,
            routes: BTreeMap::new(),
        }
    }

    pub fn route_mut(&mut self, route: RouteId) -> &mut RoutePortfolio {
        self.routes
            .entry(route.clone())
            .or_insert_with(|| RoutePortfolio::new(route))
    }

    pub fn route(&self, route: &RouteId) -> Option<&RoutePortfolio> {
        self.routes.get(route)
    }

    pub fn add_bucket(&mut self, route: RouteId, bucket: PortfolioBucket) {
        self.route_mut(route).add_bucket(bucket);
    }

    pub fn reserve(&mut self, route: &RouteId, amount: Amount) -> Result<String> {
        self.routes
            .get_mut(route)
            .ok_or_else(|| EclipseError::RouteNotFound(route.to_string()))?
            .reserve_from_first_fit(amount)
    }

    pub fn aggregate_reserved(&self) -> Result<Amount> {
        self.routes
            .values()
            .map(RoutePortfolio::total_reserved)
            .try_fold(Amount::ZERO, |acc, amount| acc.checked_add(amount?))
    }

    pub fn aggregate_free(&self) -> Result<Amount> {
        self.routes
            .values()
            .map(RoutePortfolio::total_free)
            .try_fold(Amount::ZERO, |acc, amount| acc.checked_add(amount?))
    }

    pub fn route_count(&self) -> usize {
        self.routes.len()
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PortfolioRegistry {
    portfolios: BTreeMap<OperatorId, OperatorPortfolio>,
}

impl PortfolioRegistry {
    pub fn new() -> Self {
        Self {
            portfolios: BTreeMap::new(),
        }
    }

    pub fn portfolio_mut(&mut self, operator: OperatorId) -> &mut OperatorPortfolio {
        self.portfolios
            .entry(operator.clone())
            .or_insert_with(|| OperatorPortfolio::new(operator))
    }

    pub fn portfolio(&self, operator: &OperatorId) -> Option<&OperatorPortfolio> {
        self.portfolios.get(operator)
    }

    pub fn add_bucket(&mut self, operator: OperatorId, route: RouteId, bucket: PortfolioBucket) {
        self.portfolio_mut(operator).add_bucket(route, bucket);
    }

    pub fn reserve(
        &mut self,
        operator: &OperatorId,
        route: &RouteId,
        amount: Amount,
    ) -> Result<String> {
        self.portfolios
            .get_mut(operator)
            .ok_or_else(|| EclipseError::OperatorNotFound(operator.to_string()))?
            .reserve(route, amount)
    }

    pub fn operator_count(&self) -> usize {
        self.portfolios.len()
    }
}
