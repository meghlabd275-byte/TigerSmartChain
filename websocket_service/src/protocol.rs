//! Protocol - WebSocket protocol definitions

use serde::{Deserialize, Serialize};

/// WebSocket protocol version
pub const PROTOCOL_VERSION: &str = "1.0.0";

/// Protocol message types
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProtocolMessageType {
    Subscribe,
    Unsubscribe,
    Event,
    Ping,
    Pong,
    Error,
}

/// Outgoing message structure
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OutgoingMessage {
    pub msg_type: ProtocolMessageType,
    pub subscription_id: Option<String>,
    pub data: serde_json::Value,
}

/// Incoming message structure
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IncomingMessage {
    pub msg_type: ProtocolMessageType,
    pub channel: Option<String>,
    pub filter: Option<serde_json::Value>,
    pub subscription_id: Option<String>,
}

pub struct Protocol;

impl Protocol {
    pub fn version() -> &'static str {
        PROTOCOL_VERSION
    }
}