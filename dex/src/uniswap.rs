//! Uniswap Integration for TigerScan

use crate::client::*;
use crate::client::UNISWAP_ETH_V3;
use crate::types::*;

// =============================================================================
// UNISWAP CLIENT
// =============================================================================

/// Uniswap client
pub struct UniswapClient {
    client: DEXClient,
    chain: ChainId,
}

impl UniswapClient {
    /// Create for Ethereum
    pub fn ethereum() -> Self {
        Self {
            client: DEXClient::new(),
            chain: ChainId::Ethereum,
        }
    }

    /// Create for Base
    pub fn base() -> Self {
        Self {
            client: DEXClient::new(),
            chain: ChainId::Base,
        }
    }

    /// Create for specific chain
    pub fn new(chain: ChainId) -> Self {
        Self {
            client: DEXClient::new(),
            chain,
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

    /// Get token price (USDC)
    pub async fn get_price(&self, token: &str) -> DEXResult<f64> {
        let usdc = match self.chain {
            ChainId::Ethereum => "0xa0b86991c6218b36c1d19d4a2e9eb0e3602d24c30", // USDC
            ChainId::Base => "0x833589fcd6e527b83008d7c5e2c5e2c2e2e2e2e2e", // USDC on Base
            _ => "0x55d398326f16d2596de6801afa493c3fcd2828a5ae", // USDT on BSC
        };
        
        let pair = format!("{}:{}", token.to_lowercase(), usdc);
        if let Ok(dex_pair) = self.get_pair(&pair).await {
            return Ok(dex_pair.token0_price);
        }
        Ok(0.0)
    }

    /// Get pairs by token
    pub async fn get_pairs_for_token(&self, token: &str) -> DEXResult<Vec<DEXPair>> {
        let query = r#"
            query GetPairsByToken($token: String!, $first: Int!) {
                pools(
                    first: $first,
                    orderBy: volumeUSD,
                    orderDirection: desc,
                    where: { OR: [{ token0: $token }, { token1: $token }] }
                ) {
                    id
                    token0 { id symbol decimals }
                    token1 { id symbol decimals }
                    totalValueLockedToken0
                    totalValueLockedToken1
                    volumeUSD
                    txCount
                    createdAtTimestamp
                }
            }
        "#;
        let variables = serde_json::json!({ "token": token.to_lowercase(), "first": 100 });
        let response = self.client.query_subgraph(UNISWAP_ETH_V3, query, variables).await?;
        Ok(parse_pairs(&response))
    }

    /// Get analytics
    pub async fn get_analytics(&self) -> DEXResult<DEXAnalytics> {
        self.client.get_analytics(DEXProtocol::UniswapV3, self.chain).await
    }
}

impl Default for UniswapClient {
    fn default() -> Self {
        Self::ethereum()
    }
}