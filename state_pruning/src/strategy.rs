//! Pruning Strategies

use super::*;

/// Pruning strategy executor
pub struct PruningStrategyExecutor;

impl PruningStrategyExecutor {
    /// Execute strategy
    pub fn execute(
        strategy: PruningStrategy,
        pruner: &mut StatePruner,
    ) -> Result<(), PruningError> {
        match strategy {
            PruningStrategy::None => Ok(()),
            PruningStrategy::Interval => pruner.prune_interval(),
            PruningStrategy::Absolute => pruner.prune_absolute(),
            PruningStrategy::BiMode => pruner.prune_bimode(),
            PruningStrategy::Hybrid => pruner.prune_hybrid(),
        }
    }
}
