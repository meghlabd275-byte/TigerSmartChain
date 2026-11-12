//! API middleware for TigerScan.
//!
//! The rate limiter is a real token bucket: each caller address gets a bucket
//! that refills at a fixed rate and is capped at a burst size. Requests that
//! arrive with an empty bucket are rejected with 429.

use axum::{
    extract::{ConnectInfo, Request, State},
    http::{HeaderValue, Method, StatusCode},
    middleware::Next,
    response::{IntoResponse, Response},
    Json,
};
use serde_json::json;
use std::collections::HashMap;
use std::net::{IpAddr, SocketAddr};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tower_http::cors::{AllowOrigin, CorsLayer};

use crate::handlers::AppState;

// =============================================================================
// RATE LIMITING
// =============================================================================

/// A token bucket for one caller.
#[derive(Debug, Clone, Copy)]
struct Bucket {
    tokens: f64,
    last_refill: Instant,
}

/// Token-bucket rate limiter, one bucket per client address.
pub struct RateLimiter {
    /// Buckets per second.
    refill_rate: f64,
    /// Maximum tokens a bucket may hold.
    burst: f64,
    buckets: parking_lot::Mutex<HashMap<IpAddr, Bucket>>,
}

impl RateLimiter {
    /// Build a limiter that allows `requests_per_second` sustained traffic and
    /// bursts of `burst` requests.
    pub fn new(requests_per_second: u32, burst: u32) -> Self {
        Self {
            refill_rate: f64::from(requests_per_second.max(1)),
            burst: f64::from(burst.max(1)),
            buckets: parking_lot::Mutex::new(HashMap::new()),
        }
    }

    /// Take one token for `peer`, returning `false` when the bucket is empty.
    pub fn check(&self, peer: IpAddr) -> bool {
        let now = Instant::now();
        let mut buckets = self.buckets.lock();

        // Bound memory: a client that has been idle for a full refill cycle
        // cannot be distinguished from a new one, so its bucket is dropped.
        if buckets.len() > 65_536 {
            let stale = Duration::from_secs(60);
            buckets.retain(|_, bucket| now.duration_since(bucket.last_refill) < stale);
        }

        let bucket = buckets.entry(peer).or_insert(Bucket {
            tokens: self.burst,
            last_refill: now,
        });

        let elapsed = now.duration_since(bucket.last_refill).as_secs_f64();
        bucket.tokens = (bucket.tokens + elapsed * self.refill_rate).min(self.burst);
        bucket.last_refill = now;

        if bucket.tokens < 1.0 {
            return false;
        }

        bucket.tokens -= 1.0;
        true
    }
}

/// Middleware that enforces the token bucket.
pub async fn rate_limit_middleware(
    State(state): State<Arc<AppState>>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    request: Request,
    next: Next,
) -> Response {
    if state.rate_limiter.check(peer.ip()) {
        return next.run(request).await;
    }

    (
        StatusCode::TOO_MANY_REQUESTS,
        Json(json!({
            "code": -32005,
            "message": "rate limit exceeded, retry shortly",
        })),
    )
        .into_response()
}

// =============================================================================
// REQUEST LOGGING
// =============================================================================

/// Middleware that records the request against the state counters.
pub async fn metrics_middleware(
    State(state): State<Arc<AppState>>,
    request: Request,
    next: Next,
) -> Response {
    state.record_request(request.uri().path());
    next.run(request).await
}

// =============================================================================
// CORS
// =============================================================================

/// Build the CORS layer from configuration.
///
/// A literal `*` disables credentialed requests, which is what a public
/// read-only explorer wants. Anything else is matched exactly.
pub fn cors_layer(allowed_origins: &[String]) -> CorsLayer {
    let allow_origin = if allowed_origins.iter().any(|origin| origin == "*") {
        AllowOrigin::any()
    } else {
        let origins: Vec<HeaderValue> = allowed_origins
            .iter()
            .filter_map(|origin| origin.parse::<HeaderValue>().ok())
            .collect();
        AllowOrigin::list(origins)
    };

    CorsLayer::new()
        .allow_origin(allow_origin)
        .allow_methods([Method::GET, Method::POST, Method::OPTIONS])
        .allow_headers([
            axum::http::header::CONTENT_TYPE,
            axum::http::header::ACCEPT,
        ])
        .max_age(Duration::from_secs(3600))
}

// =============================================================================
// TESTS
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_bucket_admits_its_burst_then_refuses() {
        let limiter = RateLimiter::new(1, 3);
        let peer: IpAddr = "203.0.113.7".parse().unwrap();

        assert!(limiter.check(peer));
        assert!(limiter.check(peer));
        assert!(limiter.check(peer));
        assert!(!limiter.check(peer), "fourth request exceeds the burst");
    }

    #[test]
    fn buckets_are_per_caller() {
        let limiter = RateLimiter::new(1, 1);
        let first: IpAddr = "203.0.113.7".parse().unwrap();
        let second: IpAddr = "203.0.113.8".parse().unwrap();

        assert!(limiter.check(first));
        assert!(!limiter.check(first));
        assert!(limiter.check(second), "a different caller has its own bucket");
    }

    #[test]
    fn a_bucket_refills_over_time() {
        let limiter = RateLimiter::new(1000, 1);
        let peer: IpAddr = "203.0.113.7".parse().unwrap();

        assert!(limiter.check(peer));
        assert!(!limiter.check(peer));

        // At 1000 tokens per second, a millisecond restores a whole token.
        std::thread::sleep(Duration::from_millis(5));
        assert!(limiter.check(peer));
    }
}
