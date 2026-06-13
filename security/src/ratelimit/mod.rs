//! TigerSmartChain Security Module - Rate Limiting
//! 
//! Provides advanced rate limiting with:
//! - Token bucket algorithm
//! - Sliding window
//! - Leaky bucket
//! - Per-IP and per-user limits
//! - Automatic blocking

use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::RwLock;

// ============================================================================
// CONSTANTS
// ============================================================================

pub const DEFAULT_RATE_WINDOW_MS: u64 = 60000;      // 1 minute
pub const DEFAULT_RATE_MAX_REQUESTS: u64 = 100;
pub const DEFAULT_BURST_ALLOWED: u64 = 10;
pub const DEFAULT_BLOCK_DURATION_MS: u64 = 900000;   // 15 minutes

// ============================================================================
// ERROR TYPES
// ============================================================================

#[derive(Debug, Clone)]
pub enum RateLimitError {
    RateLimitExceeded,
    Blocked,
    InvalidConfiguration,
}

impl std::fmt::Display for RateLimitError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::RateLimitExceeded => write!(f, "Rate limit exceeded"),
            Self::Blocked => write!(f, "IP blocked"),
            Self::InvalidConfiguration => write!(f, "Invalid configuration"),
        }
    }
}

impl std::error::Error for RateLimitError {}

// ============================================================================
// RATE LIMIT CONFIG
// ============================================================================

#[derive(Debug, Clone)]
pub struct RateLimitConfig {
    pub window_ms: u64,
    pub max_requests: u64,
    pub burst_allowed: u64,
    pub block_duration_ms: u64,
}

impl Default for RateLimitConfig {
    fn default() -> Self {
        Self {
            window_ms: DEFAULT_RATE_WINDOW_MS,
            max_requests: DEFAULT_RATE_MAX_REQUESTS,
            burst_allowed: DEFAULT_BURST_ALLOWED,
            block_duration_ms: DEFAULT_BLOCK_DURATION_MS,
        }
    }
}

impl RateLimitConfig {
    pub fn new(window_ms: u64, max_requests: u64) -> Self {
        Self {
            window_ms,
            max_requests,
            burst_allowed: DEFAULT_BURST_ALLOWED,
            block_duration_ms: DEFAULT_BLOCK_DURATION_MS,
        }
    }
    
    pub fn strict() -> Self {
        Self {
            window_ms: 60000,
            max_requests: 10,
            burst_allowed: 2,
            block_duration_ms: 1800000, // 30 minutes
        }
    }
    
    pub fn relaxed() -> Self {
        Self {
            window_ms: 60000,
            max_requests: 500,
            burst_allowed: 50,
            block_duration_ms: 300000, // 5 minutes
        }
    }
}

// ============================================================================
// CLIENT INFO
// ============================================================================

#[derive(Debug, Clone)]
pub struct ClientInfo {
    pub ip: String,
    pub user_agent: Option<String>,
    pub user_id: Option<String>,
    pub requests: Vec<Instant>,
    pub blocked_until: Option<Instant>,
    pub trust_score: f64,
}

impl ClientInfo {
    pub fn new(ip: String) -> Self {
        Self {
            ip,
            user_agent: None,
            user_id: None,
            requests: Vec::new(),
            blocked_until: None,
            trust_score: 100.0,
        }
    }
    
    pub fn is_blocked(&self) -> bool {
        if let Some(until) = self.blocked_until {
            return Instant::now() < until;
        }
        false
    }
    
    pub fn request_count(&self, window: Duration) -> usize {
        let now = Instant::now();
        self.requests
            .iter()
            .filter(|&&req| now.duration_since(req) < window)
            .count()
    }
}

// ============================================================================
// TOKEN BUCKET RATE LIMITER
// ============================================================================

/// Token bucket algorithm implementation
pub struct TokenBucket {
    capacity: u64,
    refill_rate: f64,
    tokens: f64,
    last_refill: Instant,
}

impl TokenBucket {
    pub fn new(capacity: u64, refill_per_second: f64) -> Self {
        Self {
            capacity,
            refill_rate: refill_per_second,
            tokens: capacity as f64,
            last_refill: Instant::now(),
        }
    }
    
    /// Try to consume a token
    pub fn try_consume(&mut self) -> bool {
        self.refill();
        
        if self.tokens >= 1.0 {
            self.tokens -= 1.0;
            true
        } else {
            false
        }
    }
    
    /// Get available tokens
    pub fn available(&self) -> f64 {
        self.tokens
    }
    
    fn refill(&mut self) {
        let elapsed = Instant::now().duration_since(self.last_refill).as_secs_f64();
        let tokens_to_add = elapsed * self.refill_rate;
        self.tokens = (self.tokens + tokens_to_add).min(self.capacity as f64);
        self.last_refill = Instant::now();
    }
}

// ============================================================================
// SLIDING WINDOW RATE LIMITER
// ============================================================================

/// Sliding window rate limiter
pub struct SlidingWindowRateLimiter {
    config: RateLimitConfig,
    clients: Arc<RwLock<HashMap<String, ClientInfo>>>,
    whitelisted_ips: Arc<RwLock<Vec<String>>>,
    blacklisted_ips: Arc<RwLock<Vec<String>>>,
}

