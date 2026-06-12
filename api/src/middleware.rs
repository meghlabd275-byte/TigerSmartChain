//! API Middleware for TigerScan

use axum::{
    extract::Request,
    middleware::Next,
    response::Response,
};
use std::time::Instant;

// =============================================================================
// MIDDLEWARE
// =============================================================================

/// Request timing middleware
pub async fn timing_middleware(
    request: Request,
    next: Next,
) -> Response {
    let start = Instant::now();
    let response = next.run(request).await;
    let duration = start.elapsed();
    println!("Request took: {:?}", duration);
    response
}

/// Logging middleware
pub async fn logging_middleware(
    request: Request,
    next: Next,
) -> Response {
    println!("{} {}", request.method(), request.uri());
    next.run(request).await
}

/// Metrics middleware
pub async fn metrics_middleware(
    request: Request,
    next: Next,
) -> Response {
    // Increment metrics
    next.run(request).await
}

// =============================================================================
// RATE LIMITING
// =============================================================================

/// Simple rate limiter
pub struct RateLimiter {
    max_requests: usize,
    window_secs: u64,
}

impl RateLimiter {
    pub fn new(max_requests: usize, window_secs: u64) -> Self {
        Self {
            max_requests,
            window_secs,
        }
    }
}

// =============================================================================
// CORS
// =============================================================================

/// CORS configuration
pub struct CorsConfig {
    pub allowed_origins: Vec<String>,
    pub allowed_methods: Vec<String>,
    pub allowed_headers: Vec<String>,
    pub max_age: u64,
}

impl Default for CorsConfig {
    fn default() -> Self {
        Self {
            allowed_origins: vec!["*".to_string()],
            allowed_methods: vec!["GET".to_string(), "POST".to_string()],
            allowed_headers: vec!["Content-Type".to_string()],
            max_age: 3600,
        }
    }
}