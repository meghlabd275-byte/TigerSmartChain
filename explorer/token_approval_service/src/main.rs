//! Token Approval Service Main

#![forbid(unsafe_code)]

use std::net::SocketAddr;
use std::sync::Arc;

use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use tiny_http::{Header, Response, StatusCode};
use tracing::{error, info, Level};
use tracing_subscriber::FmtSubscriber;

use token_approval_service::{ApprovalService, ApprovalState, Config};

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
    service: ApprovalService,
}

impl Handler {
    pub fn new(service: ApprovalService) -> Self {
        Self { service }
    }

    pub fn handle(&self, body: &[u8]) -> Response<std::io::Cursor<Vec<u8>>> {
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

        let result = match request.method.as_str() {
            "approvals_getByOwner" => self.handle_get_by_owner(request.params),
            "approvals_getByToken" => self.handle_get_by_token(request.params),
            "approvals_getBySpender" => self.handle_get_by_spender(request.params),
            "approvals_getStats" => self.handle_get_stats(),
            "approvals_analyzeRisk" => self.handle_analyze_risk(request.params),
            "approvals_generateRevocation" => self.handle_generate_revocation(request.params),
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

    fn handle_get_by_owner(&self, params: Option<serde_json::Value>) -> Result<serde_json::Value, String> {
        let owner = params
            .and_then(|p| p.get("owner").and_then(|o| o.as_str()))
            .ok_or("Missing owner")?;

        let approvals = self.service.get_by_owner(owner);

        Ok(serde_json::json!({
            "approvals": approvals,
            "total": approvals.len()
        }))
    }

    fn handle_get_by_token(&self, params: Option<serde_json::Value>) -> Result<serde_json::Value, String> {
        let token = params
            .and_then(|p| p.get("token").and_then(|t| t.as_str()))
            .ok_or("Missing token")?;

        let approvals = self.service.get_by_token(token);

        Ok(serde_json::json!({
            "approvals": approvals,
            "total": approvals.len()
        }))
    }

    fn handle_get_by_spender(&self, params: Option<serde_json::Value>) -> Result<serde_json::Value, String> {
        let spender = params
            .and_then(|p| p.get("spender").and_then(|s| s.as_str()))
            .ok_or("Missing spender")?;

        let approvals = self.service.get_by_spender(spender);

        Ok(serde_json::json!({
            "approvals": approvals,
            "total": approvals.len()
        }))
    }

    fn handle_get_stats(&self) -> Result<serde_json::Value, String> {
        let stats = self.service.get_stats();

        Ok(serde_json::to_value(stats).unwrap_or_default())
    }

    fn handle_analyze_risk(&self, params: Option<serde_json::Value>) -> Result<serde_json::Value, String> {
        let spender = params
            .and_then(|p| p.get("spender").and_then(|s| s.as_str()))
            .ok_or("Missing spender")?;

        let risk = self.service.analyze_risk(spender);

        Ok(serde_json::to_value(risk).unwrap_or_default())
    }

    fn handle_generate_revocation(&self, params: Option<serde_json::Value>) -> Result<String, String> {
        let owner = params
            .and_then(|p| p.get("owner").and_then(|o| o.as_str()))
            .ok_or("Missing owner")?;
        let token = params
            .and_then(|p| p.get("token").and_then(|t| t.as_str()))
            .ok_or("Missing token")?;
        let spender = params
            .and_then(|p| p.get("spender").and_then(|s| s.as_str()))
            .ok_or("Missing spender")?;

        self.service.generate_revocation(owner, token, spender)
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

    info!("Starting Token Approval Service");

    let config = Config::default();
    let service = ApprovalService::new(config).await?;
    let handler = Arc::new(Handler::new(service));

    let addr = SocketAddr::from(([0, 0, 0, 0], 8548));
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
                Header::from_bytes(&b"X-Content-Type-Options"[..], &b"nosniff"[..]).unwrap(),
                Header::from_bytes(&b"X-Frame-Options"[..], &b"DENY"[..]).unwrap(),
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