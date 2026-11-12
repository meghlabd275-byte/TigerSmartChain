#![forbid(unsafe_code)]
use std::net::SocketAddr;
use serde::Deserialize;
use tiny_http::Response;
use tracing::info;

#[derive(Debug, Clone, Deserialize)]
pub struct ApiRequest { pub method: String, pub params: Option<serde_json::Value> }

pub struct Handler;
impl Handler {
    pub fn handle(body: &[u8]) -> Response<std::io::Cursor<Vec<u8>>> {
        let request: ApiRequest = match serde_json::from_slice(body) { Ok(r) => r, Err(_) => return Response::from_string("{}").with_status_code(400) };
        let result = match request.method.as_str() {
            "export_createJob" => serde_json::json!({"job_id": "job-123", "status": "pending"}),
            "export_getJob" => serde_json::json!({"job_id": "job-123", "status": "completed", "download_url": "https://api.tigerscan.io/v1/export/download/job-123"}),
            "export_getCsv" => serde_json::json!({"data": "hash,from,to,value\n0x123,0xabc,0xdef,100"}),
            _ => serde_json::json!({"error": "Unknown method"}),
        };
        Response::from_string(serde_json::to_string(&result).unwrap_or_default()).with_status_code(200)
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    info!("Starting Export Service");
    let addr = SocketAddr::from(([0, 0, 0, 0], 9004));
    info!("Server listening on {}", addr);

    tigerscan_service_http::serve(addr, move |body| Handler::handle(body))?;
    Ok(())
}