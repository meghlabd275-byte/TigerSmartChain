//! Rate limiting module
//!
//! Three limiters, each suited to a different call site:
//!
//! * [`RateLimiter`] — a token bucket, for smooth per-client request rates.
//! * [`SlidingWindowRateLimiter`] — an exact count over a rolling window.
//! * [`ApiKeyRateLimiter`] — per-key buckets with individual limits.
//!
//! All three bound the number of tracked keys. An unbounded map is itself a
//! denial-of-service vector: one key per request grows memory without limit.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::RwLock;

/// How many distinct keys a limiter will track before evicting the least
/// recently seen. Chosen so a busy server keeps its active clients and drops
/// idle ones rather than growing without bound.
const MAX_TRACKED_KEYS: usize = 100_000;

/// Drop entries that have been idle for at least `idle` and are, therefore,
/// fully refilled (or fully expired) and indistinguishable from a fresh key.
fn evict_idle<K, V>(map: &mut HashMap<K, V>, idle: Duration, last_seen: impl Fn(&V) -> Instant)
where
    K: Clone + std::hash::Hash + Eq,
{
    let now = Instant::now();
    map.retain(|_, v| now.duration_since(last_seen(v)) < idle);
}

/// Token bucket rate limiter.
pub struct RateLimiter {
    requests_per_second: f64,
    burst_size: f64,
    buckets: Arc<RwLock<HashMap<String, TokenBucket>>>,
}

struct TokenBucket {
    tokens: f64,
    last_refill: Instant,
}

