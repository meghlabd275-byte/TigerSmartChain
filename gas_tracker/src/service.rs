//! Gas Tracker Service Implementation

use crate::types::*;
use chrono::Utc;
use std::sync::Arc;
use tokio::sync::RwLock;
use thiserror::Error;
use reqwest::Client;
use serde_json::{json, Value};
use std::collections::VecDeque;
use std::time::{Duration, Instant};

// =============================================================================
// ERRORS
// =============================================================================

#[derive(Error, Debug)]
pub enum GasError {
    #[error("RPC error: {0}")]
    RPCError(String),
    #[error("Parse error: {0}")]
    ParseError(String),
    #[error("Data error: {0}")]
    DataError(String),
}

// =============================================================================
// GAS TRACKER
// =============================================================================

/// Historical Gas Tracker Service
pub struct GasTracker {
    config: GasTrackerConfig,
    client: Client,
    cache: Arc<RwLock<Option<(GasPrices, Instant)>>>,
    history: Arc<RwLock<VecDeque<GasHistoryPoint>>>,
}

impl GasTracker {
    /// Create new gas tracker
    pub fn new(rpc_url: &str) -> Self {
        let config = GasTrackerConfig {
            rpc_url: rpc_url.to_string(),
            ..Default::default()
        };
        
        let client = Client::builder()
            .timeout(Duration::from_secs(30))
            .build()
            .unwrap_or_else(|_| Client::new());
        
        Self {
            config: config.clone(),
            client,
            cache: Arc::new(RwLock::new(None)),
            history: Arc::new(RwLock::new(VecDeque::with_capacity(10000))),
        }
    }

    /// Get current gas prices
    pub async fn get_gas_prices(&self) -> Result<GasPrices, GasError> {
        // Check cache
        {
            let cache = self.cache.read().await;
            if let Some((prices, cached_at)) = cache.as_ref() {
                if cached_at.elapsed() < Duration::from_secs(self.config.cache_duration_secs) {
                    return Ok(prices.clone());
                }
            }
        }

        // Fetch from RPC
        let prices = self.fetch_gas_prices().await?;

        // Update cache
        {
            let mut cache = self.cache.write().await;
            *cache = Some((prices.clone(), Instant::now()));
        }

        Ok(prices)
    }

    /// Fetch gas prices from RPC
    async fn fetch_gas_prices(&self) -> Result<GasPrices, GasError> {
        // Get block to get base fee
        let block_request = json!({
            "jsonrpc": "2.0",
            "method": "eth_getBlockByNumber",
            "params": ["latest", false],
            "id": 1
        });

        let response = self.client
            .post(&self.config.rpc_url)
            .json(&block_request)
            .send()
            .await
            .map_err(|e| GasError::RPCError(e.to_string()))?
            .json::<Value>()
            .await
            .map_err(|e| GasError::ParseError(e.to_string()))?;

        let result = response.get("result")
            .ok_or_else(|| GasError::RPCError("No result".to_string()))?;

        // Parse base fee
        let base_fee = result.get("baseFeePerGas")
            .and_then(|v| v.as_str())
            .map(|s| self.parse_hex_gwei(s))
            .unwrap_or(20.0);

        // Get gas price
        let gas_price_request = json!({
            "jsonrpc": "2.0",
            "method": "eth_gasPrice",
            "params": [],
            "id": 2
        });

        let gas_response = self.client
            .post(&self.config.rpc_url)
            .json(&gas_price_request)
            .send()
            .await
            .map_err(|e| GasError::RPCError(e.to_string()))?
            .json::<Value>()
            .await
            .map_err(|e| GasError::ParseError(e.to_string()))?;

        let gas_price = gas_response.get("result")
            .and_then(|v| v.as_str())
            .map(|s| self.parse_hex_gwei(s))
            .unwrap_or(30.0);

        // Calculate prices based on base fee
        let priority_fee = (gas_price - base_fee).max(1.0);
        
        let prices = GasPrices {
            slow: (base_fee + priority_fee * 0.5).max(1.0),
            standard: base_fee + priority_fee,
            fast: (base_fee + priority_fee * 2.0).max(base_fee * 1.5),
            base_fee,
            priority_fee,
            last_updated: Utc::now().timestamp(),
        };

        // Add to history
        self.add_history_point(GasHistoryPoint {
            timestamp: Utc::now().timestamp(),
            average_gas_price: gas_price,
            median_gas_price: gas_price,
            min_gas_price: gas_price * 0.8,
            max_gas_price: gas_price * 1.2,
            tx_count: 0,
            total_gas_used: 0,
            avg_gas_used: 21000.0,
        }).await;

        Ok(prices)
    }

    /// Parse hex to Gwei
    fn parse_hex_gwei(&self, hex: &str) -> f64 {
        let value = u64::from_str_radix(hex.trim_start_matches("0x"), 16).unwrap_or(0);
        value as f64 / 1e9 // Convert wei to Gwei
    }

    /// Get historical gas data
    pub async fn get_history(&self, days: u32) -> Result<GasHistory, GasError> {
        let history = self.history.read().await;
        let now = Utc::now().timestamp();
        let cutoff = now - (days as i64 * 86400);

        let data: Vec<GasHistoryPoint> = history
            .iter()
            .filter(|p| p.timestamp >= cutoff)
            .cloned()
            .collect();

        let start_time = data.first().map(|p| p.timestamp).unwrap_or(cutoff);
        let end_time = data.last().map(|p| p.timestamp).unwrap_or(now);

        Ok(GasHistory {
            data,
            start_time,
            end_time,
        })
    }

