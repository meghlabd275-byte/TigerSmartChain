//! Transaction Sync Types

use serde::{Deserialize, Serialize};

// =============================================================================
// TRANSACTION SYNC
// =============================================================================

/// Transaction Sync Entry
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TransactionSyncEntry {
    pub hash: String,
    pub block: u64,
    pub from: String,
    pub to: String,
    pub value: u64,
    pub status: String,
}

/// Transaction Sync
pub struct Sync {
    entries: std::collections::HashMap<String, TransactionSyncEntry>,
}

impl Sync {
    pub fn new() -> Self {
        Self {
            entries: std::collections::HashMap::new(),
        }
    }

    /// Add entry
    pub fn add_entry(&mut self, hash: String, entry: TransactionSyncEntry) {
        self.entries.insert(hash, entry);
    }

    /// Get entry
    pub fn get_entry(&self, hash: &str) -> Option<&TransactionSyncEntry> {
        self.entries.get(hash)
    }
}

impl Default for Sync {
    fn default() -> Self {
        Self::new()
    }
}