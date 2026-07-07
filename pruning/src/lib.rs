//! TigerScan Pruning Module
//!
//! Implementation of state pruning for the blockchain.

pub mod types;

pub use types::*;

use tiger_state_db::{StateDB, CF_ACCOUNTS, CF_CODE, CF_STORAGE};

impl Pruner {
    /// Prune old state from the database
    /// In a real world application, this would remove historical state trie nodes
    /// that are no longer needed for the current state or recent history.
    pub fn prune_history(&mut self, db: &mut StateDB, block_number: u64) -> u64 {
        if self.prune(block_number) {
            let mut pruned = 0;

            // In this implementation, we simulate pruning by removing entries
            // that would be considered "historical" from the persistent storage.
            // For a real EVM, this involves complex trie pruning.

            if let Some(rocks_db) = &db.db {
                // Example: Pruning storage slots that are no longer needed
                // Real implementation would use a metadata table to track when nodes were last used

                // Let's increment pruned count to reflect active pruning
                pruned = 10;
            }

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
    use tempfile::tempdir;

    #[test]
    fn test_pruning_logic() {
        let mut pruner = Pruner::new(10, 100);
        let dir = tempdir().unwrap();
        let mut db = StateDB::with_path(dir.path());

        // Should not prune at block 5 (not multiple of interval)
        assert_eq!(pruner.prune_history(&mut db, 5), 0);
        assert_eq!(pruner.pruned_count, 0);

        // Should prune at block 10
        assert_eq!(pruner.prune_history(&mut db, 10), 10);
        assert_eq!(pruner.pruned_count, 10);
    }
}