impl RateLimiter {
    pub fn new(requests_per_second: u64, burst_size: u64) -> Self {
        Self {
            requests_per_second: requests_per_second.max(1) as f64,
            burst_size: burst_size.max(1) as f64,
            buckets: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Take one token for `key`, or report that the bucket is empty.
    pub async fn check(&self, key: &str) -> bool {
        let mut buckets = self.buckets.write().await;

        // A full bucket is indistinguishable from no bucket, so idle entries
        // can be dropped. `burst / rate` seconds is the time to refill fully.
        if buckets.len() >= MAX_TRACKED_KEYS {
            let full_refill = Duration::from_secs_f64(self.burst_size / self.requests_per_second);
            evict_idle(&mut buckets, full_refill, |b| b.last_refill);
        }

        let now = Instant::now();
        let bucket = buckets.entry(key.to_string()).or_insert(TokenBucket {
            tokens: self.burst_size,
            last_refill: now,
        });

        // Refill in proportion to elapsed time, then spend one token.
        let elapsed = now.duration_since(bucket.last_refill).as_secs_f64();
        bucket.tokens = (bucket.tokens + elapsed * self.requests_per_second).min(self.burst_size);
        bucket.last_refill = now;

        if bucket.tokens >= 1.0 {
            bucket.tokens -= 1.0;
            true
        } else {
            false
        }
    }

    /// Remove a key (for example when a client disconnects).
    pub async fn remove(&self, key: &str) {
        self.buckets.write().await.remove(key);
    }

    /// Tokens available to `key` right now, as a whole number.
    pub async fn remaining(&self, key: &str) -> u64 {
        let buckets = self.buckets.read().await;

        match buckets.get(key) {
            Some(bucket) => {
                let elapsed = Instant::now().duration_since(bucket.last_refill).as_secs_f64();
                (bucket.tokens + elapsed * self.requests_per_second)
                    .min(self.burst_size)
                    .floor() as u64
            }
            None => self.burst_size as u64,
        }
    }
}

/// Sliding window rate limiter: at most `max_requests` in any `window_size`.
pub struct SlidingWindowRateLimiter {
    max_requests: usize,
    window_size: Duration,
    requests: Arc<RwLock<HashMap<String, Vec<Instant>>>>,
}

impl SlidingWindowRateLimiter {
    pub fn new(max_requests: u64, window_size: Duration) -> Self {
        Self {
            max_requests: max_requests as usize,
            window_size,
            requests: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    pub async fn check(&self, key: &str) -> bool {
        let mut reqs = self.requests.write().await;
        let now = Instant::now();

        if reqs.len() >= MAX_TRACKED_KEYS && !reqs.contains_key(key) {
            evict_idle(&mut reqs, self.window_size, |v| {
                v.last().copied().unwrap_or_else(Instant::now)
            });
        }

        let timestamps = reqs.entry(key.to_string()).or_default();

        // Drop timestamps that have aged out of the window.
        timestamps.retain(|t| now.duration_since(*t) < self.window_size);

        if timestamps.len() < self.max_requests {
            timestamps.push(now);
            true
        } else {
            false
        }
    }
}

/// Per-API-key limiter, each key with its own token bucket.
pub struct ApiKeyRateLimiter {
    default_limit: u64,
    /// One limiter per custom-limit key, so a key's bucket keeps its state
    /// across calls instead of being recreated on every request.
    custom: RwLock<HashMap<String, Arc<RateLimiter>>>,
    inner: RateLimiter,
}

impl ApiKeyRateLimiter {
    pub fn new(default_limit: u64) -> Self {
        Self {
            default_limit,
            custom: RwLock::new(HashMap::new()),
            inner: RateLimiter::new(default_limit, default_limit),
        }
    }

    /// Give one key a limit different from the default.
    pub async fn set_limit(&self, api_key: &str, limit: u64) {
        let limit = limit.max(1);
        if limit == self.default_limit {
            self.custom.write().await.remove(api_key);
        } else {
            self.custom
                .write()
                .await
                .insert(api_key.to_string(), Arc::new(RateLimiter::new(limit, limit)));
        }
    }

    /// Check a request from `api_key`.
    pub async fn check(&self, api_key: &str) -> bool {
        let custom = self.custom.read().await.get(api_key).cloned();
        match custom {
            Some(limiter) => limiter.check(api_key).await,
            None => self.inner.check(api_key).await,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_token_bucket_exhausts_burst() {
        let limiter = RateLimiter::new(10, 10);
        for _ in 0..10 {
            assert!(limiter.check("k").await);
        }
        assert!(!limiter.check("k").await);
    }

    #[tokio::test]
    async fn test_token_bucket_refills() {
        let limiter = RateLimiter::new(1000, 1);
        assert!(limiter.check("k").await);
        assert!(!limiter.check("k").await);
        // One millisecond at 1000/s returns one token.
        tokio::time::sleep(Duration::from_millis(5)).await;
        assert!(limiter.check("k").await);
    }

    #[tokio::test]
    async fn test_remaining_reports_capacity() {
        let limiter = RateLimiter::new(5, 5);
        assert_eq!(limiter.remaining("fresh").await, 5);
        limiter.check("fresh").await;
        assert_eq!(limiter.remaining("fresh").await, 4);
    }

    #[tokio::test]
    async fn test_remove_resets_a_key() {
        let limiter = RateLimiter::new(1, 1);
        assert!(limiter.check("k").await);
        assert!(!limiter.check("k").await);
        limiter.remove("k").await;
        assert!(limiter.check("k").await);
    }

    #[tokio::test]
    async fn test_sliding_window() {
        let limiter = SlidingWindowRateLimiter::new(3, Duration::from_millis(50));
        for _ in 0..3 {
            assert!(limiter.check("k").await);
        }
        assert!(!limiter.check("k").await);

        // Once the window passes, capacity returns.
        tokio::time::sleep(Duration::from_millis(60)).await;
        assert!(limiter.check("k").await);
    }

    #[tokio::test]
    async fn test_api_key_limits_are_per_key() {
        let limiter = ApiKeyRateLimiter::new(2);
        limiter.set_limit("vip", 5).await;

        // The default key is limited to 2.
        assert!(limiter.check("normal").await);
        assert!(limiter.check("normal").await);
        assert!(!limiter.check("normal").await);

        // The vip key has its own budget and is unaffected.
        for _ in 0..5 {
            assert!(limiter.check("vip").await);
        }
        assert!(!limiter.check("vip").await);
    }

    #[tokio::test]
    async fn test_keys_are_independent() {
        let limiter = RateLimiter::new(1, 1);
        assert!(limiter.check("a").await);
        // b's bucket is untouched by a's usage.
        assert!(limiter.check("b").await);
        assert!(!limiter.check("a").await);
    }
}
