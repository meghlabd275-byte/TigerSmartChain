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
    pub(crate) http: Client,
    cache: Arc<RwLock<Cache>>,
    pub(crate) config: ClientConfig,
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
        Utc::now().timestamp() >= self.expires_at
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

        let response: serde_json::Value = self.query_subgraph(PANCAKE_BSC_V2, query, variables).await?;

        let pairs = parse_pairs(&response);
        // Cache and return.
        {
            let mut cache = self.cache.write().await;
            for p in &pairs {
                cache.pairs.insert(p.id.clone(), CachedItem::new(p.clone(), self.config.cache_ttl.as_secs() as i64));
            }
        }
        Ok(pairs)
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

        let response: serde_json::Value = self.query_subgraph(PANCAKE_BSC_V2, query, variables).await?;

        let pair = response
            .pointer("/data/pair")
            .and_then(|v| if v.is_null() { None } else { Some(parse_pair(v)) })
            .ok_or_else(|| DEXError::NotFound(format!("Pair not found: {}", pair_id)))?;

        // Cache the result.
        {
            let mut cache = self.cache.write().await;
            cache.pairs.insert(pair_id.to_string(), CachedItem::new(pair.clone(), self.config.cache_ttl.as_secs() as i64));
        }
        Ok(pair)
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

        let response: serde_json::Value = self.query_subgraph(PANCAKE_BSC_V2, gql_query, variables).await?;

        Ok(parse_pairs(&response))
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

        let response: serde_json::Value = self.query_subgraph(PANCAKE_BSC_V2, query, variables).await?;

        let token = response
            .pointer("/data/token")
            .and_then(|v| if v.is_null() { None } else { Some(parse_token(v)) })
            .ok_or_else(|| DEXError::NotFound(format!("Token not found: {}", token_id)))?;

        {
            let mut cache = self.cache.write().await;
            cache.tokens.insert(token_id.to_string(), CachedItem::new(token.clone(), self.config.cache_ttl.as_secs() as i64));
        }
        Ok(token)
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

        let response: serde_json::Value = self.query_subgraph(PANCAKE_BSC_V2, query, variables).await?;

        let tokens = parse_tokens(&response);
        {
            let mut cache = self.cache.write().await;
            for t in &tokens {
                cache.tokens.insert(t.id.clone(), CachedItem::new(t.clone(), self.config.cache_ttl.as_secs() as i64));
            }
        }
        Ok(tokens)
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

        let response: serde_json::Value = self.query_subgraph(PANCAKE_BSC_V2, query, variables).await?;

        let swaps = parse_swaps(&response);
        {
            let mut cache = self.cache.write().await;
            cache.swaps.insert(pair_id.to_string(), CachedItem::new(swaps.clone(), self.config.cache_ttl.as_secs() as i64));
        }
        Ok(swaps)
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

        let response: serde_json::Value = self.query_subgraph(endpoint, query, serde_json::json!({})).await?;

        // Aggregate factory-level stats from the GraphQL response.
        let factories = response.pointer("/data/factories").and_then(|v| v.as_array());
        let (total_pairs, total_volume_24h, total_liquidity, total_swaps_24h) = factories
            .map(|arr| {
                arr.iter().fold((0u64, 0.0f64, 0.0f64, 0i64), |(p, v, l, s), f| {
                    (
                        p + f.get("poolCount").and_then(as_f64).unwrap_or(0.0) as u64,
                        v + f.get("totalVolumeUSD").and_then(as_f64).unwrap_or(0.0),
                        l + f.get("totalLiquidityUSD").and_then(as_f64).unwrap_or(0.0),
                        s + f.get("txCount").and_then(as_f64).unwrap_or(0.0) as i64,
                    )
                })
            })
            .unwrap_or((0, 0.0, 0.0, 0));

        let top_pairs = parse_pairs(&response);
        let top_tokens = parse_tokens(&response);
        let total_tokens = top_tokens.len() as u64;

        Ok(DEXAnalytics {
            protocol,
            chain_id: chain,
            total_pairs,
            total_tokens,
            total_volume_24h,
            total_volume_7d: total_volume_24h, // 7d not directly available from factories
            total_liquidity,
            total_swaps_24h,
            top_pairs,
            top_tokens,
        })
    }

    // =============================================================================
    // INTERNAL
    // =============================================================================

    /// Query subgraph
    pub(crate) async fn query_subgraph(
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
// RESPONSE PARSING
// =============================================================================

/// Coerce a JSON number/string into f64.
fn as_f64(v: &serde_json::Value) -> Option<f64> {
    v.as_f64()
        .or_else(|| v.as_str().and_then(|s| s.parse::<f64>().ok()))
}

/// Coerce a JSON number/string into i64.
fn as_i64(v: &serde_json::Value) -> Option<i64> {
    v.as_i64().or_else(|| v.as_str().and_then(|s| s.parse::<i64>().ok()))
}

/// Coerce a JSON number/string into u8.
fn as_u8(v: &serde_json::Value) -> Option<u8> {
    v.as_u64()
        .and_then(|n| u8::try_from(n).ok())
        .or_else(|| v.as_str().and_then(|s| s.parse::<u8>().ok()))
}

fn str_or_empty(v: &serde_json::Value) -> String {
    v.as_str().unwrap_or("0").to_string()
}

/// Parse a single pair object from a subgraph response.
pub(crate) fn parse_pair(v: &serde_json::Value) -> DEXPair {
    let token0 = v.get("token0").cloned().unwrap_or_default();
    let token1 = v.get("token1").cloned().unwrap_or_default();
    let reserve0 = str_or_empty(v.get("reserve0").unwrap_or(&serde_json::Value::Null));
    let reserve1 = str_or_empty(v.get("reserve1").unwrap_or(&serde_json::Value::Null));
    let volume_usd = v.get("volumeUSD").and_then(as_f64).unwrap_or(0.0);
    let reserve0_f = reserve0.parse::<f64>().unwrap_or(0.0);
    let reserve1_f = reserve1.parse::<f64>().unwrap_or(0.0);
    DEXPair {
        id: v.get("id").and_then(|x| x.as_str()).unwrap_or("").to_string(),
        token0: token0.get("id").and_then(|x| x.as_str()).unwrap_or("").to_string(),
        token1: token1.get("id").and_then(|x| x.as_str()).unwrap_or("").to_string(),
        token0_symbol: token0.get("symbol").and_then(|x| x.as_str()).unwrap_or("").to_string(),
        token1_symbol: token1.get("symbol").and_then(|x| x.as_str()).unwrap_or("").to_string(),
        token0_decimals: as_u8(token0.get("decimals").unwrap_or(&serde_json::Value::Null)).unwrap_or(18),
        token1_decimals: as_u8(token1.get("decimals").unwrap_or(&serde_json::Value::Null)).unwrap_or(18),
        reserve0,
        reserve1,
        liquidity_usd: volume_usd, // approximate
        volume_24h: volume_usd,
        volume_7d: v.get("volumeUSD7d").and_then(as_f64).unwrap_or(volume_usd),
        tx_count_24h: as_i64(v.get("txCount").unwrap_or(&serde_json::Value::Null)).unwrap_or(0),
        tx_count_7d: as_i64(v.get("txCount").unwrap_or(&serde_json::Value::Null)).unwrap_or(0),
        price: if reserve1_f > 0.0 { reserve0_f / reserve1_f } else { 0.0 },
        price_change_24h: 0.0,
        fees_24h: volume_usd * 0.0025, // 0.25% default fee tier
        token0_price: if reserve1_f > 0.0 { reserve1_f / reserve0_f } else { 0.0 },
        token1_price: if reserve0_f > 0.0 { reserve0_f / reserve1_f } else { 0.0 },
        created_at_block: 0,
        created_at_timestamp: v
            .get("createdAtTimestamp")
            .and_then(as_i64)
            .unwrap_or(0),
    }
}

/// Parse all pairs from `data.pairs`.
pub(crate) fn parse_pairs(response: &serde_json::Value) -> Vec<DEXPair> {
    response
        .pointer("/data/pairs")
        .and_then(|v| v.as_array())
        .map(|arr| arr.iter().map(parse_pair).collect())
        .unwrap_or_default()
}

/// Parse a single token object.
fn parse_token(v: &serde_json::Value) -> DEXToken {
    DEXToken {
        id: v.get("id").and_then(|x| x.as_str()).unwrap_or("").to_string(),
        symbol: v.get("symbol").and_then(|x| x.as_str()).unwrap_or("").to_string(),
        name: v.get("name").and_then(|x| x.as_str()).unwrap_or("").to_string(),
        decimals: as_u8(v.get("decimals").unwrap_or(&serde_json::Value::Null)).unwrap_or(18),
        total_supply: str_or_empty(v.get("totalSupply").unwrap_or(&serde_json::Value::Null)),
        pairs0: vec![],
        pairs1: vec![],
        volume_usd_24h: v.get("volumeUSD").and_then(as_f64).unwrap_or(0.0),
        liquidity_usd: 0.0,
        tx_count_24h: as_i64(v.get("txCount").unwrap_or(&serde_json::Value::Null)).unwrap_or(0),
    }
}

/// Parse all tokens from `data.tokens`.
fn parse_tokens(response: &serde_json::Value) -> Vec<DEXToken> {
    response
        .pointer("/data/tokens")
        .and_then(|v| v.as_array())
        .map(|arr| arr.iter().map(parse_token).collect())
        .unwrap_or_default()
}

/// Parse a single swap object.
fn parse_swap(v: &serde_json::Value) -> DEXSwap {
    let pair_id = v
        .get("pair")
        .and_then(|p| p.get("id"))
        .and_then(|x| x.as_str())
        .unwrap_or("")
        .to_string();
    let tx = v.get("transaction").cloned().unwrap_or_default();
    DEXSwap {
        id: v.get("id").and_then(|x| x.as_str()).unwrap_or("").to_string(),
        pair_id,
        timestamp: as_i64(v.get("timestamp").unwrap_or(&serde_json::Value::Null)).unwrap_or(0),
        token0_in: String::new(),
        token0_out: String::new(),
        token1_in: String::new(),
        token1_out: String::new(),
        sender: v.get("sender").and_then(|x| x.as_str()).unwrap_or("").to_string(),
        recipient: v.get("to").and_then(|x| x.as_str()).unwrap_or("").to_string(),
        origin: v.get("origin").and_then(|x| x.as_str()).unwrap_or("").to_string(),
        amount0_in: str_or_empty(v.get("amount0In").unwrap_or(&serde_json::Value::Null)),
        amount1_in: str_or_empty(v.get("amount1In").unwrap_or(&serde_json::Value::Null)),
        amount0_out: str_or_empty(v.get("amount0Out").unwrap_or(&serde_json::Value::Null)),
        amount1_out: str_or_empty(v.get("amount1Out").unwrap_or(&serde_json::Value::Null)),
        transaction_hash: tx.get("hash").and_then(|x| x.as_str()).unwrap_or("").to_string(),
        log_index: 0,
    }
}

/// Parse all swaps from `data.swaps`.
pub(crate) fn parse_swaps(response: &serde_json::Value) -> Vec<DEXSwap> {
    response
        .pointer("/data/swaps")
        .and_then(|v| v.as_array())
        .map(|arr| arr.iter().map(parse_swap).collect())
        .unwrap_or_default()
}

// =============================================================================
// BUILDER
// =============================================================================

/// Builder for DEX client
pub struct DEXClientBuilder {
    pub(crate) config: ClientConfig,
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
        assert!(!client.config.timeout.is_zero());
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