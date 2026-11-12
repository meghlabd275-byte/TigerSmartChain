//! TigerScan token migrator and airdrop finder service.
//!
//! Answers JSON-RPC over HTTP. Every answer is derived from ERC-20 `Transfer`
//! logs read from the node; a request that cannot be served is reported as an
//! error rather than answered with placeholder data.

#![forbid(unsafe_code)]

use std::net::SocketAddr;
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use tokio::runtime::Runtime;
use tracing::{info, Level};
use tracing_subscriber::FmtSubscriber;

use token_migrator_service::{
    check_range, AirdropDetector, ChainClient, Config, MigrationService,
};

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

pub struct Handler {
    chain: ChainClient,
    service: MigrationService,
}

impl Handler {
    pub fn new(chain: ChainClient, service: MigrationService) -> Self {
        Self { chain, service }
    }

    /// Read `from_block`/`to_block` from the request, defaulting to the
    /// configured window, and reject a range that is too wide.
    fn block_range(
        &self,
        params: &serde_json::Value,
        latest: u64,
    ) -> Result<(u64, u64), String> {
        let from = params
            .get("from_block")
            .and_then(|value| value.as_u64())
            .unwrap_or_else(|| self.service.from_block());

        let to = params
            .get("to_block")
            .and_then(|value| value.as_u64())
            .unwrap_or_else(|| {
                latest.min(self.service.from_block() + self.service.max_block_range())
            });

        check_range(from, to, self.service.max_block_range()).map_err(|error| error.to_string())?;

        Ok((from, to))
    }

    /// Decode a token address from the request.
    fn token(params: &serde_json::Value) -> Result<String, String> {
        let token = params
            .get("token")
            .and_then(|value| value.as_str())
            .ok_or_else(|| "missing token".to_string())?;

        let trimmed = token.trim();

        let hex_body = trimmed
            .strip_prefix("0x")
            .ok_or_else(|| "token must be a 0x-prefixed address".to_string())?;

        if hex_body.len() != 40 || !hex_body.chars().all(|c| c.is_ascii_hexdigit()) {
            return Err("token must be a 20-byte address".to_string());
        }

        Ok(trimmed.to_ascii_lowercase())
    }

    fn address(params: &serde_json::Value) -> Result<ethers::types::Address, String> {
        Self::token(params)?
            .parse()
            .map_err(|error| format!("cannot parse token address: {error}"))
    }

    /// Handle one JSON-RPC request.
    pub async fn handle(&self, body: &[u8]) -> ApiResponse {
        let request: ApiRequest = match serde_json::from_slice(body) {
            Ok(request) => request,
            Err(error) => {
                return ApiResponse {
                    jsonrpc: "2.0".to_string(),
                    result: None,
                    error: Some(ApiError {
                        code: -32700,
                        message: format!("parse error: {error}"),
                    }),
                    id: 0,
                }
            }
        };

        let params = request.params.clone().unwrap_or(serde_json::Value::Null);

        let result = match request.method.as_str() {
            "migrator_getMigrations" => self.get_migrations(&params).await,
            "migrator_getAirdrop" => self.get_airdrop(&params).await,
            "migrator_getBurns" => self.get_burns(&params).await,
            "migrator_blockNumber" => self
                .chain
                .block_number()
                .await
                .map(|number| serde_json::json!({ "block_number": number }))
                .map_err(|error| error.to_string()),
            other => Err(format!("unknown method: {other}")),
        };

        match result {
            Ok(value) => ApiResponse {
                jsonrpc: "2.0".to_string(),
                result: Some(value),
                error: None,
                id: request.id,
            },
            Err(message) => ApiResponse {
                jsonrpc: "2.0".to_string(),
                result: None,
                error: Some(ApiError {
                    code: -32000,
                    message,
                }),
                id: request.id,
            },
        }
    }

    async fn get_migrations(&self, params: &serde_json::Value) -> Result<serde_json::Value, String> {
        let token = Self::token(params)?;
        let address = Self::address(params)?;

        let latest = self
            .chain
            .block_number()
            .await
            .map_err(|error| error.to_string())?;

        let (from, to) = self.block_range(params, latest)?;

        let transfers = self
            .chain
            .transfers(address, from, to)
            .await
            .map_err(|error| error.to_string())?;

        let migrations = self.service.migrations(&token, &transfers);

        serde_json::to_value(migrations).map_err(|error| error.to_string())
    }

    async fn get_burns(&self, params: &serde_json::Value) -> Result<serde_json::Value, String> {
        let token = Self::token(params)?;
        let address = Self::address(params)?;

        let latest = self
            .chain
            .block_number()
            .await
            .map_err(|error| error.to_string())?;

        let (from, to) = self.block_range(params, latest)?;

        let transfers = self
            .chain
            .transfers(address, from, to)
            .await
            .map_err(|error| error.to_string())?;

        let burns = self.service.burns(&token, &transfers);

        serde_json::to_value(burns).map_err(|error| error.to_string())
    }

    async fn get_airdrop(&self, params: &serde_json::Value) -> Result<serde_json::Value, String> {
        let token = Self::token(params)?;
        let address = Self::address(params)?;

        let min_recipients = params
            .get("min_recipients")
            .and_then(|value| value.as_u64())
            .unwrap_or(10) as usize;

        let latest = self
            .chain
            .block_number()
            .await
            .map_err(|error| error.to_string())?;

        let (from, to) = self.block_range(params, latest)?;

        let transfers = self
            .chain
            .transfers(address, from, to)
            .await
            .map_err(|error| error.to_string())?;

        let airdrops = self.service.airdrops(&token, &transfers, min_recipients);

        // The detector is the single-recipient-set path; use it when the caller
        // asks for the mints of one token without the airdrop grouping.
        if params
            .get("single")
            .and_then(|value| value.as_bool())
            .unwrap_or(false)
        {
            let detected = AirdropDetector::detect(&token, &transfers);
            return serde_json::to_value(detected).map_err(|error| error.to_string());
        }

        serde_json::to_value(airdrops).map_err(|error| error.to_string())
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    FmtSubscriber::builder()
        .with_max_level(Level::INFO)
        .with_target(false)
        .init();

    let config = Config::default();

    info!(rpc = %config.rpc_url, "starting token migrator service");

    let chain = ChainClient::new(&config.rpc_url, config.log_chunk_size)
        .map_err(|error| format!("cannot build chain client: {error}"))?;

    let service = MigrationService::new(config.from_block, config.max_block_range);

    let handler = Arc::new(Handler::new(chain, service));

    let addr = SocketAddr::from(([0, 0, 0, 0], 8555));
    info!("Server listening on {}", addr);

    // The serving loop is synchronous; the node calls are not. Drive them on an
    // explicit runtime rather than blocking a shared async worker.
    let runtime = Runtime::new()?;

    tigerscan_service_http::serve(addr, move |body| {
        let response = runtime.block_on(handler.handle(body));
        tigerscan_service_http::json_response(&response)
    })
    .map_err(|error| error.into())
}
