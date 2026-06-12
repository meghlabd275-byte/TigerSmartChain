//! DEX Types for TigerScan
//! Data structures for DEX pairs, tokens, swaps, and analytics

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

// =============================================================================
// DATA STRUCTURES
// =============================================================================

/// DEX Pair represents a trading pair on a DEX
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DEXPair {
    pub id: String,
    pub token0: String,
    pub token1: String,
    pub token0_symbol: String,
    pub token1_symbol: String,
    pub token0_decimals: u8,
    pub token1_decimals: u8,
    pub reserve0: String,
    pub reserve1: String,
    pub liquidity_usd: f64,
    pub volume_24h: f64,
    pub volume_7d: f64,
    pub tx_count_24h: i64,
    pub tx_count_7d: i64,
    pub price: f64,
    pub price_change_24h: f64,
    pub fees_24h: f64,
    pub token0_price: f64,
    pub token1_price: f64,
    pub created_at_block: u64,
    pub created_at_timestamp: i64,
}

/// DEX Token represents token data from DEX
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DEXToken {
    pub id: String,
    pub symbol: String,
    pub name: String,
    pub decimals: u8,
    pub total_supply: String,
    pub pairs0: Vec<String>,
    pub pairs1: Vec<String>,
    pub volume_usd_24h: f64,
    pub liquidity_usd: f64,
    pub tx_count_24h: i64,
}

/// DEX Swap represents a swap transaction
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DEXSwap {
    pub id: String,
    pub pair_id: String,
    pub timestamp: i64,
    pub token0_in: String,
    pub token0_out: String,
    pub token1_in: String,
    pub token1_out: String,
    pub sender: String,
    pub recipient: String,
    pub origin: String,
    pub amount0_in: String,
    pub amount1_in: String,
    pub amount0_out: String,
    pub amount1_out: String,
    pub transaction_hash: String,
    pub log_index: i64,
}

/// DEX Liquidity Position
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DEXLiquidityPosition {
    pub id: String,
    pub pair_id: String,
    pub user: String,
    pub liquidity_token_balance: String,
    pub reserve0: String,
    pub reserve1: String,
    pub reserve0_USD: String,
    pub reserve1_USD: String,
}

/// DEX Pair Snapshot for historical data
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DEXPairSnapshot {
    pub id: String,
    pub pair_id: String,
    pub timestamp: i64,
    pub reserve0: String,
    pub reserve1: String,
    pub total_supply: String,
    pub volume_usd: f64,
    pub volume_token0: f64,
    pub volume_token1: f64,
    pub tx_count: i64,
}

// =============================================================================
// ENUMS
// =============================================================================

/// DEX Protocol
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DEXProtocol {
    PancakeSwap,
    PancakeSwapV3,
    UniswapV2,
    UniswapV3,
    SpookySwap,
    SpiritSwap,
    QuickSwap,
    ApeSwap,
    Unknown,
}

impl Default for DEXProtocol {
    fn default() -> Self {
        Self::Unknown
    }
}

impl std::fmt::Display for DEXProtocol {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DEXProtocol::PancakeSwap => write!(f, "PancakeSwap"),
            DEXProtocol::PancakeSwapV3 => write!(f, "PancakeSwap V3"),
            DEXProtocol::UniswapV2 => write!(f, "Uniswap V2"),
            DEXProtocol::UniswapV3 => write!(f, "Uniswap V3"),
            DEXProtocol::SpookySwap => write!(f, "SpookySwap"),
            DEXProtocol::SpiritSwap => write!(f, "SpiritSwap"),
            DEXProtocol::QuickSwap => write!(f, "QuickSwap"),
            DEXProtocol::ApeSwap => write!(f, "ApeSwap"),
            DEXProtocol::Unknown => write!(f, "Unknown"),
        }
    }
}

/// Chain ID
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ChainId {
    Ethereum = 1,
    Bsc = 56,
    Polygon = 137,
    Arbitrum = 42161,
    Optimism = 10,
    Avalanche = 43114,
    Fantom = 250,
    Base = 8453,
}

impl Default for ChainId {
    fn default() -> Self {
        Self::Bsc
    }
}

impl ChainId {
    pub fn from_u64(v: u64) -> Self {
        match v {
            1 => Self::Ethereum,
            56 => Self::Bsc,
            137 => Self::Polygon,
            42161 => Self::Arbitrum,
            10 => Self::Optimism,
            43114 => Self::Avalanche,
            250 => Self::Fantom,
            8453 => Self::Base,
            _ => Self::Bsc,
        }
    }

    pub fn to_u64(&self) -> u64 {
        match self {
            Self::Ethereum => 1,
            Self::Bsc => 56,
            Self::Polygon => 137,
            Self::Arbitrum => 42161,
            Self::Optimism => 10,
            Self::Avalanche => 43114,
            Self::Fantom => 250,
            Self::Base => 8453,
        }
    }
}

// =============================================================================
// ANALYTICS
// =============================================================================

