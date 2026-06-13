//! WebSocket Messages

use serde::{Deserialize, Serialize};

// =============================================================================
// CLIENT MESSAGES
// =============================================================================

/// Client message (from WebSocket client)
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum ClientMessage {
    /// Subscribe to a channel
    Subscribe {
        /// Channel name
        channel: String,
        /// Optional parameters
        params: Option<SubscribeParams>,
    },

    /// Unsubscribe from a channel
    Unsubscribe {
        /// Channel name
        channel: String,
    },

    /// Ping
    Ping,
}

/// Subscribe parameters
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubscribeParams {
    /// Address filter (for token transfers, logs, etc.)
    pub address: Option<String>,
    /// Topics filter (for logs)
    pub topics: Option<Vec<String>>,
    /// From block (for historical)
    pub from_block: Option<u64>,
}

// =============================================================================
// SERVER MESSAGES
// =============================================================================

/// Server message (to WebSocket client)
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum WsMessage {
    /// Welcome message
    Welcome(WelcomeMessage),
    /// Block event
    Block(BlockEvent),
    /// Transaction event
    Transaction(TxEvent),
    /// Pending transaction event
    PendingTransaction(PendingTxEvent),
    /// Log event
    Log(LogEvent),
    /// Gas price event
    GasPrice(GasEvent),
    /// Error
    Error(ErrorMessage),
    /// Pong
    Pong,
}

/// Welcome message
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WelcomeMessage {
    /// Client ID
    pub client_id: String,
    /// Message
    pub message: String,
}

// =============================================================================
// EVENTS
// =============================================================================

/// Block event
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BlockEvent {
    /// Block number
    pub number: u64,
    /// Block hash
    pub hash: String,
    /// Parent hash
    pub parent_hash: String,
    /// Miner
    pub miner: String,
    /// Timestamp
    pub timestamp: u64,
    /// Transaction count
    pub tx_count: u64,
    /// Gas used
    pub gas_used: u64,
    /// Base fee per gas
    pub base_fee_per_gas: Option<u64>,
}

/// Transaction event
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TxEvent {
    /// Transaction hash
    pub hash: String,
    /// Block number
    pub block_number: u64,
    /// From address
    pub from: String,
    /// To address
    pub to: Option<String>,
    /// Value (wei)
    pub value: String,
    /// Gas price
    pub gas_price: u64,
    /// Input data
    pub input: String,
    /// Timestamp
    pub timestamp: u64,
}

/// Pending transaction event
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PendingTxEvent {
    /// Transaction hash
    pub hash: String,
    /// From address
    pub from: String,
    /// To address
    pub to: Option<String>,
    /// Value (wei)
    pub value: String,
    /// Gas price
    pub gas_price: u64,
    /// Nonce
    pub nonce: u64,
    /// Received at
    pub received_at: u64,
}

/// Log event
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LogEvent {
    /// Contract address
    pub address: String,
    /// Topics
    pub topics: Vec<String>,
    /// Data
    pub data: String,
    /// Block number
    pub block_number: u64,
    /// Transaction hash
    pub transaction_hash: String,
    /// Log index
    pub log_index: u64,
}

/// Gas price event
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GasEvent {
    /// Slow gas price (wei)
    pub slow: u64,
    /// Standard gas price (wei)
    pub standard: u64,
    /// Fast gas price (wei)
    pub fast: u64,
    /// Base fee (wei)
    pub base_fee: u64,
    /// Timestamp
    pub timestamp: u64,
}

/// Error message
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ErrorMessage {
    /// Error code
    pub code: i32,
    /// Error message
    pub message: String,
}

// =============================================================================
// CHANNELS
// =============================================================================

/// Available channels
pub const CHANNELS: &[&str] = &[
    "new_blocks",
    "new_transactions",
    "pending_transactions",
    "logs",
    "gas_price",
    "token_transfers",
    "nft_transfers",
    "alerts",
];

/// Channel types
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Channel {
    NewBlocks,
    NewTransactions,
    PendingTransactions,
    Logs,
    GasPrice,
    TokenTransfers,
    NFTTransfers,
    Alerts,
}

impl Channel {
    /// Parse from string
    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "new_blocks" => Some(Self::NewBlocks),
            "new_transactions" => Some(Self::NewTransactions),
            "pending_transactions" => Some(Self::PendingTransactions),
            "logs" => Some(Self::Logs),
            "gas_price" => Some(Self::GasPrice),
            "token_transfers" => Some(Self::TokenTransfers),
            "nft_transfers" => Some(Self::NFTTransfers),
            "alerts" => Some(Self::Alerts),
            _ => None,
        }
    }
}