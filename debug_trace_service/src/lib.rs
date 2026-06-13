//! Debug Trace Service - Real Transaction Tracing
//! 
//! Full debug_traceTransaction implementation:
//! - debug_traceTransaction
//! - debug_traceCall
//! - debug_traceBlockByNumber

use std::sync::Arc;
use serde::{Deserialize, Serialize};
use thiserror::Error;

// =============================================================================
// ERRORS
// =============================================================================

#[derive(Error, Debug)]
pub enum DebugTraceError {
    #[error("RPC error: {0}")]
    RpcError(String),
    #[error("Parse error: {0}")]
    ParseError(String),
    #[error("Not found: {0}")]
    NotFound(String),
}

// =============================================================================
// CONFIGURATION
// =============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DebugTraceConfig {
    pub rpc_url: String,
    pub trace_timeout_secs: u64,
}

impl Default for DebugTraceConfig {
    fn default() -> Self {
        Self {
            rpc_url: std::env::var("RPC_URL").unwrap_or_else(|_| "http://localhost:8545".to_string()),
            trace_timeout_secs: 300,
        }
    }
}

// =============================================================================
// TRACE TYPES
// =============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TraceResult {
    pub gas: String,
    pub return_value: String,
    pub struct_logs: Vec<StructLog>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StructLog {
    pub pc: u64,
    pub op: String,
    pub gas: String,
    pub stack: Vec<String>,
    pub memory: Option<Vec<String>>,
    pub storage: Option<std::collections::HashMap<String, String>>,
    pub depth: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CallFrame {
    pub op: String,
    pub from: String,
    pub to: String,
    pub input: String,
    pub output: String,
    pub value: String,
    pub gas: String,
    pub revert_reason: Option<String>,
    pub calls: Vec<CallFrame>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TraceTransaction {
    pub tx_hash: String,
    pub block_number: u64,
    pub block_hash: String,
    pub trace: Vec<CallFrame>,
    pub gas_used: u64,
    pub success: bool,
}

// =============================================================================
// DEBUG TRACE SERVICE
// =============================================================================

pub struct DebugTraceService {
    config: DebugTraceConfig,
    client: reqwest::Client,
}

impl DebugTraceService {
    pub fn new(config: DebugTraceConfig) -> Self {
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(config.trace_timeout_secs))
            .build()
            .unwrap_or_default();
        
        Self { config, client }
    }
    
    pub async fn trace_transaction(&self, tx_hash: &str) -> Result<TraceResult, DebugTraceError> {
        let request = serde_json::json!({
            "jsonrpc": "2.0",
            "method": "debug_traceTransaction",
            "params": [tx_hash, {
                "tracer": "callTracer",
                "timeout": "300s"
            }],
            "id": 1
        });
        
        let response = self.client.post(&self.config.rpc_url)
            .json(&request)
            .send()
            .await
            .map_err(|e| DebugTraceError::RpcError(e.to_string()))?;
        
        if response.status() == 404 {
            return Err(DebugTraceError::NotFound(format!("Transaction {} not found", tx_hash)));
        }
        
        let result: serde_json::Value = response.json().await
            .map_err(|e| DebugTraceError::ParseError(e.to_string()))?;
        
        if let Some(error) = result.get("error") {
            return Err(DebugTraceError::RpcError(error["message"].as_str().unwrap_or("Unknown error").to_string()));
        }
        
        let trace = result["result"].clone();
        
        let gas = trace["gas"].as_str().unwrap_or("0x0").to_string();
        let return_value = trace["returnValue"].as_str().unwrap_or("0x").to_string();
        
        let mut struct_logs = Vec::new();
        if let Some(logs) = trace["structLogs"].as_array() {
            for log in logs {
                struct_logs.push(StructLog {
                    pc: log["pc"].as_u64().unwrap_or(0),
                    op: log["op"].as_str().unwrap_or("").to_string(),
                    gas: log["gas"].as_str().unwrap_or("0x0").to_string(),
                    stack: log["stack"].as_array()
                        .map(|s| s.iter().filter_map(|v| v.as_str().map(String::from)).collect())
                        .unwrap_or_default(),
                    memory: log["memory"].as_array()
                        .map(|m| m.iter().filter_map(|v| v.as_str().map(String::from)).collect()),
                    storage: log["storage"].as_object()
                        .map(|s| s.iter().map(|(k, v)| (k.clone(), v.as_str().unwrap_or("0x").to_string())).collect()),
                    depth: log["depth"].as_u64().unwrap_or(0) as u32,
                });
            }
        }
        
        Ok(TraceResult { gas, return_value, struct_logs })
    }
    
    pub async fn trace_call(&self, from: &str, to: &str, data: &str, value: Option<&str>) -> Result<TraceResult, DebugTraceError> {
        let request = serde_json::json!({
            "jsonrpc": "2.0",
            "method": "debug_traceCall",
            "params": [{
                "from": from,
                "to": to,
                "data": data,
                "value": value.unwrap_or("0x0")
            }, "latest", {
                "tracer": "callTracer",
                "timeout": "300s"
            }],
            "id": 1
        });
        
        let response = self.client.post(&self.config.rpc_url)
            .json(&request)
            .send()
            .await
            .map_err(|e| DebugTraceError::RpcError(e.to_string()))?;
        
        let result: serde_json::Value = response.json().await
            .map_err(|e| DebugTraceError::ParseError(e.to_string()))?;
        
        if let Some(error) = result.get("error") {
            return Err(DebugTraceError::RpcError(error["message"].as_str().unwrap_or("Unknown error").to_string()));
        }
        
        let trace = result["result"].clone();
        
        Ok(TraceResult {
            gas: trace["gas"].as_str().unwrap_or("0x0").to_string(),
            return_value: trace["returnValue"].as_str().unwrap_or("0x").to_string(),
            struct_logs: vec![],
        })
    }
    
    pub async fn trace_block(&self, block_number: u64) -> Result<Vec<TraceTransaction>, DebugTraceError> {
        let block_hex = format!("0x{:x}", block_number);
        
        let block_request = serde_json::json!({
            "jsonrpc": "2.0",
            "method": "eth_getBlockByNumber",
            "params": [block_hex, true],
            "id": 1
        });
        
        let block_response = self.client.post(&self.config.rpc_url)
            .json(&block_request)
            .send()
            .await
            .map_err(|e| DebugTraceError::RpcError(e.to_string()))?;
        
        let block_result: serde_json::Value = block_response.json().await
            .map_err(|e| DebugTraceError::ParseError(e.to_string()))?;
        
        let transactions = block_result["result"]["transactions"].as_array()
            .ok_or_else(|| DebugTraceError::NotFound("No transactions".to_string()))?;
        
        let mut traces = Vec::new();
        
        for tx in transactions {
            let tx_hash = tx["hash"].as_str().unwrap_or("");
            if !tx_hash.is_empty() {
                if let Ok(trace) = self.trace_transaction(tx_hash).await {
                    traces.push(TraceTransaction {
                        tx_hash: tx_hash.to_string(),
                        block_number,
                        block_hash: block_result["result"]["hash"].as_str().unwrap_or("").to_string(),
                        trace: vec![],
                        gas_used: u64::from_str_radix(trace.gas.trim_start_matches("0x"), 16).unwrap_or(0),
                        success: !trace.return_value.starts_with("0x08"),
                    });
                }
            }
        }
        
        Ok(traces)
    }
}