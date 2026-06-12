//! Oracle Types

use serde::{Deserialize, Serialize};

// =============================================================================
// PRICE
// =============================================================================

/// Price Feed
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PriceFeed {
    pub address: String,
    pub price: f64,
    pub updated_at: i64,
    pub round: u64,
}

/// Price Update
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PriceUpdate {
    pub token: String,
    pub price: f64,
    pub change_24h: f64,
    pub volume_24h: f64,
    pub market_cap: f64,
}

// =============================================================================
// ORACLE
// =============================================================================

/// Oracle Config
#[derive(Debug, Clone)]
pub struct OracleConfig {
    pub update_interval: u64,
    pub deviation_threshold: f64,
}

impl Default for OracleConfig {
    fn default() -> Self {
        Self {
            update_interval: 30,
            deviation_threshold: 0.5,
        }
    }
}