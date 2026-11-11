//! Token Price Service Implementation for TigerScan
//! 
//! Full implementation for real-time token price feeds with CoinGecko integration.

use crate::types::*;
use chrono::Utc;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::RwLock;
use thiserror::Error;
use reqwest::Client;
use serde_json::Value;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::time::Instant as StdInstant;

use crate::cache::PriceCache;

// =============================================================================
// ERRORS
// =============================================================================

#[derive(Error, Debug)]
pub enum PriceError {
    #[error("API error: {0}")]
    APIError(String),
    #[error("Parse error: {0}")]
    ParseError(String),
    #[error("Rate limited")]
    RateLimited,
    #[error("Token not found: {0}")]
    TokenNotFound(String),
    #[error("Cache error: {0}")]
    CacheError(String),
}

// =============================================================================
// TOKEN PRICE SERVICE
// =============================================================================

/// Token Price Service - Full implementation with CoinGecko integration
pub struct TokenPriceService {
    config: TokenPriceConfig,
    client: Client,
    cache: Arc<RwLock<PriceCache>>,
    stats: Arc<RwLock<PriceStats>>,
    rate_limiter: Arc<RwLock<RateLimiter>>,
    /// JSON-RPC endpoint used for gas prices. Gas comes from the chain being
    /// indexed, not from a price feed, so it needs its own endpoint.
    rpc_url: Option<String>,
}

impl TokenPriceService {
    /// Create new token price service
    pub fn new(api_url: &str) -> Self {
        let config = TokenPriceConfig {
            api_url: api_url.to_string(),
            ..Default::default()
        };
        Self::with_config(config)
    }

    /// Create with custom config
    pub fn with_config(config: TokenPriceConfig) -> Self {
        // CoinGecko rejects any request that does not carry a User-Agent with
        // a bare 403, and reqwest sends none by default, so a client without
        // this header can never fetch a price. The API key goes in its own
        // header rather than the query string so it does not end up in proxy
        // logs or error messages.
        let mut headers = reqwest::header::HeaderMap::new();
        headers.insert(
            reqwest::header::USER_AGENT,
            reqwest::header::HeaderValue::from_static(concat!(
                "TigerSmartChain/",
                env!("CARGO_PKG_VERSION")
            )),
        );
        if let Some(key) = config.api_key.as_deref() {
            if let Ok(value) = reqwest::header::HeaderValue::from_str(key) {
                headers.insert("x-cg-demo-api-key", value);
            }
        }

        let client = Client::builder()
            .timeout(Duration::from_secs(30))
            .default_headers(headers)
            .build()
            .unwrap_or_else(|_| Client::new());
        
        Self {
            config: config.clone(),
            client,
            cache: Arc::new(RwLock::new(PriceCache::new(config.cache_duration_secs))),
            stats: Arc::new(RwLock::new(PriceStats::default())),
            rate_limiter: Arc::new(RwLock::new(RateLimiter::new(config.rate_limit_per_minute))),
            rpc_url: None,
        }
    }

    /// Attach a JSON-RPC endpoint, which [`Self::get_gas_price`] needs.
    pub fn with_rpc_url(mut self, rpc_url: &str) -> Self {
        self.rpc_url = Some(rpc_url.to_string());
        self
    }

    /// Get current price for a token
    pub async fn get_price(&self, address: &str, currency: &str) -> Result<TokenPrice, PriceError> {
        let cache_key = format!("{}:{}", address.to_lowercase(), currency.to_lowercase());
        
        // Check cache first
        {
            let cache = self.cache.read().await;
            if let Some(price) = cache.get(&cache_key) {
                let mut stats = self.stats.write().await;
                stats.requests_cache_hit += 1;
                return Ok(price.clone());
            }
        }

        // Check rate limit. `try_acquire` records the request, so this needs
        // exclusive access; a read lock would reject every call.
        {
            let mut limiter = self.rate_limiter.write().await;
            if !limiter.try_acquire() {
                return Err(PriceError::RateLimited);
            }
        }

        // Fetch from API
        let price = self.fetch_price(address, currency).await?;
        
        // Store in cache
        {
            let mut cache = self.cache.write().await;
            cache.set(cache_key, price.clone());
        }

        // Update stats
        {
            let mut stats = self.stats.write().await;
            stats.requests_total += 1;
            stats.last_update = Utc::now().timestamp();
        }

        Ok(price)
    }