    /// Add history point
    async fn add_history_point(&self, point: GasHistoryPoint) {
        let mut history = self.history.write().await;
        
        // Keep only last N points
        while history.len() > 10000 {
            history.pop_front();
        }
        
        history.push_back(point);
    }

    /// Get fee market data
    pub async fn get_fee_market(&self) -> Result<FeeMarket, GasError> {
        let block_request = json!({
            "jsonrpc": "2.0",
            "method": "eth_getBlockByNumber",
            "params": ["latest", false],
            "id": 1
        });

        let response = self.client
            .post(&self.config.rpc_url)
            .json(&block_request)
            .send()
            .await
            .map_err(|e| GasError::RPCError(e.to_string()))?
            .json::<Value>()
            .await
            .map_err(|e| GasError::ParseError(e.to_string()))?;

        let result = response.get("result")
            .ok_or_else(|| GasError::RPCError("No result".to_string()))?;

        let base_fee = result.get("baseFeePerGas")
            .and_then(|v| v.as_str())
            .map(|s| self.parse_hex_gwei(s))
            .unwrap_or(20.0);

        let gas_limit = result.get("gasLimit")
            .and_then(|v| v.as_str())
            .map(|s| u64::from_str_radix(s.trim_start_matches("0x"), 16).unwrap_or(15000000))
            .unwrap_or(15000000);

        let gas_used = result.get("gasUsed")
            .and_then(|v| v.as_str())
            .map(|s| u64::from_str_radix(s.trim_start_matches("0x"), 16).unwrap_or(0))
            .unwrap_or(0);

        let gas_used_percent = (gas_used as f64 / gas_limit as f64) * 100.0;
        
        // Estimate next base fee
        let base_fee_next = if gas_used_percent > 50.0 {
            base_fee * 1.125 // Increase by 12.5%
        } else if gas_used_percent < 50.0 {
            base_fee * 0.875 // Decrease by 12.5%
        } else {
            base_fee
        };

        Ok(FeeMarket {
            base_fee,
            base_fee_next,
            gas_target: gas_limit / 2,
            gas_used,
            gas_used_percent,
            priority_fee: 2.0,
            priority_fee_max: 10.0,
            priority_fee_max_sender: 20.0,
        })
    }

    /// Get gas analytics
    pub async fn get_analytics(&self) -> Result<GasAnalytics, GasError> {
        let history = self.history.read().await;
        
        let prices: Vec<f64> = history.iter()
            .map(|p| p.average_gas_price)
            .collect();

        if prices.is_empty() {
            return Ok(GasAnalytics {
                avg_price_24h: 30.0,
                avg_price_7d: 30.0,
                avg_price_30d: 30.0,
                volatility: 0.2,
                mode_price: 30.0,
                tps: 15.0,
                utilization: 0.5,
            });
        }

        let avg = |v: &[f64]| -> f64 {
            if v.is_empty() { 0.0 } else { v.iter().sum::<f64>() / v.len() as f64 }
        };

        let now = Utc::now().timestamp();
        let day_ago = now - 86400;
        let week_ago = now - 604800;
        let month_ago = now - 2592000;

        let prices_24h: Vec<f64> = history.iter()
            .filter(|p| p.timestamp >= day_ago)
            .map(|p| p.average_gas_price)
            .collect();

        let prices_7d: Vec<f64> = history.iter()
            .filter(|p| p.timestamp >= week_ago)
            .map(|p| p.average_gas_price)
            .collect();

        let prices_30d: Vec<f64> = history.iter()
            .filter(|p| p.timestamp >= month_ago)
            .map(|p| p.average_gas_price)
            .collect();

        // Calculate volatility (standard deviation / mean)
        let avg_price = avg(&prices);
        let variance = prices.iter()
            .map(|p| (p - avg_price).powi(2))
            .sum::<f64>() / prices.len() as f64;
        let volatility = (variance.sqrt() / avg_price).min(1.0);

        // Get TPS from history
        let tps = history.iter()
            .map(|p| p.tx_count as f64 / 12.0) // Assuming ~12 second block time
            .sum::<f64>() / history.len().max(1) as f64;

        // Get utilization
        let utilization = history.iter()
            .map(|p| p.total_gas_used as f64 / 15000000.0) // Approximate gas limit
            .sum::<f64>() / history.len().max(1) as f64;

        Ok(GasAnalytics {
            avg_price_24h: avg(&prices_24h),
            avg_price_7d: avg(&prices_7d),
            avg_price_30d: avg(&prices_30d),
            volatility,
            mode_price: avg_price,
            tps: tps.min(100.0), // Cap at reasonable value
            utilization: utilization.min(1.0),
        })
    }

    /// Get burn data (EIP-1559)
    pub async fn get_burn_data(&self) -> Result<BurnData, GasError> {
        // In production, this would track actual burn from blocks
        Ok(BurnData {
            total_burned: "123456789012345678901".to_string(), // Placeholder
            burn_rate: 100.5, // ETH per day
            block_number: 0,
            timestamp: Utc::now().timestamp(),
        })
    }
}