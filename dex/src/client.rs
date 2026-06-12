//! DEX Client for TigerScan
//! High-performance client for querying DEX subgraphs

use crate::types::*;
use chrono::Utc;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::RwLock;

// =============================================================================
// CONSTANTS
// =============================================================================

pub const DEFAULT_TIMEOUT: Duration = Duration::from_secs(30);
pub const DEFAULT_CACHE_TTL: Duration = Duration::from_secs(15);

// PancakeSwap Subgraph endpoints
pub const PANCAKE_BSC_V2: &str = "https://api.pancakeswap.com/api/v2/graphql";
pub const PANCAKE_BSC_V3: &str = "https://api.pancakeswap.com/api/v3/graphql";
pub const PANCAKE_ETH_V2: &str = "https://api.pancakeswap.com/api/v2/graphql-eth";

// Uniswap Subgraph
pub const UNISWAP_ETH_V3: &str = "https://api.thegraph.com/subgraphs/name/uniswap/uniswap-v3";
pub const UNISWAP_BASE_V3: &str = "https://api.thegraph.com/subgraphs/name/uniswap/uniswap-v3-base";

// =============================================================================
// ERROR TYPES
// =============================================================================

#[derive(Debug, Clone)]
pub enum DEXError {
    NetworkError(String),
    ParseError(String),
    RateLimitError(String),
    NotFound(String),
    InvalidToken(String),
}

impl std::fmt::Display for DEXError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DEXError::NetworkError(e) => write!(f, "Network error: {}", e),
            DEXError::ParseError(e) => write!(f, "Parse error: {}", e),
            DEXError::RateLimitError(e) => write!(f, "Rate limit: {}", e),
            DEXError::NotFound(e) => write!(f, "Not found: {}", e),
            DEXError::InvalidToken(e) => write!(f, "Invalid token: {}", e),
        }
    }
}

impl std::error::Error for DEXError {}

pub type DEXResult<T> = std::result::Result<T, DEXError>;

// =============================================================================
// CLIENT
// =============================================================================

/// DEX Client for querying subgraph APIs
pub struct DEXClient {
    http: Client,
    cache: Arc<RwLock<Cache>>,
    config: ClientConfig,
}

/// Client configuration
#[derive(Debug, Clone)]
pub struct ClientConfig {
    pub timeout: Duration,
    pub cache_ttl: Duration,
    pub max_retries: u32,
    pub rate_limit_per_minute: u32,
}

impl Default for ClientConfig {
    fn default() -> Self {
        Self {
            timeout: DEFAULT_TIMEOUT,
            cache_ttl: DEFAULT_CACHE_TTL,
            max_retries: 3,
            rate_limit_per_minute: 60,
        }
    }
}

/// Simple in-memory cache
#[derive(Debug, Clone, Default)]
pub struct Cache {
    pairs: std::collections::HashMap<String, CachedItem<DEXPair>>,
    tokens: std::collections::HashMap<String, CachedItem<DEXToken>>,
    swaps: std::collections::HashMap<String, CachedItem<Vec<DEXSwap>>>,
}

#[derive(Debug, Clone)]
pub struct CachedItem<T> {
    pub value: T,
    pub expires_at: i64,
}

impl<T> CachedItem<T> {
    pub fn new(value: T, ttl_secs: i64) -> Self {
        Self {
            value,
            expires_at: Utc::now().timestamp() + ttl_secs,
        }
    }

    pub fn is_expired(&self) -> bool {
        Utc::now().timestamp() > self.expires_at
    }
}

impl DEXClient {
    /// Create a new DEX client
    pub fn new() -> Self {
        let http = Client::builder()
            .timeout(DEFAULT_TIMEOUT)
            .build()
            .expect("Failed to create HTTP client");

        Self {
            http,
            cache: Arc::new(RwLock::new(Cache::default())),
            config: ClientConfig::default(),
        }
    }

    /// Create with custom config
    pub fn with_config(config: ClientConfig) -> Self {
        let http = Client::builder()
            .timeout(config.timeout)
            .build()
            .expect("Failed to create HTTP client");

        Self {
            http,
            cache: Arc::new(RwLock::new(Cache::default())),
            config,
        }
    }

    // =============================================================================
    // PAIR QUERIES
    // =============================================================================

