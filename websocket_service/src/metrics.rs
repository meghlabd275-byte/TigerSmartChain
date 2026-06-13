//! Metrics Collection
//! Performance and monitoring metrics

use std::sync::atomic::{AtomicU64, Ordering};

#[derive(Debug, Clone, Default)]
pub struct Metrics {
    events_sent: AtomicU64,
    events_received: AtomicU64,
    connections_total: AtomicU64,
    connections_active: AtomicU64,
    messages_sent: AtomicU64,
    messages_received: AtomicU64,
    errors: AtomicU64,
}

impl Metrics {
    /// Create new metrics
    pub fn new() -> Self {
        Self::default()
    }
    
    /// Record event sent
    pub fn record_event_sent(&self) {
        self.events_sent.fetch_add(1, Ordering::Relaxed);
    }
    
    /// Record event received
    pub fn record_event_received(&self) {
        self.events_received.fetch_add(1, Ordering::Relaxed);
    }
    
    /// Record new connection
    pub fn record_connection(&self) {
        self.connections_total.fetch_add(1, Ordering::Relaxed);
        self.connections_active.fetch_add(1, Ordering::Relaxed);
    }
    
    /// Record disconnection
    pub fn record_disconnection(&self) {
        self.connections_active.fetch_sub(1, Ordering::Relaxed);
    }
    
    /// Record message sent
    pub fn record_message_sent(&self) {
        self.messages_sent.fetch_add(1, Ordering::Relaxed);
    }
    
    /// Record message received
    pub fn record_message_received(&self) {
        self.messages_received.fetch_add(1, Ordering::Relaxed);
    }
    
    /// Record error
    pub fn record_error(&self) {
        self.errors.fetch_add(1, Ordering::Relaxed);
    }
    
    /// Get events sent
    pub fn events_sent(&self) -> u64 {
        self.events_sent.load(Ordering::Relaxed)
    }
    
    /// Get events received
    pub fn events_received(&self) -> u64 {
        self.events_received.load(Ordering::Relaxed)
    }
    
    /// Get active connections
    pub fn connections_active(&self) -> u64 {
        self.connections_active.load(Ordering::Relaxed)
    }
    
    /// Get total messages
    pub fn messages_total(&self) -> u64 {
        self.messages_sent.load(Ordering::Relaxed) + self.messages_received.load(Ordering::Relaxed)
    }
    
    /// Get errors
    pub fn errors(&self) -> u64 {
        self.errors.load(Ordering::Relaxed)
    }
}