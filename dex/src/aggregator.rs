//! DEX Aggregator for TigerScan
//! Aggregates data from multiple DEXs

use crate::client::*;
use crate::pancake::PancakeClient;
use crate::uniswap::UniswapClient;
use crate::types::*;
use std::collections::HashMap;

// =============================================================================
// AGGREGATOR
// =============================================================================

/// DEX Aggregator - aggregates data from multiple DEXs
pub struct DEXAggregator {
    pancakeswap: PancakeClient,
    uniswap: UniswapClient,
}

impl DEXAggregator {
    /// Create new aggregator
    pub fn new() -> Self {
        Self {
            pancakeswap: PancakeClient::new(),
            uniswap: UniswapClient::ethereum(),
        }
    }

    /// Get best price across DEXs
    pub async fn get_best_price(&self, token_in: &str, token_out: &str) -> DEXResult<BestPrice> {
        let mut results = Vec::new();

        // Get from PancakeSwap
        if let Ok(pairs) = self.pancakeswap.get_pairs_for_token(token_in).await {
            for pair in pairs {
                if pair.token1.to_lowercase() == token_out.to_lowercase() {
                    results.push(PriceSource {
                        protocol: DEXProtocol::PancakeSwap,
                        price: pair.price,
                        liquidity: pair.liquidity_usd,
                    });
                }
            }
        }

        // Get from Uniswap (Ethereum mainnet)
        if let Ok(pairs) = self.uniswap.get_pairs_for_token(token_in).await {
            for pair in pairs {
                if pair.token1.to_lowercase() == token_out.to_lowercase() {
                    results.push(PriceSource {
                        protocol: DEXProtocol::UniswapV2,
                        price: pair.price,
                        liquidity: pair.liquidity_usd,
                    });
                }
            }
        }

        if results.is_empty() {
            return Err(DEXError::NotFound("No pairs found".to_string()));
        }

        // Sort by price
        results.sort_by(|a, b| b.price.partial_cmp(&a.price).unwrap());

        Ok(BestPrice {
            token_in: token_in.to_string(),
            token_out: token_out.to_string(),
            best: results[0].clone(),
            alternatives: results,
        })
    }

    /// Get all pairs across DEXs
    pub async fn get_all_pairs(&self) -> DEXResult<Vec<DEXPair>> {
        let mut all_pairs = Vec::new();

        // Get from PancakeSwap
        if let Ok(pairs) = self.pancakeswap.get_pairs(100).await {
            all_pairs.extend(pairs);
        }

        Ok(all_pairs)
    }

    /// Get analytics from all DEXs
    pub async fn get_analytics(&self) -> DEXResult<AggregatedAnalytics> {
        let mut analytics = HashMap::new();

        // PancakeSwap
        if let Ok(stats) = self.pancakeswap.get_analytics().await {
            analytics.insert("pancakeswap".to_string(), stats);
        }

        // Uniswap
        if let Ok(stats) = self.uniswap.get_analytics().await {
            analytics.insert("uniswap".to_string(), stats);
        }

        Ok(AggregatedAnalytics {
            by_protocol: analytics,
        })
    }
}

impl Default for DEXAggregator {
    fn default() -> Self {
        Self::new()
    }
}

// =============================================================================
// TYPES
// =============================================================================

/// Best price result
#[derive(Debug, Clone)]
pub struct BestPrice {
    pub token_in: String,
    pub token_out: String,
    pub best: PriceSource,
    pub alternatives: Vec<PriceSource>,
}

/// Price source
#[derive(Debug, Clone)]
pub struct PriceSource {
    pub protocol: DEXProtocol,
    pub price: f64,
    pub liquidity: f64,
}

/// Aggregated analytics
#[derive(Debug, Clone)]
pub struct AggregatedAnalytics {
    pub by_protocol: HashMap<String, DEXAnalytics>,
}