use crate::error::{EclipseError, Result};
use serde::{Deserialize, Serialize};
use std::cmp::Ordering;
use std::fmt::{Display, Formatter};

#[derive(
    Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, Default,
)]
#[serde(transparent)]
pub struct Amount(pub u128);

impl Amount {
    pub const ZERO: Amount = Amount(0);
    pub const ONE: Amount = Amount(1);

    pub fn new(value: u128) -> Self {
        Self(value)
    }

    pub fn raw(self) -> u128 {
        self.0
    }

    pub fn is_zero(self) -> bool {
        self.0 == 0
    }

    pub fn checked_add(self, rhs: Amount) -> Result<Amount> {
        self.0
            .checked_add(rhs.0)
            .map(Amount)
            .ok_or(EclipseError::AmountOverflow)
    }

    pub fn checked_sub(self, rhs: Amount) -> Result<Amount> {
        self.0
            .checked_sub(rhs.0)
            .map(Amount)
            .ok_or(EclipseError::AmountUnderflow)
    }

    pub fn checked_mul(self, rhs: u128) -> Result<Amount> {
        self.0
            .checked_mul(rhs)
            .map(Amount)
            .ok_or(EclipseError::AmountOverflow)
    }

    pub fn checked_div(self, rhs: u128) -> Result<Amount> {
        if rhs == 0 {
            return Err(EclipseError::DivisionByZero);
        }
        Ok(Amount(self.0 / rhs))
    }

    pub fn saturating_add(self, rhs: Amount) -> Amount {
        Amount(self.0.saturating_add(rhs.0))
    }

    pub fn saturating_sub(self, rhs: Amount) -> Amount {
        Amount(self.0.saturating_sub(rhs.0))
    }

    pub fn min(self, rhs: Amount) -> Amount {
        Amount(self.0.min(rhs.0))
    }

    pub fn max(self, rhs: Amount) -> Amount {
        Amount(self.0.max(rhs.0))
    }

    pub fn clamp(self, min: Amount, max: Amount) -> Amount {
        Amount(self.0.clamp(min.0, max.0))
    }

    pub fn cmp_raw(&self, raw: u128) -> Ordering {
        self.0.cmp(&raw)
    }
}

impl Display for Amount {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl From<u128> for Amount {
    fn from(value: u128) -> Self {
        Amount(value)
    }
}

impl From<u64> for Amount {
    fn from(value: u64) -> Self {
        Amount(value as u128)
    }
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BasisPoints(pub u32);

impl BasisPoints {
    pub const ZERO: BasisPoints = BasisPoints(0);
    pub const ONE_HUNDRED_PERCENT: BasisPoints = BasisPoints(10_000);

    pub fn new(value: u32) -> Result<Self> {
        if value > 10_000 {
            return Err(EclipseError::InvalidBasisPoints(value));
        }
        Ok(BasisPoints(value))
    }

    pub fn raw(self) -> u32 {
        self.0
    }

    pub fn checked_amount_floor(self, amount: Amount) -> Result<Amount> {
        mul_div_floor(amount, self.0 as u128, 10_000)
    }

    pub fn checked_amount_ceil(self, amount: Amount) -> Result<Amount> {
        mul_div_ceil(amount, self.0 as u128, 10_000)
    }

    pub fn complement(self) -> BasisPoints {
        BasisPoints(10_000_u32.saturating_sub(self.0))
    }

    pub fn is_zero(self) -> bool {
        self.0 == 0
    }
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Ratio {
    pub numerator: u128,
    pub denominator: u128,
}

impl Ratio {
    pub fn new(numerator: u128, denominator: u128) -> Result<Self> {
        if denominator == 0 {
            return Err(EclipseError::DivisionByZero);
        }
        Ok(Self {
            numerator,
            denominator,
        })
    }

    pub fn one() -> Self {
        Self {
            numerator: 1,
            denominator: 1,
        }
    }

    pub fn apply_floor(self, amount: Amount) -> Result<Amount> {
        mul_div_floor(amount, self.numerator, self.denominator)
    }

