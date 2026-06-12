//! Validator Sync Types

use serde::{Deserialize, Serialize};

// =============================================================================
// VALIDATOR SYNC
// =============================================================================

/// Validator Info
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidatorInfo {
    pub address: String,
    pub name: String,
    pub stake: u64,
    pub delegators: u32,
    pub commission: u8,
    pub uptime: f64,
    pub blocks_proposed: u64,
    pub last_proposed: u64,
}

/// Validator Set
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidatorSet {
    pub total_stake: u64,
    pub validators: Vec<ValidatorInfo>,
    pub active_validators: u32,
}

/// Sync Block
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncBlock {
    pub block_number: u64,
    pub proposer: String,
    pub tx_count: u32,
    pub gas_used: u64,
    pub timestamp: u64,
}

/// Validator Sync Status
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncStatus {
    pub last_synced_block: u64,
    pub validator_count: u32,
    pub synced_at: u64,
}

impl SyncStatus {
    pub fn new() -> Self {
        Self {
            last_synced_block: 0,
            validator_count: 0,
            synced_at: 0,
        }
    }
}

impl Default for SyncStatus {
    fn default() -> Self {
        Self::new()
    }
}