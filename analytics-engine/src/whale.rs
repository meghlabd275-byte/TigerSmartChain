//! Whale Detection

use serde::{Deserialize, Serialize};

/// Whale alert
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WhaleAlert {
    pub address: String,
    pub amount_usd: f64,
    pub timestamp: i64,
    pub alert_type: String,
    pub transaction_hash: Option<String>,
    pub tokens: Option<Vec<String>>,
}

/// Whale detection config
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WhaleConfig {
    pub threshold_small: f64,
    pub threshold_medium: f64,
    pub threshold_large: f64,
    pub notification_enabled: bool,
}

impl Default for WhaleConfig {
    fn default() -> Self {
        Self {
            threshold_small: 10_000.0,
            threshold_medium: 100_000.0,
            threshold_large: 1_000_000.0,
            notification_enabled: true,
        }
    }
}