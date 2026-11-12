//! Security Honeypot Service Main

#![forbid(unsafe_code)]

use std::net::SocketAddr;
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use tiny_http::{Response, StatusCode};
use tokio::runtime::Runtime;
use tracing::{info, Level};
use tracing_subscriber::FmtSubscriber;

use security_honeypot_service::{Config, SecurityScanner};

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
// Handler
// ============================================================================

pub struct Handler {
    scanner: SecurityScanner,
}

impl Handler {
    pub fn new(scanner: SecurityScanner) -> Self {
        Self { scanner }
    }

    pub async fn handle(&self, body: &[u8]) -> Response<std::io::Cursor<Vec<u8>>> {
        let request: ApiRequest = match serde_json::from_slice(body) {
            Ok(r) => r,
            Err(e) => {
                let response = ApiResponse {
                    jsonrpc: "2.0".to_string(),
                    result: None,
                    error: Some(ApiError { code: -32700, message: format!("Parse error: {}", e) }),
                    id: 0,
                };
                return Response::from_string(serde_json::to_string(&response).unwrap_or_default())
                    .with_status_code(StatusCode::from(400));
            }
        };

        let result = match request.method.as_str() {
            "security_analyze" => self.handle_analyze(request.params).await,
            "security_checkAddress" => self.handle_check_address(request.params).await,
            _ => Err(format!("Unknown method: {}", request.method)),
        };

        let response = match result {
            Ok(r) => ApiResponse {
                jsonrpc: "2.0".to_string(),
                result: Some(serde_json::to_value(r).unwrap_or_default()),
                error: None,
                id: request.id,
            },
            Err(e) => ApiResponse {
                jsonrpc: "2.0".to_string(),
                result: None,
                error: Some(ApiError { code: -32000, message: e }),
                id: request.id,
            },
        };

        Response::from_string(serde_json::to_string(&response).unwrap_or_default())
            .with_status_code(StatusCode::from(200))
    }

    async fn handle_analyze(&self, params: Option<serde_json::Value>) -> Result<serde_json::Value, String> {
        let address = params.as_ref().and_then(|p| p.get("address")).and_then(|a| a.as_str())
            .ok_or("Missing address")?;
        
        let report = self.scanner.analyze(address).await
            .map_err(|e| e.to_string())?;
        
        Ok(serde_json::to_value(report).unwrap_or_default())
    }

    async fn handle_check_address(
        &self,
        params: Option<serde_json::Value>,
    ) -> Result<serde_json::Value, String> {
        let address = params
            .as_ref()
            .and_then(|p| p.get("address"))
            .and_then(|a| a.as_str())
            .ok_or("missing address")?;

        let report = self
            .scanner
            .analyze(address)
            .await
            .map_err(|error| error.to_string())?;

        Ok(serde_json::json!({
            "address": report.address,
            "is_malicious": report.is_malicious,
            "risk_level": report.risk_level,
        }))
    }
}

// ============================================================================
// Main
// ============================================================================

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let subscriber = FmtSubscriber::builder()
        .with_max_level(Level::INFO)
        .with_target(false)
        .init();

    info!("Starting Security Honeypot Service");

    let config = Config::default();
    let scanner = SecurityScanner::new(config).await?;
    let handler = Arc::new(Handler::new(scanner));

    let addr = SocketAddr::from(([0, 0, 0, 0], 8553));
    info!("Server listening on {}", addr);

    // The serving loop is synchronous; the node calls are not.
    let runtime = Runtime::new()?;

    tigerscan_service_http::serve(addr, move |body| {
        runtime.block_on(handler.handle(body))
    })?;

    Ok(())
}