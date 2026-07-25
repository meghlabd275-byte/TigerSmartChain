//! State Pruning Types

use serde::{Deserialize, Serialize};

/// State root information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StateRoot {
    pub block_number: u64,
    pub state_root: [u8; 32],
    pub timestamp: u64,
}

/// Pruned block range
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PrunedRange {
    pub start_block: u64,
    pub end_block: u64,
    pub state_root: [u8; 32],
}

/// Snapshot information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SnapshotInfo {
    pub block_number: u64,
    pub state_root: [u8; 32],
    pub file_path: String,
    pub size: u64,
    pub created_at: u64,
}
