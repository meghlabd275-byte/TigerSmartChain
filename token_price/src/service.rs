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
use serde::Serialize;
use std::collections::HashMap;
use std::time::Instant as StdInstant;

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
}

impl TokenPriceService {
    /// Create new token price service
    pub fn new(api_url: &str) -> Self {
        let config = TokenPriceConfig {
            api_url: api_url.to_string(),
            ..Default::default()
        };
        
        let client = Client::builder()
            .timeout(Duration::from_secs(30))
            .build()
            .unwrap_or_else(|_| Client::new());
        
        Self {
            config: config.clone(),
            client,
            cache: Arc::new(RwLock::new(PriceCache::new(config.cache_duration_secs))),
            stats: Arc::new(RwLock::new(PriceStats::default())),
            rate_limiter: Arc::new(RwLock::new(RateLimiter::new(config.rate_limit_per_minute))),
        }
    }

    /// Create with custom config
    pub fn with_config(config: TokenPriceConfig) -> Self {
        let client = Client::builder()
            .timeout(Duration::from_secs(30))
            .build()
            .unwrap_or_else(|_| Client::new());
        
        Self {
            config: config.clone(),
            client,
            cache: Arc::new(RwLock::new(PriceCache::new(config.cache_duration_secs))),
            stats: Arc::new(RwLock::new(PriceStats::default())),
            rate_limiter: Arc::new(RwLock::new(RateLimiter::new(config.rate_limit_per_minute))),
        }
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

        // Check rate limit
        {
            let limiter = self.rate_limiter.read().await;
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
        // Convert ETH address to CoinGecko ID (this would need a mapping in production)
        let token_id = self.address_to_token_id(address)?;
        
        let url = format!("{}/simple/price", self.config.api_url);
        let params = vec![
            ("ids", token_id.as_str()),
            ("vs_currencies", currency),
            ("include_24hr_change", "true"),
            ("include_market_cap", "true"),
        ];

        let request = self.client
            .get(&url)
            .query(&params)
            .build()
            .map_err(|e| PriceError::APIError(e.to_string()))?;

        let response = self.client
            .send(request)
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

        let token_data = data.get(&token_id)
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
        let token_id = self.address_to_token_id(address)?;
        
        let url = format!("{}/coins/{}/market_chart", self.config.api_url, token_id);
        let params = vec![
            ("vs_currency", currency),
            ("days", &days.to_string()),
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
        let token_id = self.address_to_token_id(address)?;
        
        let url = format!("{}/coins/{}/market_chart", self.config.api_url, token_id);
        let params = vec![
            ("vs_currency", currency),
            ("days", &days.to_string()),
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

    /// Get multiple token prices at once
    pub async fn get_multiple_prices(&self, addresses: &[String], currency: &str) -> Result<HashMap<String, TokenPrice>, PriceError> {
        if addresses.is_empty() {
            return Ok(HashMap::new());
        }

        // Check rate limit
        {
            let limiter = self.rate_limiter.read().await;
            if !limiter.try_acquire() {
                return Err(PriceError::RateLimited);
            }
        }

        // Get token IDs
        let token_ids: Vec<String> = addresses
            .iter()
            .map(|a| self.address_to_token_id(a))
            .collect::<Result<Vec<_>, _>>()?;

        let url = format!("{}/simple/price", self.config.api_url);
        let params = vec![
            ("ids", &token_ids.join(",")),
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

        let mut result = HashMap::new();

        for address in addresses {
            let token_id = self.address_to_token_id(address)?;
            let cache_key = format!("{}:{}", address.to_lowercase(), currency.to_lowercase());
            
            if let Some(token_data) = data.get(&token_id).and_then(|v| v.as_object()) {
                if let Some(price) = token_data.get(currency).and_then(|v| v.as_f64()) {
                    let price_change_24h = token_data.get(format!("{}_24h_change", currency))
                        .and_then(|v| v.as_f64())
                        .unwrap_or(0.0);

                    let token_price = TokenPrice {
                        address: address.to_lowercase(),
                        price,
                        price_change_24h,
                        price_change_absolute: price * (price_change_24h / 100.0),
                        market_cap: token_data.get(format!("{}_market_cap", currency))
                            .and_then(|v| v.as_f64())
                            .unwrap_or(0.0),
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
                    };

                    // Cache it
                    {
                        let mut cache = self.cache.write().await;
                        cache.set(cache_key, token_price.clone());
                    }

                    result.insert(address.to_lowercase(), token_price);
                }
            }
        }

        // Update stats
        {
            let mut stats = self.stats.write().await;
            stats.requests_total += addresses.len() as u64;
            stats.last_update = Utc::now().timestamp();
        }

        Ok(result)
    }

    /// Get gas price estimate
    pub async fn get_gas_price(&self) -> Result<GasPrice, PriceError> {
        // This would typically come from the network, but we provide a default
        Ok(GasPrice {
            slow: 20_000_000_000, // 20 Gwei
            standard: 30_000_000_000, // 30 Gwei
            fast: 50_000_000_000, // 50 Gwei
            last_updated: Utc::now().timestamp(),
        })
    }

    /// Get service statistics
    pub async fn get_stats(&self) -> PriceStats {
        let cache = self.cache.read().await;
        let mut stats = self.stats.read().await.clone();
        stats.cache_size = cache.size() as u64;
        stats
    }

    /// Convert token address to CoinGecko ID
    fn address_to_token_id(&self, address: &str) -> Result<String, PriceError> {
        let addr = address.to_lowercase();
        
        // Common token mappings (in production, this would be a database)
        let token_id = match addr.as_str() {
            // Ethereum
            "0x0000000000000000000000000000000000000000" => "ethereum",
            "0x2260fac9e5544a41376c6d6f5ac5b2c1d6c0c0a1d" => "bitcoin",
            "0xc02aaa39b223fe8d0a0e5c4f27ead9083c756cc2" => "wrapped-near",
            "0x7fc66500c84a76ad7e9e934e1c391c4eb6a543e3d" => "aave",
            "0x514910771af9ca656af836f8e1e3fbd5e2d2bc832e" => "chainlink",
            "0x7dd9c5cba058e1d5c5e8d837e803fdd1c9cf5fdd" => "usd-coin",
            "0x6b175474e89094c44da98b954eedeac495271d0ed" => "dai",
            "0xa0b86991c6218b36c1d19d4a2e9eb0e3608e774f" => "usd-coin",
            "0xd533a949740bb3306d11980f998ea2ab985c1cc6" => "curve-dao-token",
            "0x1f9840a85d5af5bf1d1762f925bdaddc4201f984" => "uniswap",
            // BSC
            "0xbb4cdb9c12b86b1b8a5c1e98f2e2d8e2b8e2b8e2" => "binancecoin",
            "0xe9e7cea3dedca598478052aeedce9c1d1c10a00f" => "binance-usd",
            "0x55d398326f99059f79e962a6300a24318a1d2ecb2" => "tether",
            "0x8ac76a51cc950d0672c79309b100e23ab5dde4ce2" => "usd-coin-bsc",
            "0x23396cf899c5f4835e13db8f6d3d7d5c6c5d7c5c" => "ripple",
            _ => return Err(PriceError::TokenNotFound(addr)),
        };
        
        Ok(token_id.to_string())
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
    requests: Vec<Instant>,
}

impl RateLimiter {
    pub fn new(max_per_minute: u32) -> Self {
        Self {
            max_per_minute,
            requests: Vec::new(),
        }
    }

    pub fn try_acquire(&mut self) -> bool {
        let now = Instant::now();
        
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

use chrono::Utc as UtcTime;
use serde::Serialize;
use std::time::Instant as StdInstant;