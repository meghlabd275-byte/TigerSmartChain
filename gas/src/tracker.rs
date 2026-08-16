//! Gas Tracker for TigerScan

use crate::estimator::*;
use crate::types::*;
use std::sync::Arc;
use tokio::sync::RwLock;

// =============================================================================
// GAS TRACKER
// =============================================================================

/// Gas Tracker
pub struct GasTracker {
    estimator: Arc<RwLock<GasEstimator>>,
    running: Arc<RwLock<bool>>,
    rpc_url: String,
    http: Arc<reqwest::Client>,
}

impl GasTracker {
    /// Create new tracker
    pub fn new(config: GasConfig) -> Self {
        let rpc_url = config.rpc_url.clone();
        Self {
            estimator: Arc::new(RwLock::new(GasEstimator::new(config))),
            running: Arc::new(RwLock::new(false)),
            rpc_url,
            http: Arc::new(
                reqwest::Client::builder()
                    .timeout(std::time::Duration::from_secs(10))
                    .build()
                    .expect("failed to build HTTP client"),
            ),
        }
    }

    /// Start tracking
    pub async fn start(&self) {
        *self.running.write().await = true;
        
        while self.is_running().await {
            // Fetch latest gas prices from RPC
            self.update_gas_prices().await;
            
            tokio::time::sleep(tokio::time::Duration::from_secs(15)).await;
        }
    }

    /// Stop tracking
    pub async fn stop(&self) {
        *self.running.write().await = false;
    }

    /// Is running
    pub async fn is_running(&self) -> bool {
        *self.running.read().await
    }

    /// Get current gas price
    pub async fn get_gas_price(&self) -> GasPrice {
        self.estimator.read().await.get_gas_price()
    }

    /// Estimate transaction
    pub async fn estimate(&self, data_size: usize, speed: TransactionSpeed) -> GasEstimate {
        self.estimator.read().await.estimate(data_size, speed)
    }

    /// Get analytics
    pub async fn get_analytics(&self, period: AnalyticsPeriod) -> GasAnalytics {
        self.estimator.read().await.get_analytics(period)
    }

    /// Update gas prices from RPC
    async fn update_gas_prices(&self) {
        if self.rpc_url.is_empty() {
            log::warn!("gas tracker: no rpc_url configured, skipping update");
            return;
        }
        let gas_price_wei = match self.rpc_call::<String>("eth_gasPrice", vec![]).await {
            Ok(v) => hex_to_u64(&v).unwrap_or(0),
            Err(e) => {
                log::warn!("gas tracker: eth_gasPrice failed: {e}");
                return;
            }
        };
        let (block_number, gas_used) = match self.latest_block().await {
            Ok(b) => b,
            Err(e) => {
                log::warn!("gas tracker: eth_getBlockByNumber failed: {e}");
                (0u64, 0u64)
            }
        };
        let gwei = gas_price_wei / 1_000_000_000;
        self.estimator.write().await.update(gwei, gas_used, block_number);
    }

    async fn rpc_call<T: serde::de::DeserializeOwned>(
        &self,
        method: &str,
        params: Vec<serde_json::Value>,
    ) -> Result<T, Box<dyn std::error::Error>> {
        let body = serde_json::json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": method,
            "params": params,
        });
        let resp: serde_json::Value = self
            .http
            .post(&self.rpc_url)
            .json(&body)
            .send()
            .await?
            .json()
            .await?;
        let result = resp
            .get("result")
            .ok_or_else(|| Box::<dyn std::error::Error>::from(format!("JSON-RPC error: {resp}")))?
            .clone();
        serde_json::from_value(result).map_err(|e| Box::<dyn std::error::Error>::from(e))
    }

    async fn latest_block(&self) -> Result<(u64, u64), Box<dyn std::error::Error>> {
        let block_num_hex = self.rpc_call::<String>("eth_blockNumber", vec![]).await?;
        let block_number = hex_to_u64(&block_num_hex).unwrap_or(0);
        let block: serde_json::Value = self
            .rpc_call(
                "eth_getBlockByNumber",
                vec![serde_json::Value::String(block_num_hex), serde_json::json!(false)],
            )
            .await?;
        let gas_used = block
            .get("gasUsed")
            .and_then(|v| v.as_str())
            .and_then(hex_to_u64)
            .unwrap_or(0);
        Ok((block_number, gas_used))
    }
}

/// Parse a hex quantity string ("0x...") into u64.
fn hex_to_u64(s: &str) -> Option<u64> {
    let s = s.strip_prefix("0x").unwrap_or(s);
    u64::from_str_radix(s, 16).ok()
}

// =============================================================================
// BUILDER
// =============================================================================

/// Gas Tracker Builder
pub struct GasTrackerBuilder {
    config: GasConfig,
}

impl GasTrackerBuilder {
    pub fn new() -> Self {
        Self {
            config: GasConfig::default(),
        }
    }

    pub fn chain_id(mut self, id: u64) -> Self {
        self.config.chain_id = id;
        self
    }

    pub fn update_interval(mut self, interval: u64) -> Self {
        self.config.update_interval = interval;
        self
    }

    pub fn build(self) -> GasTracker {
        GasTracker::new(self.config)
    }
}

impl Default for GasTrackerBuilder {
    fn default() -> Self {
        Self::new()
    }
}

// =============================================================================
// TESTS
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_tracker_creation() {
        let tracker = GasTracker::new(GasConfig::default());
        assert!(!tracker.is_running().await);
    }

    #[test]
    fn test_builder() {
        let _tracker = GasTrackerBuilder::new()
            .chain_id(56)
            .build();

        // Can't easily test async in unit test
    }
}