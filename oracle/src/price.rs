//! Price Oracle

use crate::types::*;

// =============================================================================
// ORACLE
// =============================================================================

/// Price Oracle
pub struct PriceOracle {
    config: OracleConfig,
    feeds: std::collections::HashMap<String, PriceFeed>,
}

impl PriceOracle {
    pub fn new(config: OracleConfig) -> Self {
        Self {
            config,
            feeds: std::collections::HashMap::new(),
        }
    }

    /// Update price
    pub fn update(&mut self, token: &str, price: f64) {
        let feed = PriceFeed {
            address: token.to_string(),
            price,
            updated_at: 0,
            round: 0,
        };
        self.feeds.insert(token.to_string(), feed);
    }

    /// Get price
    pub fn get_price(&self, token: &str) -> Option<f64> {
        self.feeds.get(token).map(|f| f.price)
    }
}

// =============================================================================
// PRICE AGGREGATOR
// =============================================================================

/// Price Aggregator
pub struct PriceAggregator;

impl PriceAggregator {
    pub fn new() -> Self {
        Self
    }

    /// Aggregate prices
    pub fn aggregate(&self, prices: Vec<f64>) -> f64 {
        if prices.is_empty() {
            return 0.0;
        }
        let sum: f64 = prices.iter().sum();
        sum / prices.len() as f64
    }
}

impl Default for PriceAggregator {
    fn default() -> Self {
        Self::new()
    }
}