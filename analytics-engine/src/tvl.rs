//! TVL (Total Value Locked) Metrics

use serde::{Deserialize, Serialize};

/// TVL metrics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TVLMetrics {
    pub tvl_usd: f64,
    pub tvl_change_24h: f64,
    pub tvl_change_7d: f64,
    pub tvl_change_30d: f64,
    pub dominant_protocol: String,
    pub protocol_breakdown: Vec<ProtocolTVL>,
    pub timestamp: i64,
}

/// TVL breakdown by protocol
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProtocolTVL {
    pub protocol: String,
    pub tvl_usd: f64,
    pub share_percent: f64,
}

impl TVLMetrics {
    /// Calculate TVL change percentage
    pub fn calculate_change(&self, previous: &TVLMetrics) -> f64 {
        if previous.tvl_usd == 0.0 {
            return 0.0;
        }
        ((self.tvl_usd - previous.tvl_usd) / previous.tvl_usd) * 100.0
    }
}