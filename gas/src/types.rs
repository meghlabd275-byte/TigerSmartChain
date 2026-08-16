//! Gas Types for TigerScan

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

// =============================================================================
// GAS TYPES
// =============================================================================

/// Gas Price
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GasPrice {
    pub low: u64,        // Gwei
    pub medium: u64,     // Gwei
    pub high: u64,       // Gwei
    pub base_fee: u64,   // Gwei (post-EIP-1559)
    pub priority_fee: u64, // Gwei
    pub timestamp: i64,
}

/// Gas Estimate
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GasEstimate {
    pub gas_price: u64,       // Gwei
    pub estimated_gas: u64,   // Gas units
    pub estimated_cost: u64,  // Wei
    pub estimated_cost_usd: f64,
    pub confidence: f64,        // 0-100%
    pub estimated_time: u64,    // seconds
}

/// Gas History
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GasHistory {
    pub timestamp: i64,
    pub gas_price: u64,
    pub gas_used: u64,
    pub block_number: u64,
}

/// Gas Prediction
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GasPrediction {
    pub predicted_price: u64,
    pub confidence: f64,
    pub prediction_time: i64,
    pub model: PredictionModel,
}

/// Prediction Model
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum PredictionModel {
    LinearRegression,
    MovingAverage,
    LSTM,
    ARIMA,
}

// =============================================================================
// TRANSACTION TYPES
// =============================================================================

/// Transaction Gas Info
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TransactionGasInfo {
    pub gas_limit: u64,
    pub gas_used: u64,
    pub gas_price: u64,
    pub effective_gas_price: u64,
    pub max_fee_per_gas: u64,
    pub max_priority_fee_per_gas: u64,
    pub total_cost: u64,
    pub burned: u64,  // EIP-1559 burned
}

/// Gas Spender
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GasSpender {
    pub address: String,
    pub total_spent: u64,
    pub total_spent_usd: f64,
    pub tx_count: i64,
    pub average_gas_price: u64,
}

// =============================================================================
// ANALYTICS
// =============================================================================

/// Gas Analytics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GasAnalytics {
    pub average_gas_price: u64,
    pub median_gas_price: u64,
    pub min_gas_price: u64,
    pub max_gas_price: u64,
    pub total_gas_used: u64,
    pub total_fees: u64,
    pub total_fees_usd: f64,
    pub burned_amount: u64,
    pub period: AnalyticsPeriod,
}

/// Analytics Period
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum AnalyticsPeriod {
    Hour,
    Day,
    Week,
    Month,
}

// =============================================================================
// CONFIG
// =============================================================================

/// Gas Config
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GasConfig {
    pub chain_id: u64,
    pub update_interval: u64,  // seconds
    pub history_size: usize,
    pub prediction_enabled: bool,
    pub fast_confidence: f64,
    pub normal_confidence: f64,
    pub slow_confidence: f64,
    /// JSON-RPC endpoint used to fetch live gas prices (eth_gasPrice, blocks).
    pub rpc_url: String,
}

impl Default for GasConfig {
    fn default() -> Self {
        Self {
            chain_id: 9001,
            update_interval: 15,
            history_size: 1000,
            prediction_enabled: true,
            fast_confidence: 95.0,
            normal_confidence: 80.0,
            slow_confidence: 60.0,
            rpc_url: String::new(),
        }
    }
}

// =============================================================================
// HELPERS
// =============================================================================

/// Convert Wei to Gwei
pub fn wei_to_gwei(wei: u64) -> f64 {
    wei as f64 / 1_000_000_000.0
}

/// Convert Gwei to Wei
pub fn gwei_to_wei(gwei: f64) -> u64 {
    (gwei * 1_000_000_000.0) as u64
}

/// Convert Wei to ETH
pub fn wei_to_eth(wei: u64) -> f64 {
    wei as f64 / 1_000_000_000_000_000_000.0
}

/// Calculate gas cost
pub fn calculate_gas_cost(gas_used: u64, gas_price: u64) -> u64 {
    gas_used * gas_price
}

/// Calculate EIP-1559 max fee
pub fn calculate_max_fee(base_fee: u64, priority_fee: u64, multiplier: f64) -> u64 {
    ((base_fee as f64 * multiplier) + priority_fee as f64) as u64
}

// =============================================================================
// TESTS
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_wei_to_gwei() {
        assert!((wei_to_gwei(1_000_000_000) - 1.0).abs() < 0.001);
    }

    #[test]
    fn test_gwei_to_wei() {
        assert_eq!(gwei_to_wei(1.0), 1_000_000_000);
    }

    #[test]
    fn test_calculate_gas_cost() {
        // 21000 gas * 1 Gwei (1e9 wei) = 2.1e13 wei
        assert_eq!(calculate_gas_cost(21000, 1_000_000_000), 21_000_000_000_000);
    }

    #[test]
    fn test_calculate_max_fee() {
        let max = calculate_max_fee(100, 2, 2.0);
        assert_eq!(max, 202);
    }
}