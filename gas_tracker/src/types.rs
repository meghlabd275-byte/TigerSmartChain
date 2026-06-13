//! Gas Tracker Types for TigerScan

use serde::{Deserialize, Serialize};
use chrono::{DateTime, Utc};

// =============================================================================
// GAS PRICES
// =============================================================================

/// Current gas prices
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GasPrices {
    /// Slow gas price (Gwei)
    pub slow: f64,
    /// Standard gas price (Gwei)
    pub standard: f64,
    /// Fast gas price (Gwei)
    pub fast: f64,
    /// Base fee (Gwei)
    pub base_fee: f64,
    /// Priority fee (Gwei)
    pub priority_fee: f64,
    /// Last updated
    pub last_updated: i64,
}

impl Default for GasPrices {
    fn default() -> Self {
        Self {
            slow: 20.0,
            standard: 30.0,
            fast: 50.0,
            base_fee: 20.0,
            priority_fee: 10.0,
            last_updated: Utc::now().timestamp(),
        }
    }
}

// =============================================================================
// HISTORICAL GAS
// =============================================================================

/// Historical gas data point
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GasHistoryPoint {
    /// Timestamp
    pub timestamp: i64,
    /// Average gas price (Gwei)
    pub average_gas_price: f64,
    /// Median gas price (Gwei)
    pub median_gas_price: f64,
    /// Minimum gas price (Gwei)
    pub min_gas_price: f64,
    /// Maximum gas price (Gwei)
    pub max_gas_price: f64,
    /// Number of transactions
    pub tx_count: u64,
    /// Total gas used
    pub total_gas_used: u64,
    /// Average gas used per transaction
    pub avg_gas_used: f64,
}

/// Gas history
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GasHistory {
    /// Data points
    pub data: Vec<GasHistoryPoint>,
    /// Time range
    pub start_time: i64,
    /// End time
    pub end_time: i64,
}

// =============================================================================
// GAS ESTIMATES
// =============================================================================

/// Gas estimate for different conditions
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GasEstimate {
    /// Estimated gas for fast confirmation
    pub fast_gas: u64,
    /// Estimated gas for standard confirmation
    pub standard_gas: u64,
    /// Estimated gas for slow confirmation
    pub slow_gas: u64,
    /// Confidence level
    pub confidence: f64,
    /// Validity period (seconds)
    pub validity: u64,
}

// =============================================================================
// FEE MARKET
// =============================================================================

/// Fee market data (EIP-1559)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FeeMarket {
    /// Current base fee (Gwei)
    pub base_fee: f64,
    /// Current base fee in next block (Gwei)
    pub base_fee_next: f64,
    /// Gas target
    pub gas_target: u64,
    /// Gas used
    pub gas_used: u64,
    /// Gas used percentage
    pub gas_used_percent: f64,
    /// Priority fee
    pub priority_fee: f64,
    /// Priority fee max
    pub priority_fee_max: f64,
    /// Priority fee max sender
    pub priority_fee_max_sender: f64,
}

// =============================================================================
// BURN DATA
// =============================================================================

/// Burn data (EIP-1559)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BurnData {
    /// Total ETH burned
    pub total_burned: String,
    /// Burn rate (ETH per day)
    pub burn_rate: f64,
    /// Block number
    pub block_number: u64,
    /// Timestamp
    pub timestamp: i64,
}

// =============================================================================
// GAS ANALYTICS
// =============================================================================

/// Gas analytics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GasAnalytics {
    /// Average gas price (24h)
    pub avg_price_24h: f64,
    /// Average gas price (7d)
    pub avg_price_7d: f64,
    /// Average gas price (30d)
    pub avg_price_30d: f64,
    /// Gas price volatility
    pub volatility: f64,
    /// Most common gas price
    pub mode_price: f64,
    /// Transactions per second
    pub tps: f64,
    /// Network utilization
    pub utilization: f64,
}

// =============================================================================
// CONFIG
// =============================================================================

/// Gas Tracker Configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GasTrackerConfig {
    /// RPC URL
    pub rpc_url: String,
    /// Cache duration (seconds)
    pub cache_duration_secs: u64,
    /// History retention (days)
    pub history_retention_days: u32,
    /// Enable burn tracking
    pub enable_burn_tracking: bool,
    /// Enable predictions
    pub enable_predictions: bool,
}

impl Default for GasTrackerConfig {
    fn default() -> Self {
        Self {
            rpc_url: "http://localhost:8545".to_string(),
            cache_duration_secs: 30,
            history_retention_days: 365,
            enable_burn_tracking: true,
            enable_predictions: true,
        }
    }
}