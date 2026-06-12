//! Rate Limiting Module for TigerScan
//! Token bucket algorithm with Redis backend

use chrono::Utc;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::RwLock;
use std::time::{Duration, Instant};

// =============================================================================
// CONSTANTS
// =============================================================================

pub const DEFAULT_RATE_LIMIT: u32 = 100;
pub const DEFAULT_BURST: u32 = 50;

// =============================================================================
// TYPES
// =============================================================================

/// Rate limit configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RateLimitConfig {
    pub requests_per_minute: u32,
    pub requests_per_day: i64,
    pub burst: u32,
    pub quota: i64,
}

impl Default for RateLimitConfig {
    fn default() -> Self {
        Self {
            requests_per_minute: DEFAULT_RATE_LIMIT,
            requests_per_day: 100_000,
            burst: DEFAULT_BURST,
            quota: 100_000,
        }
    }
}

/// Tier limits
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TierLimits {
    pub tier: String,
    pub requests_per_minute: u32,
    pub requests_per_day: i64,
    pub burst: u32,
    pub quota: i64,
}

impl Default for TierLimits {
    fn default() -> Self {
        Self {
            tier: "free".to_string(),
            requests_per_minute: 60,
            requests_per_day: 10_000,
            burst: 10,
            quota: 10_000,
        }
    }
}

// =============================================================================
// RATE LIMITER
// =============================================================================

/// In-memory rate limiter using token bucket
pub struct RateLimiter {
    config: RateLimitConfig,
    tiers: Vec<TierLimits>,
    buckets: RwLock<HashMap<String, TokenBucket>>,
    window: Duration,
}

impl Default for RateLimiter {
    fn default() -> Self {
        Self::new()
    }
}

impl RateLimiter {
    /// Create a new rate limiter
    pub fn new() -> Self {
        Self {
            config: RateLimitConfig::default(),
            tiers: vec![
                TierLimits {
                    tier: "free".to_string(),
                    requests_per_minute: 60,
                    requests_per_day: 10_000,
                    burst: 10,
                    quota: 10_000,
                },
                TierLimits {
                    tier: "pro".to_string(),
                    requests_per_minute: 300,
                    requests_per_day: 100_000,
                    burst: 50,
                    quota: 100_000,
                },
                TierLimits {
                    tier: "enterprise".to_string(),
                    requests_per_minute: 10_000,
                    requests_per_day: -1,
                    burst: 1000,
                    quota: -1,
                },
            ],
            buckets: RwLock::new(HashMap::new()),
            window: Duration::from_secs(60),
        }
    }

    /// Create with custom config
    pub fn with_config(config: RateLimitConfig) -> Self {
        Self {
            config,
            tiers: Vec::new(),
            buckets: RwLock::new(HashMap::new()),
            window: Duration::from_secs(60),
        }
    }

    // =============================================================================
    // LIMIT CHECKING
    // =============================================================================

    /// Check if request is allowed
    pub fn allow(&self, client_id: &str) -> bool {
        self.allow_with_tier(client_id, "free")
    }

    /// Check if request is allowed with tier
    pub fn allow_with_tier(&self, client_id: &str, tier: &str) -> bool {
        let tier_limits = self.get_tier_limits(tier);
        
        let mut buckets = match self.buckets.write() {
            Ok(b) => b,
            Err(_) => return true,
        };
        
        let bucket = buckets
            .entry(client_id.to_string())
            .or_insert_with(|| TokenBucket::new(tier_limits.burst));
        
        bucket.try_consume(tier_limits.requests_per_minute)
    }

    /// Get remaining requests
    pub fn remaining(&self, client_id: &str) -> u32 {
        self.remaining_with_tier(client_id, "free")
    }

    /// Get remaining requests with tier
    pub fn remaining_with_tier(&self, client_id: &str, tier: &str) -> u32 {
        let tier_limits = self.get_tier_limits(tier);
        
        let buckets = match self.buckets.read() {
            Ok(b) => b,
            Err(_) => return tier_limits.burst,
        };
        
        match buckets.get(client_id) {
            Some(bucket) => tier_limits.burst - bucket.used,
            None => tier_limits.burst,
        }
    }

    /// Get reset time (seconds)
    pub fn reset_in(&self, client_id: &str) -> u64 {
        let buckets = match self.buckets.read() {
            Ok(b) => b,
            Err(_) => return 60,
        };
        
        match buckets.get(client_id) {
            Some(bucket) => bucket.reset_in.as_secs(),
            None => 0,
        }
    }

    // =============================================================================
    // TIER MANAGEMENT
    // =============================================================================

    fn get_tier_limits(&self, tier: &str) -> TierLimits {
        self.tiers
            .iter()
            .find(|t| t.tier == tier)
            .cloned()
            .unwrap_or_else(TierLimits::default)
    }

