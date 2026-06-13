//! WebSocket Connection Management
//! Secure connection handling with rate limiting and authentication

use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use tokio::sync::mpsc;
use uuid::Uuid;

use crate::config::Config;
use crate::error::{Error, Result};
use crate::subscription::Subscription;

/// Connection state
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConnectionState {
    /// Connecting
    Connecting,
    /// Authenticating
    Authenticating,
    /// Connected
    Connected,
    /// Disconnecting
    Disconnecting,
    /// Disconnected
    Disconnected,
}

/// Connection information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConnectionInfo {
    /// Connection ID
    pub id: String,
    /// Remote address
    pub remote_addr: String,
    /// State
    pub state: ConnectionState,
    /// Subscriptions count
    pub subscriptions: usize,
    /// Messages sent
    pub messages_sent: u64,
    /// Messages received
    pub messages_received: u64,
    /// Bytes sent
    pub bytes_sent: u64,
    /// Bytes received
    pub bytes_received: u64,
    /// Connected at
    pub connected_at: i64,
    /// Last activity at
    pub last_activity_at: i64,
    /// Protocol version
    pub protocol_version: Option<String>,
}

/// Connection manager
pub struct ConnectionManager {
    config: Config,
    connections: RwLock<HashMap<String, Connection>>,
    rate_limiters: RwLock<HashMap<String, RateLimiter>>,
}

impl ConnectionManager {
    /// Create new connection manager
    pub fn new(config: Config) -> Self {
        Self {
            config,
            connections: RwLock::new(HashMap::new()),
            rate_limiters: RwLock::new(HashMap::new()),
        }
    }
    
    /// Create new connection
    pub fn create_connection(&self, remote_addr: String) -> Result<Connection> {
        let id = Uuid::new_v4().to_string();
        
        let connection = Connection::new(
            id.clone(),
            remote_addr,
            self.config.clone(),
        );
        
        // Check max connections
        {
            let connections = self.connections.read();
            if connections.len() >= self.config.max_connections {
                return Err(Error::connection("Maximum connections reached"));
            }
        }
        
        // Add connection
        {
            let mut connections = self.connections.write();
            connections.insert(id.clone(), connection.clone());
        }
        
        // Initialize rate limiter
        {
            let mut rate_limiters = self.rate_limiters.write();
            rate_limiters.insert(id.clone(), RateLimiter::new(self.config.rate_limit));
        }
        
        Ok(connection)
    }
    
    /// Get connection by ID
    pub fn get_connection(&self, id: &str) -> Option<Connection> {
        let connections = self.connections.read();
        connections.get(id).cloned()
    }
    
    /// Remove connection
    pub fn remove_connection(&self, id: &str) {
        let mut connections = self.connections.write();
        connections.remove(id);
        
        let mut rate_limiters = self.rate_limiters.write();
        rate_limiters.remove(id);
    }
    
    /// Get all connections
    pub fn get_connections(&self) -> Vec<ConnectionInfo> {
        let connections = self.connections.read();
        connections.values().map(|c| c.info()).collect()
    }
    
    /// Check rate limit
    pub fn check_rate_limit(&self, id: &str) -> Result<()> {
        let rate_limiters = self.rate_limiters.read();
        if let Some(limiter) = rate_limiters.get(id) {
            if !limiter.check() {
                return Err(Error::rate_limit("Rate limit exceeded"));
            }
        }
        Ok(())
    }
    
    /// Get connection count
    pub fn connection_count(&self) -> usize {
        self.connections.read().len()
    }
}

/// Connection entity
#[derive(Debug, Clone)]
pub struct Connection {
    id: String,
    remote_addr: String,
    state: RwLock<ConnectionState>,
    subscriptions: RwLock<Vec<Subscription>>,
    config: Config,
    sender: RwLock<Option<mpsc::Sender<Vec<u8>>>>,
    stats: RwLock<ConnectionStats>,
    created_at: Instant,
    last_activity: RwLock<Instant>,
}

#[derive(Debug, Clone, Default)]
struct ConnectionStats {
    messages_sent: u64,
    messages_received: u64,
    bytes_sent: u64,
    bytes_received: u64,
}

