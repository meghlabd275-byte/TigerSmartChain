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
}

impl GasTracker {
    /// Create new tracker
    pub fn new(config: GasConfig) -> Self {
        Self {
            estimator: Arc::new(RwLock::new(GasEstimator::new(config))),
            running: Arc::new(RwLock::new(false)),
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
        // Would fetch from RPC in production
        let mock_price = 20;
        let mock_gas = 50000;
        let mock_block = 1000;
        
        self.estimator.write().await.update(mock_price, mock_gas, mock_block);
    }
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

    #[test]
    fn test_tracker_creation() {
        let tracker = GasTracker::new(GasConfig::default());
        assert!(!tracker.is_running().await);
    }

    #[test]
    fn test_builder() {
        let tracker = GasTrackerBuilder::new()
            .chain_id(56)
            .build();
        
        // Can't easily test async in unit test
    }
}