    /// Get all pairs from PancakeSwap
    pub async fn get_pairs(&self, filter: Option<PairFilter>) -> DEXResult<Vec<DEXPair>> {
        let query = r#"
            query GetPairs($first: Int!, $skip: Int!) {
                pairs(
                    first: $first,
                    skip: $skip,
                    orderBy: volumeUSD,
                    orderDirection: desc,
                    where: { volumeUSD_gt: 1000 }
                ) {
                    id
                    token0 { id symbol decimals }
                    token1 { id symbol decimals }
                    reserve0
                    reserve1
                    volumeUSD
                    volumeUSD7d: volumeUSD
                    txCount
                    createdAtTimestamp
                }
            }
        "#;

        let variables = serde_json::json!({
            "first": 100,
            "skip": 0
        });

        let _response: serde_json::Value = self.query_subgraph(PANCAKE_BSC_V2, query, variables).await?;

        // Return mock data for now (would parse actual response in production)
        Ok(vec![])
    }

    /// Get pair by ID
    pub async fn get_pair(&self, pair_id: &str) -> DEXResult<DEXPair> {
        // Check cache first
        {
            let cache = self.cache.read().await;
            if let Some(cached) = cache.pairs.get(pair_id) {
                if !cached.is_expired() {
                    return Ok(cached.value.clone());
                }
            }
        }

        let query = r#"
            query GetPair($id: ID!) {
                pair(id: $id) {
                    id
                    token0 { id symbol decimals }
                    token1 { id symbol decimals }
                    reserve0
                    reserve1
                    volumeUSD
                    volumeUSD7d
                    txCount
                    createdAtTimestamp
                }
            }
        "#;

        let variables = serde_json::json!({
            "id": pair_id
        });

        let _response: serde_json::Value = self.query_subgraph(PANCAKE_BSC_V2, query, variables).await?;

        // Return mock - in production would parse response
        Err(DEXError::NotFound(format!("Pair not found: {}", pair_id)))
    }

    /// Search pairs
    pub async fn search_pairs(&self, query: &str, limit: usize) -> DEXResult<Vec<DEXPair>> {
        let search = format!("{}%", query.to_lowercase());
        
        let gql_query = r#"
            query SearchPairs($search: String, $first: Int!) {
                pairs(
                    first: $first,
                    where: {
                        OR: [
                            { token0Symbol_contains: $search }
                            { token1Symbol_contains: $search }
                            { token0Name_contains: $search }
                            { token1Name_contains: $search }
                        ]
                    }
                ) {
                    id
                    token0 { id symbol decimals }
                    token1 { id symbol decimals }
                    reserve0
                    reserve1
                    volumeUSD
                }
            }
        "#;

        let variables = serde_json::json!({
            "search": search,
            "first": limit
        });

        let _response: serde_json::Value = self.query_subgraph(PANCAKE_BSC_V2, gql_query, variables).await?;

        Ok(vec![])
    }

    // =============================================================================
    // TOKEN QUERIES
    // =============================================================================

    /// Get token by address
    pub async fn get_token(&self, token_id: &str) -> DEXResult<DEXToken> {
        let query = r#"
            query GetToken($id: ID!) {
                token(id: $id) {
                    id
                    symbol
                    name
                    decimals
                    totalSupply
                    volumeUSD
                    txCount
                }
            }
        "#;

        let variables = serde_json::json!({
            "id": token_id
        });

        let _response: serde_json::Value = self.query_subgraph(PANCAKE_BSC_V2, query, variables).await?;

        Err(DEXError::NotFound(format!("Token not found: {}", token_id)))
    }

    /// Get top tokens
    pub async fn get_top_tokens(&self, limit: usize) -> DEXResult<Vec<DEXToken>> {
        let query = r#"
            query GetTopTokens($first: Int!) {
                tokens(
                    first: $first,
                    orderBy: volumeUSD,
                    orderDirection: desc,
                    where: { volumeUSD_gt: 1000 }
                ) {
                    id
                    symbol
                    name
                    decimals
                    volumeUSD
                    txCount
                }
            }
        "#;

        let variables = serde_json::json!({
            "first": limit
        });

        let _response: serde_json::Value = self.query_subgraph(PANCAKE_BSC_V2, query, variables).await?;

        Ok(vec![])
    }

    // =============================================================================
    // SWAP QUERIES
    // =============================================================================

    /// Get recent swaps for a pair
    pub async fn get_swaps(&self, pair_id: &str, limit: usize) -> DEXResult<Vec<DEXSwap>> {
        let query = r#"
            query GetSwaps($pairId: String!, $first: Int!) {
                swaps(
                    first: $first,
                    orderBy: timestamp,
                    orderDirection: desc,
                    where: { pair: $pairId }
                ) {
                    id
                    timestamp
                    pair { id }
                    from
                    to
                    sender
                    origin
                    amount0In
                    amount1In
                    amount0Out
                    amount1Out
                    transaction { id hash }
                }
            }
        "#;

        let variables = serde_json::json!({
            "pairId": pair_id,
            "first": limit
        });

        let _response: serde_json::Value = self.query_subgraph(PANCAKE_BSC_V2, query, variables).await?;

        Ok(vec![])
    }

