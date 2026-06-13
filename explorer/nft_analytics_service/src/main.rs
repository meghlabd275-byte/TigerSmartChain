//! NFT Analytics Service Main

#![forbid(unsafe_code)]

use std::net::SocketAddr;
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use tiny_http::{Header, Response, StatusCode};
use tracing::{info, Level};
use tracing_subscriber::FmtSubscriber;

use nft_analytics_service::{Config, NFTAnalyticsService};

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
    service: NFTAnalyticsService,
}

impl Handler {
    pub fn new(service: NFTAnalyticsService) -> Self {
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
            "nft_getCollectionAnalytics" => self.handle_collection_analytics(request.params),
            "nft_getRarity" => self.handle_rarity(request.params),
            "nft_getTopRarity" => self.handle_top_rarity(request.params),
            "nft_getFloorHistory" => self.handle_floor_history(request.params),
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

    fn handle_collection_analytics(&self, params: Option<serde_json::Value>) -> Result<serde_json::Value, String> {
        let address = params
            .and_then(|p| p.get("collection").and_then(|c| c.as_str()))
            .ok_or("Missing collection")?;

        let analytics = self.service.get_collection_analytics(address)
            .ok_or("Collection not found")?;

        Ok(serde_json::to_value(analytics).unwrap_or_default())
    }

    fn handle_rarity(&self, params: Option<serde_json::Value>) -> Result<serde_json::Value, String> {
        let collection = params
            .and_then(|p| p.get("collection").and_then(|c| c.as_str()))
            .ok_or("Missing collection")?;
        let token_id = params
            .and_then(|p| p.get("token_id").and_then(|t| t.as_str()))
            .ok_or("Missing token_id")?;

        let rarity = self.service.calculate_rarity(collection, token_id)
            .ok_or("NFT not found")?;

        Ok(serde_json::to_value(rarity).unwrap_or_default())
    }

    fn handle_top_rarity(&self, params: Option<serde_json::Value>) -> Result<serde_json::Value, String> {
        let collection = params
            .and_then(|p| p.get("collection").and_then(|c| c.as_str()))
            .ok_or("Missing collection")?;
        let limit = params
            .and_then(|p| p.get("limit").and_then(|l| l.as_u64()))
            .unwrap_or(10) as usize;

        let nfts = self.service.get_top_rarity(collection, limit);

        Ok(serde_json::json!({ "nfts": nfts }))
    }

    fn handle_floor_history(&self, params: Option<serde_json::Value>) -> Result<serde_json::Value, String> {
        let address = params
            .and_then(|p| p.get("collection").and_then(|c| c.as_str()))
            .ok_or("Missing collection")?;

        let history = self.service.get_floor_price_history(address)
            .ok_or("Collection not found")?;

        Ok(serde_json::to_value(history).unwrap_or_default())
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

    info!("Starting NFT Analytics Service");

    let config = Config::default();
    let service = NFTAnalyticsService::new(config).await?;
    let handler = Arc::new(Handler::new(service));

    let addr = SocketAddr::from(([0, 0, 0, 0], 8549));
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