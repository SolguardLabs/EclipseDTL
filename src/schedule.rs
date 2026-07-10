use crate::error::{EclipseError, Result};
use crate::ids::BatchId;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct Timestamp(pub u64);

impl Timestamp {
    pub fn new(value: u64) -> Self {
        Self(value)
    }

    pub fn raw(self) -> u64 {
        self.0
    }

    pub fn saturating_add(self, seconds: u64) -> Self {
        Self(self.0.saturating_add(seconds))
    }

    pub fn saturating_sub(self, seconds: u64) -> Self {
        Self(self.0.saturating_sub(seconds))
    }
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TimeWindow {
    pub start: Timestamp,
    pub end: Timestamp,
}

impl TimeWindow {
    pub fn new(start: Timestamp, end: Timestamp) -> Result<Self> {
        if end < start {
            return Err(EclipseError::InvalidScenario(format!(
                "invalid time window {}>{}",
                start.raw(),
                end.raw()
            )));
        }
        Ok(Self { start, end })
    }

    pub fn contains(&self, timestamp: Timestamp) -> bool {
        timestamp >= self.start && timestamp <= self.end
    }

    pub fn overlaps(&self, other: &TimeWindow) -> bool {
        self.start <= other.end && other.start <= self.end
    }

    pub fn duration(&self) -> u64 {
        self.end.raw().saturating_sub(self.start.raw())
    }

    pub fn expand(&self, seconds: u64) -> Self {
        Self {
            start: self.start.saturating_sub(seconds),
            end: self.end.saturating_add(seconds),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum BatchPriority {
    Low,
    Normal,
    High,
    Critical,
}

impl BatchPriority {
    pub fn weight(&self) -> u32 {
        match self {
            BatchPriority::Low => 10,
            BatchPriority::Normal => 50,
            BatchPriority::High => 80,
            BatchPriority::Critical => 100,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ScheduledBatch {
    pub batch: BatchId,
    pub window: TimeWindow,
    pub priority: BatchPriority,
    pub sequence: u64,
}

impl ScheduledBatch {
    pub fn new(batch: BatchId, window: TimeWindow, priority: BatchPriority, sequence: u64) -> Self {
        Self {
            batch,
            window,
            priority,
            sequence,
        }
    }

    pub fn is_live(&self, timestamp: Timestamp) -> bool {
        self.window.contains(timestamp)
    }

    pub fn sort_key(&self) -> (std::cmp::Reverse<u32>, u64, BatchId) {
        (
            std::cmp::Reverse(self.priority.weight()),
            self.sequence,
            self.batch.clone(),
        )
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct BatchScheduler {
    scheduled: BTreeMap<BatchId, ScheduledBatch>,
    next_sequence: u64,
}

impl BatchScheduler {
    pub fn new() -> Self {
        Self {
            scheduled: BTreeMap::new(),
            next_sequence: 1,
        }
    }

    pub fn schedule(
        &mut self,
        batch: BatchId,
        window: TimeWindow,
        priority: BatchPriority,
    ) -> Result<()> {
        if self.scheduled.contains_key(&batch) {
            return Err(EclipseError::DuplicateId(batch.to_string()));
        }
        let sequence = self.next_sequence;
        self.next_sequence = self.next_sequence.saturating_add(1);
        self.scheduled.insert(
            batch.clone(),
            ScheduledBatch::new(batch, window, priority, sequence),
        );
        Ok(())
    }

    pub fn remove(&mut self, batch: &BatchId) -> Option<ScheduledBatch> {
        self.scheduled.remove(batch)
    }

    pub fn get(&self, batch: &BatchId) -> Option<&ScheduledBatch> {
        self.scheduled.get(batch)
    }

    pub fn live_at(&self, timestamp: Timestamp) -> Vec<&ScheduledBatch> {
        let mut live: Vec<&ScheduledBatch> = self
            .scheduled
            .values()
            .filter(|scheduled| scheduled.is_live(timestamp))
            .collect();
        live.sort_by_key(|scheduled| scheduled.sort_key());
        live
    }

    pub fn next_ready(&self, timestamp: Timestamp) -> Option<&ScheduledBatch> {
        self.live_at(timestamp).into_iter().next()
    }

    pub fn pending_before(&self, timestamp: Timestamp) -> Vec<&ScheduledBatch> {
        let mut pending: Vec<&ScheduledBatch> = self
            .scheduled
            .values()
            .filter(|scheduled| scheduled.window.start <= timestamp)
            .collect();
        pending.sort_by_key(|scheduled| scheduled.sort_key());
        pending
    }

    pub fn count(&self) -> usize {
        self.scheduled.len()
    }

    pub fn is_empty(&self) -> bool {
        self.scheduled.is_empty()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Epoch {
    pub index: u64,
    pub window: TimeWindow,
    pub max_batches: usize,
}

impl Epoch {
    pub fn new(index: u64, window: TimeWindow, max_batches: usize) -> Self {
        Self {
            index,
            window,
            max_batches,
        }
    }

    pub fn contains(&self, timestamp: Timestamp) -> bool {
        self.window.contains(timestamp)
    }

    pub fn has_capacity(&self, count: usize) -> bool {
        count < self.max_batches
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct EpochCalendar {
    epochs: Vec<Epoch>,
}

impl EpochCalendar {
    pub fn new() -> Self {
        Self { epochs: Vec::new() }
    }

    pub fn add_epoch(&mut self, epoch: Epoch) -> Result<()> {
        for existing in &self.epochs {
            if existing.window.overlaps(&epoch.window) {
                return Err(EclipseError::InvalidScenario(format!(
                    "epoch {} overlaps epoch {}",
                    epoch.index, existing.index
                )));
            }
        }
        self.epochs.push(epoch);
        self.epochs.sort_by_key(|epoch| epoch.index);
        Ok(())
    }

    pub fn epoch_at(&self, timestamp: Timestamp) -> Option<&Epoch> {
        self.epochs.iter().find(|epoch| epoch.contains(timestamp))
    }

    pub fn next_epoch_after(&self, timestamp: Timestamp) -> Option<&Epoch> {
        self.epochs
            .iter()
            .filter(|epoch| epoch.window.start > timestamp)
            .min_by_key(|epoch| epoch.window.start)
    }

    pub fn len(&self) -> usize {
        self.epochs.len()
    }

    pub fn is_empty(&self) -> bool {
        self.epochs.is_empty()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BatchDeadline {
    pub batch: BatchId,
    pub soft_deadline: Timestamp,
    pub hard_deadline: Timestamp,
}

impl BatchDeadline {
    pub fn new(batch: BatchId, soft_deadline: Timestamp, hard_deadline: Timestamp) -> Result<Self> {
        if hard_deadline < soft_deadline {
            return Err(EclipseError::InvalidScenario(format!(
                "deadline order for {}",
                batch
            )));
        }
        Ok(Self {
            batch,
            soft_deadline,
            hard_deadline,
        })
    }

    pub fn state_at(&self, timestamp: Timestamp) -> DeadlineState {
        if timestamp <= self.soft_deadline {
            DeadlineState::Open
        } else if timestamp <= self.hard_deadline {
            DeadlineState::SoftExpired
        } else {
            DeadlineState::HardExpired
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum DeadlineState {
    Open,
    SoftExpired,
    HardExpired,
}
