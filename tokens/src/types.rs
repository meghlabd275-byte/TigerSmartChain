//! Token Types

use serde::{Deserialize, Serialize};

/// Token
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Token {
    pub address: String,
    pub name: String,
    pub symbol: String,
    pub decimals: u8,
    pub total_supply: String,
    pub holders: i64,
    pub transfers: i64,
}

/// Token Transfer
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenTransfer {
    pub hash: String,
    pub from: String,
    pub to: String,
    pub value: String,
    pub timestamp: i64,
}

/// Token Holder
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenHolder {
    pub address: String,
    pub balance: String,
    pub percent: f64,
}

/// Token Price
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenPrice {
    pub address: String,
    pub price: f64,
    pub price_change_24h: f64,
    pub volume_24h: f64,
    pub market_cap: f64,
}