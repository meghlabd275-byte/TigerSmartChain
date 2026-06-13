#![forbid(unsafe_code)]
use std::net::SocketAddr;
use std::sync::Arc;
use serde::{Deserialize, Serialize};
use tiny_http::{Header, Response, StatusCode};
use tracing::info;
use token_migrator_service::{Config, MigratorService};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiRequest { pub jsonrpc: String, pub method: String, pub params: Option<serde_json::Value>, pub id: u64 }
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiResponse { pub jsonrpc: String, pub result: Option<serde_json::Value>, pub error: Option<ApiError>, pub id: u64 }
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiError { pub code: i32, pub message: String }

pub struct Handler { service: MigratorService }
impl Handler {
    pub fn handle(&self, body: &[u8]) -> Response<std::io::Cursor<Vec<u8>>> {
        let request: ApiRequest = match serde_json::from_slice(body) { Ok(r) => r, Err(_) => return Response::from_string("{}").with_status_code(StatusCode::from(400)) };
        let result = match request.method.as_str() {
            "migrator_getMigrations" => { let token = request.params.and_then(|p| p.get("token").and_then(|t| t.as_str())).unwrap_or(""); Ok(serde_json::json!(self.service.get_migrations(token))) },
            "migrator_getAirdrop" => { let token = request.params.and_then(|p| p.get("token").and_then(|t| t.as_str())).unwrap_or(""); Ok(serde_json::to_value(self.service.get_airdrop(token)).unwrap_or_default()) },
            "migrator_getBurns" => { let token = request.params.and_then(|p| p.get("token").and_then(|t| t.as_str())).unwrap_or(""); Ok(serde_json::to_value(self.service.get_burns(token)).unwrap_or_default()) },
            _ => Err(format!("Unknown method")),
        };
        let response = match result { Ok(r) => ApiResponse { jsonrpc: "2.0".to_string(), result: Some(r), error: None, id: request.id }, Err(e) => ApiResponse { jsonrpc: "2.0".to_string(), result: None, error: Some(ApiError { code: -32000, message: e }), id: request.id } };
        Response::from_string(serde_json::to_string(&response).unwrap_or_default()).with_status_code(StatusCode::from(200))
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    info!("Starting Token Migrator Service");
    let service = MigratorService::new(Config::default()).await?;
    let handler = Arc::new(Handler { service });
    let server = tiny_http::Server::http(SocketAddr::from(([0, 0, 0, 0], 8555)))?;
    for request in server.incoming_requests() {
        let handler = handler.clone();
        tokio::spawn(async move {
            let mut resp = request.respond(handler.handle(&request.as_vec())).unwrap();
            for h in [Header::from_bytes(&b"Content-Type"[..], &b"application/json"[..]).unwrap()] { resp.add_header(h); }
        });
    }
    Ok(())
}