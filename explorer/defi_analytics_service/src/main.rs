//! TigerScan DeFi Analytics Service Main
//! High-performance API server for DeFi analytics

#![forbid(unsafe_code)]

use std::net::SocketAddr;
use std::sync::Arc;

use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use tiny_http::{Header, Response, Server, StatusCode};
use tracing::{error, info, Level};
use tracing_subscriber::FmtSubscriber;

use tigerscan_defi_analytics::{Config, DeFiAnalytics, PoolData, TVLData, YieldOpportunity};

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
// Handler
// ============================================================================

pub struct Handler {
    analytics: Arc<DeFiAnalytics>,
}

impl Handler {
    pub fn new(analytics: Arc<DeFiAnalytics>) -> Self {
        Self { analytics }
    }

    pub fn handle(&self, body: &[u8]) -> Response<std::io::Cursor<Vec<u8>>> {
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
            "tvl" => self.handle_tvl(request.params).await,
            "pools" => self.handle_pools(request.params).await,
            "yields" => self.handle_yields(request.params).await,
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

    async fn handle_tvl(&self, params: Option<serde_json::Value>) -> Result<serde_json::Value, String> {
        let protocols = self.analytics.get_tvl_data()
            .await
            .map_err(|e| e.to_string())?;

        Ok(serde_json::json!({
            "protocols": protocols
        }))
    }

    async fn handle_pools(&self, params: Option<serde_json::Value>) -> Result<serde_json::Value, String> {
        let limit = params.as_ref()
            .and_then(|p| p.get("limit").and_then(|v| v.as_u64()))
            .unwrap_or(20) as usize;

        let pools = self.analytics.get_top_pools(limit)
            .await
            .map_err(|e| e.to_string())?;

        Ok(serde_json::json!({
            "pools": pools
        }))
    }

    async fn handle_yields(&self, params: Option<serde_json::Value>) -> Result<serde_json::Value, String> {
        let limit = params.as_ref()
            .and_then(|p| p.get("limit").and_then(|v| v.as_u64()))
            .unwrap_or(20) as usize;

        let yields = self.analytics.get_top_yields(limit)
            .await
            .map_err(|e| e.to_string())?;

        Ok(serde_json::json!({
            "yields": yields
        }))
    }

    async fn handle_search(&self, params: Option<serde_json::Value>) -> Result<serde_json::Value, String> {
        let query = params.as_ref()
            .and_then(|p| p.get("q").and_then(|v| v.as_str()))
            .ok_or("Missing search query")?;

        // Search pools by token symbol
        let pools = self.analytics.search_pools(query)
            .await
            .map_err(|e| e.to_string())?;

        Ok(serde_json::json!({
            "pools": pools
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

    info!("Starting DeFi Analytics Service");

    // Load configuration
    let config = Config::default();

    // Create analytics service
    let analytics = Arc::new(DeFiAnalytics::new(config).await?);

    // Create handler
    let handler = Arc::new(Handler::new(analytics));

    // Start HTTP server
    let addr = SocketAddr::from(([0, 0, 0, 0], 8547));
    let server = Server::http(addr)?;

    info!("Server listening on {}", addr);

    for request in server.incoming_requests() {
        let handler = handler.clone();

        tokio::spawn(async move {
            let body = request.as_vec();

            let response = handler.handle(&body);

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