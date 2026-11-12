//! TigerScan Price Feed Service Main
//! High-performance API server for real-time token prices

#![forbid(unsafe_code)]

use std::net::SocketAddr;
use std::sync::Arc;

use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use tiny_http::{Response, StatusCode};
use tokio::runtime::Runtime;
use tracing::{info, Level};
use tracing_subscriber::FmtSubscriber;

use price_feed::{Config, PriceFeedService};

// ============================================================================
// API Types
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiRequest {
    pub method: String,
    pub params: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiResponse {
    pub success: bool,
    pub result: Option<serde_json::Value>,
    pub error: Option<String>,
}

// ============================================================================
// Rate Limiter
// ============================================================================

#[derive(Debug)]
pub struct RateLimiter {
    requests: Arc<RwLock<RateLimitStore>>,
    config: RateLimitConfig,
}

#[derive(Debug, Default)]
pub struct RateLimitStore {
    pub requests: std::collections::HashMap<String, Vec<std::time::Instant>>,
}

#[derive(Debug, Clone)]
pub struct RateLimitConfig {
    pub requests_per_minute: u32,
    pub burst_size: u32,
}

impl RateLimiter {
    pub fn new() -> Self {
        Self {
            requests: Arc::new(RwLock::new(RateLimitStore::default())),
            config: RateLimitConfig {
                requests_per_minute: 300,
                burst_size: 20,
            },
        }
    }

    pub fn check(&self, identifier: &str) -> Result<(), String> {
        let now = std::time::Instant::now();
        let store = self.requests.read();
        
        let requests = store.requests.get(identifier)
            .cloned()
            .unwrap_or_default();
        
        let last_minute: Vec<_> = requests.iter()
            .filter(|t| now.duration_since(**t).as_secs() < 60)
            .collect();
        
        let count = last_minute.len() as u32;
        if count >= self.config.burst_size {
            return Err("Rate limit exceeded - burst".to_string());
        }
        
        if count >= self.config.requests_per_minute {
            return Err("Rate limit exceeded - per minute".to_string());
        }
        
        Ok(())
    }

    pub fn record(&self, identifier: &str) {
        let now = std::time::Instant::now();
        let mut store = self.requests.write();
        
        let requests = store.requests.entry(identifier.to_string())
            .or_insert_with(Vec::new);
        
        requests.push(now);
        
        // Clean old entries
        let cutoff = now.checked_sub(std::time::Duration::from_secs(60))
            .unwrap_or(now);
        
        requests.retain(|t| *t > cutoff);
    }
}

// ============================================================================
// Handler
// ============================================================================

pub struct Handler {
    service: Arc<PriceFeedService>,
    rate_limiter: RateLimiter,
}

impl Handler {
    pub fn new(service: Arc<PriceFeedService>) -> Self {
        Self {
            service,
            rate_limiter: RateLimiter::new(),
        }
    }

    pub async fn handle(&self, body: &[u8], ip: &str) -> Response<std::io::Cursor<Vec<u8>>> {
        // Check rate limit
        if let Err(e) = self.rate_limiter.check(ip) {
            let response = ApiResponse {
                success: false,
                result: None,
                error: Some(e),
            };
            return Response::from_string(serde_json::to_string(&response).unwrap_or_default())
                .with_status_code(StatusCode::from(429));
        }
        
        self.rate_limiter.record(ip);

        // Parse request
        let request: ApiRequest = match serde_json::from_slice(body) {
            Ok(r) => r,
            Err(e) => {
                let response = ApiResponse {
                    success: false,
                    result: None,
                    error: Some(format!("Parse error: {}", e)),
                };
                return Response::from_string(serde_json::to_string(&response).unwrap_or_default())
                    .with_status_code(StatusCode::from(400));
            }
        };

        // Handle method
        let result = match request.method.as_str() {
            "price" => self.handle_price(request.params).await,
            "prices" => self.handle_prices(request.params).await,
            "history" => self.handle_history(request.params).await,
            "gas" => self.handle_gas(request.params).await,
            _ => Err(format!("Unknown method: {}", request.method)),
        };

        let response = match result {
            Ok(result) => ApiResponse {
                success: true,
                result: Some(result),
                error: None,
            },
            Err(e) => ApiResponse {
                success: false,
                result: None,
                error: Some(e),
            },
        };

        Response::from_string(serde_json::to_string(&response).unwrap_or_default())
            .with_status_code(StatusCode::from(200))
    }

    async fn handle_price(&self, params: Option<serde_json::Value>) -> Result<serde_json::Value, String> {
        let address = params.as_ref()
            .and_then(|p| p.get("address").and_then(|v| v.as_str()))
            .ok_or("Missing address")?;

        let requested = vec![address.to_string()];

        let prices = self
            .service
            .get_prices(&requested)
            .await
            .map_err(|error| error.to_string())?;

        let update = prices
            .into_iter()
            .next()
            .ok_or_else(|| format!("no price available for {address}"))?;

        Ok(serde_json::json!(update))
    }

    async fn handle_prices(&self, params: Option<serde_json::Value>) -> Result<serde_json::Value, String> {
        let addresses: Vec<String> = params
            .as_ref()
            .and_then(|p| p.get("addresses").and_then(|v| v.as_array()))
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str())
                    .map(String::from)
                    .collect()
            })
            .ok_or("Missing addresses")?;

        let prices = self
            .service
            .get_prices(&addresses)
            .await
            .map_err(|error| error.to_string())?;

        Ok(serde_json::json!({ "prices": prices }))
    }

    async fn handle_history(&self, params: Option<serde_json::Value>) -> Result<serde_json::Value, String> {
        let address = params.as_ref()
            .and_then(|p| p.get("address").and_then(|v| v.as_str()))
            .ok_or("Missing address")?;
            
        let days = params.as_ref()
            .and_then(|p| p.get("days").and_then(|v| v.as_u64()))
            .unwrap_or(7) as i64;

        let history = self.service.get_price_history(address, days)
            .await
            .map_err(|e| e.to_string())?;

        Ok(serde_json::json!({
            "history": history
        }))
    }

    async fn handle_gas(&self, _params: Option<serde_json::Value>) -> Result<serde_json::Value, String> {
        let gas = self.service.get_gas_oracle()
            .await
            .map_err(|e| e.to_string())?;

        Ok(serde_json::json!({
            "gas": gas
        }))
    }
}

// ============================================================================
// Main
// ============================================================================

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize logging
    let subscriber = FmtSubscriber::builder()
        .with_max_level(Level::INFO)
        .with_target(false)
        .init();

    info!("Starting Price Feed Service");

    // Load configuration
    let config = Config::default();

    // Create service
    let service = Arc::new(PriceFeedService::new(config).await?);

    // Create handler
    let handler = Arc::new(Handler::new(service));

    let addr = SocketAddr::from(([0, 0, 0, 0], 8549));
    info!("Server listening on {}", addr);

    // The serving loop is synchronous; the price lookups are not.
    let runtime = Runtime::new()?;

    tigerscan_service_http::serve_with_ip(addr, move |body, ip| {
        runtime.block_on(handler.handle(body, ip))
    })?;

    Ok(())
}