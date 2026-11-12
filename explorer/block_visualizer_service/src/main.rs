#![forbid(unsafe_code)]
use std::net::SocketAddr;
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use tiny_http::{Response, StatusCode};
use tokio::runtime::Runtime;
use tracing::{info, Level};
use tracing_subscriber::FmtSubscriber;

use block_visualizer_service::{Config, Exporter, VisualizerService};

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
    service: Arc<VisualizerService>,
    config: Config,
}

impl Handler {
    pub fn new(service: Arc<VisualizerService>, config: Config) -> Self {
        Self { service, config }
    }

    pub async fn handle(&self, body: &[u8]) -> Response<std::io::Cursor<Vec<u8>>> {
        let request: ApiRequest = match serde_json::from_slice(body) {
            Ok(request) => request,
            Err(error) => {
                return Self::reply(
                    0,
                    Err(ApiError {
                        code: -32700,
                        message: format!("parse error: {error}"),
                    }),
                    StatusCode::from(400),
                )
            }
        };

        let id = request.id;

        let result = match request.method.as_str() {
            "visualizer_getBlock" => self.get_block(request.params).await,
            "visualizer_getTxFlow" => self.get_tx_flow(request.params).await,
            "visualizer_latestBlock" => self
                .service
                .latest_block_number()
                .await
                .map(|number| serde_json::json!({ "blockNumber": number }))
                .map_err(|message| ApiError {
                    code: -32000,
                    message,
                }),
            other => Err(ApiError {
                code: -32601,
                message: format!("unknown method: {other}"),
            }),
        };

        Self::reply(id, result, StatusCode::from(200))
    }

    async fn get_block(&self, params: Option<serde_json::Value>) -> Result<serde_json::Value, ApiError> {
        let block_number = params
            .as_ref()
            .and_then(|params| params.get("block"))
            .and_then(|block| block.as_u64())
            .or_else(|| {
                params
                    .as_ref()
                    .and_then(|params| params.get("block"))
                    .and_then(|block| block.as_str())
                    .and_then(|block| block.strip_prefix("0x"))
                    .and_then(|block| u64::from_str_radix(block, 16).ok())
            })
            .ok_or(ApiError {
                code: -32602,
                message: "missing block number".to_string(),
            })?;

        let block = self
            .service
            .visualize_block(block_number)
            .await
            .map_err(|message| ApiError {
                code: -32000,
                message,
            })?;

        Ok(serde_json::json!({
            "block": block,
            "mermaid": Exporter::to_mermaid(&block),
        }))
    }

    async fn get_tx_flow(
        &self,
        params: Option<serde_json::Value>,
    ) -> Result<serde_json::Value, ApiError> {
        let tx_hash = params
            .as_ref()
            .and_then(|params| params.get("tx"))
            .and_then(|tx| tx.as_str())
            .ok_or(ApiError {
                code: -32602,
                message: "missing transaction hash".to_string(),
            })?;

        let flow = self
            .service
            .visualize_transaction(&self.config, tx_hash)
            .await
            .map_err(|message| ApiError {
                code: -32000,
                message,
            })?;

        Ok(serde_json::json!({
            "flow": flow,
            "dot": Exporter::to_dot(&flow),
        }))
    }

    fn reply(
        id: u64,
        result: Result<serde_json::Value, ApiError>,
        status: StatusCode,
    ) -> Response<std::io::Cursor<Vec<u8>>> {
        let response = match result {
            Ok(result) => ApiResponse {
                jsonrpc: "2.0".to_string(),
                result: Some(result),
                error: None,
                id,
            },
            Err(error) => ApiResponse {
                jsonrpc: "2.0".to_string(),
                result: None,
                error: Some(error),
                id,
            },
        };

        Response::from_string(serde_json::to_string(&response).unwrap_or_default())
            .with_status_code(status)
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    FmtSubscriber::builder()
        .with_max_level(Level::INFO)
        .init();

    let config = Config::default();

    info!(rpc_url = %config.rpc_url, "Starting Block Visualizer Service");

    let service = Arc::new(VisualizerService::new(config.clone()));
    let handler = Arc::new(Handler::new(service, config));

    let addr = SocketAddr::from(([0, 0, 0, 0], 8541));
    info!("Server listening on {}", addr);

    // The serving loop is synchronous; the RPC reads are not.
    let runtime = Runtime::new()?;

    tigerscan_service_http::serve(addr, move |body| runtime.block_on(handler.handle(body)))?;

    Ok(())
}
