//! Mempool Sync Types

use serde::{Deserialize, Serialize};

// =============================================================================
// MEMPOOL SYNC
// =============================================================================

/// Mempool Transaction
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MempoolTransaction {
    pub hash: String,
    pub from: String,
    pub to: String,
    pub value: u64,
    pub gas: u64,
    pub gas_price: u64,
    pub nonce: u64,
    pub input: Vec<u8>,
}

/// Mempool Entry
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MempoolEntry {
    pub tx: MempoolTransaction,
    pub received_at: u64,
    pub gas_used: u64,
}

/// Mempool Sync
pub struct Sync {
    entries: std::collections::HashMap<String, MempoolEntry>,
}

impl Sync {
    pub fn new() -> Self {
        Self {
            entries: std::collections::HashMap::new(),
        }
    }

    /// Add transaction
    pub fn add(&mut self, entry: MempoolEntry) {
        self.entries.insert(entry.tx.hash.clone(), entry);
    }

    /// Get transaction
    pub fn get(&self, hash: &str) -> Option<&MempoolEntry> {
        self.entries.get(hash)
    }

    /// Remove transaction
    pub fn remove(&mut self, hash: &str) {
        self.entries.remove(hash);
    }
}

impl Default for Sync {
    fn default() -> Self {
        Self::new()
    }
}