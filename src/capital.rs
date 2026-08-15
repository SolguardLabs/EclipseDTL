use crate::error::{EclipseError, Result};
use crate::operators::{OperatorBook, OperatorProfile};
use serde::{Deserialize, Serialize};

const BPS_DENOMINATOR: u128 = 10_000;
const COVERAGE_CAP_BPS: u32 = 1_000_000;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CapitalPolicy {
    pub minimum_coverage_bps: u32,
    pub warning_coverage_bps: u32,
    pub maximum_utilization_bps: u32,
    pub exposure_addon_bps: u32,
    pub locked_capital_haircut_bps: u32,
}

impl Default for CapitalPolicy {
    fn default() -> Self {
        Self {
            minimum_coverage_bps: 10_000,
            warning_coverage_bps: 12_000,
            maximum_utilization_bps: 8_500,
            exposure_addon_bps: 500,
            locked_capital_haircut_bps: 1_000,
        }
    }
}

impl CapitalPolicy {
    pub fn validate(&self) -> Result<()> {
        if self.minimum_coverage_bps < 10_000
            || self.warning_coverage_bps < self.minimum_coverage_bps
            || self.warning_coverage_bps > COVERAGE_CAP_BPS
        {
            return Err(EclipseError::InvalidScenario(
                "capital coverage thresholds are inconsistent".to_owned(),
            ));
        }
        if self.maximum_utilization_bps > 10_000
            || self.exposure_addon_bps > 10_000
            || self.locked_capital_haircut_bps > 10_000
        {
            return Err(EclipseError::InvalidScenario(
                "capital policy basis points exceed 10000".to_owned(),
            ));
        }
        Ok(())
    }
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CapitalBand {
    Healthy,
    Watch,
    Constrained,
    Exhausted,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OperatorCapitalSnapshot {
    pub operator: String,
    pub pledged: u128,
    pub locked: u128,
    pub pending_release: u128,
    pub available: u128,
    pub route_exposure: u128,
    pub external_exposure: u128,
    pub recorded_exposure: u128,
    pub stressed_exposure: u128,
    pub effective_guarantee: u128,
    pub surplus: u128,
    pub shortfall: u128,
    pub coverage_bps: u32,
    pub utilization_bps: u32,
    pub largest_bucket_share_bps: u32,
    pub concentration_hhi_bps: u32,
    pub band: CapitalBand,
}

impl OperatorCapitalSnapshot {
    pub fn assess(operator: &OperatorProfile, policy: &CapitalPolicy) -> Result<Self> {
        policy.validate()?;
        let pledged = operator.guarantee.pledged.raw();
        let locked = operator.guarantee.locked.raw();
        let pending_release = operator.guarantee.pending_release.raw();
        let available = operator.available_guarantee()?.raw();
        let route_exposure = operator.exposure.total_route_exposure()?.raw();
        let external_exposure = operator.exposure.external_commitment.raw();
        let recorded_exposure = route_exposure
            .checked_add(external_exposure)
            .ok_or(EclipseError::AmountOverflow)?;
        let addon = mul_bps_ceil(recorded_exposure, policy.exposure_addon_bps)?;
        let stressed_exposure = recorded_exposure
            .checked_add(addon)
            .ok_or(EclipseError::AmountOverflow)?;
        let locked_haircut = mul_bps_ceil(locked, policy.locked_capital_haircut_bps)?;
        let effective_guarantee = pledged.saturating_sub(locked_haircut);
        let coverage_bps = ratio_bps(effective_guarantee, stressed_exposure, COVERAGE_CAP_BPS);
        let utilization_bps = ratio_bps(locked, pledged, 10_000);

        let mut buckets = operator
            .exposure
            .route_views()
            .into_iter()
            .map(|route| route.committed)
            .filter(|amount| *amount > 0)
            .collect::<Vec<_>>();
        if external_exposure > 0 {
            buckets.push(external_exposure);
        }
        let largest_bucket = buckets.iter().copied().max().unwrap_or(0);
        let largest_bucket_share_bps = ratio_bps(largest_bucket, recorded_exposure, 10_000);
        let concentration_hhi_bps = concentration_hhi_bps(&buckets, recorded_exposure);
        let surplus = effective_guarantee.saturating_sub(stressed_exposure);
        let shortfall = stressed_exposure.saturating_sub(effective_guarantee);
        let band = classify(
            effective_guarantee,
            stressed_exposure,
            coverage_bps,
            utilization_bps,
            policy,
        );

        Ok(Self {
            operator: operator.id.to_string(),
            pledged,
            locked,
            pending_release,
            available,
            route_exposure,
            external_exposure,
            recorded_exposure,
            stressed_exposure,
            effective_guarantee,
            surplus,
            shortfall,
            coverage_bps,
            utilization_bps,
            largest_bucket_share_bps,
            concentration_hhi_bps,
            band,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NetworkCapitalReport {
    pub operators: Vec<OperatorCapitalSnapshot>,
    pub total_pledged: u128,
    pub total_locked: u128,
    pub total_recorded_exposure: u128,
    pub total_stressed_exposure: u128,
    pub total_effective_guarantee: u128,
    pub total_surplus: u128,
    pub total_shortfall: u128,
    pub coverage_bps: u32,
    pub largest_operator_share_bps: u32,
    pub operator_concentration_hhi_bps: u32,
    pub healthy_operators: usize,
    pub watch_operators: usize,
    pub constrained_operators: usize,
    pub exhausted_operators: usize,
}

impl NetworkCapitalReport {
    pub fn assess(book: &OperatorBook, policy: &CapitalPolicy) -> Result<Self> {
        let operators = book
            .list()
            .into_iter()
            .map(|operator| OperatorCapitalSnapshot::assess(operator, policy))
            .collect::<Result<Vec<_>>>()?;

        let total_pledged = checked_sum(operators.iter().map(|item| item.pledged))?;
        let total_locked = checked_sum(operators.iter().map(|item| item.locked))?;
        let total_recorded_exposure =
            checked_sum(operators.iter().map(|item| item.recorded_exposure))?;
        let total_stressed_exposure =
            checked_sum(operators.iter().map(|item| item.stressed_exposure))?;
        let total_effective_guarantee =
            checked_sum(operators.iter().map(|item| item.effective_guarantee))?;
        let total_surplus = total_effective_guarantee.saturating_sub(total_stressed_exposure);
        let total_shortfall = total_stressed_exposure.saturating_sub(total_effective_guarantee);
        let coverage_bps = ratio_bps(
            total_effective_guarantee,
            total_stressed_exposure,
            COVERAGE_CAP_BPS,
        );
        let operator_exposures = operators
            .iter()
            .map(|item| item.stressed_exposure)
            .filter(|amount| *amount > 0)
            .collect::<Vec<_>>();
        let largest_operator = operator_exposures.iter().copied().max().unwrap_or(0);
        let largest_operator_share_bps =
            ratio_bps(largest_operator, total_stressed_exposure, 10_000);
        let operator_concentration_hhi_bps =
            concentration_hhi_bps(&operator_exposures, total_stressed_exposure);

        Ok(Self {
            healthy_operators: count_band(&operators, CapitalBand::Healthy),
            watch_operators: count_band(&operators, CapitalBand::Watch),
            constrained_operators: count_band(&operators, CapitalBand::Constrained),
            exhausted_operators: count_band(&operators, CapitalBand::Exhausted),
            operators,
            total_pledged,
            total_locked,
            total_recorded_exposure,
            total_stressed_exposure,
            total_effective_guarantee,
            total_surplus,
            total_shortfall,
            coverage_bps,
            largest_operator_share_bps,
            operator_concentration_hhi_bps,
        })
    }
}

fn classify(
    effective_guarantee: u128,
    stressed_exposure: u128,
    coverage_bps: u32,
    utilization_bps: u32,
    policy: &CapitalPolicy,
) -> CapitalBand {
    if effective_guarantee == 0 && stressed_exposure > 0 {
        CapitalBand::Exhausted
    } else if coverage_bps < policy.minimum_coverage_bps || utilization_bps >= 10_000 {
        CapitalBand::Constrained
    } else if coverage_bps < policy.warning_coverage_bps
        || utilization_bps > policy.maximum_utilization_bps
    {
        CapitalBand::Watch
    } else {
        CapitalBand::Healthy
    }
}

fn checked_sum(mut values: impl Iterator<Item = u128>) -> Result<u128> {
    values.try_fold(0_u128, |total, value| {
        total.checked_add(value).ok_or(EclipseError::AmountOverflow)
    })
}

fn mul_bps_ceil(amount: u128, bps: u32) -> Result<u128> {
    let quotient = amount / BPS_DENOMINATOR;
    let remainder = amount % BPS_DENOMINATOR;
    let whole = quotient
        .checked_mul(u128::from(bps))
        .ok_or(EclipseError::AmountOverflow)?;
    let partial = remainder
        .checked_mul(u128::from(bps))
        .ok_or(EclipseError::AmountOverflow)?;
    let rounded = partial
        .checked_add(BPS_DENOMINATOR - 1)
        .ok_or(EclipseError::AmountOverflow)?
        / BPS_DENOMINATOR;
    whole
        .checked_add(rounded)
        .ok_or(EclipseError::AmountOverflow)
}

fn ratio_bps(numerator: u128, denominator: u128, cap: u32) -> u32 {
    if denominator == 0 {
        return if numerator == 0 { 0 } else { cap };
    }
    let quotient = numerator / denominator;
    let remainder = numerator % denominator;
    let scaled = quotient
        .saturating_mul(BPS_DENOMINATOR)
        .saturating_add(remainder.saturating_mul(BPS_DENOMINATOR) / denominator);
    scaled.min(u128::from(cap)) as u32
}

fn concentration_hhi_bps(values: &[u128], total: u128) -> u32 {
    values
        .iter()
        .map(|value| u128::from(ratio_bps(*value, total, 10_000)))
        .map(|share| share.saturating_mul(share) / BPS_DENOMINATOR)
        .fold(0_u128, u128::saturating_add)
        .min(10_000) as u32
}

fn count_band(operators: &[OperatorCapitalSnapshot], band: CapitalBand) -> usize {
    operators.iter().filter(|item| item.band == band).count()
}
