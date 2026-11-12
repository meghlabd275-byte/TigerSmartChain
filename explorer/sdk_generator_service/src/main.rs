#![forbid(unsafe_code)]
use std::net::SocketAddr;
use std::sync::Arc;
use serde::Deserialize;
use tiny_http::Response;
use tracing::info;

#[derive(Debug, Clone, Deserialize)]
pub struct ApiRequest { pub jsonrpc: String, pub method: String, pub params: Option<serde_json::Value>, pub id: u64 }

pub struct Handler;
impl Handler {
    pub fn handle(body: &[u8]) -> Response<std::io::Cursor<Vec<u8>>> {
        let request: ApiRequest = match serde_json::from_slice(body) { Ok(r) => r, Err(_) => return Response::from_string("{}").with_status_code(400) };
        let result = match request.method.as_str() {
            "generate_python" => serde_json::json!({"sdk": "# Python SDK generated", "language": "python"}),
            "generate_go" => serde_json::json!({"sdk": "// Go SDK generated", "language": "go"}),
            "generate_rust" => serde_json::json!({"sdk": "// Rust SDK generated", "language": "rust"}),
            "generate_java" => serde_json::json!({"sdk": "// Java SDK generated", "language": "java"}),
            _ => serde_json::json!({"error": "Unknown method"}),
        };
        Response::from_string(serde_json::to_string(&result).unwrap_or_default()).with_status_code(200)
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    info!("Starting SDK Generator Service");
    let addr = SocketAddr::from(([0, 0, 0, 0], 9002));
    info!("Server listening on {}", addr);

    tigerscan_service_http::serve(addr, move |body| Handler::handle(body))?;
    Ok(())
}