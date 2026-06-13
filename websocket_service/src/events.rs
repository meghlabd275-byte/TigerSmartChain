//! WebSocket Events - Real-time blockchain event types
//! High-performance event types for blockchain updates

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// Event types supported by the WebSocket service
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EventType {
    /// New block event
    Block,
    /// Transaction event
    Tx,
    /// Pending transaction event
    PendingTx,
    /// Token transfer event
    TokenTransfer,
    /// Token approval event
    TokenApproval,
    /// NFT transfer event
    NftTransfer,
    /// Contract event
    Contract,
    /// Validator event
    Validator,
    /// Block finality event
    Finality,
    /// Custom event
    Custom,
}

impl EventType {
    /// Get event name as string
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Block => "block",
            Self::Tx => "tx",
            Self::PendingTx => "pending_tx",
            Self::TokenTransfer => "token_transfer",
            Self::TokenApproval => "token_approval",
            Self::NftTransfer => "nft_transfer",
            Self::Contract => "contract",
            Self::Validator => "validator",
            Self::Finality => "finality",
            Self::Custom => "custom",
        }
    }
    
    /// Parse from string
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "block" => Some(Self::Block),
            "tx" => Some(Self::Tx),
            "pending_tx" => Some(Self::PendingTx),
            "token_transfer" => Some(Self::TokenTransfer),
            "token_approval" => Some(Self::TokenApproval),
            "nft_transfer" => Some(Self::NftTransfer),
            "contract" => Some(Self::Contract),
            "validator" => Some(Self::Validator),
            "finality" => Some(Self::Finality),
            "custom" => Some(Self::Custom),
            _ => None,
        }
    }
}

/// Base event structure
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Event {
    /// Event ID
    pub id: String,
    /// Event type
    pub event_type: EventType,
    /// Chain ID
    pub chain_id: u64,
    /// Timestamp
    pub timestamp: DateTime<Utc>,
    /// Event data
    pub data: EventData,
}

/// Event data variants
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum EventData {
    /// Block event data
    Block(BlockEvent),
    /// Transaction event data
    Tx(TxEvent),
    /// Mempool event data
    Mempool(MempoolEvent),
    /// Token transfer event data
    TokenTransfer(TokenTransferEvent),
    /// Custom event data
    Custom(serde_json::Value),
}

/// Block event
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BlockEvent {
    /// Block number
    pub block_number: u64,
    /// Block hash
    pub block_hash: String,
    /// Parent hash
    pub parent_hash: String,
    /// Timestamp
    pub timestamp: u64,
    /// Validator
    pub validator: String,
    /// Transaction count
    pub tx_count: u32,
    /// Gas used
    pub gas_used: u64,
    /// Gas limit
    pub gas_limit: u64,
    /// Base fee per gas
    pub base_fee_per_gas: Option<u64>,
    /// Difficulty
    pub difficulty: Option<String>,
}

impl BlockEvent {
    /// Create new block event
    pub fn new(
        block_number: u64,
        block_hash: String,
        parent_hash: String,
        timestamp: u64,
        validator: String,
    ) -> Self {
        Self {
            block_number,
            block_hash,
            parent_hash,
            timestamp,
            validator,
            tx_count: 0,
            gas_used: 0,
            gas_limit: 0,
            base_fee_per_gas: None,
            difficulty: None,
        }
    }
}

/// Transaction event
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TxEvent {
    /// Transaction hash
    pub hash: String,
    /// Block number
    pub block_number: Option<u64>,
    /// Block hash
    pub block_hash: Option<String>,
    /// From address
    pub from: String,
    /// To address
    pub to: Option<String>,
    /// Value
    pub value: String,
    /// Gas price
    pub gas_price: u64,
    /// Gas limit
    pub gas_limit: u64,
    /// Nonce
    pub nonce: u64,
    /// Transaction index
    pub transaction_index: Option<u32>,
    /// Status
    pub status: Option<String>,
    /// Timestamp
    pub timestamp: u64,
}

impl TxEvent {
    /// Create new transaction event
    pub fn new(
        hash: String,
        from: String,
        to: Option<String>,
        value: String,
    ) -> Self {
        Self {
            hash,
            block_number: None,
            block_hash: None,
            from,
            to,
            value,
            gas_price: 0,
            gas_limit: 0,
            nonce: 0,
            transaction_index: None,
            status: None,
            timestamp: 0,
        }
    }
}

/// Mempool event
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MempoolEvent {
    /// Transaction hash
    pub hash: String,
    /// From address
    pub from: String,
    /// To address
    pub to: Option<String>,
    /// Value
    pub value: String,
    /// Gas price
    pub gas_price: u64,
    /// Gas limit
    pub gas_limit: u64,
    /// Nonce
    pub nonce: u64,
    /// Received at timestamp
    pub received_at: u64,
    /// Transaction data
    pub data: Option<String>,
}

impl MempoolEvent {
    /// Create new mempool event
    pub fn new(hash: String, from: String, gas_price: u64) -> Self {
        Self {
            hash,
            from,
            to: None,
            value: "0x0".to_string(),
            gas_price,
            gas_limit: 0,
            nonce: 0,
            received_at: 0,
            data: None,
        }
    }
}

/// Token transfer event
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenTransferEvent {
    /// Transaction hash
    pub hash: String,
    /// Token address
    pub token: String,
    /// From address
    pub from: String,
    /// To address
    pub to: String,
    /// Value
    pub value: String,
    /// Token ID (for NFTs)
    pub token_id: Option<String>,
    /// Amount (for ERC1155)
    pub amount: Option<String>,
    /// Block number
    pub block_number: u64,
    /// Log index
    pub log_index: u32,
    /// Timestamp
    pub timestamp: u64,
}

impl TokenTransferEvent {
    /// Create new token transfer event
    pub fn new(
        hash: String,
        token: String,
        from: String,
        to: String,
        value: String,
        block_number: u64,
    ) -> Self {
        Self {
            hash,
            token,
            from,
            to,
            value,
            token_id: None,
            amount: None,
            block_number,
            log_index: 0,
            timestamp: 0,
        }
    }
}