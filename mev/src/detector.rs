//! MEV Detector for TigerScan

use crate::types::*;
use std::collections::HashMap;

// =============================================================================
// MEV DETECTOR
// =============================================================================

/// MEV Detector
pub struct MEVDetector {
    config: MEVConfig,
    detected: HashMap<String, MEVTransaction>,
    searchers: HashMap<String, MEVSearcher>,
}

impl MEVDetector {
    /// Create new detector
    pub fn new(config: MEVConfig) -> Self {
        Self {
            config,
            detected: HashMap::new(),
            searchers: HashMap::new(),
        }
    }

    /// Detect MEV in transaction
    pub fn detect(&mut self, tx: &str, data: &str) -> Option<MEVType> {
        let data_lower = data.to_lowercase();
        
        // Check for common MEV patterns
        if data_lower.contains("0x095ea7b3") // approve
            || data_lower.contains("0x23b872dd") // transferFrom
            || data_lower.contains("0xa9059cbb") // transfer
        {
            // Could be arbitrage or sandwich
            return Some(MEVType::Arbitrage);
        }
        
        if data_lower.contains("0x3fd5a3d") // liquidate
            || data_lower.contains("0x00dc318c") // absorb
        {
            return Some(MEVType::Liquidation);
        }
        
        // Check for flash loan patterns
        if data_lower.contains("0x890e8a6f") // flash
            || data_lower.contains("0x5c60da1b") // flashLoan
        {
            return Some(MEVType::Arbitrage);
        }
        
        None
    }

    /// Analyze block for MEV
    pub fn analyze_block(&self, transactions: Vec<String>) -> MEVBlock {
        let mut txs = Vec::new();
        let mut arbitrage = 0;
        let mut liquidation = 0;
        let mut sandwich = 0;
        let mut frontrun = 0;
        let mut backrun = 0;

        for tx in transactions {
            if let Some(detected_tx) = self.detected.get(&tx) {
                txs.push(detected_tx.clone());
                
                match detected_tx.mev_type {
                    MEVType::Arbitrage => arbitrage += 1,
                    MEVType::Liquidation => liquidation += 1,
                    MEVType::Sandwich => sandwich += 1,
                    MEVType::FrontRun => frontrun += 1,
                    MEVType::BackRun => backrun += 1,
                    _ => {}
                }
            }
        }

        let total_profit: u64 = txs.iter().map(|t| t.profit).sum();

        MEVBlock {
            block_number: 0,
            total_profit,
            transactions: txs,
            mev_type_breakdown: MEVBreakdown {
                arbitrage_count: arbitrage,
                liquidation_count: liquidation,
                sandwich_count: sandwich,
                frontrun_count: frontrun,
                backrun_count: backrun,
            },
        }
    }

    /// Get analytics
    pub fn get_analytics(&self, period: AnalyticsPeriod) -> MEVAnalytics {
        let mut searchers: Vec<_> = self.searchers.values().cloned().collect();
        searchers.sort_by(|a, b| b.total_profit.cmp(&a.total_profit));
        searchers.truncate(10);

        MEVAnalytics {
            total_profit: 0,
            total_transactions: 0,
            by_type: MEVBreakdown::default(),
            top_searchers: searchers,
            period,
        }
    }
}

impl Default for MEVDetector {
    fn default() -> Self {
        Self::new(MEVConfig::default())
    }
}

// =============================================================================
// TESTS
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_detector() {
        let detector = MEVDetector::new(MEVConfig::default());
        
        // Test detection
        let result = detector.detect(
            "0x1234",
            "0x095ea7b3000000000000000000000000000000000000000000000000000000000"
        );
        
        assert!(result.is_some());
    }
}