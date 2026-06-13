//! Contract Diff Service Main

#![forbid(unsafe_code)]

use std::net::SocketAddr;
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use tiny_http::{Header, Response, StatusCode};
use tracing::{info, Level};
use tracing_subscriber::FmtSubscriber;

use contract_diff_service::{Config, ContractDiffService};

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
    service: ContractDiffService,
}

impl Handler {
    pub fn new(service: ContractDiffService) -> Self {
        Self { service }
    }

    pub fn handle(&self, body: &[u8]) -> Response<std::io::Cursor<Vec<u8>>> {
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
            "diff_getHistory" => self.handle_history(request.params),
            "diff_compare" => self.handle_compare(request.params),
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

    fn handle_history(&self, params: Option<serde_json::Value>) -> Result<serde_json::Value, String> {
        let address = params.and_then(|p| p.get("address").and_then(|a| a.as_str()))
            .ok_or("Missing address")?;
        
        let history = self.service.get_history(address)
            .ok_or("No history found")?;
        
        Ok(serde_json::to_value(history).unwrap_or_default())
    }

    fn handle_compare(&self, params: Option<serde_json::Value>) -> Result<serde_json::Value, String> {
        let address = params.and_then(|p| p.get("address").and_then(|a| a.as_str()))
            .ok_or("Missing address")?;
        let from = params.and_then(|p| p.get("from_version").and_then(|v| v.as_u64()))
            .ok_or("Missing from_version")? as u32;
        let to = params.and_then(|p| p.get("to_version").and_then(|v| v.as_u64()))
            .ok_or("Missing to_version")? as u32;
        
        let diff = self.service.compare(address, from, to)
            .ok_or("Version not found")?;
        
        Ok(serde_json::to_value(diff).unwrap_or_default())
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

    info!("Starting Contract Diff Service");

    let config = Config::default();
    let service = ContractDiffService::new(config).await?;
    let handler = Arc::new(Handler::new(service));

    let addr = SocketAddr::from(([0, 0, 0, 0], 8552));
    let server = tiny_http::Server::http(addr)?;

    info!("Server listening on {}", addr);

    for request in server.incoming_requests() {
        let handler = handler.clone();
        tokio::spawn(async move {
            let body = request.as_vec();
            let response = handler.handle(&body);
            let mut resp = request.respond(response)?;
            
            let headers = vec![
                Header::from_bytes(&b"Content-Type"[..], &b"application/json"[..]).unwrap(),
                Header::from_bytes(&b"X-Frame-Options"[..], &b"DENY"[..]).unwrap(),
                Header::from_bytes(&b"Strict-Transport-Security"[..], &b"max-age=31536000"[..]).unwrap(),
            ];
            
            for header in headers { resp.add_header(header); }
            Ok(())
        });
    }

    Ok(())
}