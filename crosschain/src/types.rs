//! Cross-Chain Types

use serde::{Deserialize, Serialize};

// =============================================================================
// CROSS-CHAIN
// =============================================================================

/// Bridge
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Bridge {
    pub id: String,
    pub name: String,
    pub source_chain: String,
    pub target_chain: String,
    pub token: String,
    pub total_locked: u64,
    pub total_bridged: u64,
}

/// Cross-Chain Transfer
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Transfer {
    pub id: String,
    pub hash: String,
    pub source_chain: String,
    pub target_chain: String,
    pub sender: String,
    pub recipient: String,
    pub token: String,
    pub amount: u64,
    pub status: String,
    pub timestamp: u64,
}

/// Bridge Analytics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BridgeAnalytics {
    pub bridge_id: String,
    pub daily_volume: u64,
    pub weekly_volume: u64,
    pub monthly_volume: u64,
    pub total_users: u32,
}

/// Token Mapping
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenMapping {
    pub original_token: String,
    pub wrapped_token: String,
    pub chain_id: u64,
    pub bridge: String,
}