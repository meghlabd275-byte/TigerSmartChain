//! WebSocket Server

use crate::types::*;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

// =============================================================================
// SERVER
// =============================================================================

/// WebSocket Server
pub struct WSServer {
    config: WSConfig,
    clients: Arc<RwLock<HashMap<String, WSClient>>>,
    subscriptions: Arc<RwLock<HashMap<String, Vec<String>>>>,
}

impl WSServer {
    pub fn new(config: WSConfig) -> Self {
        Self {
            config,
            clients: Arc::new(RwLock::new(HashMap::new())),
            subscriptions: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Start server
    pub async fn start(&self) -> Result<(), String> {
        let addr = format!("{}:{}", self.config.host, self.config.port);
        log::info!("Starting WS server on {}", addr);
        Ok(())
    }

    /// Handle connection
    pub async fn handle_connection(&self, client_id: &str) {
        let client = WSClient {
            id: client_id.to_string(),
            subscriptions: vec![],
            connected_at: Utc::now().timestamp(),
        };
        self.clients.write().await.insert(client_id.to_string(), client);
    }

    /// Subscribe
    pub async fn subscribe(&self, client_id: &str, channel: Channel) -> Result<(), String> {
        let mut clients = self.clients.write().await;
        if let Some(client) = clients.get_mut(client_id) {
            client.subscriptions.push(format!("{:?}", channel));
        }
        Ok(())
    }

    /// Broadcast to channel
    pub async fn broadcast(&self, channel: &str, event: &WSEvent) {
        let subscriptions = self.subscriptions.read().await;
        if let Some(clients) = subscriptions.get(channel) {
            for client_id in clients {
                // Would send to client
            }
        }
    }
}

// =============================================================================
// BUILDER
// =============================================================================

/// Builder
pub struct WSServerBuilder {
    config: WSConfig,
}

impl WSServerBuilder {
    pub fn new() -> Self {
        Self { config: WSConfig::default() }
    }

    pub fn port(mut self, port: u16) -> Self {
        self.config.port = port;
        self
    }

    pub fn build(self) -> WSServer {
        WSServer::new(self.config)
    }
}

impl Default for WSServerBuilder {
    fn default() -> Self {
        Self::new()
    }
}