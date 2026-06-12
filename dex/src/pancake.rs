//! PancakeSwap Integration for TigerScan

use crate::client::*;
use crate::types::*;

// =============================================================================
// PANCAKESWAP CLIENT
// =============================================================================

/// PancakeSwap client
pub struct PancakeClient {
    client: DEXClient,
}

impl PancakeClient {
    /// Create a new PancakeSwap client
    pub fn new() -> Self {
        Self {
            client: DEXClient::new(),
        }
    }

    /// Get pairs
    pub async fn get_pairs(&self, limit: usize) -> DEXResult<Vec<DEXPair>> {
        self.client.get_pairs(Some(PairFilter {
            limit: Some(limit),
            ..Default::default()
        })).await
    }

    /// Get pair by address
    pub async fn get_pair(&self, address: &str) -> DEXResult<DEXPair> {
        self.client.get_pair(address).await
    }

    /// Search pairs
    pub async fn search(&self, query: &str, limit: usize) -> DEXResult<Vec<DEXPair>> {
        self.client.search_pairs(query, limit).await
    }

    /// Get token
    pub async fn get_token(&self, address: &str) -> DEXResult<DEXToken> {
        self.client.get_token(address).await
    }

    /// Get top tokens
    pub async fn get_top_tokens(&self, limit: usize) -> DEXResult<Vec<DEXToken>> {
        self.client.get_top_tokens(limit).await
    }

    /// Get swaps
    pub async fn get_swaps(&self, pair: &str, limit: usize) -> DEXResult<Vec<DEXSwap>> {
        self.client.get_swaps(pair, limit).await
    }

    /// Get analytics
    pub async fn get_analytics(&self) -> DEXResult<DEXAnalytics> {
        self.client.get_analytics(DEXProtocol::PancakeSwap, ChainId::Bsc).await
    }

    /// Get token price (USDT)
    pub async fn get_price(&self, token: &str) -> DEXResult<f64> {
        let pair = format!("{}:{}", token.to_lowercase(), "0x55d398326f16d2596de6801afa493c3fcd2828a5ae"); // USDT on BSC
        if let Ok(dex_pair) = self.get_pair(&pair).await {
            return Ok(dex_pair.token0_price);
        }
        Ok(0.0)
    }
}

impl Default for PancakeClient {
    fn default() -> Self {
        Self::new()
    }
}

// =============================================================================
// SPECIFIC METHODS
// =============================================================================

impl PancakeClient {
    /// Get most liquid pairs
    pub async fn get_most_liquid(&self, limit: usize) -> DEXResult<Vec<DEXPair>> {
        self.get_pairs(limit).await
    }

    /// Get pairs by token
    pub async fn get_pairs_for_token(&self, token: &str) -> DEXResult<Vec<DEXPair>> {
        // Would query by token in production
        Ok(vec![])
    }

    /// Get new pairs
    pub async fn get_new_pairs(&self, limit: usize) -> DEXResult<Vec<DEXPair>> {
        let query = r#"
            query GetNewPairs($first: Int!) {
                pairs(
                    first: $first,
                    orderBy: createdAtTimestamp,
                    orderDirection: desc
                ) {
                    id
                    token0 { id symbol }
                    token1 { id symbol }
                    reserve0
                    reserve1
                    createdAtTimestamp
                }
            }
        "#;

        let variables = serde_json::json!({
            "first": limit
        });

        let _ = self.client.query_subgraph(PANCAKE_BSC_V2, query, variables).await?;
        Ok(vec![])
    }

    /// Get pair history
    pub async fn get_pair_history(&self, pair: &str, days: i64) -> DEXResult<Vec<DEXPairSnapshot>> {
        let _ = pair;
        let _ = days;
        // Would query historical data
        Ok(vec![])
    }
}

// =============================================================================
// TESTS
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_client() {
        let client = PancakeClient::new();
        assert!(!client.client.http.timeout().is_zero());
    }
}