    /// Fetch price from CoinGecko API
    async fn fetch_price(&self, address: &str, currency: &str) -> Result<TokenPrice, PriceError> {
        let addr = self.validate_address(address)?;
        
        // The contract-address endpoint answers by address directly, so the
        // response is keyed by the address rather than by a token id.
        let url = format!(
            "{}/simple/token_price/{}",
            self.config.api_url, self.config.platform
        );
        let params: [(&str, &str); 4] = [
            ("contract_addresses", addr.as_str()),
            ("vs_currencies", currency),
            ("include_24hr_change", "true"),
            ("include_market_cap", "true"),
        ];

        let response = self.client
            .get(&url)
            .query(&params)
            .send()
            .await
            .map_err(|e| PriceError::APIError(e.to_string()))?;

        if response.status() == 429 {
            return Err(PriceError::RateLimited);
        }

        if !response.status().is_success() {
            return Err(PriceError::APIError(format!("HTTP error: {}", response.status())));
        }

        let data: Value = response
            .json()
            .await
            .map_err(|e| PriceError::ParseError(e.to_string()))?;

        let token_data = data
            .get(&addr)
            .ok_or_else(|| PriceError::TokenNotFound(address.to_string()))?;
        
        let price = token_data.get(currency)
            .and_then(|v| v.as_f64())
            .ok_or_else(|| PriceError::ParseError("Price not found".to_string()))?;

        let price_change_24h = token_data.get(format!("{}_24h_change", currency))
            .and_then(|v| v.as_f64())
            .unwrap_or(0.0);

        let market_cap = token_data.get(format!("{}_market_cap", currency))
            .and_then(|v| v.as_f64())
            .unwrap_or(0.0);

        Ok(TokenPrice {
            address: address.to_lowercase(),
            price,
            price_change_24h,
            price_change_absolute: price * (price_change_24h / 100.0),
            market_cap,
            market_cap_rank: None,
            total_volume: 0.0,
            volume_24h: 0.0,
            circulating_supply: 0.0,
            total_supply: None,
            max_supply: None,
            ath: price,
            ath_change_percentage: 0.0,
            ath_date: String::new(),
            atl: price,
            atl_change_percentage: 0.0,
            atl_date: String::new(),
            last_updated: Utc::now().to_rfc3339(),
        })
    }

    /// Get price history for a token
    pub async fn get_price_history(&self, address: &str, currency: &str, days: u32) -> Result<PriceHistory, PriceError> {
        let addr = self.validate_address(address)?;

        let url = format!(
            "{}/coins/{}/contract/{}/market_chart",
            self.config.api_url, self.config.platform, addr
        );
        let days_str = days.to_string();
        let params: [(&str, &str); 2] = [("vs_currency", currency), ("days", &days_str)];

        let response = self.client
            .get(&url)
            .query(&params)
            .send()
            .await
            .map_err(|e| PriceError::APIError(e.to_string()))?;

        if response.status() == 429 {
            return Err(PriceError::RateLimited);
        }

        if !response.status().is_success() {
            return Err(PriceError::APIError(format!("HTTP error: {}", response.status())));
        }

        let data: Value = response
            .json()
            .await
            .map_err(|e| PriceError::ParseError(e.to_string()))?;

        let prices_array = data.get("prices")
            .and_then(|v| v.as_array())
            .ok_or_else(|| PriceError::ParseError("Prices not found".to_string()))?;

        let prices: Vec<PriceHistoryPoint> = prices_array
            .iter()
            .filter_map(|p| {
                let arr = p.as_array()?;
                let timestamp = arr.get(0)?.as_i64()?;
                let price = arr.get(1)?.as_f64()?;
                Some(PriceHistoryPoint {
                    timestamp,
                    price,
                    market_cap: None,
                    total_volume: None,
                })
            })
            .collect();

        Ok(PriceHistory {
            address: address.to_lowercase(),
            currency: currency.to_lowercase(),
            prices,
        })
    }

