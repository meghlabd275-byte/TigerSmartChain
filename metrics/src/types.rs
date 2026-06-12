//! Metrics Types

use std::sync::*;

// =============================================================================
// METRICS
// =============================================================================

/// Counter
pub struct Counter {
    value: std::sync::atomic::AtomicU64,
}

impl Counter {
    pub fn new() -> Self {
        Self {
            value: std::sync::atomic::AtomicU64::new(0),
        }
    }

    pub fn inc(&self) {
        self.value.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    }

    pub fn get(&self) -> u64 {
        self.value.load(std::sync::atomic::Ordering::Relaxed)
    }
}

/// Gauge
pub struct Gauge {
    value: std::sync::atomic::AtomicI64,
}

impl Gauge {
    pub fn new() -> Self {
        Self {
            value: std::sync::atomic::AtomicI64::new(0),
        }
    }

    pub fn set(&self, v: i64) {
        self.value.store(v, std::sync::atomic::Ordering::Relaxed);
    }

    pub fn get(&self) -> i64 {
        self.value.load(std::sync::atomic::Ordering::Relaxed)
    }
}