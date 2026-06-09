//! TigerSmartChain Analytics Engine
//! 
//! High-performance analytics for:
//! - TVL (Total Value Locked)
//! - Whale detection
//! - Token ranking
//! - DEX analytics
//! - NFT analytics

pub mod tvl;
pub mod whale;
pub mod ranking;
pub mod dex;
pub mod nft;
pub mod engine;

pub use engine::AnalyticsEngine;
pub use tvl::TVLMetrics;
pub use whale::WhaleAlert;
pub use ranking::TokenRank;

/// Analytics configuration
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct AnalyticsConfig {
    pub update_interval_seconds: u64,
    pub history_retention_days: u64,
    pub whale_threshold_usd: f64,
    pub top_tokens_count: usize,
    pub price_sources: Vec<String>,
}

impl Default for AnalyticsConfig {
    fn default() -> Self {
        Self {
            update_interval_seconds: 60,
            history_retention_days: 90,
            whale_threshold_usd: 100_000.0,
            top_tokens_count: 100,
            price_sources: vec![
                "coingecko".to_string(),
                "binance".to_string(),
            ],
        }
    }
}

/// Time range for analytics
#[derive(Debug, Clone, Copy, serde::Serialize, serde::Deserialize)]
pub enum TimeRange {
    Hour,
    Day,
    Week,
    Month,
    Year,
    All,
}

impl TimeRange {
    pub fn seconds(&self) -> i64 {
        match self {
            TimeRange::Hour => 3600,
            TimeRange::Day => 86400,
            TimeRange::Week => 604800,
            TimeRange::Month => 2592000,
            TimeRange::Year => 31536000,
            TimeRange::All => i64::MAX,
        }
    }
}

/// Network statistics
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct NetworkStats {
    pub tps: f64,
    pub avg_gas_price: f64,
    pub total_transactions: u64,
    pub total_blocks: u64,
    pub active_addresses: u64,
    pub tvl_usd: f64,
    pub timestamp: i64,
}

/// Price data
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct PriceData {
    pub token: String,
    pub price_usd: f64,
    pub change_24h: f64,
    pub change_7d: f64,
    pub volume_24h: f64,
    pub market_cap: f64,
    pub timestamp: i64,
}