impl SlidingWindowRateLimiter {
    pub fn new(config: RateLimitConfig) -> Self {
        Self {
            config,
            clients: Arc::new(RwLock::new(HashMap::new())),
            whitelisted_ips: Arc::new(RwLock::new(Vec::new())),
            blacklisted_ips: Arc::new(RwLock::new(Vec::new())),
        }
    }
    
    /// Check if request is allowed
    pub async fn check_request(&self, client_ip: &str) -> Result<ClientInfo, RateLimitError> {
        let window = Duration::from_millis(self.config.window_ms);
        
        // Check blacklist
        let blacklist = self.blacklisted_ips.read().await;
        if blacklist.iter().any(|ip| ip == client_ip) {
            return Err(RateLimitError::Blocked);
        }
        
        // Check whitelist
        let whitelist = self.whitelisted_ips.read().await;
        if whitelist.iter().any(|ip| ip == client_ip) {
            return Ok(ClientInfo::new(client_ip.to_string()));
        }
        
        // Get or create client
        let mut clients = self.clients.write().await;
        let client = clients
            .entry(client_ip.to_string())
            .or_insert_with(|| ClientInfo::new(client_ip.to_string()));
        
        // Check if blocked
        if client.is_blocked() {
            return Err(RateLimitError::Blocked);
        }
        
        // Check rate limit
        let count = client.request_count(window);
        if count >= self.config.max_requests as usize {
            // Block the client
            client.blocked_until = Some(
                Instant::now() + Duration::from_millis(self.config.block_duration_ms)
            );
            return Err(RateLimitError::RateLimitExceeded);
        }
        
        // Record request
        client.requests.push(Instant::now());
        
        // Clean old requests
        client.requests.retain(|&req| Instant::now().duration_since(req) < window);
        
        Ok(client.clone())
    }
    
    /// Add to whitelist
    pub async fn whitelist(&self, ip: &str) {
        let mut whitelist = self.whitelisted_ips.write().await;
        if !whitelist.contains(&ip.to_string()) {
            whitelist.push(ip.to_string());
        }
    }
    
    /// Add to blacklist
    pub async fn blacklist(&self, ip: &str) {
        let mut blacklist = self.blacklisted_ips.write().await;
        if !blacklist.contains(&ip.to_string()) {
            blacklist.push(ip.to_string());
        }
    }
    
    /// Remove from blacklist
    pub async fn unblacklist(&self, ip: &str) {
        let mut blacklist = self.blacklisted_ips.write().await;
        blacklist.retain(|i| i != ip);
    }
    
    /// Get client info
    pub async fn get_client(&self, ip: &str) -> Option<ClientInfo> {
        let clients = self.clients.read().await;
        clients.get(ip).cloned()
    }
    
    /// Get stats
    pub async fn stats(&self) -> RateLimitStats {
        let clients = self.clients.read().await;
        let blocked = clients.values().filter(|c| c.is_blocked()).count();
        let active = clients.len() - blocked;
        
        RateLimitStats {
            total_clients: clients.len(),
            active_clients: active,
            blocked_clients: blocked,
            whitelisted: self.whitelisted_ips.read().await.len(),
            blacklisted: self.blacklisted_ips.read().await.len(),
        }
    }
}

// ============================================================================
// STATS
// ============================================================================

#[derive(Debug, Clone)]
pub struct RateLimitStats {
    pub total_clients: usize,
    pub active_clients: usize,
    pub blocked_clients: usize,
    pub whitelisted: usize,
    pub blacklisted: usize,
}

// ============================================================================
// LEAKY BUCKET RATE LIMITER
// ============================================================================

/// Leaky bucket algorithm
pub struct LeakyBucket {
    capacity: u64,
    leak_rate: Duration,
    drops: Vec<Instant>,
}

impl LeakyBucket {
    pub fn new(capacity: u64, leak_rate_ms: u64) -> Self {
        Self {
            capacity,
            leak_rate: Duration::from_millis(leak_rate_ms),
            drops: Vec::new(),
        }
    }
    
    /// Try to add a drop
    pub fn try_add(&mut self) -> bool {
        self.leak();
        
        if self.drops.len() < self.capacity as usize {
            self.drops.push(Instant::now());
            true
        } else {
            false
        }
    }
    
    fn leak(&mut self) {
        let now = Instant::now();
        self.drops.retain(|&drop| now.duration_since(drop) > self.leak_rate);
    }
}

// ============================================================================
// MIDDLEWARE
// ============================================================================

/// Axum middleware for rate limiting
pub async fn rate_limit_middleware(
    rate_limiter: &SlidingWindowRateLimiter,
    ip: &str,
) -> Result<(), RateLimitError> {
    rate_limiter.check_request(ip).await?;
    Ok(())
}

// ============================================================================
// EXPORT
// ============================================================================

pub use self::{
    config::RateLimitConfig,
    error::RateLimitError,
    client::ClientInfo,
    token_bucket::TokenBucket,
    sliding_window::SlidingWindowRateLimiter,
    leaky_bucket::LeakyBucket,
    stats::RateLimitStats,
};

mod config {
    pub use super::RateLimitConfig;
}

mod error {
    pub use super::RateLimitError;
}

mod client {
    pub use super::ClientInfo;
}

mod token_bucket {
    pub use super::TokenBucket;
}

mod sliding_window {
    pub use super::SlidingWindowRateLimiter;
}

mod leaky_bucket {
    pub use super::LeakyBucket;
}

mod stats {
    pub use super::RateLimitStats;
}