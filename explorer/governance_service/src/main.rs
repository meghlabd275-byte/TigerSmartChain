//! Governance Service Main

#![forbid(unsafe_code)]

use std::net::SocketAddr;
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use tiny_http::{Response, StatusCode};
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

    pub async fn handle(&self, body: &[u8]) -> Response<std::io::Cursor<Vec<u8>>> {
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
            "governance_getProposals" => self.handle_get_proposals(request.params).await,
            "governance_getProposal" => self.handle_get_proposal(request.params).await,
            "governance_getVotes" => self.handle_get_votes(request.params).await,
            "governance_getStats" => self.handle_get_stats().await,
            "governance_getDelegate" => self.handle_get_delegate(request.params).await,
            "governance_getParams" => self.handle_get_params().await,
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

    async fn handle_get_proposals(&self, params: Option<serde_json::Value>) -> Result<serde_json::Value, String> {
        let status = params
            .as_ref()
            .and_then(|p| p.get("status").and_then(|s| s.as_str()))
            .and_then(|s| match s {
                "active" => Some(ProposalStatus::Active),
                "pending" => Some(ProposalStatus::Pending),
                "executed" => Some(ProposalStatus::Executed),
                "defeated" => Some(ProposalStatus::Defeated),
                _ => None,
            });

        let proposals = self.service.get_proposals(status).await;

        Ok(serde_json::json!({ "proposals": proposals }))
    }

    async fn handle_get_proposal(
        &self,
        params: Option<serde_json::Value>,
    ) -> Result<serde_json::Value, String> {
        let id = params
            .and_then(|p| p.get("proposal_id").and_then(|id| id.as_u64()))
            .ok_or("Missing proposal_id")?;

        let proposal = self
            .service
            .get_proposal(id)
            .await
            .map_err(|error| error.to_string())?;

        Ok(serde_json::to_value(proposal).unwrap_or_default())
    }

    async fn handle_get_votes(
        &self,
        params: Option<serde_json::Value>,
    ) -> Result<serde_json::Value, String> {
        let id = params
            .and_then(|p| p.get("proposal_id").and_then(|id| id.as_u64()))
            .ok_or("Missing proposal_id")?;

        let votes = self
            .service
            .get_proposal_votes(id)
            .await
            .map_err(|error| error.to_string())?;

        Ok(serde_json::to_value(votes).unwrap_or_default())
    }

    /// The governor's own parameters, read from the contract.
    async fn handle_get_params(&self) -> Result<serde_json::Value, String> {
        let voting_delay = self.service.voting_delay().await.map_err(|e| e.to_string())?;
        let voting_period = self.service.voting_period().await.map_err(|e| e.to_string())?;
        let proposal_threshold = self
            .service
            .proposal_threshold()
            .await
            .map_err(|e| e.to_string())?;

        Ok(serde_json::json!({
            "votingDelay": voting_delay.to_string(),
            "votingPeriod": voting_period.to_string(),
            "proposalThreshold": proposal_threshold.to_string(),
            "governanceAddress": self.service.config().governance_address,
            "tokenAddress": self.service.config().token_address,
        }))
    }

    async fn handle_get_stats(&self) -> Result<serde_json::Value, String> {
        let stats = self.service.get_stats().await;

        Ok(serde_json::to_value(stats).unwrap_or_default())
    }

    async fn handle_get_delegate(
        &self,
        params: Option<serde_json::Value>,
    ) -> Result<serde_json::Value, String> {
        let address = params
            .as_ref()
            .and_then(|p| p.get("address").and_then(|a| a.as_str()))
            .ok_or("Missing address")?;

        let delegate = self
            .service
            .get_delegate(address)
            .await
            .map_err(|error| error.to_string())?;

        Ok(serde_json::to_value(delegate).unwrap_or_default())
    }
}

// ============================================================================
// Main
// ============================================================================

fn main() -> Result<(), Box<dyn std::error::Error>> {
    FmtSubscriber::builder()
        .with_max_level(Level::INFO)
        .with_target(false)
        .init();

    info!("Starting Governance Service");

    let runtime = tokio::runtime::Runtime::new()?;
    let config = Config::default();
    let service = runtime.block_on(GovernanceService::new(config))?;
    let handler = Arc::new(Handler::new(service));

    let addr = SocketAddr::from(([0, 0, 0, 0], 8560));
    info!("Server listening on {}", addr);

    // The serving loop is synchronous; the governor reads are not.
    tigerscan_service_http::serve(addr, move |body| runtime.block_on(handler.handle(body)))?;

    Ok(())
}