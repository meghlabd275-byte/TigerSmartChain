//! User Service Main
#![forbid(unsafe_code)]
use std::net::SocketAddr;
use std::sync::Arc;
use serde::{Deserialize, Serialize};
use tiny_http::{Header, Response, StatusCode};
use tracing::info;

use user_service::{AuthService, WatchlistService, NotesService, AlertsService, DashboardService, Config};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiRequest { pub jsonrpc: String, pub method: String, pub params: Option<serde_json::Value>, pub id: u64 }
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiResponse { pub jsonrpc: String, pub result: Option<serde_json::Value>, pub error: Option<ApiError>, pub id: u64 }
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiError { pub code: i32, pub message: String }

pub struct Handler {
    auth: AuthService,
    watchlist: WatchlistService,
    notes: NotesService,
    alerts: AlertsService,
    dashboard: DashboardService,
}
impl Handler {
    pub fn new() -> Self { Self { auth: AuthService::new(Config::default()), watchlist: WatchlistService::new(), notes: NotesService::new(), alerts: AlertsService::new(), dashboard: DashboardService::new() } }
    pub fn handle(&self, body: &[u8]) -> Response<std::io::Cursor<Vec<u8>>> {
        let request: ApiRequest = match serde_json::from_slice(body) { Ok(r) => r, Err(_) => return Response::from_string("{}").with_status_code(StatusCode::from(400)) };
        let result = match request.method.as_str() {
            "user_register" => { let p = request.params.unwrap(); self.handle_register(&p) },
            "user_login" => { let p = request.params.unwrap(); self.handle_login(&p) },
            "user_logout" => Ok(serde_json::json!({"success": true})),
            "watchlist_create" => { let p = request.params.unwrap(); self.handle_watchlist_create(&p) },
            "watchlist_add" => { let p = request.params.unwrap(); self.handle_watchlist_add(&p) },
            "notes_save" => { let p = request.params.unwrap(); self.handle_notes_save(&p) },
            "alerts_create" => { let p = request.params.unwrap(); self.handle_alerts_create(&p) },
            "dashboard_create" => { let p = request.params.unwrap(); self.handle_dashboard_create(&p) },
            _ => Err("Unknown method".to_string()),
        };
        let response = match result { Ok(r) => ApiResponse { jsonrpc: "2.0".to_string(), result: Some(r), error: None, id: request.id }, Err(e) => ApiResponse { jsonrpc: "2.0".to_string(), result: None, error: Some(ApiError { code: -32000, message: e }), id: request.id } };
        Response::from_string(serde_json::to_string(&response).unwrap_or_default()).with_status_code(StatusCode::from(200))
    }
    fn handle_register(&self, p: &serde_json::Value) -> Result<serde_json::Value, String> {
        let email = p.get("email").and_then(|v| v.as_str()).unwrap_or("");
        let username = p.get("username").and_then(|v| v.as_str()).unwrap_or("");
        let password = p.get("password").and_then(|v| v.as_str()).unwrap_or("");
        let user = self.auth.register(email, username, password).map_err(|e| e.to_string())?;
        Ok(serde_json::json!({"user": user}))
    }
    fn handle_login(&self, p: &serde_json::Value) -> Result<serde_json::Value, String> {
        let email = p.get("email").and_then(|v| v.as_str()).unwrap_or("");
        let password = p.get("password").and_then(|v| v.as_str()).unwrap_or("");
        let session = self.auth.login(email, password).map_err(|e| e.to_string())?;
        Ok(serde_json::json!({"token": session.token}))
    }
    fn handle_watchlist_create(&self, p: &serde_json::Value) -> Result<serde_json::Value, String> {
        let user_id = p.get("user_id").and_then(|v| v.as_str()).unwrap_or("");
        let name = p.get("name").and_then(|v| v.as_str()).unwrap_or("My Watchlist");
        let watchlist = self.watchlist.create(user_id, name);
        Ok(serde_json::json!({"watchlist": watchlist}))
    }
    fn handle_watchlist_add(&self, p: &serde_json::Value) -> Result<serde_json::Value, String> {
        let id = p.get("id").and_then(|v| v.as_str()).unwrap_or("");
        let addr = p.get("address").and_then(|v| v.as_str()).unwrap_or("");
        let label = p.get("label").and_then(|v| v.as_str()).unwrap_or("");
        let note = p.get("note").and_then(|v| v.as_str()).map(|s| s.to_string());
        let watchlist = self.watchlist.add_address(id, addr, label, note).map_err(|e| e.to_string())?;
        Ok(serde_json::json!({"watchlist": watchlist}))
    }
    fn handle_notes_save(&self, p: &serde_json::Value) -> Result<serde_json::Value, String> {
        let user_id = p.get("user_id").and_then(|v| v.as_str()).unwrap_or("");
        let address = p.get("address").and_then(|v| v.as_str()).unwrap_or("");
        let note = p.get("note").and_then(|v| v.as_str()).unwrap_or("");
        let tags: Vec<String> = p.get("tags").and_then(|v| v.as_array()).map(|arr| arr.iter().filter_map(|v| v.as_str().map(|s| s.to_string())).collect()).unwrap_or_default();
        let note = self.notes.save_note(user_id, address, note, tags);
        Ok(serde_json::json!({"note": note}))
    }
    fn handle_alerts_create(&self, p: &serde_json::Value) -> Result<serde_json::Value, String> {
        let user_id = p.get("user_id").and_then(|v| v.as_str()).unwrap_or("");
        let alert_type = p.get("alert_type").and_then(|v| v.as_str()).unwrap_or("price_above");
        let address = p.get("address").and_then(|v| v.as_str()).map(|s| s.to_string());
        let threshold = p.get("threshold").and_then(|v| v.as_str()).map(|s| s.to_string());
        let alert = self.alerts.create(user_id, user_service::AlertType::PriceAbove, user_service::AlertCondition { address, threshold, percentage: None }, vec![]);
        Ok(serde_json::json!({"alert": alert}))
    }
    fn handle_dashboard_create(&self, p: &serde_json::Value) -> Result<serde_json::Value, String> {
        let user_id = p.get("user_id").and_then(|v| v.as_str()).unwrap_or("");
        let name = p.get("name").and_then(|v| v.as_str()).unwrap_or("My Dashboard");
        let dashboard = self.dashboard.create(user_id, name, user_service::DashboardLayout { columns: 12, rows: 12 });
        Ok(serde_json::json!({"dashboard": dashboard}))
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    info!("Starting User Service");
    let handler = Arc::new(Handler::new());
    let server = tiny_http::Server::http(SocketAddr::from(([0, 0, 0, 0], 9001))?;
    info!("Server listening on port 9001");
    for request in server.incoming_requests() {
        let handler = handler.clone();
        tokio::spawn(async move {
            let mut resp = request.respond(handler.handle(&request.as_vec())).unwrap();
            for h in [Header::from_bytes(&b"Content-Type"[..], &b"application/json"[..]).unwrap(), Header::from_bytes(&b"X-Frame-Options"[..], &b"DENY"[..]).unwrap()] { resp.add_header(h); }
        });
    }
    Ok(())
}