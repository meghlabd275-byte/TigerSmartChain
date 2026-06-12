//! Snapshot Types

use serde::{Deserialize, Serialize};

// =============================================================================
// SNAPSHOT
// =============================================================================

/// Snapshot
pub struct Snapshot {
    block_number: u64,
    root: String,
}

impl Snapshot {
    pub fn new(block_number: u64, root: String) -> Self {
        Self { block_number, root }
    }

    /// Get block number
    pub fn block_number(&self) -> u64 {
        self.block_number
    }

    /// Get root
    pub fn root(&self) -> &str {
        &self.root
    }
}

/// Snapshot List
pub struct SnapshotList {
    snapshots: Vec<Snapshot>,
}

impl SnapshotList {
    pub fn new() -> Self {
        Self {
            snapshots: vec![],
        }
    }

    /// Add snapshot
    pub fn add(&mut self, snapshot: Snapshot) {
        self.snapshots.push(snapshot);
    }

    /// Get latest
    pub fn latest(&self) -> Option<&Snapshot> {
        self.snapshots.last()
    }
}

impl Default for SnapshotList {
    fn default() -> Self {
        Self::new()
    }
}