impl Connection {
    /// Create new connection
    pub fn new(id: String, remote_addr: String, config: Config) -> Self {
        Self {
            id,
            remote_addr,
            state: RwLock::new(ConnectionState::Connecting),
            subscriptions: RwLock::new(Vec::new()),
            config,
            sender: RwLock::new(None),
            stats: RwLock::new(ConnectionStats::default()),
            created_at: Instant::now(),
            last_activity: RwLock::new(Instant::now()),
        }
    }
    
    /// Get connection ID
    pub fn id(&self) -> &str {
        &self.id
    }
    
    /// Get remote address
    pub fn remote_addr(&self) -> &str {
        &self.remote_addr
    }
    
    /// Get connection state
    pub fn state(&self) -> ConnectionState {
        *self.state.read()
    }
    
    /// Set connection state
    pub fn set_state(&self, state: ConnectionState) {
        *self.state.write() = state;
    }
    
    /// Add subscription
    pub fn add_subscription(&self, sub: Subscription) {
        let mut subs = self.subscriptions.write();
        subs.push(sub);
    }
    
    /// Remove subscription
    pub fn remove_subscription(&self, id: &str) {
        let mut subs = self.subscriptions.write();
        subs.retain(|s| s.id() != id);
    }
    
    /// Get subscriptions
    pub fn subscriptions(&self) -> Vec<Subscription> {
        self.subscriptions.read().clone()
    }
    
    /// Set message sender
    pub fn set_sender(&self, sender: mpsc::Sender<Vec<u8>>) {
        *self.sender.write() = Some(sender);
    }
    
    /// Send message
    pub async fn send(&self, data: Vec<u8>) -> Result<()> {
        let sender = self.sender.read();
        if let Some(sender) = sender.as_ref() {
            sender.send(data).await.map_err(|_| Error::connection("Channel closed"))?;
            
            let mut stats = self.stats.write();
            stats.messages_sent += 1;
            stats.bytes_sent += data.len() as u64;
        }
        Ok(())
    }
    
    /// Record message received
    pub fn record_received(&self, size: usize) {
        let mut stats = self.stats.write();
        stats.messages_received += 1;
        stats.bytes_received += size as u64;
        *self.last_activity.write() = Instant::now();
    }
    
    /// Get connection info
    pub fn info(&self) -> ConnectionInfo {
        let state = self.state();
        let subs = self.subscriptions();
        let stats = self.stats.read();
        
        ConnectionInfo {
            id: self.id.clone(),
            remote_addr: self.remote_addr.clone(),
            state,
            subscriptions: subs.len(),
            messages_sent: stats.messages_sent,
            messages_received: stats.messages_received,
            bytes_sent: stats.bytes_sent,
            bytes_received: stats.bytes_received,
            connected_at: self.created_at.elapsed().as_secs() as i64,
            last_activity_at: self.last_activity.read().elapsed().as_secs() as i64,
            protocol_version: None,
        }
    }
    
    /// Check if connection is active
    pub fn is_active(&self) -> bool {
        matches!(self.state(), ConnectionState::Connected)
    }
}

/// Rate limiter
#[derive(Debug, Clone)]
pub struct RateLimiter {
    limit: usize,
    window: Duration,
    tokens: RwLock<usize>,
    last_refill: RwLock<Instant>,
}

impl RateLimiter {
    /// Create new rate limiter
    pub fn new(limit: usize) -> Self {
        Self {
            limit,
            window: Duration::from_secs(1),
            tokens: RwLock::new(limit),
            last_refill: RwLock::new(Instant::now()),
        }
    }
    
    /// Check if request is allowed
    pub fn check(&self) -> bool {
        self.refill();
        let tokens = self.tokens.read();
        if *tokens > 0 {
            *tokens -= 1;
            true
        } else {
            false
        }
    }
    
    /// Refill tokens
    fn refill(&self) {
        let last = *self.last_refill.read();
        if last.elapsed() >= self.window {
            *self.tokens.write() = self.limit;
            *self.last_refill.write() = Instant::now();
        }
    }
}