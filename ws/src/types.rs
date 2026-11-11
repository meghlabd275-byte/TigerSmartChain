//! WebSocket message and subscription types.

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

/// A frame delivered to a subscriber.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct WSMessage {
    pub r#type: MessageType,
    pub payload: String,
    pub timestamp: i64,
    /// Subscription id the frame belongs to, when it is a notification.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub channel: Option<String>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum MessageType {
    Subscribe,
    Unsubscribe,
    Event,
    Error,
}

/// A subscription request, before the server assigns it an id.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Subscription {
    pub id: String,
    pub channel: Channel,
    pub filter: Option<String>,
}

/// What a client wants to receive.
///
/// The address-carrying variants filter: a subscription with addresses only
/// matches broadcasts that name one of them.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum Channel {
    NewBlocks,
    NewTransactions,
    PendingTransactions,
    Logs { addresses: Vec<String> },
    Addresses(Vec<String>),
    Contracts(Vec<String>),
    Tokens(Vec<String>),
}

impl Channel {
    /// The `eth_subscribe` params that produce this channel.
    pub fn subscribe_params(&self) -> Value {
        match self {
            Channel::NewBlocks => json!(["newHeads"]),
            Channel::NewTransactions => json!(["newTransactions"]),
            Channel::PendingTransactions => json!(["newPendingTransactions"]),
            Channel::Logs { addresses } => {
                if addresses.is_empty() {
                    json!(["logs"])
                } else {
                    json!(["logs", { "address": addresses }])
                }
            }
            Channel::Addresses(a) => json!(["addresses", a]),
            Channel::Contracts(a) => json!(["contracts", a]),
            Channel::Tokens(a) => json!(["tokens", a]),
        }
    }

    /// Whether a broadcast on `other` should reach a subscriber of `self`.
    ///
    /// Address filters are matched case-insensitively, and a subscriber with no
    /// filter receives everything on its channel kind.
    pub fn matches(&self, other: &Channel) -> bool {
        match (self, other) {
            (Channel::NewBlocks, Channel::NewBlocks)
            | (Channel::NewTransactions, Channel::NewTransactions)
            | (Channel::PendingTransactions, Channel::PendingTransactions) => true,

            (Channel::Logs { addresses }, Channel::Logs { addresses: event }) => {
                addresses.is_empty() || addresses.iter().any(|a| contains_ci(event, a))
            }
            (Channel::Addresses(wanted), Channel::Addresses(event)) => {
                wanted.is_empty() || wanted.iter().any(|a| contains_ci(event, a))
            }
            (Channel::Contracts(wanted), Channel::Contracts(event)) => {
                wanted.is_empty() || wanted.iter().any(|a| contains_ci(event, a))
            }
            (Channel::Tokens(wanted), Channel::Tokens(event)) => {
                wanted.is_empty() || wanted.iter().any(|a| contains_ci(event, a))
            }
            _ => false,
        }
    }
}

fn contains_ci(haystack: &[String], needle: &str) -> bool {
    haystack
        .iter()
        .any(|item| item.eq_ignore_ascii_case(needle))
}

/// An event ready to broadcast.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct WSEvent {
    pub channel: String,
    pub data: Value,
    pub timestamp: i64,
}

/// Server-side record of a connected client.
#[derive(Debug, Clone, PartialEq)]
pub struct WSClient {
    pub id: String,
    pub subscriptions: Vec<String>,
    pub connected_at: i64,
}

/// Server configuration.
#[derive(Debug, Clone)]
pub struct WSConfig {
    pub host: String,
    pub port: u16,
    pub max_connections: usize,
    pub ping_interval: u64,
    /// Frames queued per client before the slowest ones are dropped.
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_unfiltered_subscription_matches_everything_on_its_kind() {
        let all_logs = Channel::Logs {
            addresses: Vec::new(),
        };
        assert!(all_logs.matches(&Channel::Logs {
            addresses: vec!["0xdead".into()]
        }));
    }

    #[test]
    fn address_filters_are_case_insensitive() {
        let filtered = Channel::Addresses(vec!["0xABCDEF".into()]);
        assert!(filtered.matches(&Channel::Addresses(vec!["0xabcdef".into()])));
        assert!(!filtered.matches(&Channel::Addresses(vec!["0x123456".into()])));
    }

    #[test]
    fn channels_of_different_kinds_never_match() {
        let logs = Channel::Logs {
            addresses: Vec::new(),
        };
        assert!(!logs.matches(&Channel::NewBlocks));
        assert!(!Channel::NewBlocks.matches(&logs));
        assert!(!Channel::Tokens(vec![]).matches(&Channel::Contracts(vec![])));
    }

    #[test]
    fn subscribe_params_omit_an_empty_address_list() {
        assert_eq!(
            Channel::Logs {
                addresses: Vec::new()
            }
            .subscribe_params(),
            json!(["logs"])
        );
    }
}
