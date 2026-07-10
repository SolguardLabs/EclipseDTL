use crate::amount::{Amount, BasisPoints};
use crate::error::Result;
use crate::ids::{BatchId, OperatorId, RouteId};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Counter {
    pub name: String,
    pub value: u128,
}

impl Counter {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            value: 0,
        }
    }

    pub fn increment(&mut self, by: u128) {
        self.value = self.value.saturating_add(by);
    }

    pub fn reset(&mut self) {
        self.value = 0;
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Gauge {
    pub name: String,
    pub value: i128,
}

impl Gauge {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            value: 0,
        }
    }

    pub fn set(&mut self, value: i128) {
        self.value = value;
    }

    pub fn add(&mut self, delta: i128) {
        self.value = self.value.saturating_add(delta);
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Histogram {
    pub name: String,
    pub buckets: Vec<u128>,
    pub observations: Vec<u128>,
}

impl Histogram {
    pub fn new(name: impl Into<String>, buckets: Vec<u128>) -> Self {
        Self {
            name: name.into(),
            buckets,
            observations: Vec::new(),
        }
    }

    pub fn observe(&mut self, value: u128) {
        self.observations.push(value);
    }

    pub fn count(&self) -> usize {
        self.observations.len()
    }

    pub fn sum(&self) -> u128 {
        self.observations
            .iter()
            .copied()
            .fold(0_u128, |acc, value| acc.saturating_add(value))
    }

    pub fn max(&self) -> Option<u128> {
        self.observations.iter().copied().max()
    }

    pub fn bucket_counts(&self) -> Vec<(u128, usize)> {
        self.buckets
            .iter()
            .copied()
            .map(|bucket| {
                let count = self
                    .observations
                    .iter()
                    .filter(|value| **value <= bucket)
                    .count();
                (bucket, count)
            })
            .collect()
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TelemetryRegistry {
    counters: BTreeMap<String, Counter>,
    gauges: BTreeMap<String, Gauge>,
    histograms: BTreeMap<String, Histogram>,
}

impl TelemetryRegistry {
    pub fn new() -> Self {
        Self {
            counters: BTreeMap::new(),
            gauges: BTreeMap::new(),
            histograms: BTreeMap::new(),
        }
    }

    pub fn counter_mut(&mut self, name: impl Into<String>) -> &mut Counter {
        let name = name.into();
        self.counters
            .entry(name.clone())
            .or_insert_with(|| Counter::new(name))
    }

    pub fn gauge_mut(&mut self, name: impl Into<String>) -> &mut Gauge {
        let name = name.into();
        self.gauges
            .entry(name.clone())
            .or_insert_with(|| Gauge::new(name))
    }

    pub fn histogram_mut(&mut self, name: impl Into<String>, buckets: Vec<u128>) -> &mut Histogram {
        let name = name.into();
        self.histograms
            .entry(name.clone())
            .or_insert_with(|| Histogram::new(name, buckets))
    }

    pub fn record_batch_settled(
        &mut self,
        batch: &BatchId,
        route: &RouteId,
        operator: &OperatorId,
    ) {
        self.counter_mut("batch.settled").increment(1);
        self.counter_mut(format!("batch.settled.route.{route}"))
            .increment(1);
        self.counter_mut(format!("batch.settled.operator.{operator}"))
            .increment(1);
        self.gauge_mut(format!("batch.{batch}.terminal")).set(1);
    }

    pub fn record_output(&mut self, route: &RouteId, gross: Amount, net: Amount) {
        self.histogram_mut("settlement.gross", vec![100, 1_000, 10_000, 100_000])
            .observe(gross.raw());
        self.histogram_mut("settlement.net", vec![100, 1_000, 10_000, 100_000])
            .observe(net.raw());
        self.histogram_mut(
            format!("route.{route}.gross"),
            vec![100, 1_000, 10_000, 100_000],
        )
        .observe(gross.raw());
    }

    pub fn record_guarantee_margin(&mut self, operator: &OperatorId, margin: i128) {
        self.gauge_mut(format!("operator.{operator}.guarantee_margin"))
            .set(margin);
    }

    pub fn counter_value(&self, name: &str) -> u128 {
        self.counters
            .get(name)
            .map(|counter| counter.value)
            .unwrap_or(0)
    }

    pub fn gauge_value(&self, name: &str) -> i128 {
        self.gauges.get(name).map(|gauge| gauge.value).unwrap_or(0)
    }

    pub fn snapshot(&self) -> TelemetrySnapshot {
        TelemetrySnapshot {
            counters: self.counters.values().cloned().collect(),
            gauges: self.gauges.values().cloned().collect(),
            histograms: self.histograms.values().cloned().collect(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TelemetrySnapshot {
    pub counters: Vec<Counter>,
    pub gauges: Vec<Gauge>,
    pub histograms: Vec<Histogram>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ServiceLevelWindow {
    pub name: String,
    pub target_bps: BasisPoints,
    pub successful: u128,
    pub total: u128,
}

impl ServiceLevelWindow {
    pub fn new(name: impl Into<String>, target_bps: BasisPoints) -> Self {
        Self {
            name: name.into(),
            target_bps,
            successful: 0,
            total: 0,
        }
    }

    pub fn record(&mut self, success: bool) {
        self.total = self.total.saturating_add(1);
        if success {
            self.successful = self.successful.saturating_add(1);
        }
    }

    pub fn observed_bps(&self) -> Result<BasisPoints> {
        if self.total == 0 {
            return Ok(BasisPoints::ZERO);
        }
        let raw = self.successful.saturating_mul(10_000) / self.total;
        BasisPoints::new(raw.min(10_000) as u32)
    }

    pub fn meets_target(&self) -> Result<bool> {
        Ok(self.observed_bps()?.raw() >= self.target_bps.raw())
    }
}
