//! Block Sync Types

use serde::{Deserialize, Serialize};

// =============================================================================
// BLOCK SYNC
// =============================================================================

/// Block Sync Entry
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BlockSyncEntry {
    pub number: u64,
    pub hash: String,
    pub parent_hash: String,
    pub timestamp: u64,
    pub transactions: u64,
}

/// Block Sync
pub struct Sync {
    entries: std::collections::HashMap<u64, BlockSyncEntry>,
}

impl Sync {
    pub fn new() -> Self {
        Self {
            entries: std::collections::HashMap::new(),
        }
    }

    /// Add block
    pub fn add_block(&mut self, entry: BlockSyncEntry) {
        self.entries.insert(entry.number, entry);
    }

    /// Get block
    pub fn get_block(&self, number: u64) -> Option<&BlockSyncEntry> {
        self.entries.get(&number)
    }
}

impl Default for Sync {
    fn default() -> Self {
        Self::new()
    }
}