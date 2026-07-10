use serde::{Deserialize, Serialize};
use std::fmt::{Display, Formatter};
use std::hash::Hash;

macro_rules! define_id {
    ($name:ident, $prefix:literal) => {
        #[derive(
            Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, Default,
        )]
        #[serde(transparent)]
        pub struct $name(pub String);

        impl $name {
            pub fn new(value: impl Into<String>) -> Self {
                Self(value.into())
            }

            pub fn generated(seed: u64) -> Self {
                Self(format!("{}-{:08}", $prefix, seed))
            }

            pub fn as_str(&self) -> &str {
                self.0.as_str()
            }

            pub fn into_inner(self) -> String {
                self.0
            }

            pub fn is_empty(&self) -> bool {
                self.0.trim().is_empty()
            }

            pub fn tagged(&self, suffix: impl AsRef<str>) -> String {
                format!("{}:{}", self.0, suffix.as_ref())
            }

            pub fn starts_with_prefix(&self) -> bool {
                self.0.starts_with($prefix)
            }
        }

        impl Display for $name {
            fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
                f.write_str(self.0.as_str())
            }
        }

        impl From<&str> for $name {
            fn from(value: &str) -> Self {
                Self(value.to_owned())
            }
        }

        impl From<String> for $name {
            fn from(value: String) -> Self {
                Self(value)
            }
        }
    };
}

define_id!(AssetId, "asset");
define_id!(AccountId, "acct");
define_id!(OperatorId, "op");
define_id!(RouteId, "route");
define_id!(BatchId, "batch");
define_id!(BidId, "bid");

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct IdPair<L, R>
where
    L: Clone + Eq + Hash,
    R: Clone + Eq + Hash,
{
    pub left: L,
    pub right: R,
}

impl<L, R> IdPair<L, R>
where
    L: Clone + Eq + Hash,
    R: Clone + Eq + Hash,
{
    pub fn new(left: L, right: R) -> Self {
        Self { left, right }
    }

    pub fn map_left<N>(self, next: N) -> IdPair<N, R>
    where
        N: Clone + Eq + Hash,
    {
        IdPair {
            left: next,
            right: self.right,
        }
    }

    pub fn map_right<N>(self, next: N) -> IdPair<L, N>
    where
        N: Clone + Eq + Hash,
    {
        IdPair {
            left: self.left,
            right: next,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AssetPair {
    pub source: AssetId,
    pub target: AssetId,
}

impl AssetPair {
    pub fn new(source: AssetId, target: AssetId) -> Self {
        Self { source, target }
    }

    pub fn reversed(&self) -> Self {
        Self {
            source: self.target.clone(),
            target: self.source.clone(),
        }
    }

    pub fn contains(&self, asset: &AssetId) -> bool {
        &self.source == asset || &self.target == asset
    }

    pub fn key(&self) -> String {
        format!("{}>{}", self.source, self.target)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RouteKey {
    pub route_id: RouteId,
    pub source: AssetId,
    pub target: AssetId,
}

impl RouteKey {
    pub fn new(route_id: RouteId, source: AssetId, target: AssetId) -> Self {
        Self {
            route_id,
            source,
            target,
        }
    }

    pub fn matches_pair(&self, pair: &AssetPair) -> bool {
        self.source == pair.source && self.target == pair.target
    }

    pub fn label(&self) -> String {
        format!("{}:{}>{}", self.route_id, self.source, self.target)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SettlementKey {
    pub batch_id: BatchId,
    pub bid_id: BidId,
    pub route_id: RouteId,
}

impl SettlementKey {
    pub fn new(batch_id: BatchId, bid_id: BidId, route_id: RouteId) -> Self {
        Self {
            batch_id,
            bid_id,
            route_id,
        }
    }

    pub fn label(&self) -> String {
        format!("{}/{}/{}", self.batch_id, self.bid_id, self.route_id)
    }
}
