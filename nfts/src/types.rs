//! NFT Types

use serde::{Deserialize, Serialize};

/// NFT Collection
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NFTCollection {
    pub address: String,
    pub name: String,
    pub symbol: String,
    pub nft_type: NFTType,
    pub total_supply: String,
    pub holders: i64,
}

/// NFT Type
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum NFTType {
    ERC721,
    ERC721A,
    ERC1155,
}

/// NFT
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NFT {
    pub address: String,
    pub token_id: String,
    pub owner: String,
    pub uri: String,
    pub metadata: Option<String>,
}

/// NFT Transfer
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NFTTransfer {
    pub hash: String,
    pub from: String,
    pub to: String,
    pub token_id: String,
    pub amount: String,
    pub timestamp: i64,
}

/// NFT Holder
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NFTHolder {
    pub address: String,
    pub balance: i64,
}