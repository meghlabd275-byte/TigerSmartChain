//! Governance Service Main

#![forbid(unsafe_code)]

use std::net::SocketAddr;
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use tiny_http::{Header, Response, StatusCode};
use tracing::{info, Level};
use tracing_subscriber::FmtSubscriber;

use governance_service::{Config, GovernanceService, ProposalStatus};

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
    service: GovernanceService,
}

impl Handler {
    pub fn new(service: GovernanceService) -> Self {
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
            "governance_getProposals" => self.handle_get_proposals(request.params),
            "governance_getProposal" => self.handle_get_proposal(request.params),
            "governance_getVotes" => self.handle_get_votes(request.params),
            "governance_castVote" => self.handle_cast_vote(request.params).await,
            "governance_getStats" => self.handle_get_stats(),
            "governance_getDelegate" => self.handle_get_delegate(request.params),
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

    fn handle_get_proposals(&self, params: Option<serde_json::Value>) -> Result<serde_json::Value, String> {
        let status = params
            .and_then(|p| p.get("status").and_then(|s| s.as_str()))
            .and_then(|s| match s {
                "active" => Some(ProposalStatus::Active),
                "pending" => Some(ProposalStatus::Pending),
                "executed" => Some(ProposalStatus::Executed),
                "defeated" => Some(ProposalStatus::Defeated),
                _ => None,
            });
        
        let proposals = self.service.get_proposals(status);
        
        Ok(serde_json::json!({ "proposals": proposals }))
    }

    fn handle_get_proposal(&self, params: Option<serde_json::Value>) -> Result<serde_json::Value, String> {
        let id = params
            .and_then(|p| p.get("proposal_id").and_then(|id| id.as_u64()))
            .ok_or("Missing proposal_id")?;
        
        let proposal = self.service.get_proposal(id)
            .ok_or("Proposal not found")?;
        
        Ok(serde_json::to_value(proposal).unwrap_or_default())
    }

    fn handle_get_votes(&self, params: Option<serde_json::Value>) -> Result<serde_json::Value, String> {
        let id = params
            .and_then(|p| p.get("proposal_id").and_then(|id| id.as_u64()))
            .ok_or("Missing proposal_id")?;
        
        let votes = self.service.get_proposal_votes(id)
            .ok_or("Proposal not found")?;
        
        Ok(serde_json::to_value(votes).unwrap_or_default())
    }

    async fn handle_cast_vote(&self, params: Option<serde_json::Value>) -> Result<serde_json::Value, String> {
        let proposal_id = params
            .and_then(|p| p.get("proposal_id").and_then(|id| id.as_u64()))
            .ok_or("Missing proposal_id")?;
        
        let voter = params
            .and_then(|p| p.get("voter").and_then(|v| v.as_str()))
            .ok_or("Missing voter")?;
        
        let support = params
            .and_then(|p| p.get("support").and_then(|s| s.as_str()))
            .ok_or("Missing support")?;
        
        let votes = params
            .and_then(|p| p.get("votes").and_then(|v| v.as_str()))
            .unwrap_or("0");
        
        let reason = params
            .and_then(|p| p.get("reason").and_then(|r| r.as_str()))
            .map(|s| s.to_string());
        
        let support = match support {
            "for" => governance_service::VoteChoice::For,
            "against" => governance_service::VoteChoice::Against,
            _ => governance_service::VoteChoice::Abstain,
        };
        
        self.service.cast_vote(proposal_id, voter, support, votes, reason)
            .map_err(|e| e.to_string())?;
        
        Ok(serde_json::json!({ "success": true }))
    }

    fn handle_get_stats(&self) -> Result<serde_json::Value, String> {
        let stats = self.service.get_stats();
        
        Ok(serde_json::to_value(stats).unwrap_or_default())
    }

    fn handle_get_delegate(&self, params: Option<serde_json::Value>) -> Result<serde_json::Value, String> {
        let address = params
            .and_then(|p| p.get("address").and_then(|a| a.as_str()))
            .ok_or("Missing address")?;
        
        let delegate = self.service.get_delegate(address)
            .ok_or("Delegate not found")?;
        
        Ok(serde_json::to_value(delegate).unwrap_or_default())
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

    info!("Starting Governance Service");

    let config = Config::default();
    let service = GovernanceService::new(config).await?;
    let handler = Arc::new(Handler::new(service));

    let addr = SocketAddr::from(([0, 0, 0, 0], 8550));
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