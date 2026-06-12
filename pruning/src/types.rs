//! Pruning Types

// =============================================================================
// PRUNING
// =============================================================================

/// Pruner
pub struct Pruner {
    interval: u64,
    retain: u64,
}

impl Pruner {
    pub fn new(interval: u64, retain: u64) -> Self {
        Self { interval, retain }
    }

    /// Prune
    pub fn prune(&self, block_number: u64) -> bool {
        block_number % self.interval == 0
    }
}