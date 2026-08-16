//! Gas Estimator for TigerScan

use crate::types::*;
use chrono::Utc;
use std::collections::VecDeque;

// =============================================================================
// GAS ESTIMATOR
// =============================================================================

/// Gas Estimator
pub struct GasEstimator {
    config: GasConfig,
    history: VecDeque<GasHistory>,
}

impl GasEstimator {
    /// Create new estimator
    pub fn new(config: GasConfig) -> Self {
        let history_size = config.history_size;
        Self {
            config,
            history: VecDeque::with_capacity(history_size),
        }
    }

    /// Update history
    pub fn update(&mut self, gas_price: u64, gas_used: u64, block_number: u64) {
        let entry = GasHistory {
            timestamp: Utc::now().timestamp(),
            gas_price,
            gas_used,
            block_number,
        };

        if self.history.len() >= self.config.history_size {
            self.history.pop_front();
        }
        self.history.push_back(entry);
    }

    /// Get current gas price
    pub fn get_gas_price(&self) -> GasPrice {
        let recent: Vec<_> = self.history.iter().rev().take(10).collect();
        
        if recent.is_empty() {
            return GasPrice {
                low: 1,
                medium: 2,
                high: 5,
                base_fee: 1,
                priority_fee: 1,
                timestamp: Utc::now().timestamp(),
            };
        }

        let prices: Vec<u64> = recent.iter().map(|g| g.gas_price).collect();
        let len = prices.len() as u64;

        GasPrice {
            low: *prices.iter().min().unwrap_or(&1),
            medium: prices.iter().sum::<u64>() / len,
            high: *prices.iter().max().unwrap_or(&5),
            base_fee: prices.iter().sum::<u64>() / len,
            priority_fee: 2,
            timestamp: Utc::now().timestamp(),
        }
    }

    /// Estimate gas for transaction
    pub fn estimate(&self, data_size: usize, speed: TransactionSpeed) -> GasEstimate {
        let gas_price = match speed {
            TransactionSpeed::Fast => self.get_gas_price().high,
            TransactionSpeed::Normal => self.get_gas_price().medium,
            TransactionSpeed::Slow => self.get_gas_price().low,
        };

        // Estimate gas based on data size
        let base_gas: u64 = 21000;
        let data_gas: u64 = (data_size / 68) as u64 * 16; // 16 gas per non-zero byte
        let estimated_gas = base_gas + data_gas;

        let confidence = match speed {
            TransactionSpeed::Fast => self.config.fast_confidence,
            TransactionSpeed::Normal => self.config.normal_confidence,
            TransactionSpeed::Slow => self.config.slow_confidence,
        };

        let estimated_time = match speed {
            TransactionSpeed::Fast => 15,
            TransactionSpeed::Normal => 60,
            TransactionSpeed::Slow => 300,
        };

        GasEstimate {
            gas_price,
            estimated_gas,
            estimated_cost: estimated_gas * gas_price,
            estimated_cost_usd: 0.0, // Would calculate with ETH price
            confidence,
            estimated_time,
        }
    }

    /// Get analytics
    pub fn get_analytics(&self, period: AnalyticsPeriod) -> GasAnalytics {
        let count = match period {
            AnalyticsPeriod::Hour => 60,
            AnalyticsPeriod::Day => 1440,
            AnalyticsPeriod::Week => 10080,
            AnalyticsPeriod::Month => 43200,
        };

        let history: Vec<_> = self.history.iter().rev().take(count).collect();
        
        if history.is_empty() {
            return GasAnalytics {
                average_gas_price: 0,
                median_gas_price: 0,
                min_gas_price: 0,
                max_gas_price: 0,
                total_gas_used: 0,
                total_fees: 0,
                total_fees_usd: 0.0,
                burned_amount: 0,
                period,
            };
        }

        let prices: Vec<u64> = history.iter().map(|g| g.gas_price).collect();
        let gas_used: Vec<u64> = history.iter().map(|g| g.gas_used).collect();

        GasAnalytics {
            average_gas_price: prices.iter().sum::<u64>() / prices.len() as u64,
            median_gas_price: median(&prices),
            min_gas_price: *prices.iter().min().unwrap_or(&0),
            max_gas_price: *prices.iter().max().unwrap_or(&0),
            total_gas_used: gas_used.iter().sum(),
            total_fees: 0, // Would calculate
            total_fees_usd: 0.0,
            burned_amount: 0,
            period,
        }
    }
}

/// Transaction Speed
#[derive(Debug, Clone, Copy)]
pub enum TransactionSpeed {
    Fast,
    Normal,
    Slow,
}

// =============================================================================
// HELPERS
// =============================================================================

fn median(values: &[u64]) -> u64 {
    if values.is_empty() {
        return 0;
    }
    let mut sorted = values.to_vec();
    sorted.sort();
    let len = sorted.len();
    if len % 2 == 0 {
        (sorted[len / 2 - 1] + sorted[len / 2]) / 2
    } else {
        sorted[len / 2]
    }
}

// =============================================================================
// TESTS
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_estimator() {
        let estimator = GasEstimator::new(GasConfig::default());
        let price = estimator.get_gas_price();
        
        assert!(price.medium > 0);
    }

    #[test]
    fn test_estimate() {
        let mut estimator = GasEstimator::new(GasConfig::default());
        
        // Add some history
        for i in 0..10 {
            estimator.update(10 + i as u64, 50000, i);
        }

        let estimate = estimator.estimate(100, TransactionSpeed::Fast);
        
        assert!(estimate.estimated_gas > 0);
        assert!(estimate.gas_price > 0);
    }

    #[test]
    fn test_median() {
        let values = vec![1, 2, 3, 4, 5];
        assert_eq!(median(&values), 3);
        
        let values = vec![1, 2, 3, 4];
        assert_eq!(median(&values), 2);
    }
}