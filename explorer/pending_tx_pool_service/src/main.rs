//! TigerScan Pending Transaction Pool Service Main
//! High-performance WebSocket API server for live mempool data

#![forbid(unsafe_code)]

use std::net::SocketAddr;
use std::sync::Arc;

use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use tiny_http::{Header, Response, StatusCode};
use tracing::{error, info, Level};
use tracing_subscriber::FmtSubscriber;

use pending_tx_pool_service::{
    Config, MempoolApiRequest, MempoolApiResponse, MempoolService, MempoolUpdate,
    PoolStats,
};

// ============================================================================
// API Types
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiRequest {
    pub jsonrpc: String,
    pub method: String,
    pub params: Option<serde_json::Value>,
    pub id: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiResponse {
    pub jsonrpc: String,
    pub result: Option<serde_json::Value>,
    pub error: Option<ApiError>,
    pub id: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiError {
    pub code: i32,
    pub message: String,
}

// ============================================================================
// Rate Limiter
// ============================================================================

#[derive(Debug)]
pub struct RateLimiter {
    requests: Arc<RwLock<RateLimitStore>>,
}

#[derive(Debug, Default)]
pub struct RateLimitStore {
    pub requests: std::collections::HashMap<String, Vec<std::time::Instant>>,
}

impl RateLimiter {
    pub fn new() -> Self {
        Self {
            requests: Arc::new(RwLock::new(RateLimitStore::default())),
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
        
        if last_minute.len() >= 60 {
            return Err("Rate limit exceeded".to_string());
        }
        
        Ok(())
    }

    pub fn record(&self, identifier: &str) {
        let now = std::time::Instant::now();
        let mut store = self.requests.write();
        
        let requests = store.requests.entry(identifier.to_string())
            .or_insert_with(Vec::new);
        
        requests.push(now);
        
        let cutoff = now.checked_sub(std::time::Duration::from_secs(60))
            .unwrap_or(now);
        
        requests.retain(|t| *t > cutoff);
    }
}

// ============================================================================
// WebSocket Handler
// ============================================================================

pub struct Handler {
    service: MempoolService,
    rate_limiter: RateLimiter,
}

impl Handler {
    pub fn new(service: MempoolService) -> Self {
        Self {
            service,
            rate_limiter: RateLimiter::new(),
        }
    }

    pub fn handle(&self, body: &[u8], ip: &str) -> Response<std::io::Cursor<Vec<u8>>> {
        // Check rate limit
        if let Err(e) = self.rate_limiter.check(ip) {
            let response = ApiResponse {
                jsonrpc: "2.0".to_string(),
                result: None,
                error: Some(ApiError {
                    code: -32000,
                    message: e,
                }),
                id: 0,
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
                    jsonrpc: "2.0".to_string(),
                    result: None,
                    error: Some(ApiError {
                        code: -32700,
                        message: format!("Parse error: {}", e),
                    }),
                    id: 0,
                };
                return Response::from_string(serde_json::to_string(&response).unwrap_or_default())
                    .with_status_code(StatusCode::from(400));
            }
        };
        
        // Handle method
        let result = match request.method.as_str() {
            "mempool_getPending" => self.handle_get_pending(request.params),
            "mempool_getStats" => self.handle_get_stats(),
            "mempool_getGasOracle" => self.handle_gas_oracle().await,
            "mempool_getBySender" => self.handle_get_by_sender(request.params),
            "mempool_subscribe" => self.handle_subscribe(),
            _ => Err(format!("Unknown method: {}", request.method)),
        };
        
        let response = match result {
            Ok(result) => ApiResponse {
                jsonrpc: "2.0".to_string(),
                result: Some(serde_json::to_value(result).unwrap_or_default()),
                error: None,
                id: request.id,
            },
            Err(e) => ApiResponse {
                jsonrpc: "2.0".to_string(),
                result: None,
                error: Some(ApiError {
                    code: -32000,
                    message: e,
                }),
                id: request.id,
            },
        };
        
        Response::from_string(serde_json::to_string(&response).unwrap_or_default())
            .with_status_code(StatusCode::from(200))
    }

    fn handle_get_pending(&self, params: Option<serde_json::Value>) -> Result<serde_json::Value, String> {
        let params: MempoolApiRequest = params
            .and_then(|p| serde_json::from_value(p).ok())
            .unwrap_or_default();
        
        let txs = self.service.get_pending(params.limit, params.offset);
        
        // Filter by gas price if specified
        let txs = if let Some(min_gas) = params.min_gas {
            txs.into_iter()
                .filter(|tx| {
                    let gas = pending_tx_pool_service::to_gwei(&tx.gas_price);
                    gas >= min_gas
                })
                .collect()
        } else {
            txs
        };
        
        Ok(serde_json::json!({
            "transactions": txs,
            "total": self.service.get_stats().total_pending
        }))
    }

    fn handle_get_stats(&self) -> Result<PoolStats, String> {
        Ok(self.service.get_stats())
    }

    async fn handle_gas_oracle(&self) -> Result<serde_json::Value, String> {
        let oracle = self.service.get_gas_oracle().await
            .map_err(|e| e.to_string())?;
        
        Ok(serde_json::json!({
            "slow": oracle.slow,
            "average": oracle.average,
            "fast": oracle.fast,
            "base_fee": oracle.base_fee,
            "last_update": oracle.last_update
        }))
    }

    fn handle_get_by_sender(&self, params: Option<serde_json::Value>) -> Result<serde_json::Value, String> {
        let sender = params
            .and_then(|p| p.get("sender").and_then(|s| s.as_str()))
            .ok_or("Missing sender")?;
        
        let txs = self.service.get_by_sender(sender);
        
        Ok(serde_json::json!({
            "transactions": txs
        }))
    }

    fn handle_subscribe(&self) -> Result<serde_json::Value, String> {
        // Return subscription info
        Ok(serde_json::json!({
            "subscription": "mempool_updates",
            "message": "Subscribe via WebSocket for real-time updates"
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
    
    info!("Starting Pending Transaction Pool Service");
    
    // Load configuration
    let config = Config::default();
    
    // Create service
    let service = MempoolService::new(config).await?;
    let mut service = service;
    service.start();
    
    // Create handler
    let handler = Arc::new(Handler::new(service));
    
    // Start HTTP server
    let addr = SocketAddr::from(([0, 0, 0, 0], 8547));
    let server = tiny_http::Server::http(addr)?;
    
    info!("Server listening on {}", addr);
    
    for request in server.incoming_requests() {
        let handler = handler.clone();
        
        tokio::spawn(async move {
            let ip = request.remote_addr()
                .map(|a| a.ip().to_string())
                .unwrap_or_else(|| "unknown".to_string());
            
            let body = request.as_vec();
            
            let response = handler.handle(&body, &ip);
            
            let mut resp = request.respond(response)?;
            
            // Add security headers
            let headers = vec![
                Header::from_bytes(&b"Content-Type"[..], &b"application/json"[..]).unwrap(),
                Header::from_bytes(&b"X-Content-Type-Options"[..], &b"nosniff"[..]).unwrap(),
                Header::from_bytes(&b"X-Frame-Options"[..], &b"DENY"[..]).unwrap(),
                Header::from_bytes(&b"X-XSS-Protection"[..], &b"1; mode=block"[..]).unwrap(),
                Header::from_bytes(&b"Strict-Transport-Security"[..], &b"max-age=31536000"[..]).unwrap(),
            ];
            
            for header in headers {
                resp.add_header(header);
            }
            
            Ok(())
        });
    }
    
    Ok(())
}