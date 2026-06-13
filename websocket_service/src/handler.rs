//! WebSocket Message Handler
//! Secure message handling with validation

use crate::config::Config;
use crate::connection::Connection;
use crate::error::{Error, Result};
use crate::events::{Event, EventType};
use crate::subscription::{Channel, Filter, Subscription};

pub struct Handler {
    config: Config,
}

impl Handler {
    /// Create new handler
    pub fn new(config: Config) -> Self {
        Self { config }
    }
    
    /// Handle incoming message
    pub async fn handle_message(&self, connection: &Connection, data: &[u8]) -> Result<()> {
        // Parse message
        let msg: serde_json::Value = serde_json::from_slice(data)
            .map_err(|e| Error::message(format!("Invalid JSON: {}", e)))?;
        
        // Get message type
        let msg_type = msg.get("type")
            .and_then(|v| v.as_str())
            .ok_or_else(|| Error::message("Missing message type"))?;
        
        match msg_type {
            "subscribe" => self.handle_subscribe(connection, msg).await,
            "unsubscribe" => self.handle_unsubscribe(connection, msg).await,
            "ping" => self.handle_ping(connection, msg).await,
            _ => Err(Error::message(format!("Unknown message type: {}", msg_type))),
        }
    }
    
    /// Handle subscribe message
    async fn handle_subscribe(&self, connection: &Connection, msg: serde_json::Value) -> Result<()> {
        let channel = msg.get("channel")
            .and_then(|v| v.as_str())
            .ok_or_else(|| Error::message("Missing channel"))?;
        
        let channel = Channel::parse(channel)
            .ok_or_else(|| Error::message("Invalid channel"))?;
        
        let filter = msg.get("filter")
            .and_then(|v| serde_json::from_value(v.clone()).ok())
            .unwrap_or_default();
        
        let subscription = Subscription::new(channel, filter);
        
        connection.add_subscription(subscription);
        
        Ok(())
    }
    
    /// Handle unsubscribe message
    async fn handle_unsubscribe(&self, connection: &Connection, msg: serde_json::Value) -> Result<()> {
        let subscription_id = msg.get("subscription_id")
            .and_then(|v| v.as_str())
            .ok_or_else(|| Error::message("Missing subscription_id"))?;
        
        connection.remove_subscription(subscription_id);
        
        Ok(())
    }
    
    /// Handle ping message
    async fn handle_ping(&self, connection: &Connection, _msg: serde_json::Value) -> Result<()> {
        let response = serde_json::json!({
            "type": "pong",
            "timestamp": std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs(),
        });
        
        let data = serde_json::to_vec(&response)
            .map_err(|e| Error::message(format!("Serialization error: {}", e)))?;
        
        connection.send(data).await?;
        
        Ok(())
    }
}

impl Clone for Handler {
    fn clone(&self) -> Self {
        Self {
            config: self.config.clone(),
        }
    }
}