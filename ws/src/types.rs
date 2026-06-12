//! WebSocket Types

use serde::{Deserialize, Serialize};

// =============================================================================
// MESSAGE TYPES
// =============================================================================

/// WebSocket Message
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WSMessage {
    pub r#type: MessageType,
    pub payload: String,
    pub timestamp: i64,
}

/// Message Type
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum MessageType {
    Subscribe,
    Unsubscribe,
    NewBlock,
    NewTx,
    NewPendingTx,
    NewLog,
    NewAccount,
    Error,
}

/// Subscription
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Subscription {
    pub id: String,
    pub channel: Channel,
    pub filter: Option<String>,
}

/// Channel
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Channel {
    NewBlocks,
    NewTransactions,
    PendingTransactions,
    Logs,
    Addresses(Vec<String>),
    Contracts(Vec<String>),
    Tokens(Vec<String>),
}

/// Event
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WSEvent {
    pub channel: String,
    pub data: serde_json::Value,
    pub timestamp: i64,
}

/// Client Info
#[derive(Debug, Clone)]
pub struct WSClient {
    pub id: String,
    pub subscriptions: Vec<String>,
    pub connected_at: i64,
}

// =============================================================================
// CONFIG
// =============================================================================

/// WS Config
#[derive(Debug, Clone)]
pub struct WSConfig {
    pub host: String,
    pub port: u16,
    pub max_connections: usize,
    pub ping_interval: u64,
    pub message_limit: usize,
}

impl Default for WSConfig {
    fn default() -> Self {
        Self {
            host: "0.0.0.0".to_string(),
            port: 8546,
            max_connections: 1000,
            ping_interval: 30,
            message_limit: 1000,
        }
    }
}