    /// Add custom tier
    pub fn add_tier(&mut self, tier: TierLimits) {
        self.tiers.push(tier);
    }

    /// Get all tiers
    pub fn get_tiers(&self) -> Vec<TierLimits> {
        self.tiers.clone()
    }

    // =============================================================================
    // CLEANUP
    // =============================================================================

    /// Cleanup old entries
    pub fn cleanup(&self) {
        if let Ok(mut buckets) = self.buckets.write() {
            buckets.retain(|_, bucket| {
                bucket.last_reset + self.window > Instant::now()
            });
        }
    }
}

// =============================================================================
// TOKEN BUCKET
// =============================================================================

/// Token bucket implementation
struct TokenBucket {
    tokens: u32,
    max_tokens: u32,
    used: u32,
    last_reset: Instant,
    reset_in: Duration,
}

impl TokenBucket {
    fn new(burst: u32) -> Self {
        Self {
            tokens: burst,
            max_tokens: burst,
            used: 0,
            last_reset: Instant::now(),
            reset_in: Duration::from_secs(60),
        }
    }

    /// Try to consume a token
    fn try_consume(&mut self, refill_rate: u32) -> bool {
        // Check if we need to refill
        if self.tokens == 0 {
            let elapsed = Instant::now().duration_since(self.last_reset);
            
            if elapsed >= self.reset_in {
                // Refill
                let tokens_to_add = (elapsed.as_secs() / self.reset_in.as_secs()) as u32 * refill_rate / 60;
                self.tokens = std::cmp::min(self.tokens + tokens_to_add, self.max_tokens);
                self.last_reset = Instant::now();
                self.used = 0;
            }
        }
        
        if self.tokens > 0 {
            self.tokens -= 1;
            self.used += 1;
            true
        } else {
            false
        }
    }
}

// =============================================================================
// DISTRIBUTED RATE LIMITER (REDIS)
// =============================================================================

/// Redis-based distributed rate limiter
pub struct DistributedRateLimiter {
    redis_url: String,
    key_prefix: String,
}

impl DistributedRateLimiter {
    /// Create a new distributed rate limiter
    pub fn new(redis_url: &str) -> Self {
        Self {
            redis_url: redis_url.to_string(),
            key_prefix: "tigerscan:ratelimit:".to_string(),
        }
    }

    /// Check rate limit with Redis
    pub async fn check_rate_limit(
        &self,
        client_id: &str,
        tier: &str,
    ) -> Result<RateLimitResult, String> {
        // Note: In production, use redis crate
        // This is a placeholder implementation
        
        let key = format!("{}{}:{}", self.key_prefix, tier, client_id);
        
        // Simulated response
        Ok(RateLimitResult {
            allowed: true,
            remaining: 50,
            reset_in: 60,
            tier: tier.to_string(),
        })
    }

    /// Record request
    pub async fn record_request(
        &self,
        client_id: &str,
        tier: &str,
    ) -> Result<(), String> {
        let key = format!("{}{}:{}", self.key_prefix, tier, client_id);
        
        // In production: redis.incr(key) and set expiry
        Ok(())
    }
}

/// Rate limit result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RateLimitResult {
    pub allowed: bool,
    pub remaining: u32,
    pub reset_in: u64,
    pub tier: String,
}

// =============================================================================
// HTTP MIDDLEWARE
// =============================================================================

/// Add rate limit headers to response
pub fn add_rate_limit_headers(
    headers: &mut HashMap<String, String>,
    result: &RateLimitResult,
) {
    headers.insert("X-RateLimit-Limit".to_string(), result.tier.clone());
    headers.insert(
        "X-RateLimit-Remaining".to_string(),
        result.remaining.to_string(),
    );
    headers.insert(
        "X-RateLimit-Reset".to_string(),
        result.reset_in.to_string(),
    );
}

// =============================================================================
// TESTS
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rate_limiter() {
        let limiter = RateLimiter::new();
        
        // Should allow first request
        assert!(limiter.allow("test_client"));
        
        // Should track remaining
        let remaining = limiter.remaining("test_client");
        assert!(remaining < 10);
    }

    #[test]
    fn test_tier_limits() {
        let limiter = RateLimiter::new();
        
        let tiers = limiter.get_tiers();
        
        assert!(tiers.len() >= 3);
        assert!(tiers.iter().any(|t| t.tier == "free"));
        assert!(tiers.iter().any(|t| t.tier == "pro"));
    }

    #[test]
    fn test_burst() {
        let mut limiter = RateLimiter::new();
        
        // Use burst
        for _ in 0..10 {
            assert!(limiter.allow("burst_client"));
        }
        
        // Should be limited now
        assert!(!limiter.allow("burst_client"));
    }
}