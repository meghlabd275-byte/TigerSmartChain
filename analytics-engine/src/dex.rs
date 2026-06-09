//! DEX Analytics

use serde::{Deserialize, Serialize};

/// DEX pool info
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DexPool {
    pub address: String,
    pub dex: String,
    pub token_a: String,
    pub token_b: String,
    pub reserve_a: String,
    pub reserve_b: String,
    pub volume_24h: String,
    pub fees_24h: String,
}

/// DEX stats
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DexStats {
    pub dex: String,
    pub total_volume_24h: f64,
    pub total_fees_24h: f64,
    pub total_tvl: f64,
    pub pool_count: usize,
}