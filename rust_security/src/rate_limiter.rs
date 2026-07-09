//! Rate limiting module

use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::RwLock;

/// Rate limiter implementation using token bucket algorithm
pub struct RateLimiter {
    requests_per_second: u64,
    burst_size: u64,
    buckets: Arc<RwLock<HashMap<String, TokenBucket>>>,
}

struct TokenBucket {
    tokens: u64,
    last_refill: Instant,
}

impl RateLimiter {
    pub fn new(requests_per_second: u64, burst_size: u64) -> Self {
        Self {
            requests_per_second,
            burst_size,
            buckets: Arc::new(RwLock::new(HashMap::new())),
        }
    }
    
    /// Check if request is allowed for the given key
    pub async fn check(&self, key: &str) -> bool {
        let mut buckets = self.buckets.write().await;
        
        let bucket = buckets.entry(key.to_string()).or_insert(TokenBucket {
            tokens: self.burst_size,
            last_refill: Instant::now(),
        });
        
        // Refill tokens based on time elapsed
        let now = Instant::now();
        let elapsed = now.duration_since(bucket.last_refill).as_secs_f64();
        let tokens_to_add = (elapsed * self.requests_per_second as f64) as u64;
        
        bucket.tokens = std::cmp::min(self.burst_size, bucket.tokens + tokens_to_add);
        bucket.last_refill = now;
        
        if bucket.tokens >= 1 {
            bucket.tokens -= 1;
            true
        } else {
            false
        }
    }
    
    /// Remove key from rate limiter (cleanup)
    pub async fn remove(&self, key: &str) {
        let mut buckets = self.buckets.write().await;
        buckets.remove(key);
    }
    
    /// Get remaining requests for key
    pub async fn remaining(&self, key: &str) -> u64 {
        let buckets = self.buckets.read().await;
        
        if let Some(bucket) = buckets.get(key) {
            let now = Instant::now();
            let elapsed = now.duration_since(bucket.last_refill).as_secs_f64();
            let tokens_to_add = (elapsed * self.requests_per_second as f64) as u64;
            std::cmp::min(self.burst_size, bucket.tokens + tokens_to_add)
        } else {
            self.burst_size
        }
    }
}

/// Sliding window rate limiter
pub struct SlidingWindowRateLimiter {
    max_requests: u64,
    window_size: Duration,
    requests: Arc<RwLock<HashMap<String, Vec<Instant>>>>,
}

impl SlidingWindowRateLimiter {
    pub fn new(max_requests: u64, window_size: Duration) -> Self {
        Self {
            max_requests,
            window_size,
            requests: Arc::new(RwLock::new(HashMap::new())),
        }
    }
    
    pub async fn check(&self, key: &str) -> bool {
        let mut reqs = self.requests.write().await;
        let now = Instant::now();
        
        let timestamps = reqs.entry(key.to_string()).or_insert_with(Vec::new);
        
        // Remove old requests outside window
        timestamps.retain(|t| now.duration_since(*t) < self.window_size);
        
        if timestamps.len() < self.max_requests as usize {
            timestamps.push(now);
            true
        } else {
            false
        }
    }
}

/// API key based rate limiter
pub struct ApiKeyRateLimiter {
    limits: HashMap<String, u64>,
    inner: RateLimiter,
}

impl ApiKeyRateLimiter {
    pub fn new(default_limit: u64) -> Self {
        Self {
            limits: HashMap::new(),
            inner: RateLimiter::new(default_limit, default_limit),
        }
    }
    
    pub fn set_limit(&mut self, api_key: &str, limit: u64) {
        self.limits.insert(api_key.to_string(), limit);
    }
    
    pub async fn check(&self, api_key: &str) -> bool {
        self.inner.check(api_key).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;
    
    #[tokio::test]
    async fn test_rate_limiter() {
        let limiter = RateLimiter::new(10, 10);
        
        // First 10 requests should succeed
        for _ in 0..10 {
            assert!(limiter.check("test_key").await);
        }
        
        // Next request should fail
        assert!(!limiter.check("test_key").await);
    }
}
