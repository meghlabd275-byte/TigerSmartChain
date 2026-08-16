//! TigerSmartChain Bridge Engine
//! 
//! A comprehensive cross-chain bridge system supporting:
//! - Ethereum
//! - Polygon
//! - Arbitrum
//! - Optimism
//! - Base
//!
//! Features:
//! - Token transfers
//! - NFT transfers  
//! - Message passing
//! - Relay support
//! - Validator signatures

pub mod ethereum;
pub mod polygon;
pub mod arbitrum;
pub mod optimism;
pub mod base;
pub mod validator;
pub mod proof;
pub mod message;
pub mod relayer;
pub mod engine;

pub use engine::BridgeEngine;
pub use message::Message;
pub use proof::Proof;

/// Chain identifier
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum Chain {
    TigerSmartChain,
    Ethereum,
    Polygon,
    Arbitrum,
    Optimism,
    Base,
}

impl Chain {
    pub fn chain_id(&self) -> u64 {
        match self {
            Chain::TigerSmartChain => 6666,
            Chain::Ethereum => 1,
            Chain::Polygon => 137,
            Chain::Arbitrum => 42161,
            Chain::Optimism => 10,
            Chain::Base => 8453,
        }
    }

    pub fn from_chain_id(id: u64) -> Option<Self> {
        match id {
            6666 => Some(Chain::TigerSmartChain),
            1 => Some(Chain::Ethereum),
            137 => Some(Chain::Polygon),
            42161 => Some(Chain::Arbitrum),
            10 => Some(Chain::Optimism),
            8453 => Some(Chain::Base),
            _ => None,
        }
    }
}

/// Transfer token type
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub enum TokenType {
    ERC20,
    ERC721,
    ERC1155,
}

/// Transfer status
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TransferStatus {
    Pending,
    Minting,
    Completed,
    Failed,
    Refunded,
}

/// Transfer result
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Transfer {
    pub id: String,
    pub source_chain: Chain,
    pub destination_chain: Chain,
    pub sender: String,
    pub recipient: String,
    pub token: String,
    pub token_type: TokenType,
    pub amount: String,
    pub token_id: Option<String>,
    pub status: TransferStatus,
    pub source_tx: String,
    pub destination_tx: Option<String>,
    pub timestamp: i64,
    pub confirmations: u64,
}

/// Bridge configuration
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct BridgeConfig {
    pub chains: Vec<ChainConfig>,
    pub relayers: Vec<String>,
    pub validators: Vec<String>,
    pub signature_threshold: usize,
    pub confirmation_blocks: u64,
    pub fee: FeeConfig,
    #[serde(default)]
    pub database_url: String,
}

/// Chain configuration
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ChainConfig {
    pub chain: Chain,
    pub rpc_url: String,
    pub contract_address: String,
    pub start_block: u64,
}

/// Fee configuration
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct FeeConfig {
    pub flat_fee: String,
    pub percentage_fee: f64,
    pub min_fee: String,
    pub max_fee: String,
}