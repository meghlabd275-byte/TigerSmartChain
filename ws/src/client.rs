//! WebSocket Client

use crate::types::*;
use std::sync::Arc;
use tokio::sync::RwLock;

// =============================================================================
// CLIENT
// =============================================================================

/// WebSocket Client
pub struct WSClientConnection {
    url: String,
    connected: Arc<RwLock<bool>>,
    subscriptions: Arc<RwLock<Vec<String>>>,
}

impl WSClientConnection {
    pub fn new(url: &str) -> Self {
        Self {
            url: url.to_string(),
            connected: Arc::new(RwLock::new(false)),
            subscriptions: Arc::new(RwLock::new(vec![])),
        }
    }

    /// Connect
    pub async fn connect(&self) -> Result<(), String> {
        *self.connected.write().await = true;
        Ok(())
    }

    /// Disconnect
    pub async fn disconnect(&self) {
        *self.connected.write().await = false;
    }

    /// Subscribe
    pub async fn subscribe(&self, channel: &str) -> Result<(), String> {
        if !*self.connected.read().await {
            return Err("Not connected".to_string());
        }
        self.subscriptions.write().await.push(channel.to_string());
        Ok(())
    }

    /// Receive message
    pub async fn receive(&self) -> Option<WSMessage> {
        None
    }

    /// Is connected
    pub async fn is_connected(&self) -> bool {
        *self.connected.read().await
    }
}

// =============================================================================
// TESTS
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_client() {
        let client = WSClientConnection::new("ws://localhost:8546");
        assert!(!client.is_connected());
    }
}