//! Pruning Types

// =============================================================================
// PRUNING
// =============================================================================

/// Pruner
pub struct Pruner {
    pub interval: u64,
    pub retain: u64,
    pub pruned_count: u64,
}

impl Pruner {
    pub fn new(interval: u64, retain: u64) -> Self {
        Self {
            interval,
            retain,
            pruned_count: 0,
        }
    }

    /// Prune
    pub fn prune(&self, block_number: u64) -> bool {
        block_number % self.interval == 0
    }
}