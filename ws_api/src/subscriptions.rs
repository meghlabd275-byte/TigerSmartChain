//! WebSocket Subscriptions

use serde::{Deserialize, Serialize};

// =============================================================================
// SUBSCRIPTION
// =============================================================================

/// Subscription
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Subscription {
    /// Subscription ID
    pub id: String,
    /// Client ID
    pub client_id: String,
    /// Channel name
    pub channel: String,
    /// Parameters
    pub params: Option<SubscribeParams>,
}

/// Subscribe parameters
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubscribeParams {
    /// Address filter
    pub address: Option<String>,
    /// Topics filter
    pub topics: Option<Vec<String>>,
    /// From block
    pub from_block: Option<u64>,
}

// =============================================================================
// CHANNEL MANAGER
// =============================================================================

/// Channel manager for handling subscriptions
pub struct ChannelManager {
    subscriptions: std::collections::HashMap<String, Vec<Subscription>>,
}

impl ChannelManager {
    /// Create new channel manager
    pub fn new() -> Self {
        Self {
            subscriptions: std::collections::HashMap::new(),
        }
    }

    /// Subscribe to a channel
    pub fn subscribe(&mut self, channel: String, subscription: Subscription) {
        self.subscriptions
            .entry(channel)
            .or_insert_with(Vec::new)
            .push(subscription);
    }

    /// Unsubscribe from a channel
    pub fn unsubscribe(&mut self, channel: &str, client_id: &str) {
        if let Some(subs) = self.subscriptions.get_mut(channel) {
            subs.retain(|s| s.client_id != client_id);
        }
    }

    /// Get subscribers for a channel
    pub fn get_subscribers(&self, channel: &str) -> Vec<&Subscription> {
        self.subscriptions
            .get(channel)
            .map(|s| s.iter().collect())
            .unwrap_or_default()
    }

    /// Get client subscriptions
    pub fn get_client_subscriptions(&self, client_id: &str) -> Vec<Subscription> {
        self.subscriptions
            .values()
            .flatten()
            .filter(|s| s.client_id == client_id)
            .cloned()
            .collect()
    }
}

impl Default for ChannelManager {
    fn default() -> Self {
        Self::new()
    }
}