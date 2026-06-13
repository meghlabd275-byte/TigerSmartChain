//! Token Price Types for TigerScan

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

// =============================================================================
// PRICE DATA
// =============================================================================

/// Token Price
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenPrice {
    /// Token address (lowercase)
    pub address: String,
    /// Current price in currency
    pub price: f64,
    /// Price change in last 24h (%)
    pub price_change_24h: f64,
    /// Price change in last 24h (absolute)
    pub price_change_absolute: f64,
    /// Market cap
    pub market_cap: f64,
    /// Market cap rank
    pub market_cap_rank: Option<u32>,
    /// Total volume
    pub total_volume: f64,
    /// 24h volume
    pub volume_24h: f64,
    /// Circulating supply
    pub circulating_supply: f64,
    /// Total supply
    pub total_supply: Option<f64>,
    /// Max supply
    pub max_supply: Option<f64>,
    /// Ath (all time high)
    pub ath: f64,
    /// Ath change percentage
    pub ath_change_percentage: f64,
    /// Ath date
    pub ath_date: String,
    /// Atl (all time low)
    pub atl: f64,
    /// Atl change percentage
    pub atl_change_percentage: f64,
    /// Atl date
    pub atl_date: String,
    /// Last updated timestamp
    pub last_updated: String,
}

impl TokenPrice {
    pub fn new(address: String, price: f64) -> Self {
        Self {
            address,
            price,
            price_change_24h: 0.0,
            price_change_absolute: 0.0,
            market_cap: 0.0,
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
        }
    }
}

// =============================================================================
// PRICE HISTORY
// =============================================================================

/// Price History Point
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PriceHistoryPoint {
    /// Timestamp
    pub timestamp: i64,
    /// Price
    pub price: f64,
    /// Market cap
    pub market_cap: Option<f64>,
    /// Total volume
    pub total_volume: Option<f64>,
}

/// Price History
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PriceHistory {
    /// Token address
    pub address: String,
    /// Currency
    pub currency: String,
    /// History points
    pub prices: Vec<PriceHistoryPoint>,
}

// =============================================================================
// MARKET CHART
// =============================================================================

/// Market Chart Data
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MarketChart {
    /// Token address
    pub address: String,
    /// Currency
    pub currency: String,
    /// Price history
    pub prices: Vec<PriceHistoryPoint>,
    /// Market cap history
    pub market_caps: Vec<PriceHistoryPoint>,
    /// Volume history
    pub total_volumes: Vec<PriceHistoryPoint>,
}

// =============================================================================
// SIMPLE PRICE
// =============================================================================

/// Simple Price Response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SimplePrice {
    #[serde(flatten)]
    pub prices: HashMap<String, HashMap<String, f64>>,
}

// =============================================================================
// TOKEN INFO
// =============================================================================

/// Token Info from CoinGecko
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenInfo {
    pub id: String,
    pub symbol: String,
    pub name: String,
    pub image: TokenImage,
    pub current_price: f64,
    pub market_cap: f64,
    pub market_cap_rank: u32,
    pub total_volume: f64,
    pub high_24h: f64,
    pub low_24h: f64,
    pub price_change_24h: f64,
    pub price_change_percentage_24h: f64,
    pub market_cap_change_24h: f64,
    pub market_cap_change_percentage_24h: f64,
    pub circulating_supply: f64,
    pub total_supply: Option<f64>,
    pub max_supply: Option<f64>,
    pub ath: f64,
    pub ath_change_percentage: f64,
    pub ath_date: String,
    pub atl: f64,
    pub atl_change_percentage: f64,
    pub atl_date: String,
    pub last_updated: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenImage {
    pub large: String,
    pub small: String,
    pub thumb: String,
}

// =============================================================================
// CONFIG
// =============================================================================

/// Token Price Service Configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenPriceConfig {
    /// CoinGecko API URL
    pub api_url: String,
    /// API key (optional)
    pub api_key: Option<String>,
    /// Cache duration in seconds
    pub cache_duration_secs: u64,
    /// Rate limit requests per minute
    pub rate_limit_per_minute: u32,
    /// Enable price history
    pub enable_history: bool,
    /// History days
    pub history_days: u32,
    /// Enable market chart
    pub enable_market_chart: bool,
    /// Default currency
    pub default_currency: String,
    /// Supported currencies
    pub supported_currencies: Vec<String>,
}

impl Default for TokenPriceConfig {
    fn default() -> Self {
        Self {
            api_url: "https://api.coingecko.com/api/v3".to_string(),
            api_key: None,
            cache_duration_secs: 60, // 1 minute cache
            rate_limit_per_minute: 10,
            enable_history: true,
            history_days: 30,
            enable_market_chart: true,
            default_currency: "usd".to_string(),
            supported_currencies: vec![
                "usd".to_string(),
                "eth".to_string(),
                "btc".to_string(),
                "eur".to_string(),
                "gbp".to_string(),
            ],
        }
    }
}

// =============================================================================
// ALERT
// =============================================================================

/// Price Alert
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PriceAlert {
    pub id: String,
    pub token_address: String,
    pub condition: AlertCondition,
    pub target_price: f64,
    pub triggered: bool,
    pub created_at: i64,
}

/// Alert Condition
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum AlertCondition {
    Above,
    Below,
    CrossAbove,
    CrossBelow,
}

// =============================================================================
// DEX PAIR
// =============================================================================

/// DEX Pair Price
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DexPair {
    pub pair_address: String,
    pub token0_address: String,
    pub token1_address: String,
    pub token0_price: f64,
    pub token1_price: f64,
    pub reserve0: f64,
    pub reserve1: f64,
    pub volume_24h: f64,
    pub liquidity: f64,
    pub dex: String,
}

// =============================================================================
// STATS
// =============================================================================

/// Price Service Statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PriceStats {
    pub requests_total: u64,
    pub requests_cache_hit: u64,
    pub requests_failed: u64,
    pub last_update: i64,
    pub cache_size: u64,
}

impl Default for PriceStats {
    fn default() -> Self {
        Self {
            requests_total: 0,
            requests_cache_hit: 0,
            requests_failed: 0,
            last_update: Utc::now().timestamp(),
            cache_size: 0,
        }
    }
}