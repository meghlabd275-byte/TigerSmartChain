//! Token Sync Types

use serde::{Deserialize, Serialize};

// =============================================================================
// TOKEN SYNC
// =============================================================================

/// Token Metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenMetadata {
    pub address: String,
    pub name: String,
    pub symbol: String,
    pub decimals: u8,
    pub total_supply: u64,
    pub holders: u32,
    pub transfers: u64,
}

/// Holder
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Holder {
    pub address: String,
    pub balance: u64,
    pub last_update: u64,
}

/// Transfer Event
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TransferEvent {
    pub token: String,
    pub from: String,
    pub to: String,
    pub value: u64,
    pub block: u64,
    pub timestamp: u64,
}

/// Sync Status
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncStatus {
    pub last_block: u64,
    pub tokens_synced: u32,
    pub holders_synced: u32,
    pub transfers_synced: u64,
}

impl SyncStatus {
    pub fn new() -> Self {
        Self {
            last_block: 0,
            tokens_synced: 0,
            holders_synced: 0,
            transfers_synced: 0,
        }
    }
}

impl Default for SyncStatus {
    fn default() -> Self {
        Self::new()
    }
}