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
        
        // Failing closed: a poisoned lock means the limiter state cannot be
        // trusted, and admitting the request would remove the limit entirely.
        let mut buckets = match self.buckets.write() {
            Ok(b) => b,
            Err(_) => return false,
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

/// Redis-based distributed rate limiter.
///
/// Each client and tier gets a counter key. The counter is incremented and
/// given a TTL atomically by a small Lua script, so two nodes sharing one
/// Redis instance cannot each grant a full burst.
pub struct DistributedRateLimiter {
    redis_url: String,
    key_prefix: String,
    tiers: Vec<TierLimits>,
    window_secs: u64,
}

/// Increment a counter and set its expiry on first use, returning the counter
/// and its remaining TTL. Doing both in one round trip keeps the counter and
/// the window from drifting apart.
const RATE_LIMIT_SCRIPT: &str = r"
local current = redis.call('INCR', KEYS[1])
if current == 1 then
    redis.call('EXPIRE', KEYS[1], ARGV[1])
end
local ttl = redis.call('TTL', KEYS[1])
return {current, ttl}
";

impl DistributedRateLimiter {
    /// Create a new distributed rate limiter
    pub fn new(redis_url: &str) -> Self {
        Self {
            redis_url: redis_url.to_string(),
            key_prefix: "tigerscan:ratelimit:".to_string(),
            tiers: RateLimiter::new().get_tiers(),
            window_secs: 60,
        }
    }

    /// The counter key for a client on a tier.
    fn key(&self, client_id: &str, tier: &str) -> String {
        format!("{}{}:{}", self.key_prefix, tier, client_id)
    }

    fn limit_for(&self, tier: &str) -> u32 {
        self.tiers
            .iter()
            .find(|t| t.tier == tier)
            .map(|t| t.requests_per_minute)
            .unwrap_or(DEFAULT_RATE_LIMIT)
    }

    async fn connection(&self) -> Result<redis::aio::MultiplexedConnection, String> {
        let client =
            redis::Client::open(self.redis_url.clone()).map_err(|e| format!("redis url: {e}"))?;
        client
            .get_multiplexed_async_connection()
            .await
            .map_err(|e| format!("redis connect: {e}"))
    }

    /// Check and consume one request for a client.
    ///
    /// Returns `Err` when Redis is unreachable. This never reports
    /// `allowed: true` for a request it could not actually count, so a caller
    /// that propagates the error cannot be tricked into admitting unlimited
    /// traffic during an outage.
    pub async fn check_rate_limit(
        &self,
        client_id: &str,
        tier: &str,
    ) -> Result<RateLimitResult, String> {
        let mut conn = self.connection().await?;
        let key = self.key(client_id, tier);

        let (current, ttl): (i64, i64) = redis::cmd("EVAL")
            .arg(RATE_LIMIT_SCRIPT)
            .arg(1)
            .arg(&key)
            .arg(self.window_secs)
            .query_async(&mut conn)
            .await
            .map_err(|e| format!("redis eval: {e}"))?;

        let limit = self.limit_for(tier);
        let used = current.max(0) as u32;
        // A TTL of -1 means the key somehow has no expiry. Report the full
        // window rather than a negative reset time.
        let reset_in = if ttl > 0 { ttl as u64 } else { self.window_secs };

        Ok(RateLimitResult {
            allowed: used <= limit,
            remaining: limit.saturating_sub(used),
            reset_in,
            limit,
            tier: tier.to_string(),
        })
    }

    /// Count a request without checking it.
    ///
    /// Use this when the decision was already made elsewhere, for example by
    /// the in-process limiter, and the distributed counter still needs to see
    /// the traffic.
    pub async fn record_request(&self, client_id: &str, tier: &str) -> Result<(), String> {
        let mut conn = self.connection().await?;
        let key = self.key(client_id, tier);

        redis::cmd("EVAL")
            .arg(RATE_LIMIT_SCRIPT)
            .arg(1)
            .arg(&key)
            .arg(self.window_secs)
            .query_async::<_, (i64, i64)>(&mut conn)
            .await
            .map_err(|e| format!("redis eval: {e}"))?;

        Ok(())
    }

    /// Reset a client's counter, for example after a plan change.
    pub async fn reset(&self, client_id: &str, tier: &str) -> Result<(), String> {
        let mut conn = self.connection().await?;

        redis::cmd("DEL")
            .arg(self.key(client_id, tier))
            .query_async::<_, i64>(&mut conn)
            .await
            .map_err(|e| format!("redis del: {e}"))?;

        Ok(())
    }
}

/// Rate limit result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RateLimitResult {
    pub allowed: bool,
    pub remaining: u32,
    pub reset_in: u64,
    /// The ceiling the tier allows per window.
    pub limit: u32,
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
    headers.insert("X-RateLimit-Limit".to_string(), result.limit.to_string());
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

    #[tokio::test]
    async fn distributed_limiter_errors_when_redis_is_unreachable() {
        // Port 1 is not a Redis server. The limiter must report the failure
        // rather than admitting the request, so callers can fail closed.
        let limiter = DistributedRateLimiter::new("redis://127.0.0.1:1");
        let result = limiter.check_rate_limit("client", "free").await;
        assert!(result.is_err(), "an unreachable backend must not allow requests");
    }

    #[test]
    fn the_limit_header_reports_the_ceiling_not_the_tier_name() {
        let mut headers = HashMap::new();
        add_rate_limit_headers(
            &mut headers,
            &RateLimitResult {
                allowed: true,
                remaining: 42,
                reset_in: 30,
                limit: 60,
                tier: "free".to_string(),
            },
        );

        assert_eq!(headers.get("X-RateLimit-Limit").unwrap(), "60");
        assert_eq!(headers.get("X-RateLimit-Remaining").unwrap(), "42");
        assert_eq!(headers.get("X-RateLimit-Reset").unwrap(), "30");
    }
}
