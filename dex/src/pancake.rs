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
        let query = r#"
            query GetPairsByToken($token: String!, $first: Int!) {
                pairs(
                    first: $first,
                    orderBy: volumeUSD,
                    orderDirection: desc,
                    where: { OR: [{ token0: $token }, { token1: $token }] }
                ) {
                    id
                    token0 { id symbol decimals }
                    token1 { id symbol decimals }
                    reserve0
                    reserve1
                    volumeUSD
                    txCount
                    createdAtTimestamp
                }
            }
        "#;
        let variables = serde_json::json!({ "token": token.to_lowercase(), "first": 100 });
        let response = self.client.query_subgraph(PANCAKE_BSC_V2, query, variables).await?;
        Ok(parse_pairs(&response))
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
                    token0 { id symbol decimals }
                    token1 { id symbol decimals }
                    reserve0
                    reserve1
                    volumeUSD
                    txCount
                    createdAtTimestamp
                }
            }
        "#;

        let variables = serde_json::json!({
            "first": limit
        });

        let response = self.client.query_subgraph(PANCAKE_BSC_V2, query, variables).await?;
        Ok(parse_pairs(&response))
    }

    /// Get pair history (daily snapshots)
    pub async fn get_pair_history(&self, pair: &str, days: i64) -> DEXResult<Vec<DEXPairSnapshot>> {
        // Fetch daily snapshots over the last `days` days. PancakeSwap v2 subgraph
        // exposes pairDayData keyed by (pair, day).
        let query = r#"
            query GetPairDayData($pair: String!, $days: Int!) {
                pairDayDatas(
                    first: $days,
                    orderBy: date,
                    orderDirection: desc,
                    where: { pairAddress: $pair }
                ) {
                    id
                    date
                    dailyVolumeUSD
                    dailyTxns
                    reserve0
                    reserve1
                    totalSupply
                }
            }
        "#;
        let variables = serde_json::json!({ "pair": pair.to_lowercase(), "days": days as i32 });
        let response = self.client.query_subgraph(PANCAKE_BSC_V2, query, variables).await?;
        let snaps = response
            .pointer("/data/pairDayDatas")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .map(|d| DEXPairSnapshot {
                        id: d.get("id").and_then(|x| x.as_str()).unwrap_or("").to_string(),
                        pair_id: pair.to_string(),
                        timestamp: d.get("date").and_then(|x| x.as_i64()).unwrap_or(0),
                        reserve0: d.get("reserve0").and_then(|x| x.as_str()).unwrap_or("0").to_string(),
                        reserve1: d.get("reserve1").and_then(|x| x.as_str()).unwrap_or("0").to_string(),
                        total_supply: d.get("totalSupply").and_then(|x| x.as_str()).unwrap_or("0").to_string(),
                        volume_usd: d.get("dailyVolumeUSD").and_then(|x| x.as_str()).and_then(|s| s.parse::<f64>().ok()).unwrap_or(0.0),
                        volume_token0: 0.0,
                        volume_token1: 0.0,
                        tx_count: d.get("dailyTxns").and_then(|x| x.as_i64()).unwrap_or(0),
                    })
                    .collect()
            })
            .unwrap_or_default();
        Ok(snaps)
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
        assert!(!client.client.config.timeout.is_zero());
    }
}