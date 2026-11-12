//! TigerScan DeFi Analytics Service Main
//! High-performance API server for DeFi analytics

#![forbid(unsafe_code)]

use std::net::SocketAddr;
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use tiny_http::{Response, StatusCode};
use tokio::runtime::Runtime;
use tracing::{info, Level};
use tracing_subscriber::FmtSubscriber;

use tigerscan_defi_analytics::{Config, DeFiAnalyticsService};

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
    analytics: Arc<DeFiAnalyticsService>,
}

impl Handler {
    pub fn new(analytics: Arc<DeFiAnalyticsService>) -> Self {
        Self { analytics }
    }

    pub async fn handle(&self, body: &[u8]) -> Response<std::io::Cursor<Vec<u8>>> {
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
            "tvl" => self.handle_tvl().await,
            "pools" => self.handle_pools(request.params).await,
            "yields" => self.handle_yields(request.params).await,
            "lendingRates" => self.handle_lending_rates().await,
            "tvlHistory" => self.handle_tvl_history(request.params).await,
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

    async fn handle_tvl(&self) -> Result<serde_json::Value, String> {
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

    async fn handle_lending_rates(&self) -> Result<serde_json::Value, String> {
        let rates = self.analytics.get_lending_rates()
            .await
            .map_err(|e| e.to_string())?;

        Ok(serde_json::json!({
            "lendingRates": rates
        }))
    }

    async fn handle_tvl_history(&self, params: Option<serde_json::Value>) -> Result<serde_json::Value, String> {
        let protocol = params.as_ref()
            .and_then(|p| p.get("protocol").and_then(|v| v.as_str()))
            .ok_or("Missing protocol")?;

        let days = params.as_ref()
            .and_then(|p| p.get("days").and_then(|v| v.as_i64()))
            .unwrap_or(30);

        let history = self.analytics.get_tvl_history(protocol, days as i32)
            .await
            .map_err(|e| e.to_string())?;

        Ok(serde_json::json!({
            "history": history
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
    FmtSubscriber::builder()
        .with_max_level(Level::INFO)
        .with_target(false)
        .init();

    info!("Starting DeFi Analytics Service");

    // Load configuration
    let config = Config::default();

    // Create analytics service
    let analytics = Arc::new(DeFiAnalyticsService::new(config).await?);

    // The polling loop keeps the tables current; the API serves the same rows.
    let updater = Arc::clone(&analytics);
    tokio::spawn(async move {
        updater.start().await;
    });

    // Create handler
    let handler = Arc::new(Handler::new(analytics));

    let addr = SocketAddr::from(([0, 0, 0, 0], 8562));
    info!("Server listening on {}", addr);

    // The serving loop is synchronous; the analytics lookups are not.
    let runtime = Runtime::new()?;

    tigerscan_service_http::serve(addr, move |body| runtime.block_on(handler.handle(body)))?;

    Ok(())
}