//! State Service Types

use serde::{Deserialize, Serialize};

// =============================================================================
// STATE SERVICE
// =============================================================================

/// State Account
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StateAccount {
    pub address: String,
    pub balance: u64,
    pub nonce: u64,
    pub code_hash: String,
    pub storage_root: String,
}

/// State Slot
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StateSlot {
    pub address: String,
    pub slot: String,
    pub value: String,
}

/// State Snapshot
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StateSnapshot {
    pub block: u64,
    pub accounts: Vec<StateAccount>,
    pub slots: Vec<StateSlot>,
}

/// State Service
pub struct Service {
    snapshots: std::collections::HashMap<u64, StateSnapshot>,
}

impl Service {
    pub fn new() -> Self {
        Self {
            snapshots: std::collections::HashMap::new(),
        }
    }

    /// Add snapshot
    pub fn add_snapshot(&mut self, block: u64, snapshot: StateSnapshot) {
        self.snapshots.insert(block, snapshot);
    }

    /// Get snapshot
    pub fn get_snapshot(&self, block: u64) -> Option<&StateSnapshot> {
        self.snapshots.get(&block)
    }
}

impl Default for Service {
    fn default() -> Self {
        Self::new()
    }
}