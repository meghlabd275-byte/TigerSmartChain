#![forbid(unsafe_code)]
use std::net::SocketAddr;
use std::sync::Arc;
use serde::{Deserialize, Serialize};
use tiny_http::{Header, Response, StatusCode};
use tracing::info;
use block_visualizer_service::{Config, VisualizerService};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiRequest { pub jsonrpc: String, pub method: String, pub params: Option<serde_json::Value>, pub id: u64 }
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiResponse { pub jsonrpc: String, pub result: Option<serde_json::Value>, pub error: Option<ApiError>, pub id: u64 }
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiError { pub code: i32, pub message: String }

pub struct Handler { service: VisualizerService }
impl Handler {
    pub fn handle(&self, body: &[u8]) -> Response<std::io::Cursor<Vec<u8>>> {
        let request: ApiRequest = match serde_json::from_slice(body) { Ok(r) => r, Err(_) => return Response::from_string("{}").with_status_code(StatusCode::from(400)) };
        let result = match request.method.as_str() {
            "visualizer_getBlock" => {
                let block = request.params.and_then(|p| p.get("block").and_then(|b| b.as_u64())).unwrap_or(0);
                // Note: Would need async handler in production
                Ok(serde_json::json!({"block": block}))
            },
            "visualizer_getTxFlow" => {
                let tx = request.params.and_then(|p| p.get("tx").and_then(|t| t.as_str())).unwrap_or("");
                Ok(serde_json::json!({"tx": tx}))
            },
            _ => Err("Unknown method".to_string()),
        };
        let response = match result {
            Ok(r) => ApiResponse { jsonrpc: "2.0".to_string(), result: Some(r), error: None, id: request.id },
            Err(e) => ApiResponse { jsonrpc: "2.0".to_string(), result: None, error: Some(ApiError { code: -32000, message: e }), id: request.id },
        };
        Response::from_string(serde_json::to_string(&response).unwrap_or_default()).with_status_code(StatusCode::from(200))
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    info!("Starting Block Visualizer Service");
    let service = VisualizerService::new(Config::default()).await?;
    let handler = Arc::new(Handler { service });
    let server = tiny_http::Server::http(SocketAddr::from(([0, 0, 0, 0], 8556)))?;
    for request in server.incoming_requests() {
        let handler = handler.clone();
        tokio::spawn(async move {
            let mut resp = request.respond(handler.handle(&request.as_vec())).unwrap();
            for h in [Header::from_bytes(&b"Content-Type"[..], &b"application/json"[..]).unwrap()] { resp.add_header(h); }
        });
    }
    Ok(())
}