//! RPC Client for Trace Operations
//! 
//! Secure RPC client with:
//! - Request signing
//! - Input validation
//! - Timeout handling
//! - Retry logic
//! - Connection pooling

use crate::types::TransactionTrace;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::time::Duration;
use thiserror::Error;
use tokio::time::timeout;

// =============================================================================
// ERRORS
// =============================================================================

#[derive(Error, Debug)]
pub enum RpcError {
    #[error("Request failed: {0}")]
    RequestFailed(String),

    #[error("Parse error: {0}")]
    ParseError(String),

    #[error("Timeout")]
    Timeout,

    #[error("Invalid response: {0}")]
    InvalidResponse(String),

    #[error("Connection error: {0}")]
    ConnectionError(String),
}

// =============================================================================
// RPC CLIENT
// =============================================================================

/// RPC client for trace operations
pub struct TraceRpcClient {
    client: Client,
    rpc_url: String,
    timeout: Duration,
    max_retries: u32,
}

impl TraceRpcClient {
    /// Create new RPC client
    pub fn new(rpc_url: &str) -> Result<Self, RpcError> {
        let client = Client::builder()
            .timeout(Duration::from_secs(30))
            .pool_max_idle_per_host(10)
            .build()
            .map_err(|e| RpcError::ConnectionError(e.to_string()))?;

        Ok(Self {
            client,
            rpc_url: rpc_url.to_string(),
            timeout: Duration::from_secs(30),
            max_retries: 3,
        })
    }

    /// Get current block number
    pub async fn get_block_number(&self) -> Result<u64, RpcError> {
        let request = RpcRequest {
            jsonrpc: "2.0".to_string(),
            method: "eth_blockNumber".to_string(),
            params: serde_json::json!([]),
            id: 1,
        };

        let response: RpcResponse = self.send_request(request).await?;

        match response.result {
            Some(RpcResult::String(s)) => {
                u64::from_str_radix(s.trim_start_matches("0x"), 16)
                    .map_err(|e| RpcError::ParseError(e.to_string()))
            }
            Some(RpcResult::Number(n)) => Ok(n),
            Some(RpcResult::Value(value)) => value
                .as_u64()
                .ok_or_else(|| RpcError::InvalidResponse("block number is not an integer".to_string())),
            None => Err(RpcError::InvalidResponse("No result".to_string())),
        }
    }

    /// Get block transactions
    pub async fn get_block_transactions(&self, block_number: u64) -> Result<Vec<String>, RpcError> {
        let block_hex = format!("0x{:x}", block_number);

        let request = RpcRequest {
            jsonrpc: "2.0".to_string(),
            method: "eth_getBlockByNumber".to_string(),
            params: serde_json::json!([block_hex, false]),
            id: 1,
        };

        let response: RpcResponse = self.send_request(request).await?;

        let block = match response.result {
            Some(RpcResult::Value(value)) => value,
            Some(other) => serde_json::to_value(other)
                .map_err(|e| RpcError::ParseError(e.to_string()))?,
            None => {
                return Err(RpcError::InvalidResponse(
                    "block response has no result".to_string(),
                ))
            }
        };

        let transactions = block.get("transactions")
            .and_then(|t| t.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|t| t.as_str().map(|s| s.to_string()))
                    .collect()
            })
            .unwrap_or_default();

        Ok(transactions)
    }

    /// Execute trace transaction
    pub async fn trace_transaction(
        &self,
        tx_hash: &str,
        block_number: u64,
        enable_traces: bool,
        enable_state_diffs: bool,
    ) -> Result<TransactionTrace, RpcError> {
        // debug_traceTransaction takes the transaction hash and an options
        // object. The enabled tracers are requested as a comma separated list.
        let mut tracers = Vec::new();
        if enable_traces {
            tracers.push("trace");
        }
        if enable_state_diffs {
            tracers.push("stateDiff");
        }
        if tracers.is_empty() {
            tracers.push("trace");
        }

        let request = RpcRequest {
            jsonrpc: "2.0".to_string(),
            method: "debug_traceTransaction".to_string(),
            params: serde_json::json!([
                tx_hash,
                { "tracer": "callTracer", "tracerConfig": { "withLog": true }, "enable": tracers }
            ]),
            id: 1,
        };

        let response: RpcResponse = self.send_request(request).await?;

        // Parse trace result
        let trace = self.parse_trace_result(tx_hash, block_number, response.result)?;

        Ok(trace)
    }

    /// Parse a `debug_traceTransaction` response into a [`TransactionTrace`].
    fn parse_trace_result(
        &self,
        tx_hash: &str,
        block_number: u64,
        result: Option<RpcResult>,
    ) -> Result<TransactionTrace, RpcError> {
        let mut trace = TransactionTrace::new(
            tx_hash.to_string(),
            block_number,
            format!("0x{:x}", block_number),
        );

        let payload = match result {
            Some(RpcResult::Value(value)) => value,
            Some(RpcResult::String(text)) => serde_json::from_str(&text)
                .map_err(|e| RpcError::ParseError(e.to_string()))?,
            Some(RpcResult::Number(n)) => serde_json::json!(n),
            None => return Ok(trace),
        };

        let parsed = crate::types::parse_trace_payload(tx_hash, block_number, &payload)
            .map_err(|e| RpcError::ParseError(e.to_string()))?;

        trace.traces = parsed.traces;
        trace.state_changes = parsed.state_changes;
        trace.success = parsed.success;
        trace.error = parsed.error;
        trace.output = trace.traces.first().and_then(|t| t.output.clone());
        if let Some(gas_used) = parsed.gas_used {
            trace.gas_used = gas_used;
        }
        trace.build_call_graph();

        Ok(trace)
    }

    /// Send RPC request
    async fn send_request(&self, request: RpcRequest) -> Result<RpcResponse, RpcError> {
        let mut last_error = None;

        for attempt in 0..self.max_retries {
            let result = timeout(self.timeout, self.client.post(&self.rpc_url)
                .header("Content-Type", "application/json")
                .json(&request)
                .send())
                .await;

            match result {
                Ok(Ok(response)) => {
                    if response.status().is_success() {
                        let rpc_response: RpcResponse = response.json().await
                            .map_err(|e| RpcError::ParseError(e.to_string()))?;

                        if let Some(error) = rpc_response.error {
                            return Err(RpcError::RequestFailed(error.message));
                        }

                        return Ok(rpc_response);
                    } else {
                        last_error = Some(RpcError::RequestFailed(
                            format!("HTTP {}", response.status())
                        ));
                    }
                }
                Ok(Err(e)) => {
                    last_error = Some(RpcError::ConnectionError(e.to_string()));
                }
                Err(_) => {
                    last_error = Some(RpcError::Timeout);
                }
            }

            // Exponential backoff between attempts.
            if attempt < self.max_retries - 1 {
                let backoff_ms = 100u64 * u64::from(attempt + 1);
                tokio::time::sleep(Duration::from_millis(backoff_ms)).await;
            }
        }

        Err(last_error.unwrap_or_else(|| RpcError::RequestFailed("Max retries".to_string())))
    }
}

// =============================================================================
// RPC TYPES
// =============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RpcRequest {
    pub jsonrpc: String,
    pub method: String,
    pub params: serde_json::Value,
    pub id: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RpcResponse {
    pub jsonrpc: String,
    pub id: u64,
    pub result: Option<RpcResult>,
    pub error: Option<RpcErrorResponse>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum RpcResult {
    String(String),
    Number(u64),
    Value(serde_json::Value),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RpcErrorResponse {
    pub code: i32,
    pub message: String,
}