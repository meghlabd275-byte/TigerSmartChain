//! TigerScan NFT Metadata Service Main
//! High-performance API server for NFT metadata and analytics

#![forbid(unsafe_code)]

use std::net::SocketAddr;
use std::sync::Arc;

use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use tiny_http::{Header, Response, Server, StatusCode};
use tracing::{error, info, Level};
use tracing_subscriber::FmtSubscriber;

use tigerscan_nft_metadata::{Collection, Config, NFTData, NFTMetadataService, RarityRank};

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
                requests_per_minute: 120,
                burst_size: 10,
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
    service: Arc<NFTMetadataService>,
    rate_limiter: RateLimiter,
}

impl Handler {
    pub fn new(service: Arc<NFTMetadataService>) -> Self {
        Self {
            service,
            rate_limiter: RateLimiter::new(),
        }
    }

    pub fn handle(&self, body: &[u8], ip: &str) -> Response<std::io::Cursor<Vec<u8>>> {
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
            "nft" => self.handle_nft(request.params).await,
            "collection" => self.handle_collection(request.params).await,
            "rarity" => self.handle_rarity(request.params).await,
            "search" => self.handle_search(request.params).await,
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

    async fn handle_nft(&self, params: Option<serde_json::Value>) -> Result<serde_json::Value, String> {
        let (collection, token_id) = params.as_ref()
            .and_then(|p| {
                let c = p.get("collection").and_then(|v| v.as_str())?;
                let t = p.get("token_id").and_then(|v| v.as_str())?;
                Some((c, t))
            })
            .ok_or("Missing collection or token_id")?;

        let nft = self.service.get_nft(collection, token_id)
            .await
            .map_err(|e| e.to_string())?;

        Ok(serde_json::json!({
            "nft": nft
        }))
    }

    async fn handle_collection(&self, params: Option<serde_json::Value>) -> Result<serde_json::Value, String> {
        let address = params.as_ref()
            .and_then(|p| p.get("address").and_then(|v| v.as_str()))
            .ok_or("Missing address")?;

        let collection = self.service.get_collection(address)
            .await
            .map_err(|e| e.to_string())?;

        Ok(serde_json::json!({
            "collection": collection
        }))
    }

    async fn handle_rarity(&self, params: Option<serde_json::Value>) -> Result<serde_json::Value, String> {
        let (collection, token_id) = params.as_ref()
            .and_then(|p| {
                let c = p.get("collection").and_then(|v| v.as_str())?;
                let t = p.get("token_id").and_then(|v| v.as_str())?;
                Some((c, t))
            })
            .ok_or("Missing collection or token_id")?;

        let rarity = self.service.get_rarity_rank(collection, token_id)
            .await
            .map_err(|e| e.to_string())?;

        Ok(serde_json::json!({
            "rarity": rarity
        }))
    }

    async fn handle_search(&self, params: Option<serde_json::Value>) -> Result<serde_json::Value, String> {
        let query = params.as_ref()
            .and_then(|p| p.get("q").and_then(|v| v.as_str()))
            .ok_or("Missing search query")?;

        let results = self.service.search_collections(query)
            .await
            .map_err(|e| e.to_string())?;

        Ok(serde_json::json!({
            "results": results
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

    info!("Starting NFT Metadata Service");

    // Load configuration
    let config = Config::default();

    // Create service
    let service = Arc::new(NFTMetadataService::new(config).await?);

    // Create handler
    let handler = Arc::new(Handler::new(service));

    // Start HTTP server
    let addr = SocketAddr::from(([0, 0, 0, 0], 8548));
    let server = Server::http(addr)?;

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
                Header::from_bytes(&b"Content-Security-Policy"[..], &b"default-src 'none'"[..]).unwrap(),
            ];

            for header in headers {
                resp.add_header(header);
            }

            Ok(())
        });
    }

    Ok(())
}