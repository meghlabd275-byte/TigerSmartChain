//! NFT Analytics Service — JSON-RPC 2.0 API.
//!
//! Transport is axum over HTTP with a hard request-body limit, security
//! headers, and a per-client token bucket. The RPC layer validates every
//! parameter before it reaches the service and returns standard JSON-RPC error
//! objects, so a malformed request produces a well-formed error rather than a
//! panic or an empty result.

#![forbid(unsafe_code)]

use std::collections::HashMap;
use std::net::{IpAddr, SocketAddr};
use std::sync::Arc;
use std::time::Instant;

use axum::extract::{ConnectInfo, DefaultBodyLimit, State};
use axum::http::{header, HeaderName, HeaderValue, StatusCode};
use axum::response::IntoResponse;
use axum::routing::{get, post};
use axum::{Json, Router};
use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use tower_http::set_header::SetResponseHeaderLayer;
use tracing::{info, warn, Level};
use tracing_subscriber::FmtSubscriber;

use nft_analytics_service::{Config, NFTAnalyticsService};

/// JSON-RPC parse error.
const PARSE_ERROR: i32 = -32700;
/// JSON-RPC invalid request.
const INVALID_REQUEST: i32 = -32600;
/// JSON-RPC method not found.
const METHOD_NOT_FOUND: i32 = -32601;
/// JSON-RPC invalid params.
const INVALID_PARAMS: i32 = -32602;
/// JSON-RPC server error.
const SERVER_ERROR: i32 = -32000;
/// Rate limited.
const RATE_LIMITED: i32 = -32029;

/// Largest accepted request body. A JSON-RPC call is a few hundred bytes; the
/// cap bounds the memory a single client can make the process allocate.
const MAX_BODY_BYTES: usize = 64 * 1024;

// ============================================================================
// API Types
// ============================================================================

#[derive(Debug, Clone, Deserialize)]
pub struct ApiRequest {
    pub jsonrpc: String,
    pub method: String,
    #[serde(default)]
    pub params: Option<serde_json::Value>,
    #[serde(default)]
    pub id: serde_json::Value,
}

#[derive(Debug, Clone, Serialize)]
pub struct ApiResponse {
    pub jsonrpc: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<ApiError>,
    pub id: serde_json::Value,
}

#[derive(Debug, Clone, Serialize)]
pub struct ApiError {
    pub code: i32,
    pub message: String,
}

impl ApiResponse {
    fn ok(id: serde_json::Value, result: serde_json::Value) -> Self {
        Self { jsonrpc: "2.0", result: Some(result), error: None, id }
    }

    fn err(id: serde_json::Value, code: i32, message: impl Into<String>) -> Self {
        Self {
            jsonrpc: "2.0",
            result: None,
            error: Some(ApiError { code, message: message.into() }),
            id,
        }
    }
}

// ============================================================================
// Rate limiting
// ============================================================================

struct Bucket {
    tokens: f64,
    last_refill: Instant,
}

/// Token bucket per client address.
pub struct RateLimiter {
    buckets: Mutex<HashMap<IpAddr, Bucket>>,
    per_second: f64,
    burst: f64,
}

impl RateLimiter {
    fn new(per_second: u32, burst: u32) -> Self {
        Self {
            buckets: Mutex::new(HashMap::new()),
            per_second: per_second.max(1) as f64,
            burst: burst.max(1) as f64,
        }
    }

    /// Charge one request against `ip`. Returns `false` when the bucket is
    /// empty. The map is bounded: past a fixed ceiling the least recently seen
    /// half is dropped, so a client cycling through source addresses cannot
    /// grow it without limit.
    fn allow(&self, ip: IpAddr) -> bool {
        const MAX_BUCKETS: usize = 100_000;

        let now = Instant::now();
        let mut buckets = self.buckets.lock();

        if buckets.len() > MAX_BUCKETS {
            let mut by_age: Vec<(IpAddr, Instant)> =
                buckets.iter().map(|(ip, bucket)| (*ip, bucket.last_refill)).collect();
            by_age.sort_by_key(|(_, seen)| *seen);
            for (ip, _) in by_age.into_iter().take(MAX_BUCKETS / 2) {
                buckets.remove(&ip);
            }
        }

        let bucket = buckets.entry(ip).or_insert(Bucket { tokens: self.burst, last_refill: now });

        let elapsed = now.duration_since(bucket.last_refill).as_secs_f64();
        bucket.tokens = (bucket.tokens + elapsed * self.per_second).min(self.burst);
        bucket.last_refill = now;

        if bucket.tokens < 1.0 {
            return false;
        }
        bucket.tokens -= 1.0;
        true
    }
}

// ============================================================================
// Application state
// ============================================================================

pub struct AppState {
    service: NFTAnalyticsService,
    limiter: RateLimiter,
}

impl AppState {
    pub fn new(service: NFTAnalyticsService, config: &Config) -> Self {
        Self {
            service,
            limiter: RateLimiter::new(config.rate_limit_per_second, config.rate_limit_burst),
        }
    }