    /// Get market chart data
    pub async fn get_market_chart(&self, address: &str, currency: &str, days: u32) -> Result<MarketChart, PriceError> {
        let addr = self.validate_address(address)?;

        let url = format!(
            "{}/coins/{}/contract/{}/market_chart",
            self.config.api_url, self.config.platform, addr
        );
        let days_str = days.to_string();
        let params: [(&str, &str); 2] = [("vs_currency", currency), ("days", &days_str)];

        let response = self.client
            .get(&url)
            .query(&params)
            .send()
            .await
            .map_err(|e| PriceError::APIError(e.to_string()))?;

        if response.status() == 429 {
            return Err(PriceError::RateLimited);
        }

        if !response.status().is_success() {
            return Err(PriceError::APIError(format!("HTTP error: {}", response.status())));
        }

        let data: Value = response
            .json()
            .await
            .map_err(|e| PriceError::ParseError(e.to_string()))?;

        let parse_points = |key: &str| -> Vec<PriceHistoryPoint> {
            data.get(key)
                .and_then(|v| v.as_array())
                .map(|arr| {
                    arr.iter()
                        .filter_map(|p| {
                            let pts = p.as_array()?;
                            let timestamp = pts.get(0)?.as_i64()?;
                            let price = pts.get(1)?.as_f64()?;
                            Some(PriceHistoryPoint {
                                timestamp,
                                price,
                                market_cap: None,
                                total_volume: None,
                            })
                        })
                        .collect()
                })
                .unwrap_or_default()
        };

        Ok(MarketChart {
            address: address.to_lowercase(),
            currency: currency.to_lowercase(),
            prices: parse_points("prices"),
            market_caps: parse_points("market_caps"),
            total_volumes: parse_points("total_volumes"),
        })
    }

    /// Get prices for several tokens in one call.
    ///
    /// The CoinGecko contract-address endpoint accepts a limited number of
    /// addresses per request (one on the free tier, and it answers 400 rather
    /// than truncating when that is exceeded). So the work is split into
    /// batches of `max_addresses_per_request`, and each batch is one request.
    /// A caller passing twenty addresses gets twenty priced tokens rather than
    /// an error.
    ///
    /// Cached tokens are served without a request. A batch that fails does not
    /// discard the batches that succeeded, but the error is still returned if
    /// no token could be priced, so a caller cannot mistake an outage for
    /// "these tokens have no price".
    pub async fn get_multiple_prices(
        &self,
        addresses: &[String],
        currency: &str,
    ) -> Result<HashMap<String, TokenPrice>, PriceError> {
        if addresses.is_empty() {
            return Ok(HashMap::new());
        }

        let currency_key = currency.to_lowercase();
        let mut result: HashMap<String, TokenPrice> = HashMap::new();
        let mut to_fetch: Vec<String> = Vec::new();

        // Serve what is cached and collect the rest.
        {
            let cache = self.cache.read().await;
            for address in addresses {
                let addr = self.validate_address(address)?;
                let cache_key = format!("{addr}:{currency_key}");
                match cache.get(&cache_key) {
                    Some(price) => {
                        result.insert(addr, price);
                    }
                    None => to_fetch.push(addr),
                }
            }
        }

        if to_fetch.is_empty() {
            let mut stats = self.stats.write().await;
            stats.requests_cache_hit += addresses.len() as u64;
            return Ok(result);
        }

        // One rate-limit token per HTTP request, not per address.
        let batch_size = self.config.max_addresses_per_request.max(1) as usize;
        let batches: Vec<&[String]> = to_fetch.chunks(batch_size).collect();

        {
            let mut limiter = self.rate_limiter.write().await;
            for _ in 0..batches.len() {
                if !limiter.try_acquire() {
                    return Err(PriceError::RateLimited);
                }
            }
        }

        let mut last_error: Option<PriceError> = None;
        for batch in batches {
            match self.fetch_price_batch(batch, currency).await {
                Ok(data) => {
                    for addr in batch {
                        if let Some(token_price) = Self::parse_token_price(&data, addr, currency) {
                            let cache_key = format!("{addr}:{currency_key}");
                            {
                                let mut cache = self.cache.write().await;
                                cache.set(cache_key, token_price.clone());
                            }
                            result.insert(addr.clone(), token_price);
                        }
                    }
                }
                // Keep going: one bad batch should not hide the good ones.
                Err(e) => last_error = Some(e),
            }
        }

        {
            let mut stats = self.stats.write().await;
            stats.requests_total += batches_len(&to_fetch, batch_size) as u64;
            stats.last_update = Utc::now().timestamp();
            if result.is_empty() {
                stats.requests_failed += 1;
            }
        }

        if result.is_empty() {
            if let Some(e) = last_error {
                return Err(e);
            }
            return Err(PriceError::TokenNotFound(
                "none of the requested tokens could be priced".to_string(),
            ));
        }

        Ok(result)
    }

