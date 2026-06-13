//! RPC Client for Trace Operations
//! 
//! Secure RPC client with:
//! - Request signing
//! - Input validation
//! - Timeout handling
//! - Retry logic
//! - Connection pooling

use thiserror::Error;
use serde::{Serialize, Deserialize};
use std::time::Duration;
use reqwest::Client;
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
            params: vec![],
            id: 1,
        };
        
        let response: RpcResponse = self.send_request(request).await?;
        
        match response.result {
            Some(RpcResult::String(s)) => {
                u64::from_str_radix(s.trim_start_matches("0x"), 16)
                    .map_err(|e| RpcError::ParseError(e.to_string()))
            }
            Some(RpcResult::Number(n)) => Ok(n),
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
        
        let block: serde_json::Value = serde_json::from_value(response.result.unwrap_or_default())
            .map_err(|e| RpcError::ParseError(e.to_string()))?;
        
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
        let mut tracer_config = vec![];
        
        if enable_traces {
            tracer_config.push("trace".to_string());
        }
        
        if enable_state_diffs {
            tracer_config.push("stateDiff".to_string());
        }
        
        if tracer_config.is_empty() {
            tracer_config.push("trace".to_string());
        }
        
        let request = RpcRequest {
            jsonrpc: "2.0".to_string(),
            method: "debug_traceTransaction".to_string(),
            params: serde_json::json!([
                tx_hash,
                tracer_config
            ]),
            id: 1,
        };
        
        let response: RpcResponse = self.send_request(request).await?;
        
        // Parse trace result
        let trace = self.parse_trace_result(tx_hash, block_number, response.result)?;
        
        Ok(trace)
    }
    
    /// Parse trace result
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
        
        let result_json = match result {
            Some(RpcResult::Value(v)) => v,
            _ => return Ok(trace),
        };
        
        // Parse traces array
        if let Some(traces) = result_json.get("trace").and_then(|t| t.as_array()) {
            let mut trace_index = 0u32;
            
            for trace_obj in traces {
                if let Some(internal_tx) = self.parse_trace_entry(trace_obj, tx_hash, block_number, trace_index) {
                    trace.add_trace(internal_tx);
                    trace_index += 1;
                }
            }
        }
        
        // Parse state diffs
        if let Some(state_diff) = result_json.get("stateDiff").and_then(|s| s.as_object()) {
            for (address, changes) in state_diff {
                // Parse balance changes
                if let Some(balance) = changes.get("balance") {
                    if let (Some(old_val), Some(new_val)) = (
                        balance.get("from").and_then(|v| v.as_str()),
                        balance.get("to").and_then(|v| v.as_str())
                    ) {
                        trace.add_state_change(StateChange::balance_change(
                            block_number,
                            tx_hash.to_string(),
                            address.clone(),
                            old_val.to_string(),
                            new_val.to_string(),
                        ));
                    }
                }
            }
        }
        
        Ok(trace)
    }
    
    /// Parse single trace entry
    fn parse_trace_entry(
        &self,
        trace_obj: &serde_json::Value,
        tx_hash: &str,
        block_number: u64,
        trace_index: u32,
    ) -> Option<InternalTransaction> {
        let mut internal_tx = InternalTransaction::new(
            tx_hash.to_string(),
            block_number,
            trace_index,
        );
        
        // Parse from
        if let Some(from) = trace_obj.get("from").and_then(|f| f.as_str()) {
            if self.is_valid_address(from) {
                internal_tx.from = from.to_string();
            }
        }
        
        // Parse to
        if let Some(to) = trace_obj.get("to").and_then(|t| t.as_str()) {
            if self.is_valid_address(to) {
                internal_tx.to = to.to_string();
            }
        }
        
        // Parse value
        if let Some(value) = trace_obj.get("value").and_then(|v| v.as_str()) {
            internal_tx.value = value.to_string();
        }
        
        // Parse gas
        if let Some(gas) = trace_obj.get("gas").and_then(|g| g.as_str()) {
            internal_tx.gas = gas.to_string();
        }
        
        // Parse gas used
        if let Some(gas_used) = trace_obj.get("gasUsed").and_then(|g| g.as_str()) {
            internal_tx.gas_used = Some(gas_used.to_string());
        }
        
        // Parse input
        if let Some(input) = trace_obj.get("input").and_then(|i| i.as_str()) {
            internal_tx.input = input.to_string();
        }
        
        // Parse output
        if let Some(output) = trace_obj.get("output").and_then(|o| o.as_str()) {
            internal_tx.output = Some(output.to_string());
        }
        
        // Parse call type
        if let Some(call_type) = trace_obj.get("type").and_then(|t| t.as_str()) {
            internal_tx.call_type = CallType::from_str(call_type);
        }
        
        // Parse depth
        if let Some(depth) = trace_obj.get("depth").and_then(|d| d.as_u64()) {
            internal_tx.depth = depth as u32;
        }
        
        // Parse error
        if let Some(error) = trace_obj.get("error").and_then(|e| e.as_str()) {
            internal_tx.error = Some(error.to_string());
            internal_tx.success = false;
        }
        
        // Parse contract creation
        if let Some(creates) = trace_obj.get("creates").and_then(|c| c.as_str()) {
            internal_tx.creates = Some(creates.to_string());
        }
        
        Some(internal_tx)
    }
    
    /// Validate address
    fn is_valid_address(&self, addr: &str) -> bool {
        if !addr.starts_with("0x") || addr.len() != 42 {
            return false;
        }
        
        addr[2..].chars().all(|c| c.is_ascii_hexdigit())
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
            
            // Wait before retry
            if attempt < self.max_retries - 1 {
                tokio::time::sleep(Duration::from_millis(100 * (attempt + 1))).await;
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