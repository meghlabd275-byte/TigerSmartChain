//! Security Module for Internal Transaction Indexer
//! 
//! Includes:
//! - Rate limiting
//! - Circuit breaker
//! - Input validation
//! - Attack prevention

use thiserror::Error;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};
use std::collections::VecDeque;

// =============================================================================
// ERRORS
// =============================================================================

#[derive(Error, Debug)]
pub enum SecurityError {
    #[error("Rate limit exceeded")]
    RateLimitExceeded,
    
    #[error("Invalid input: {0}")]
    InvalidInput(String),
    
    #[error("Circuit breaker open")]
    CircuitBreakerOpen,
}

// =============================================================================
// RATE LIMITER
// =============================================================================

/// Token bucket rate limiter
pub struct RateLimiter {
    rate: u64,
    bucket: AtomicU64,
    last_refill: std::sync::Mutex<Instant>,
}

impl RateLimiter {
    /// Create new rate limiter
    pub fn new(rate: u64) -> Self {
        Self {
            rate,
            bucket: AtomicU64::new(rate),
            last_refill: std::sync::Mutex::new(Instant::now()),
        }
    }
    
    /// Check if request is allowed
    pub fn allow(&self) -> bool {
        let mut last = self.last_refill.lock().unwrap();
        let now = Instant::now();
        
        // Refill bucket every second
        if now.duration_since(*last).as_secs() >= 1 {
            self.bucket.store(self.rate, Ordering::SeqCst);
            *last = now;
        }
        
        // Try to consume token
        let current = self.bucket.load(Ordering::SeqCst);
        if current > 0 {
            self.bucket.store(current - 1, Ordering::SeqCst);
            true
        } else {
            false
        }
    }
}

// =============================================================================
// CIRCUIT BREAKER
// =============================================================================

/// Circuit breaker for failed operations
pub struct CircuitBreaker {
    threshold: u32,
    timeout: Duration,
    failures: AtomicU64,
    last_failure: std::sync::Mutex<Option<Instant>>,
    is_open: std::sync::Mutex<bool>,
}

impl CircuitBreaker {
    /// Create new circuit breaker
    pub fn new(threshold: u32, timeout: Duration) -> Self {
        Self {
            threshold,
            timeout,
            failures: AtomicU64::new(0),
            last_failure: std::sync::Mutex::new(None),
            is_open: std::sync::Mutex::new(false),
        }
    }
    
    /// Record success
    pub fn record_success(&self) {
        self.failures.store(0, Ordering::SeqCst);
        *self.is_open.lock().unwrap() = false;
    }
    
    /// Record failure
    pub fn record_failure(&self) {
        let failures = self.failures.fetch_add(1, Ordering::SeqCst) + 1;
        
        if failures >= self.threshold as u64 {
            *self.last_failure.lock().unwrap() = Some(Instant::now());
            *self.is_open.lock().unwrap() = true;
        }
    }
    
    /// Check if circuit is open
    pub fn is_open(&self) -> bool {
        let is_open = *self.is_open.lock().unwrap();
        
        if is_open {
            // Check if timeout has passed
            if let Some(last) = *self.last_failure.lock().unwrap() {
                if last.elapsed() >= self.timeout {
                    *self.is_open.lock().unwrap() = false;
                    self.failures.store(0, Ordering::SeqCst);
                    return false;
                }
            }
        }
        
        is_open
    }
}

// =============================================================================
// INPUT VALIDATOR
// =============================================================================

/// Input validation for security
pub struct InputValidator {
    max_tx_hash_length: usize,
    max_address_length: usize,
    max_block_number: u64,
}

impl InputValidator {
    /// Create new validator
    pub fn new() -> Self {
        Self {
            max_tx_hash_length: 66,
            max_address_length: 42,
            max_block_number: 100_000_000,
        }
    }
    
    /// Validate transaction hash
    pub fn validate_tx_hash(&self, tx_hash: &str) -> Result<String, SecurityError> {
        let hash = tx_hash.trim();
        
        // Check length
        if hash.len() > self.max_tx_hash_length {
            return Err(SecurityError::InvalidInput("Transaction hash too long".to_string()));
        }
        
        // Check prefix
        if !hash.starts_with("0x") {
            return Err(SecurityError::InvalidInput("Invalid transaction hash format".to_string()));
        }
        
        // Check hex characters
        let hex_part = &hash[2..];
        if !hex_part.chars().all(|c| c.is_ascii_hexdigit()) {
            return Err(SecurityError::InvalidInput("Invalid hex characters".to_string()));
        }
        
        Ok(hash.to_string())
    }
    
    /// Validate address
    pub fn validate_address(&self, address: &str) -> Result<String, SecurityError> {
        let addr = address.trim();
        
        // Check length
        if addr.len() > self.max_address_length {
            return Err(SecurityError::InvalidInput("Address too long".to_string()));
        }
        
        // Check prefix
        if !addr.starts_with("0x") {
            return Err(SecurityError::InvalidInput("Invalid address format".to_string()));
        }
        
        // Check hex characters
        let hex_part = &addr[2..];
        if !hex_part.chars().all(|c| c.is_ascii_hexdigit()) {
            return Err(SecurityError::InvalidInput("Invalid hex characters".to_string()));
        }
        
        Ok(addr.to_string())
    }
    
    /// Validate block number
    pub fn validate_block_number(&self, block: u64) -> Result<u64, SecurityError> {
        if block > self.max_block_number {
            return Err(SecurityError::InvalidInput("Block number too high".to_string()));
        }
        
        Ok(block)
    }
}

impl Default for InputValidator {
    fn default() -> Self {
        Self::new()
    }
}