/// DEX Analytics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DEXAnalytics {
    pub protocol: DEXProtocol,
    pub chain_id: ChainId,
    pub total_pairs: u64,
    pub total_tokens: u64,
    pub total_volume_24h: f64,
    pub total_volume_7d: f64,
    pub total_liquidity: f64,
    pub total_swaps_24h: i64,
    pub top_pairs: Vec<DEXPair>,
    pub top_tokens: Vec<DEXToken>,
}

/// DEX Trade
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DEXTrade {
    pub pair_id: String,
    pub pair_name: String,
    pub from_token: String,
    pub to_token: String,
    pub from_amount: f64,
    pub to_amount: f64,
    pub price: f64,
    pub price_usd: f64,
    pub timestamp: i64,
    pub transaction_hash: String,
}

/// DEX Pool Data
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DEXPoolData {
    pub pair_id: String,
    pub token0: String,
    pub token1: String,
    pub reserve0: f64,
    pub reserve1: f64,
    pub liquidity_usd: f64,
    pub apy: f64,
    pub apr: f64,
    pub daily_fees: f64,
    pub volume_24h: f64,
}

// =============================================================================
// REQUEST/RESPONSE
// =============================================================================

/// Pair Filter
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct PairFilter {
    pub token0: Option<String>,
    pub token1: Option<String>,
    pub min_liquidity: Option<f64>,
    pub min_volume: Option<f64>,
    pub search: Option<String>,
    pub limit: Option<usize>,
    pub offset: Option<usize>,
}

/// Swap Filter
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SwapFilter {
    pub pair_id: Option<String>,
    pub from_block: Option<u64>,
    pub to_block: Option<u64>,
    pub from_time: Option<i64>,
    pub to_time: Option<i64>,
    pub limit: Option<usize>,
}

/// Token Filter
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct TokenFilter {
    pub search: Option<String>,
    pub min_liquidity: Option<f64>,
    pub min_volume: Option<f64>,
    pub limit: Option<usize>,
    pub offset: Option<usize>,
}

// =============================================================================
// RESPONSE TYPES
// =============================================================================

/// API Response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DEXResponse<T> {
    pub success: bool,
    pub data: Option<T>,
    pub error: Option<String>,
    pub timestamp: i64,
}

impl<T> DEXResponse<T> {
    pub fn ok(data: T) -> Self {
        Self {
            success: true,
            data: Some(data),
            error: None,
            timestamp: Utc::now().timestamp(),
        }
    }

    pub fn err(error: String) -> Self {
        Self {
            success: false,
            data: None,
            error: Some(error),
            timestamp: Utc::now().timestamp(),
        }
    }
}

/// Paginated Response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PaginatedResponse<T> {
    pub items: Vec<T>,
    pub total: usize,
    pub limit: usize,
    pub offset: usize,
    pub has_more: bool,
}

// =============================================================================
// CONVERSION HELPERS
// =============================================================================

/// Convert string to f64 safely
pub fn parse_f64(s: &str) -> f64 {
    s.parse().unwrap_or(0.0)
}

/// Convert wei to ether
pub fn wei_to_ether(wei: &str, decimals: u8) -> f64 {
    let value: f64 = wei.parse().unwrap_or(0.0);
    value / (10_f64.powi(decimals as i32))
}

/// Convert ether to wei
pub fn ether_to_wei(ether: f64, decimals: u8) -> String {
    let value = ether * (10_f64.powi(decimals as i32));
    format!("{:0.0}", value)
}

/// Calculate price change percentage
pub fn calculate_price_change(old_price: f64, new_price: f64) -> f64 {
    if old_price == 0.0 {
        return 0.0;
    }
    ((new_price - old_price) / old_price) * 100.0
}

/// Calculate APY from daily reward
pub fn calculate_apy(daily_reward_percent: f64) -> f64 {
    // APY = (1 + daily_rate/365)^365 - 1
    let rate = daily_reward_percent / 100.0;
    ((1.0 + rate / 365.0).powi(365) - 1.0) * 100.0
}

/// Calculate APR from fees
pub fn calculate_apr(volume_usd: f64, liquidity_usd: f64, days: i64) -> f64 {
    if liquidity_usd == 0.0 {
        return 0.0;
    }
    (volume_usd * 0.003 * days as f64 / liquidity_usd) * 100.0
}

// =============================================================================
// TESTS
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_f64() {
        assert_eq!(parse_f64("100.5"), 100.5);
        assert_eq!(parse_f64("invalid"), 0.0);
    }

    #[test]
    fn test_wei_to_ether() {
        let wei = "1000000000000000000"; // 1 ETH
        assert!((wei_to_ether(wei, 18) - 1.0).abs() < 0.001);
    }

    #[test]
    fn test_price_change() {
        let old = 100.0;
        let new = 110.0;
        assert!((calculate_price_change(old, new) - 10.0).abs() < 0.001);
    }

    #[test]
    fn test_chain_id() {
        assert_eq!(ChainId::Bsc.to_u64(), 56);
        assert_eq!(ChainId::from_u64(56), ChainId::Bsc);
    }
}