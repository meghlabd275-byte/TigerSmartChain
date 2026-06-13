//! API Analytics Service Main
#![forbid(unsafe_code)]
use std::net::SocketAddr;
use std::sync::Arc;
use serde::{Deserialize, Serialize};
use tiny_http::{Header, Response, StatusCode};
use tracing::{info, Level};
use tracing_subscriber::FmtSubscriber;
use api_analytics_service::{Config, AnalyticsService, APIRequest};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiRequest { pub jsonrpc: String, pub method: String, pub params: Option<serde_json::Value>, pub id: u64 }
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiResponse { pub jsonrpc: String, pub result: Option<serde_json::Value>, pub error: Option<ApiError>, pub id: u64 }
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiError { pub code: i32, pub message: String }

pub struct Handler { service: AnalyticsService }
impl Handler {
    pub fn new(service: AnalyticsService) -> Self { Self { service } }
    pub fn handle(&self, body: &[u8]) -> Response<std::io::Cursor<Vec<u8>>> {
        let request: ApiRequest = match serde_json::from_slice(body) {
            Ok(r) => r,
            Err(e) => return Response::from_string(serde_json::to_string(&ApiResponse { jsonrpc: "2.0".to_string(), result: None, error: Some(ApiError { code: -32700, message: format!("Parse error: {}", e) }), id: 0 }).unwrap_or_default()).with_status_code(StatusCode::from(400)),
        };
        let result = match request.method.as_str() {
            "analytics_getDashboard" => Ok(serde_json::to_value(self.service.get_analytics()).unwrap_or_default()),
            "analytics_record" => { if let Some(params) = request.params { if let Ok(req) = serde_json::from_value::<APIRequest>(params) { self.service.record_request(req); } } Ok(serde_json::json!({"success": true})) },
            "analytics_checkRateLimit" => { let ip = request.params.and_then(|p| p.get("ip").and_then(|i| i.as_str())).unwrap_or(""); Ok(serde_json::to_value(self.service.check_rate_limit(ip)).unwrap_or_default()) },
            _ => Err(format!("Unknown method: {}", request.method)),
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
    FmtSubscriber::builder().with_max_level(Level::INFO).init();
    info!("Starting API Analytics Service");
    let service = AnalyticsService::new(Config::default());
    let handler = Arc::new(Handler::new(service));
    let server = tiny_http::Server::http(SocketAddr::from(([0, 0, 0, 0], 8554)))?;
    info!("Server listening on port 8554");
    for request in server.incoming_requests() {
        let handler = handler.clone();
        tokio::spawn(async move {
            let mut resp = request.respond(handler.handle(&request.as_vec()))?;
            for h in [Header::from_bytes(&b"Content-Type"[..], &b"application/json"[..]).unwrap(), Header::from_bytes(&b"X-Frame-Options"[..], &b"DENY"[..]).unwrap()] { resp.add_header(h); }
            Ok(())
        });
    }
    Ok(())
}