    /// One request for a batch of contract addresses.
    async fn fetch_price_batch(
        &self,
        addresses: &[String],
        currency: &str,
    ) -> Result<Value, PriceError> {
        let url = format!(
            "{}/simple/token_price/{}",
            self.config.api_url, self.config.platform
        );
        let ids = addresses.join(",");
        let params: [(&str, &str); 4] = [
            ("contract_addresses", &ids),
            ("vs_currencies", currency),
            ("include_24hr_change", "true"),
            ("include_market_cap", "true"),
        ];

        let response = self
            .client
            .get(&url)
            .query(&params)
            .send()
            .await
            .map_err(|e| PriceError::APIError(e.to_string()))?;

        if response.status() == 429 {
            return Err(PriceError::RateLimited);
        }
        if !response.status().is_success() {
            return Err(PriceError::APIError(format!(
                "HTTP error: {}",
                response.status()
            )));
        }

        response
            .json()
            .await
            .map_err(|e| PriceError::ParseError(e.to_string()))
    }

    /// Build a `TokenPrice` from one entry of a `simple/token_price` response.
    ///
    /// The response is keyed by the contract address, and the fields carry the
    /// currency as a prefix: `usd`, `usd_24h_change`, `usd_market_cap`.
    /// Returns `None` when the address is absent or has no price in the
    /// requested currency, which is how the API reports a token it does not
    /// track.
    fn parse_token_price(data: &Value, addr: &str, currency: &str) -> Option<TokenPrice> {
        let token_data = data.get(addr)?.as_object()?;
        let price = token_data.get(currency)?.as_f64()?;

        let price_change_24h = token_data
            .get(&format!("{currency}_24h_change"))
            .and_then(|v| v.as_f64())
            .unwrap_or(0.0);
        let market_cap = token_data
            .get(&format!("{currency}_market_cap"))
            .and_then(|v| v.as_f64())
            .unwrap_or(0.0);

        Some(TokenPrice {
            address: addr.to_string(),
            price,
            price_change_24h,
            price_change_absolute: price * (price_change_24h / 100.0),
            market_cap,
            market_cap_rank: None,
            total_volume: 0.0,
            volume_24h: 0.0,
            circulating_supply: 0.0,
            total_supply: None,
            max_supply: None,
            ath: price,
            ath_change_percentage: 0.0,
            ath_date: String::new(),
            atl: price,
            atl_change_percentage: 0.0,
            atl_date: String::new(),
            last_updated: Utc::now().to_rfc3339(),
        })
    }

    /// Get gas price estimates for the attached chain.
    ///
    /// The values come from the chain's own JSON-RPC endpoint, which is the
    /// only source that reflects what a transaction will actually cost:
    ///
    ///   * `standard` is the current base fee plus a tip, from the latest
    ///     block's `baseFeePerGas` and `eth_maxPriorityFeePerGas`.
    ///   * `slow` uses a smaller tip, for a transaction that can wait.
    ///   * `fast` uses a larger tip, for one that should land in the next
    ///     block.
    ///
    /// On a pre-EIP-1559 chain there is no `baseFeePerGas`, so the legacy
    /// `eth_gasPrice` is used as the standard value and the other two are
    /// scaled around it.
    ///
    /// Returns an error when no endpoint is attached or the node cannot be
    /// reached. It does not fall back to a fixed number: a stale guess shown as
    /// a live gas price is worse than an error, because a caller will use it to
    /// price a real transaction.
    pub async fn get_gas_price(&self) -> Result<GasPrice, PriceError> {
        let rpc_url = self.rpc_url.as_deref().ok_or_else(|| {
            PriceError::APIError(
                "no JSON-RPC endpoint attached; call with_rpc_url() before requesting gas prices"
                    .to_string(),
            )
        })?;

        let latest = self.rpc_call(rpc_url, "eth_getBlockByNumber", serde_json::json!(["latest", false])).await?;
        let base_fee = latest
            .get("baseFeePerGas")
            .and_then(|v| v.as_str())
            .and_then(|s| u128::from_str_radix(s.trim_start_matches("0x"), 16).ok());

        let tip = self
            .rpc_call(rpc_url, "eth_maxPriorityFeePerGas", serde_json::json!([]))
            .await
            .ok()
            .and_then(|v| v.as_str().and_then(|s| u128::from_str_radix(s.trim_start_matches("0x"), 16).ok()));

        let (slow, standard, fast) = match base_fee {
            // EIP-1559: base fee plus a tip that reflects priority.
            Some(base) => {
                let tip = tip.unwrap_or(1_000_000_000); // 1 gwei when the node does not advise one
                (
                    base + tip / 2,
                    base + tip,
                    base + tip * 2,
                )
            }
            // Pre-EIP-1559: the node's gas price is the whole answer.
            None => {
                let gas_price = self
                    .rpc_call(rpc_url, "eth_gasPrice", serde_json::json!([]))
                    .await?
                    .as_str()
                    .and_then(|s| u128::from_str_radix(s.trim_start_matches("0x"), 16).ok())
                    .ok_or_else(|| PriceError::ParseError("eth_gasPrice returned no value".to_string()))?;
                (gas_price, gas_price, gas_price * 2)
            }
        };

        // The wire format is wei, but the fields are u64 and a mainnet gas
        // price fits comfortably; guard the cast rather than truncating.
        let clamp = |v: u128| -> Result<u64, PriceError> {
            u64::try_from(v).map_err(|_| {
                PriceError::ParseError(format!("gas price {v} wei does not fit in u64"))
            })
        };

        Ok(GasPrice {
            slow: clamp(slow)?,
            standard: clamp(standard)?,
            fast: clamp(fast)?,
            last_updated: Utc::now().timestamp(),
        })
    }

