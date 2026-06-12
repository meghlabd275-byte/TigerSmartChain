//! Analytics Types

use serde::{Deserialize, Serialize};

/// Analytics Data Point
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnalyticsDataPoint {
    pub timestamp: i64,
    pub value: f64,
}

/// Network Analytics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetworkAnalytics {
    pub tps: f64,
    pub gas_price: u64,
    pub active_addresses: i64,
    pub new_addresses: i64,
    pub block_time: f64,
    pub total_transactions: i64,
}

/// Address Analytics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AddressAnalytics {
    pub address: String,
    pub tx_count: i64,
    pub total_sent: u64,
    pub total_received: u64,
    pub first_seen: i64,
    pub last_seen: i64,
}

/// Contract Analytics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContractAnalytics {
    pub address: String,
    pub call_count: i64,
    pub unique_callers: i64,
    pub total_gas_used: u64,
}