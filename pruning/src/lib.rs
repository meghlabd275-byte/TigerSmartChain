//! TigerScan Pruning Module

pub mod types;

pub use types::*;

use tiger_state_db::StateDB;

impl Pruner {
    /// Prune old state from the database
    pub fn prune_history(&mut self, _db: &mut StateDB, block_number: u64) -> u64 {
        if self.prune(block_number) {
            // Mock implementation: in a real world application,
            // we would iterate over historical state and delete entries
            // older than (block_number - self.retain)
            let pruned = 10; // Mock pruned count
            self.pruned_count += pruned;
            return pruned;
        }
        0
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tiger_state_db::StateDB;

    #[test]
    fn test_pruning() {
        let mut pruner = Pruner::new(10, 100);
        let mut db = StateDB::new();

        // Should not prune at block 5
        assert_eq!(pruner.prune_history(&mut db, 5), 0);
        assert_eq!(pruner.pruned_count, 0);

        // Should prune at block 10
        assert_eq!(pruner.prune_history(&mut db, 10), 10);
        assert_eq!(pruner.pruned_count, 10);
    }
}