use thiserror::Error;

pub type Result<T> = std::result::Result<T, EclipseError>;

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum EclipseError {
    #[error("amount overflow")]
    AmountOverflow,
    #[error("amount underflow")]
    AmountUnderflow,
    #[error("division by zero")]
    DivisionByZero,
    #[error("invalid basis points: {0}")]
    InvalidBasisPoints(u32),
    #[error("asset not found: {0}")]
    AssetNotFound(String),
    #[error("asset disabled: {0}")]
    AssetDisabled(String),
    #[error("account not found: {0}")]
    AccountNotFound(String),
    #[error("operator not found: {0}")]
    OperatorNotFound(String),
    #[error("operator unavailable: {0}")]
    OperatorUnavailable(String),
    #[error("route not found: {0}")]
    RouteNotFound(String),
    #[error("route disabled: {0}")]
    RouteDisabled(String),
    #[error("batch not found: {0}")]
    BatchNotFound(String),
    #[error("bid not found: {0}")]
    BidNotFound(String),
    #[error("duplicate id: {0}")]
    DuplicateId(String),
    #[error(
        "insufficient balance for {account} asset {asset}: available {available}, needed {needed}"
    )]
    InsufficientBalance {
        account: String,
        asset: String,
        available: u128,
        needed: u128,
    },
    #[error("insufficient guarantee for {operator}: available {available}, needed {needed}")]
    InsufficientGuarantee {
        operator: String,
        available: u128,
        needed: u128,
    },
    #[error("bid rejected: {0}")]
    BidRejected(String),
    #[error("batch closed: {0}")]
    BatchClosed(String),
    #[error("batch not ready: {0}")]
    BatchNotReady(String),
    #[error("no eligible bids for {0}")]
    NoEligibleBids(String),
    #[error("settlement floor not met: net {net}, minimum {minimum}")]
    SettlementFloor { net: u128, minimum: u128 },
    #[error("route asset mismatch")]
    RouteAssetMismatch,
    #[error("invalid scenario: {0}")]
    InvalidScenario(String),
    #[error("serialization error: {0}")]
    Serialization(String),
    #[error("io error: {0}")]
    Io(String),
}

impl EclipseError {
    pub fn code(&self) -> &'static str {
        match self {
            EclipseError::AmountOverflow => "AMOUNT_OVERFLOW",
            EclipseError::AmountUnderflow => "AMOUNT_UNDERFLOW",
            EclipseError::DivisionByZero => "DIVISION_BY_ZERO",
            EclipseError::InvalidBasisPoints(_) => "INVALID_BPS",
            EclipseError::AssetNotFound(_) => "ASSET_NOT_FOUND",
            EclipseError::AssetDisabled(_) => "ASSET_DISABLED",
            EclipseError::AccountNotFound(_) => "ACCOUNT_NOT_FOUND",
            EclipseError::OperatorNotFound(_) => "OPERATOR_NOT_FOUND",
            EclipseError::OperatorUnavailable(_) => "OPERATOR_UNAVAILABLE",
            EclipseError::RouteNotFound(_) => "ROUTE_NOT_FOUND",
            EclipseError::RouteDisabled(_) => "ROUTE_DISABLED",
            EclipseError::BatchNotFound(_) => "BATCH_NOT_FOUND",
            EclipseError::BidNotFound(_) => "BID_NOT_FOUND",
            EclipseError::DuplicateId(_) => "DUPLICATE_ID",
            EclipseError::InsufficientBalance { .. } => "INSUFFICIENT_BALANCE",
            EclipseError::InsufficientGuarantee { .. } => "INSUFFICIENT_GUARANTEE",
            EclipseError::BidRejected(_) => "BID_REJECTED",
            EclipseError::BatchClosed(_) => "BATCH_CLOSED",
            EclipseError::BatchNotReady(_) => "BATCH_NOT_READY",
            EclipseError::NoEligibleBids(_) => "NO_ELIGIBLE_BIDS",
            EclipseError::SettlementFloor { .. } => "SETTLEMENT_FLOOR",
            EclipseError::RouteAssetMismatch => "ROUTE_ASSET_MISMATCH",
            EclipseError::InvalidScenario(_) => "INVALID_SCENARIO",
            EclipseError::Serialization(_) => "SERIALIZATION",
            EclipseError::Io(_) => "IO",
        }
    }

    pub fn is_retryable(&self) -> bool {
        matches!(
            self,
            EclipseError::InsufficientBalance { .. }
                | EclipseError::SettlementFloor { .. }
                | EclipseError::NoEligibleBids(_)
        )
    }

    pub fn is_configuration_error(&self) -> bool {
        matches!(
            self,
            EclipseError::AssetNotFound(_)
                | EclipseError::AccountNotFound(_)
                | EclipseError::OperatorNotFound(_)
                | EclipseError::RouteNotFound(_)
                | EclipseError::InvalidScenario(_)
        )
    }
}

impl From<std::io::Error> for EclipseError {
    fn from(value: std::io::Error) -> Self {
        EclipseError::Io(value.to_string())
    }
}

impl From<serde_json::Error> for EclipseError {
    fn from(value: serde_json::Error) -> Self {
        EclipseError::Serialization(value.to_string())
    }
}
