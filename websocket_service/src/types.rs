//! WebSocket Service Types

use serde::{Deserialize, Serialize};

// =============================================================================
// WEBSOCKET SERVICE
// =============================================================================

/// WebSocket Event
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WebSocketEvent {
    pub event_type: String,
    pub data: String,
    pub timestamp: u64,
}

/// Subscription
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Subscription {
    pub id: String,
    pub channel: String,
    pub filters: std::collections::HashMap<String, String>,
}

/// WebSocket Service
pub struct Service {
    subscriptions: std::collections::HashMap<String, Subscription>,
}

impl Service {
    pub fn new() -> Self {
        Self {
            subscriptions: std::collections::HashMap::new(),
        }
    }

    /// Subscribe
    pub fn subscribe(&mut self, subscription: Subscription) {
        self.subscriptions.insert(subscription.id.clone(), subscription);
    }

    /// Unsubscribe
    pub fn unsubscribe(&mut self, id: &str) {
        self.subscriptions.remove(id);
    }
}

impl Default for Service {
    fn default() -> Self {
        Self::new()
    }
}