    /// Dispatch one JSON-RPC call.
    fn dispatch(&self, request: &ApiRequest) -> ApiResponse {
        let id = request.id.clone();

        if request.jsonrpc != "2.0" {
            return ApiResponse::err(id, INVALID_REQUEST, "jsonrpc must be \"2.0\"");
        }

        let result = match request.method.as_str() {
            "nft_getCollectionAnalytics" => self.collection_analytics(request.params.as_ref()),
            "nft_getRarity" => self.rarity(request.params.as_ref()),
            "nft_getTopRarity" => self.top_rarity(request.params.as_ref()),
            "nft_getFloorHistory" => self.floor_history(request.params.as_ref()),
            "nft_health" => Ok(serde_json::json!({ "status": "ok" })),
            other => {
                return ApiResponse::err(id, METHOD_NOT_FOUND, format!("unknown method: {other}"))
            }
        };

        match result {
            Ok(value) => ApiResponse::ok(id, value),
            Err((code, message)) => ApiResponse::err(id, code, message),
        }
    }

    fn collection_analytics(
        &self,
        params: Option<&serde_json::Value>,
    ) -> Result<serde_json::Value, (i32, String)> {
        let address = required_str(params, "collection")?;
        self.service
            .get_collection_analytics(address)
            .and_then(|analytics| serde_json::to_value(analytics).ok())
            .ok_or_else(|| (SERVER_ERROR, format!("collection not indexed: {address}")))
    }

    fn rarity(
        &self,
        params: Option<&serde_json::Value>,
    ) -> Result<serde_json::Value, (i32, String)> {
        let collection = required_str(params, "collection")?;
        let token_id = required_str(params, "token_id")?;
        self.service
            .calculate_rarity(collection, token_id)
            .and_then(|rarity| serde_json::to_value(rarity).ok())
            .ok_or_else(|| (SERVER_ERROR, format!("token not indexed: {collection}/{token_id}")))
    }

    fn top_rarity(
        &self,
        params: Option<&serde_json::Value>,
    ) -> Result<serde_json::Value, (i32, String)> {
        let collection = required_str(params, "collection")?;
        // A caller-supplied limit is clamped: an unbounded value would let one
        // request clone an entire collection.
        let limit = params
            .and_then(|p| p.get("limit"))
            .and_then(|limit| limit.as_u64())
            .unwrap_or(10)
            .clamp(1, 100) as usize;

        let nfts = self
            .service
            .get_top_rarity(collection, limit)
            .ok_or_else(|| (SERVER_ERROR, format!("collection not indexed: {collection}")))?;
        Ok(serde_json::json!({ "collection": collection, "nfts": nfts }))
    }

    fn floor_history(
        &self,
        params: Option<&serde_json::Value>,
    ) -> Result<serde_json::Value, (i32, String)> {
        let collection = required_str(params, "collection")?;
        self.service
            .get_floor_price_history(collection)
            .and_then(|history| serde_json::to_value(history).ok())
            .ok_or_else(|| (SERVER_ERROR, format!("no floor history for: {collection}")))
    }
}

/// Read a required non-empty string parameter.
fn required_str<'a>(
    params: Option<&'a serde_json::Value>,
    name: &str,
) -> Result<&'a str, (i32, String)> {
    params
        .and_then(|p| p.get(name))
        .and_then(|value| value.as_str())
        .filter(|value| !value.is_empty())
        .ok_or_else(|| (INVALID_PARAMS, format!("missing or empty parameter: {name}")))
}

// ============================================================================
// HTTP handlers
// ============================================================================

async fn rpc_handler(
    State(state): State<Arc<AppState>>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    body: String,
) -> impl IntoResponse {
    if !state.limiter.allow(peer.ip()) {
        let response =
            ApiResponse::err(serde_json::Value::Null, RATE_LIMITED, "rate limit exceeded");
        return (StatusCode::TOO_MANY_REQUESTS, Json(response));
    }

    let request: ApiRequest = match serde_json::from_str(&body) {
        Ok(request) => request,
        Err(error) => {
            let response =
                ApiResponse::err(serde_json::Value::Null, PARSE_ERROR, format!("invalid JSON: {error}"));
            return (StatusCode::BAD_REQUEST, Json(response));
        }
    };

    (StatusCode::OK, Json(state.dispatch(&request)))
}

async fn health_handler() -> impl IntoResponse {
    (StatusCode::OK, Json(serde_json::json!({ "status": "ok" })))
}

