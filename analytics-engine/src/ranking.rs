//! Token Ranking

use serde::{Deserialize, Serialize};

/// Token rank
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenRank {
    pub rank: usize,
    pub address: String,
    pub name: String,
    pub symbol: String,
    pub price_usd: f64,
    pub change_24h: f64,
    pub volume_24h: f64,
    pub market_cap: f64,
    pub holders: u64,
    pub transfers: u64,
    pub score: f64,
}

/// Sort by metric
#[derive(Debug, Clone, Copy)]
pub enum RankBy {
    Volume,
    MarketCap,
    Holders,
    Transfers,
    PriceChange,
    Score,
}