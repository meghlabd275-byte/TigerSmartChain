//! Staking Sync Types

use serde::{Deserialize, Serialize};

// =============================================================================
// STAKING SYNC
// =============================================================================

/// Validator Stake
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidatorStake {
    pub validator: String,
    pub staked_amount: u64,
    pub delegators: u32,
    pub last_update: u64,
}

/// Delegation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Delegation {
    pub delegator: String,
    pub validator: String,
    pub amount: u64,
    pub rewards: u64,
}

/// Staking Pool
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StakingPool {
    pub total_staked: u64,
    pub validators: Vec<ValidatorStake>,
    pub rewards_rate: f64,
}

/// Sync Status
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncStatus {
    pub last_block: u64,
    pub synced_at: u64,
    pub pending: u32,
}

impl SyncStatus {
    pub fn new() -> Self {
        Self {
            last_block: 0,
            synced_at: 0,
            pending: 0,
        }
    }
}

impl Default for SyncStatus {
    fn default() -> Self {
        Self::new()
    }
}