// ============================================================================
// Main
// ============================================================================

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    FmtSubscriber::builder()
        .with_max_level(Level::INFO)
        .with_target(false)
        .init();

    let config = Config::default();
    let bind: SocketAddr = std::env::var("BIND_ADDR")
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or_else(|| SocketAddr::from(([0, 0, 0, 0], 8549)));

    info!("Starting NFT Analytics Service");

    let service = NFTAnalyticsService::new(config.clone()).await?;
    let state = Arc::new(AppState::new(service, &config));

    let app = Router::new()
        .route("/", post(rpc_handler))
        .route("/health", get(health_handler))
        .layer(DefaultBodyLimit::max(MAX_BODY_BYTES))
        .layer(SetResponseHeaderLayer::overriding(
            header::CONTENT_TYPE,
            HeaderValue::from_static("application/json"),
        ))
        .layer(SetResponseHeaderLayer::overriding(
            header::X_CONTENT_TYPE_OPTIONS,
            HeaderValue::from_static("nosniff"),
        ))
        .layer(SetResponseHeaderLayer::overriding(
            header::CACHE_CONTROL,
            HeaderValue::from_static("no-store"),
        ))
        .layer(SetResponseHeaderLayer::overriding(
            HeaderName::from_static("x-frame-options"),
            HeaderValue::from_static("DENY"),
        ))
        .layer(SetResponseHeaderLayer::overriding(
            HeaderName::from_static("referrer-policy"),
            HeaderValue::from_static("no-referrer"),
        ))
        .layer(SetResponseHeaderLayer::overriding(
            HeaderName::from_static("content-security-policy"),
            HeaderValue::from_static("default-src 'none'; frame-ancestors 'none'"),
        ))
        .with_state(state);

    info!("Listening on {bind}");

    let listener = std::net::TcpListener::bind(bind)?;
    listener.set_nonblocking(true)?;
    let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel::<()>();

    tokio::spawn(async move {
        if let Err(error) = tokio::signal::ctrl_c().await {
            warn!("failed to listen for shutdown signal: {error}");
        }
        let _ = shutdown_tx.send(());
    });

    axum::Server::from_tcp(listener)?
        .serve(app.into_make_service_with_connect_info::<SocketAddr>())
        .with_graceful_shutdown(async {
            let _ = shutdown_rx.await;
            info!("shutting down");
        })
        .await?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::OnceLock;

    /// The provider inside the service keeps a reactor alive, so the runtime has
    /// to outlive every service built in this process.
    fn runtime() -> &'static tokio::runtime::Runtime {
        static RUNTIME: OnceLock<tokio::runtime::Runtime> = OnceLock::new();
        RUNTIME.get_or_init(|| tokio::runtime::Runtime::new().expect("tokio runtime"))
    }

    fn state() -> Arc<AppState> {
        let config = Config::default();
        let service = runtime()
            .block_on(NFTAnalyticsService::new(config.clone()))
            .expect("service");
        Arc::new(AppState::new(service, &config))
    }

    fn request(method: &str, params: serde_json::Value) -> ApiRequest {
        ApiRequest {
            jsonrpc: "2.0".to_string(),
            method: method.to_string(),
            params: Some(params),
            id: serde_json::json!(1),
        }
    }

    #[test]
    fn unknown_method_is_method_not_found() {
        let response = state().dispatch(&request("nft_nope", serde_json::json!({})));
        assert_eq!(response.error.expect("error").code, METHOD_NOT_FOUND);
    }

    #[test]
    fn wrong_jsonrpc_version_is_invalid_request() {
        let mut request = request("nft_health", serde_json::json!({}));
        request.jsonrpc = "1.0".to_string();
        assert_eq!(state().dispatch(&request).error.expect("error").code, INVALID_REQUEST);
    }

    #[test]
    fn health_succeeds() {
        let response = state().dispatch(&request("nft_health", serde_json::json!({})));
        assert_eq!(response.result.expect("result")["status"], "ok");
    }

    #[test]
    fn missing_param_is_invalid_params() {
        let response = state().dispatch(&request("nft_getRarity", serde_json::json!({})));
        assert_eq!(response.error.expect("error").code, INVALID_PARAMS);
    }

    #[test]
    fn empty_param_is_rejected() {
        let params = serde_json::json!({ "collection": "", "token_id": "1" });
        let response = state().dispatch(&request("nft_getRarity", params));
        assert_eq!(response.error.expect("error").code, INVALID_PARAMS);
    }

    #[test]
    fn unindexed_collection_is_an_error_not_an_empty_result() {
        let params = serde_json::json!({ "collection": "0xabc" });
        let response = state().dispatch(&request("nft_getCollectionAnalytics", params));
        assert_eq!(response.error.expect("error").code, SERVER_ERROR);
        assert!(response.result.is_none());
    }

    #[test]
    fn rate_limiter_enforces_the_burst() {
        let limiter = RateLimiter::new(1, 3);
        let ip: IpAddr = "203.0.113.7".parse().unwrap();
        assert!(limiter.allow(ip));
        assert!(limiter.allow(ip));
        assert!(limiter.allow(ip));
        assert!(!limiter.allow(ip), "fourth request in the burst must be refused");
        assert!(limiter.allow("203.0.113.8".parse().unwrap()), "another client is unaffected");
    }

    #[test]
    fn oversized_top_rarity_limit_is_clamped() {
        let params = serde_json::json!({ "collection": "0xabc", "limit": 10_000_000u64 });
        let response = state().dispatch(&request("nft_getTopRarity", params));
        assert_eq!(response.error.expect("error").code, SERVER_ERROR);
    }
}