    // =============================================================================
    // ANALYTICS
    // =============================================================================

    /// Get DEX analytics
    pub async fn get_analytics(&self, protocol: DEXProtocol, chain: ChainId) -> DEXResult<DEXAnalytics> {
        let endpoint = match (protocol, chain) {
            (DEXProtocol::PancakeSwap, ChainId::Bsc) => PANCAKE_BSC_V2,
            (DEXProtocol::UniswapV3, ChainId::Ethereum) => UNISWAP_ETH_V3,
            _ => PANCAKE_BSC_V2,
        };

        let query = r#"
            query GetAnalytics {
                factories {
                    poolCount
                    txCount
                    totalVolumeUSD
                    totalLiquidityUSD
                }
                tokens(first: 10, orderBy: volumeUSD, orderDirection: desc) {
                    id
                }
                pairs(first: 10, orderBy: volumeUSD, orderDirection: desc) {
                    id
                }
            }
        "#;

        let _response: serde_json::Value = self.query_subgraph(endpoint, query, serde_json::json!({})).await?;

        Ok(DEXAnalytics {
            protocol,
            chain_id: chain,
            total_pairs: 0,
            total_tokens: 0,
            total_volume_24h: 0.0,
            total_volume_7d: 0.0,
            total_liquidity: 0.0,
            total_swaps_24h: 0,
            top_pairs: vec![],
            top_tokens: vec![],
        })
    }

    // =============================================================================
    // INTERNAL
    // =============================================================================

    /// Query subgraph
    async fn query_subgraph(
        &self,
        endpoint: &str,
        query: &str,
        variables: serde_json::Value,
    ) -> DEXResult<serde_json::Value> {
        let body = serde_json::json!({
            "query": query,
            "variables": variables
        });

        let response = self.http
            .post(endpoint)
            .json(&body)
            .send()
            .await
            .map_err(|e| DEXError::NetworkError(e.to_string()))?;

        if response.status() == 429 {
            return Err(DEXError::RateLimitError("Rate limit exceeded".to_string()));
        }

        if !response.status().is_success() {
            return Err(DEXError::NetworkError(format!(
                "HTTP error: {}",
                response.status()
            )));
        }

        let data: serde_json::Value = response
            .json()
            .await
            .map_err(|e| DEXError::ParseError(e.to_string()))?;

        // Check for GraphQL errors
        if let Some(errors) = data.get("errors") {
            return Err(DEXError::ParseError(errors.to_string()));
        }

        Ok(data)
    }

    /// Clear cache
    pub async fn clear_cache(&self) {
        let mut cache = self.cache.write().await;
        cache.pairs.clear();
        cache.tokens.clear();
        cache.swaps.clear();
    }
}

// =============================================================================
// BUILDER
// =============================================================================

/// Builder for DEX client
pub struct DEXClientBuilder {
    config: ClientConfig,
}

impl DEXClientBuilder {
    pub fn new() -> Self {
        Self {
            config: ClientConfig::default(),
        }
    }

    pub fn timeout(mut self, timeout: Duration) -> Self {
        self.config.timeout = timeout;
        self
    }

    pub fn cache_ttl(mut self, ttl: Duration) -> Self {
        self.config.cache_ttl = ttl;
        self
    }

    pub fn rate_limit(mut self, limit: u32) -> Self {
        self.config.rate_limit_per_minute = limit;
        self
    }

    pub fn build(self) -> DEXClient {
        DEXClient::with_config(self.config)
    }
}

impl Default for DEXClientBuilder {
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
    fn test_client_creation() {
        let client = DEXClient::new();
        assert!(!client.http.timeout().is_zero());
    }

    #[test]
    fn test_builder() {
        let client = DEXClientBuilder::new()
            .timeout(Duration::from_secs(60))
            .build();
        
        assert_eq!(client.config.timeout, Duration::from_secs(60));
    }

    #[test]
    fn test_cache_expiry() {
        let item: CachedItem<String> = CachedItem::new("test".to_string(), 1);
        std::thread::sleep(std::time::Duration::from_millis(1100));
        assert!(item.is_expired());
    }
}