    /// Issue a JSON-RPC call and return its `result`.
    async fn rpc_call(
        &self,
        rpc_url: &str,
        method: &str,
        params: serde_json::Value,
    ) -> Result<Value, PriceError> {
        let body = serde_json::json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": method,
            "params": params,
        });

        let response = self
            .client
            .post(rpc_url)
            .json(&body)
            .send()
            .await
            .map_err(|e| PriceError::APIError(format!("{method}: {e}")))?;

        if !response.status().is_success() {
            return Err(PriceError::APIError(format!(
                "{method}: HTTP {}",
                response.status()
            )));
        }

        let payload: Value = response
            .json()
            .await
            .map_err(|e| PriceError::ParseError(format!("{method}: {e}")))?;

        if let Some(error) = payload.get("error") {
            return Err(PriceError::APIError(format!("{method}: {error}")));
        }

        payload
            .get("result")
            .cloned()
            .ok_or_else(|| PriceError::ParseError(format!("{method}: no result")))
    }

    /// Get service statistics
    pub async fn get_stats(&self) -> PriceStats {
        let cache = self.cache.read().await;
        let mut stats = self.stats.read().await.clone();
        stats.cache_size = cache.size() as u64;
        stats
    }

    /// Validate an EVM contract address and return it in lowercase.
    ///
    /// The CoinGecko contract-address endpoints take the address directly, so
    /// no local address-to-id table is needed. The previous table mapped
    /// addresses to ids that did not match the tokens (for example
    /// `0xc02aaa39b223fe8d0a0e5c4f27ead9083c756cc2`, which is WETH, was mapped
    /// to `wrapped-near`), so lookups silently returned another token's price.
    /// Asking the API by address cannot drift like that.
    fn validate_address(&self, address: &str) -> Result<String, PriceError> {
        let addr = address.trim().to_lowercase();
        let hex = addr
            .strip_prefix("0x")
            .ok_or_else(|| PriceError::TokenNotFound(address.to_string()))?;
        if hex.len() != 40 || !hex.bytes().all(|b| b.is_ascii_hexdigit()) {
            return Err(PriceError::TokenNotFound(address.to_string()));
        }
        Ok(addr)
    }

    /// Clear the cache
    pub async fn clear_cache(&self) {
        let mut cache = self.cache.write().await;
        cache.clear();
    }
}

// =============================================================================
// GAS PRICE
// =============================================================================

/// Gas Price
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GasPrice {
    pub slow: u64,
    pub standard: u64,
    pub fast: u64,
    pub last_updated: i64,
}

// =============================================================================
// RATE LIMITER
// =============================================================================

/// Rate Limiter
pub struct RateLimiter {
    max_per_minute: u32,
    requests: Vec<StdInstant>,
}

impl RateLimiter {
    pub fn new(max_per_minute: u32) -> Self {
        Self {
            max_per_minute,
            requests: Vec::new(),
        }
    }

    pub fn try_acquire(&mut self) -> bool {
        let now = StdInstant::now();
        
        // Remove old requests (older than 1 minute)
        self.requests.retain(|t| now.duration_since(*t).as_secs() < 60);
        
        if self.requests.len() < self.max_per_minute as usize {
            self.requests.push(now);
            true
        } else {
            false
        }
    }
}


/// Number of batches `total` items split into, for the stats counter.
fn batches_len(total: &[String], batch_size: usize) -> usize {
    total.len().div_ceil(batch_size.max(1))
}