    pub fn apply_ceil(self, amount: Amount) -> Result<Amount> {
        mul_div_ceil(amount, self.numerator, self.denominator)
    }

    pub fn invert(self) -> Result<Self> {
        if self.numerator == 0 {
            return Err(EclipseError::DivisionByZero);
        }
        Ok(Self {
            numerator: self.denominator,
            denominator: self.numerator,
        })
    }

    pub fn multiply(self, rhs: Ratio) -> Result<Ratio> {
        Ok(Ratio {
            numerator: self
                .numerator
                .checked_mul(rhs.numerator)
                .ok_or(EclipseError::AmountOverflow)?,
            denominator: self
                .denominator
                .checked_mul(rhs.denominator)
                .ok_or(EclipseError::AmountOverflow)?,
        })
    }

    pub fn as_bps_floor(self) -> Result<BasisPoints> {
        let raw = self
            .numerator
            .checked_mul(10_000)
            .ok_or(EclipseError::AmountOverflow)?
            / self.denominator;
        BasisPoints::new(raw.min(10_000) as u32)
    }
}

pub fn mul_div_floor(amount: Amount, numerator: u128, denominator: u128) -> Result<Amount> {
    if denominator == 0 {
        return Err(EclipseError::DivisionByZero);
    }
    amount
        .0
        .checked_mul(numerator)
        .map(|value| Amount(value / denominator))
        .ok_or(EclipseError::AmountOverflow)
}

pub fn mul_div_ceil(amount: Amount, numerator: u128, denominator: u128) -> Result<Amount> {
    if denominator == 0 {
        return Err(EclipseError::DivisionByZero);
    }
    let product = amount
        .0
        .checked_mul(numerator)
        .ok_or(EclipseError::AmountOverflow)?;
    let adjusted = product
        .checked_add(denominator.saturating_sub(1))
        .ok_or(EclipseError::AmountOverflow)?;
    Ok(Amount(adjusted / denominator))
}

pub fn checked_sum<I>(values: I) -> Result<Amount>
where
    I: IntoIterator<Item = Amount>,
{
    values
        .into_iter()
        .try_fold(Amount::ZERO, |acc, value| acc.checked_add(value))
}

pub fn checked_difference<I>(start: Amount, values: I) -> Result<Amount>
where
    I: IntoIterator<Item = Amount>,
{
    values
        .into_iter()
        .try_fold(start, |acc, value| acc.checked_sub(value))
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AmountWindow {
    pub lower: Amount,
    pub upper: Amount,
}

impl AmountWindow {
    pub fn new(lower: Amount, upper: Amount) -> Result<Self> {
        if lower > upper {
            return Err(EclipseError::AmountUnderflow);
        }
        Ok(Self { lower, upper })
    }

    pub fn contains(self, value: Amount) -> bool {
        value >= self.lower && value <= self.upper
    }

    pub fn width(self) -> Result<Amount> {
        self.upper.checked_sub(self.lower)
    }

    pub fn expand(self, amount: Amount) -> Result<Self> {
        Ok(Self {
            lower: self.lower.saturating_sub(amount),
            upper: self.upper.checked_add(amount)?,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AmountBreakdown {
    pub gross: Amount,
    pub fee: Amount,
    pub net: Amount,
}

impl AmountBreakdown {
    pub fn new(gross: Amount, fee_bps: BasisPoints) -> Result<Self> {
        let fee = fee_bps.checked_amount_floor(gross)?;
        let net = gross.checked_sub(fee)?;
        Ok(Self { gross, fee, net })
    }

    pub fn with_fee(gross: Amount, fee: Amount) -> Result<Self> {
        let net = gross.checked_sub(fee)?;
        Ok(Self { gross, fee, net })
    }

    pub fn is_empty(&self) -> bool {
        self.gross.is_zero() && self.fee.is_zero() && self.net.is_zero()
    }

    pub fn fee_ratio(&self) -> Result<Ratio> {
        if self.gross.is_zero() {
            return Ok(Ratio::one());
        }
        Ratio::new(self.fee.raw(), self.gross.raw())
    }
}
