//! NFT Analytics

use serde::{Deserialize, Serialize};

/// NFT collection stats
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NFTCollectionStats {
    pub address: String,
    pub name: String,
    pub floor_price: f64,
    pub volume_24h: f64,
    pub sales_24h: u64,
    pub holders: u64,
    pub total_supply: u64,
    pub average_price: f64,
    pub price_change_24h: f64,
}

/// NFT sale
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NFTSale {
    pub token_id: String,
    pub seller: String,
    pub buyer: String,
    pub price: String,
    pub timestamp: i64,
    pub transaction_hash: String,
}