// =============================================================================
// TESTS
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    /// A real USDT contract on Ethereum mainnet. Used by the live tests below.
    const USDT: &str = "0xdac17f958d2ee523a2206206994597c13d831ec7";

    fn service() -> TokenPriceService {
        TokenPriceService::new("https://api.coingecko.com/api/v3")
    }

    #[test]
    fn accepts_a_well_formed_address() {
        let s = service();
        assert_eq!(s.validate_address(USDT).unwrap(), USDT);
        // Mixed case is normalised, and surrounding whitespace is ignored.
        assert_eq!(
            s.validate_address("  0xDAC17F958D2ee523a2206206994597C13D831ec7  ").unwrap(),
            USDT
        );
    }

    #[test]
    fn rejects_malformed_addresses() {
        let s = service();
        for bad in [
            "",
            "0x",
            "dac17f958d2ee523a2206206994597c13d831ec7",          // no 0x
            "0xdac17f958d2ee523a2206206994597c13d831ec",          // 39 hex digits
            "0xdac17f958d2ee523a2206206994597c13d831ec77",        // 41 hex digits
            "0xzzc17f958d2ee523a2206206994597c13d831ec7",         // non-hex
            "ethereum",                                            // not an address
        ] {
            assert!(
                s.validate_address(bad).is_err(),
                "{bad:?} should be rejected"
            );
        }
    }

    #[test]
    fn the_rate_limiter_admits_up_to_the_limit_then_refuses() {
        let mut limiter = RateLimiter::new(3);
        assert!(limiter.try_acquire());
        assert!(limiter.try_acquire());
        assert!(limiter.try_acquire());
        assert!(!limiter.try_acquire(), "the fourth request exceeds the limit");
    }

    #[test]
    fn the_rate_limiter_refills_as_requests_age_out() {
        let mut limiter = RateLimiter::new(1);
        assert!(limiter.try_acquire());
        assert!(!limiter.try_acquire());

        // Age the recorded request past the one-minute window.
        limiter.requests[0] = StdInstant::now() - Duration::from_secs(61);
        assert!(limiter.try_acquire(), "the window should have refilled");
    }

    #[test]
    fn the_cache_returns_a_stored_price_and_expires() {
        let mut cache = PriceCache::new(60);
        assert!(cache.get("k").is_none());

        cache.set("k".to_string(), TokenPrice::new("0xabc".to_string(), 1.5));
        assert_eq!(cache.get("k").unwrap().price, 1.5);
        assert_eq!(cache.size(), 1);

        // A zero-TTL cache expires immediately, which exercises the
        // expiry branch without waiting a minute.
        let mut expired = PriceCache::new(0);
        expired.set("k".to_string(), TokenPrice::new("0xabc".to_string(), 1.5));
        assert!(expired.get("k").is_none(), "a zero TTL must not serve");
    }

    /// Serve one HTTP response per connection and count the requests.
    ///
    /// This is a fake *server*, not a mock of the code under test: the service
    /// makes real HTTP requests through reqwest, and the test observes the
    /// requests that arrive. It is what makes the batching behaviour testable
    /// without depending on an external rate limit, and it pins the request
    /// count, the query parameters and the response parsing all at once.
    async fn spawn_recording_server(
        body: &'static str,
    ) -> (String, std::sync::Arc<std::sync::Mutex<Vec<String>>>) {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};

        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let requests = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
        let seen = requests.clone();

        tokio::spawn(async move {
            loop {
                let Ok((mut socket, _)) = listener.accept().await else {
                    return;
                };
                let seen = seen.clone();
                tokio::spawn(async move {
                    let mut buf = vec![0u8; 8192];
                    let n = socket.read(&mut buf).await.unwrap_or(0);
                    let request = String::from_utf8_lossy(&buf[..n]).to_string();
                    // Record the request line so the test can inspect the path
                    // and query string.
                    if let Some(line) = request.lines().next() {
                        seen.lock().unwrap().push(line.to_string());
                    }
                    let response = format!(
                        "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                        body.len(),
                        body
                    );
                    let _ = socket.write_all(response.as_bytes()).await;
                    let _ = socket.flush().await;
                });
            }
        });

        (format!("http://{addr}"), requests)
    }

    #[tokio::test]
    async fn batching_sends_one_request_per_configured_batch_size() {
        // Three addresses, a batch size of one: expect three requests, each
        // carrying exactly one contract address.
        let body = r#"{"0x1111111111111111111111111111111111111111":{"usd":1.5,"usd_24h_change":2.0,"usd_market_cap":1000.0}}"#;
        let (base, requests) = spawn_recording_server(body).await;

        let s = TokenPriceService::with_config(TokenPriceConfig {
            api_url: base,
            max_addresses_per_request: 1,
            rate_limit_per_minute: 100,
            ..Default::default()
        });

        let addresses = vec![
            "0x1111111111111111111111111111111111111111".to_string(),
            "0x2222222222222222222222222222222222222222".to_string(),
            "0x3333333333333333333333333333333333333333".to_string(),
        ];
        let prices = s.get_multiple_prices(&addresses, "usd").await.unwrap();

        let seen = requests.lock().unwrap().clone();
        assert_eq!(
            seen.len(),
            3,
            "a batch size of 1 over 3 addresses must be 3 requests, saw {seen:?}"
        );
        for line in &seen {
            assert!(
                line.contains("contract_addresses=0x"),
                "each request must name a contract address: {line}"
            );
            // With a batch size of one there is never a comma in the list.
            assert!(
                !line.contains("%2C") && !line.contains(','),
                "a batch of one must not join addresses: {line}"
            );
        }
        // The recorded body only prices the first address, so exactly that one
        // resolves — which confirms the response is keyed by address.
        assert_eq!(prices.len(), 1);
        assert_eq!(prices[&addresses[0]].price, 1.5);
        assert_eq!(prices[&addresses[0]].price_change_24h, 2.0);
        assert_eq!(prices[&addresses[0]].market_cap, 1000.0);
    }

    #[tokio::test]
    async fn a_larger_batch_size_joins_addresses_into_one_request() {
        let body = r#"{"0x1111111111111111111111111111111111111111":{"usd":1.5}}"#;
        let (base, requests) = spawn_recording_server(body).await;

        let s = TokenPriceService::with_config(TokenPriceConfig {
            api_url: base,
            max_addresses_per_request: 10,
            rate_limit_per_minute: 100,
            ..Default::default()
        });

        let addresses = vec![
            "0x1111111111111111111111111111111111111111".to_string(),
            "0x2222222222222222222222222222222222222222".to_string(),
        ];
        let _ = s.get_multiple_prices(&addresses, "usd").await.unwrap();

        let seen = requests.lock().unwrap().clone();
        assert_eq!(seen.len(), 1, "a batch size of 10 must be one request");
        // reqwest percent-encodes the comma separating the addresses.
        assert!(
            seen[0].contains("%2C") || seen[0].contains(','),
            "both addresses must be in one request: {}",
            seen[0]
        );
    }

    #[tokio::test]
    async fn a_cached_token_is_served_without_a_request() {
        let body = r#"{"0x1111111111111111111111111111111111111111":{"usd":7.25}}"#;
        let (base, requests) = spawn_recording_server(body).await;

        let s = TokenPriceService::with_config(TokenPriceConfig {
            api_url: base,
            max_addresses_per_request: 1,
            rate_limit_per_minute: 100,
            ..Default::default()
        });

        let addresses = vec!["0x1111111111111111111111111111111111111111".to_string()];
        let first = s.get_multiple_prices(&addresses, "usd").await.unwrap();
        assert_eq!(first[&addresses[0]].price, 7.25);
        assert_eq!(requests.lock().unwrap().len(), 1);

        // The second call is served from the cache.
        let second = s.get_multiple_prices(&addresses, "usd").await.unwrap();
        assert_eq!(second[&addresses[0]].price, 7.25);
        assert_eq!(
            requests.lock().unwrap().len(),
            1,
            "a cached token must not trigger a request"
        );
    }

    #[tokio::test]
    async fn an_http_error_is_surfaced_when_nothing_could_be_priced() {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};

        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            loop {
                let Ok((mut socket, _)) = listener.accept().await else {
                    return;
                };
                tokio::spawn(async move {
                    let mut buf = vec![0u8; 4096];
                    let _ = socket.read(&mut buf).await;
                    let _ = socket
                        .write_all(b"HTTP/1.1 500 Internal Server Error\r\nContent-Length: 0\r\nConnection: close\r\n\r\n")
                        .await;
                });
            }
        });

        let s = TokenPriceService::with_config(TokenPriceConfig {
            api_url: format!("http://{addr}"),
            rate_limit_per_minute: 100,
            ..Default::default()
        });

        let addresses = vec!["0x1111111111111111111111111111111111111111".to_string()];
        let err = s.get_multiple_prices(&addresses, "usd").await.unwrap_err();
        assert!(
            matches!(err, PriceError::APIError(_)),
            "a failed batch must not look like a token with no price, got {err:?}"
        );
    }

    #[tokio::test]
    async fn gas_price_requires_an_endpoint() {
        let s = service();
        let err = s.get_gas_price().await.unwrap_err();
        assert!(
            matches!(err, PriceError::APIError(_)),
            "expected a clear error without an endpoint, got {err:?}"
        );
    }

    #[tokio::test]
    async fn an_unsupported_address_is_rejected_before_any_request() {
        let s = service();
        let err = s.get_price("not-an-address", "usd").await.unwrap_err();
        assert!(matches!(err, PriceError::TokenNotFound(_)));
    }

    // ---- Live tests ----
    //
    // These hit the real CoinGecko API and a real node. They are the only way
    // to check the request shapes, the response field names and the units,
    // since a mock would just restate whatever the implementation assumes.
    // Run them with `cargo test -- --ignored`.
    //
    // The public CoinGecko tier allows only a handful of calls per minute, so
    // these run serially and back off when throttled. `retry` is what keeps
    // them from failing for a reason that has nothing to do with the code
    // under test.

    /// Run `f` until it succeeds, waiting out a rate limit.
    ///
    /// Only `RateLimited` is retried. Any other error is a real failure and
    /// is returned immediately, so a broken request shape still fails fast.
    async fn retry<T, F, Fut>(mut f: F) -> Result<T, PriceError>
    where
        F: FnMut() -> Fut,
        Fut: std::future::Future<Output = Result<T, PriceError>>,
    {
        for attempt in 0..6 {
            match f().await {
                Err(PriceError::RateLimited) => {
                    tokio::time::sleep(Duration::from_secs(15 * (attempt + 1))).await;
                }
                other => return other,
            }
        }
        Err(PriceError::RateLimited)
    }

    #[tokio::test]
    #[ignore = "requires network access to api.coingecko.com"]
    async fn live_price_of_usdt_is_near_one_dollar() {
        let s = service();
        let price = retry(|| s.get_price(USDT, "usd")).await.unwrap();
        assert_eq!(price.address, USDT);
        // A stablecoin's price is the one value known in advance, which is
        // exactly what makes it useful as an end-to-end check.
        assert!(
            (0.9..1.1).contains(&price.price),
            "USDT priced at {} — the response shape is probably wrong",
            price.price
        );
        assert!(price.market_cap > 0.0, "market cap should be populated");
        assert!(!price.last_updated.is_empty());
    }

    #[tokio::test]
    #[ignore = "requires network access to api.coingecko.com"]
    async fn live_price_history_is_ordered_and_non_empty() {
        let s = service();
        let history = retry(|| s.get_price_history(USDT, "usd", 1)).await.unwrap();
        assert!(!history.prices.is_empty(), "expected history points");
        let mut previous = i64::MIN;
        for point in &history.prices {
            assert!(point.timestamp > previous, "timestamps must increase");
            assert!(point.price > 0.0);
            previous = point.timestamp;
        }
    }

    #[tokio::test]
    #[ignore = "requires network access to api.coingecko.com"]
    async fn live_batch_price_returns_every_requested_token() {
        let s = service();
        // USDT and DAI, both Ethereum mainnet contracts. Two addresses
        // exercises the batching path, which is the point: the free tier
        // rejects a request carrying more than one.
        let addresses = vec![
            USDT.to_string(),
            "0x6b175474e89094c44da98b954eedeac495271d0f".to_string(),
        ];
        let prices = retry(|| s.get_multiple_prices(&addresses, "usd")).await.unwrap();
        assert_eq!(prices.len(), 2, "both tokens should be priced");
        for address in &addresses {
            let price = prices.get(address).expect("every address must appear");
            assert!(price.price > 0.0);
        }
    }

    #[tokio::test]
    #[ignore = "requires network access to a JSON-RPC endpoint"]
    async fn live_gas_price_comes_from_the_chain() {
        // Any public Ethereum endpoint; the point is that the values are the
        // chain's, not a constant.
        let s = service().with_rpc_url("https://ethereum-rpc.publicnode.com");
        let gas = retry(|| s.get_gas_price()).await.unwrap();
        assert!(gas.standard > 0, "a live chain has a non-zero gas price");
        assert!(
            gas.slow <= gas.standard && gas.standard <= gas.fast,
            "the tiers must be ordered: slow={} standard={} fast={}",
            gas.slow,
            gas.standard,
            gas.fast
        );
    }
}
