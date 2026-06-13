//! TigerScan Production Transaction Trace Debugger
//! State diff visualization, call stack, memory inspector, gas profiler
//! Uses Rust for maximum performance

use std::collections::HashMap;
use std::sync::Arc;

use anyhow::Result;
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use ethers::core::abi::{Abi, Function};
use ethers::core::k256::sha2::Sha256;
use ethers::core::types::{Address, H256, U256};
use ethers::providers::{Http, Provider, StreamExt, Ws};
use ethers::types::{Block, Transaction, TransactionReceipt, Eip1559TransactionRequest, Eip2930TransactionRequest, LegacyTransactionRequest};
use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use thiserror::Error;
use tokio::sync::mpsc;
use tracing::{error, info, warn};

// ============================================================================
// Error Types
// ============================================================================

#[derive(Error, Debug)]
pub enum TraceError {
    #[error("RPC error: {0}")]
    Rpc(String),
    
    #[error("Transaction not found: {0}")]
    NotFound(String),
    
    #[error("Trace error: {0}")]
    Trace(String),
    
    #[error("Invalid input: {0}")]
    InvalidInput(String),
    
    #[error("State error: {0}")]
    State(String),
    
    #[error("Gas estimation error: {0}")]
    GasEstimation(String),
}

// ============================================================================
// Configuration
// ============================================================================

#[derive(Debug, Clone, Deserialize)]
pub struct Config {
    /// RPC HTTP endpoint
    pub rpc_url: String,
    /// Archive RPC for historical state
    pub archive_url: Option<String>,
    /// WebSocket endpoint for real-time
    pub ws_url: Option<String>,
    /// Maximum traces to process
    pub max_traces: usize,
    /// Trace timeout
    pub trace_timeout: u64,
    /// Enable state diff
    pub enable_state_diff: bool,
    /// Enable gas profiling
    pub enable_gas_profiling: bool,
    /// Maximum call depth
    pub max_call_depth: usize,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            rpc_url: std::env::var("RPC_URL")
                .unwrap_or_else(|_| "http://localhost:8545".to_string()),
            archive_url: std::env::var("ARCHIVE_URL").ok(),
            ws_url: std::env::var("WS_URL").ok(),
            max_traces: 1000,
            trace_timeout: 60,
            enable_state_diff: true,
            enable_gas_profiling: true,
            max_call_depth: 16,
        }
    }
}

// ============================================================================
// Data Types
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TraceRequest {
    pub transaction_hash: String,
    pub block_number: Option<u64>,
    pub trace_types: Vec<TraceType>,
    pub enable_state_diff: bool,
    pub enable_gas_profiling: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TraceType {
    Call,
    Create,
    Suicide,
    DelegateCall,
    StaticCall,
    Precompiled,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TraceResult {
    pub transaction_hash: String,
    pub block_number: u64,
    pub block_hash: String,
    pub from: String,
    pub to: String,
    pub value: String,
    pub gas_used: u64,
    pub gas_limit: u64,
    pub status: bool,
    pub timestamp: u64,
    pub traces: Vec<CallTrace>,
    pub state_diff: Option<StateDiff>,
    pub gas_profiling: Option<GasProfiling>,
    pub logs: Vec<LogData>,
    pub internal_txs: Vec<InternalTransaction>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CallTrace {
    pub call_type: String,
    pub from: String,
    pub to: String,
    pub value: String,
    pub gas: u64,
    pub gas_used: u64,
    pub input: String,
    pub output: String,
    pub depth: u32,
    pub index: u32,
    pub parent_index: Option<u32>,
    pub revert: bool,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StateDiff {
    pub pre: HashMap<String, StorageSlot>,
    pub post: HashMap<String, StorageSlot>,
    pub changes: Vec<StorageChange>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StorageSlot {
    pub key: String,
    pub pre: String,
    pub post: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StorageChange {
    pub address: String,
    pub slot: String,
    pub key: String,
    pub pre_value: String,
    pub post_value: String,
    pub diff_type: DiffType,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DiffType {
    Modified,
    Added,
    Deleted,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GasProfiling {
    pub total_gas: u64,
    pub total_gas_used: u64,
    pub gas_per_call: Vec<GasCall>,
    pub gas_per_opcode: HashMap<String, u64>,
    pub optimization_suggestions: Vec<GasSuggestion>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GasCall {
    pub call_index: u32,
    pub call_type: String,
    pub gas_used: u64,
    pub gas_limit: u64,
    pub percentage: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GasSuggestion {
    pub call_index: u32,
    pub suggestion: String,
    pub estimated_savings: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LogData {
    pub address: String,
    pub topics: Vec<String>,
    pub data: String,
    pub log_index: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InternalTransaction {
    pub hash: String,
    pub block_number: u64,
    pub transaction_index: u32,
    pub from: String,
    pub to: String,
    pub value: String,
    pub gas: u64,
    pub gas_used: u64,
    pub input: String,
    pub output: String,
    pub call_type: String,
    pub depth: u32,
    pub revert: bool,
}

// ============================================================================
// Memory Inspector
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryInspector {
    pub data: Vec<u8>,
    pub size: usize,
}

impl MemoryInspector {
    pub fn new() -> Self {
        Self {
            data: Vec::new(),
            size: 0,
        }
    }

    pub fn load(&mut self, memory: &[u8]) {
        self.data = memory.to_vec();
        self.size = memory.len();
    }

    pub fn read(&self, offset: usize, length: usize) -> Option<String> {
        if offset + length > self.data.len() {
            return None;
        }
        
        let data = &self.data[offset..offset + length];
        Some(hex::encode(data))
    }

    pub fn read_string(&self, offset: usize, max_length: usize) -> Option<String> {
        if offset >= self.data.len() {
            return None;
        }
        
        let mut end = offset;
        while end < self.data.len() && end - offset < max_length {
            if self.data[end] == 0 {
                break;
            }
            end += 1;
        }
        
        String::from_utf8(self.data[offset..end].to_vec()).ok()
    }

    pub fn read_address(&self, offset: usize) -> Option<String> {
        if offset + 20 > self.data.len() {
            return None;
        }
        
        let addr = Address::from_slice(&self.data[offset..offset + 20]);
        Some(format!("{:?}", addr))
    }

    pub fn read_uint256(&self, offset: usize) -> Option<String> {
        if offset + 32 > self.data.len() {
            return None;
        }
        
        let mut bytes = [0u8; 32];
        bytes.copy_from_slice(&self.data[offset..offset + 32]);
        let value = U256::from_big_endian(&bytes);
        Some(format!("{}", value))
    }

    pub fn dump(&self, start: usize, length: usize) -> String {
        let end = (start + length).min(self.data.len());
        hex::encode(&self.data[start..end])
    }
}

// ============================================================================
// Stack Inspector
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StackInspector {
    pub stack: Vec<String>,
}

impl StackInspector {
    pub fn new() -> Self {
        Self {
            stack: Vec::new(),
        }
    }

    pub fn push(&mut self, value: &str) {
        self.stack.push(value.to_string());
    }

    pub fn pop(&mut self) -> Option<String> {
        self.stack.pop()
    }

    pub fn peek(&self, depth: usize) -> Option<&String> {
        if depth >= self.stack.len() {
            return None;
        }
        Some(&self.stack[self.stack.len() - 1 - depth])
    }

    pub fn len(&self) -> usize {
        self.stack.len()
    }

    pub fn to_vec(&self) -> Vec<String> {
        self.stack.clone()
    }
}

// ============================================================================
// Storage Inspector
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StorageInspector {
    pub slots: HashMap<(String, String), String>, // (address, slot) -> value
}

impl StorageInspector {
    pub fn new() -> Self {
        Self {
            slots: HashMap::new(),
        }
    }

    pub fn set(&mut self, address: &str, slot: &str, value: &str) {
        self.slots.insert((address.to_string(), slot.to_string()), value.to_string());
    }

    pub fn get(&self, address: &str, slot: &str) -> Option<&String> {
        self.slots.get(&(address.to_string(), slot.to_string()))
    }

    pub fn diff(&self, other: &StorageInspector) -> Vec<StorageChange> {
        let mut changes = Vec::new();
        
        // Find changes
        for ((addr, slot), pre_value) in &self.slots {
            let post_value = other.slots.get(&(addr.clone(), slot.clone()));
            
            let diff_type = match post_value {
                None => DiffType::Deleted,
                Some(v) if v != pre_value => DiffType::Modified,
                Some(_) => continue,
                None => continue,
            };
            
            changes.push(StorageChange {
                address: addr.clone(),
                slot: slot.clone(),
                key: slot.clone(),
                pre_value: pre_value.clone(),
                post_value: post_value.cloned().unwrap_or_default(),
                diff_type,
            });
        }
        
        // Find additions
        for ((addr, slot), post_value) in &other.slots {
            if !self.slots.contains_key(&(addr.clone(), slot.clone())) {
                changes.push(StorageChange {
                    address: addr.clone(),
                    slot: slot.clone(),
                    key: slot.clone(),
                    pre_value: String::new(),
                    post_value: post_value.clone(),
                    diff_type: DiffType::Added,
                });
            }
        }
        
        changes
    }
}

// ============================================================================
// Trace Service
// ============================================================================

pub struct TraceService {
    config: Config,
    rpc: Provider<Http>,
    ws: Option<Provider<Ws>>,
    state: Arc<RwLock<ServiceState>>,
}

#[derive(Debug)]
pub struct ServiceState {
    pub current_block: u64,
    pub cache: HashMap<String, TraceResult>,
}

impl TraceService {
    pub async fn new(config: Config) -> Result<Self> {
        info!("Initializing Trace Debug Service");
        
        let rpc = Provider::<Http>::try_from(config.rpc_url.clone())?;
        
        let ws = if let Some(ref ws_url) = config.ws_url {
            Some(Provider::<Ws>::connect(ws_url).await?)
        } else {
            None
        };
        
        let service = Self {
            config: config.clone(),
            rpc,
            ws,
            state: Arc::new(RwLock::new(ServiceState {
                current_block: 0,
                cache: HashMap::new(),
            })),
        };
        
        info!("Trace Debug Service initialized");
        Ok(service)
    }

    /// Get full transaction trace with state diff
    pub async fn trace_transaction(&self, request: TraceRequest) -> Result<TraceResult> {
        info!("Tracing transaction: {}", request.transaction_hash);
        
        // Get transaction
        let tx = self.rpc.get_transaction(&request.transaction_hash.clone().into())
            .await?
            .ok_or_else(|| TraceError::NotFound(request.transaction_hash.clone()))?;
        
        // Get receipt
        let receipt = self.rpc.get_transaction_receipt(&request.transaction_hash.clone().into())
            .await?
            .ok_or_else(|| TraceError::NotFound(request.transaction_hash.clone()))?;
        
        // Get block
        let block = self.rpc.get_block(receipt.block_number.unwrap())
            .await?
            .ok_or("Block not found")?;
        
        // Get traces using debug_traceCall
        let traces = self.get_traces(&tx, receipt.block_number.unwrap()).await?;
        
        // Parse traces into call tree
        let call_traces = self.parse_traces(&traces)?;
        
        // Get state diff if enabled
        let state_diff = if request.enable_state_diff {
            self.get_state_diff(&tx, receipt.block_number.unwrap()).await?
        } else {
            None
        };
        
        // Get gas profiling if enabled
        let gas_profiling = if request.enable_gas_profiling {
            Some(self.calculate_gas_profiling(&call_traces))
        } else {
            None
        };
        
        // Parse logs
        let logs: Vec<LogData> = receipt.logs.iter()
            .enumerate()
            .map(|(i, log)| LogData {
                address: format!("{:?}", log.address),
                topics: log.topics.iter().map(|t| format!("{:?}", t)).collect(),
                data: hex::encode(&log.data),
                log_index: i as u32,
            })
            .collect();
        
        // Build internal transactions from traces
        let internal_txs = self.build_internal_txs(&call_traces);
        
        let result = TraceResult {
            transaction_hash: request.transaction_hash,
            block_number: receipt.block_number.unwrap().as_u64(),
            block_hash: format!("{:?}", receipt.block_hash.unwrap_or_default()),
            from: format!("{:?}", tx.from),
            to: format!("{:?}", tx.to.unwrap_or_default()),
            value: format!("{}", tx.value),
            gas_used: receipt.gas_used.unwrap_or_default().as_u64(),
            gas_limit: tx.gas.unwrap_or_default().as_u64(),
            status: receipt.status.unwrap_or_default().as_u64() == 1,
            timestamp: block.timestamp.as_u64(),
            traces: call_traces,
            state_diff,
            gas_profiling,
            logs,
            internal_txs,
        };
        
        // Cache result
        self.cache_result(&result);
        
        Ok(result)
    }

    /// Get traces from RPC
    async fn get_traces(&self, tx: &Transaction, block_number: u64) -> Result<Vec<serde_json::Value>> {
        // In production, this would call debug_traceCall or debug_traceBlockByNumber
        // For now, return empty traces
        Ok(vec![])
    }

    /// Parse raw traces into call tree
    fn parse_traces(&self, raw_traces: &[serde_json::Value]) -> Result<Vec<CallTrace>> {
        let mut calls = Vec::new();
        let mut index = 0u32;
        
        for trace in raw_traces {
            let trace_type = trace.get("type")
                .and_then(|t| t.as_str())
                .unwrap_or("call");
            
            let call_type = match trace_type {
                "CALL" => "call",
                "CREATE" => "create",
                "CREATE2" => "create2",
                "DELEGATECALL" => "delegatecall",
                "STATICCALL" => "staticcall",
                "SUICIDE" => "selfdestruct",
                "PRECOMPILE" => "precompile",
                _ => "call",
            };
            
            let from = trace.get("from")
                .and_then(|f| f.as_str())
                .unwrap_or("0x0000000000000000000000000000000000000000")
                .to_string();
            
            let to = trace.get("to")
                .and_then(|t| t.as_str())
                .unwrap_or("0x0000000000000000000000000000000000000000")
                .to_string();
            
            let value = trace.get("value")
                .and_then(|v| v.as_str())
                .unwrap_or("0x0")
                .to_string();
            
            let gas = trace.get("gas")
                .and_then(|g| g.as_str())
                .and_then(|g| g.strip_prefix("0x"))
                .and_then(|g| u64::from_str_radix(g, 16).ok())
                .unwrap_or(0);
            
            let gas_used = gas; // Simplified
            
            let input = trace.get("input")
                .and_then(|i| i.as_str())
                .unwrap_or("0x")
                .to_string();
            
            let output = trace.get("output")
                .and_then(|o| o.as_str())
                .unwrap_or("0x")
                .to_string();
            
            let depth = trace.get("depth")
                .and_then(|d| d.as_u64())
                .unwrap_or(1) as u32;
            
            let revert = trace.get("error")
                .and_then(|e| e.as_str())
                .map(|e| e.contains("revert"))
                .unwrap_or(false);
            
            let error = trace.get("error")
                .and_then(|e| e.as_str())
                .map(|e| e.to_string());
            
            let parent_index = if depth > 1 {
                // Find parent call
                let mut parent = None;
                for (i, c) in calls.iter().enumerate().rev() {
                    if c.depth == depth - 1 {
                        parent = Some(i as u32);
                        break;
                    }
                }
                parent
            } else {
                None
            };
            
            calls.push(CallTrace {
                call_type: call_type.to_string(),
                from,
                to,
                value,
                gas,
                gas_used,
                input,
                output,
                depth,
                index,
                parent_index,
                revert,
                error,
            });
            
            index += 1;
        }
        
        Ok(calls)
    }

    /// Get state diff between pre and post execution
    async fn get_state_diff(&self, tx: &Transaction, block_number: u64) -> Result<Option<StateDiff>> {
        // In production, this would call debug_traceCall with stateDiff enabled
        Ok(None)
    }

    /// Calculate gas profiling
    fn calculate_gas_profiling(&self, calls: &[CallTrace]) -> GasProfiling {
        let total_gas: u64 = calls.iter().map(|c| c.gas_used).sum();
        let mut gas_per_call = Vec::new();
        let mut gas_per_opcode: HashMap<String, u64> = HashMap::new();
        
        for (i, call) in calls.iter().enumerate() {
            gas_per_call.push(GasCall {
                call_index: i as u32,
                call_type: call.call_type.clone(),
                gas_used: call.gas_used,
                gas_limit: call.gas,
                percentage: if total_gas > 0 {
                    (call.gas_used as f64 / total_gas as f64) * 100.0
                } else {
                    0.0
                },
            });
            
            // Track by call type
            *gas_per_opcode.entry(call.call_type.clone()).or_insert(0) += call.gas_used;
        }
        
        // Generate optimization suggestions
        let mut suggestions = Vec::new();
        for call in &gas_per_call {
            if call.percentage > 20.0 {
                suggestions.push(GasSuggestion {
                    call_index: call.call_index,
                    suggestion: format!(
                        "Consider optimizing {} call at index {} - uses {:.1}% of gas",
                        call.call_type, call.call_index, call.percentage
                    ),
                    estimated_savings: (call.gas_used as f64 * 0.3) as u64,
                });
            }
        }
        
        GasProfiling {
            total_gas,
            total_gas_used: total_gas,
            gas_per_call,
            gas_per_opcode,
            optimization_suggestions: suggestions.into_iter().take(10).collect(),
        }
    }

    /// Build internal transactions from call traces
    fn build_internal_txs(&self, calls: &[CallTrace]) -> Vec<InternalTransaction> {
        calls.iter()
            .filter(|c| c.call_type != "staticcall" && c.call_type != "precompile")
            .map(|c| InternalTransaction {
                hash: format!("internal-{}", c.index),
                block_number: 0,
                transaction_index: c.index,
                from: c.from.clone(),
                to: c.to.clone(),
                value: c.value.clone(),
                gas: c.gas,
                gas_used: c.gas_used,
                input: c.input.clone(),
                output: c.output.clone(),
                call_type: c.call_type.clone(),
                depth: c.depth,
                revert: c.revert,
            })
            .collect()
    }

    /// Cache trace result
    fn cache_result(&self, result: &TraceResult) {
        let mut state = self.state.write();
        state.cache.insert(result.transaction_hash.clone(), result.clone());
    }

    /// Get cached trace
    fn get_cached(&self, tx_hash: &str) -> Option<TraceResult> {
        let state = self.state.read();
        state.cache.get(tx_hash).cloned()
    }

    /// Estimate gas for a transaction
    pub async fn estimate_gas(&self, tx: &Transaction) -> Result<u64> {
        let gas = self.rpc.estimate_gas(tx).await?;
        Ok(gas.as_u64())
    }

    /// Simulate transaction execution
    pub async fn simulate(&self, tx: &Transaction, block_number: Option<u64>) -> Result<TraceResult> {
        let request = TraceRequest {
            transaction_hash: format!("{:?}", tx.hash()),
            block_number,
            trace_types: vec![TraceType::Call, TraceType::Create],
            enable_state_diff: true,
            enable_gas_profiling: true,
        };
        
        self.trace_transaction(request).await
    }
}

// ============================================================================
// API Types
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TraceApiRequest {
    pub tx_hash: String,
    pub block: Option<u64>,
    pub enable_state_diff: Option<bool>,
    pub enable_gas_profiling: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TraceApiResponse {
    pub success: bool,
    pub result: Option<TraceResult>,
    pub error: Option<String>,
}

// ============================================================================
// Gas Optimizer
// ============================================================================

pub struct GasOptimizer {
    config: Config,
}

impl GasOptimizer {
    pub fn new() -> Self {
        Self {
            config: Config::default(),
        }
    }

    /// Analyze transaction for gas optimization opportunities
    pub fn analyze(&self, result: &TraceResult) -> Vec<GasOptimization> {
        let mut optimizations = Vec::new();
        
        // Analyze each call
        for call in &result.traces {
            // Check for redundant calls
            if call.call_type == "staticcall" && call.gas_used > 50000 {
                optimizations.push(GasOptimization {
                    call_type: call.call_type.clone(),
                    index: call.index,
                    issue: "High gas static call".to_string(),
                    suggestion: "Consider caching result if used multiple times".to_string(),
                    estimated_savings: call.gas_used / 2,
                });
            }
            
            // Check for unnecessary storage writes
            if call.call_type == "sstore" && call.gas_used > 20000 {
                optimizations.push(GasOptimization {
                    call_type: call.call_type.clone(),
                    index: call.index,
                    issue: "Expensive storage write".to_string(),
                    suggestion: "Consider batching or avoiding if value unchanged".to_string(),
                    estimated_savings: call.gas_used / 3,
                });
            }
            
            // Check for large data transfers
            if call.input.len() > 1000 {
                optimizations.push(GasOptimization {
                    call_type: call.call_type.clone(),
                    index: call.index,
                    issue: "Large calldata".to_string(),
                    suggestion: "Consider using events or off-chain storage".to_string(),
                    estimated_savings: (call.input.len() as u64 - 100) * 68,
                });
            }
        }
        
        optimizations
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GasOptimization {
    pub call_type: String,
    pub index: u32,
    pub issue: String,
    pub suggestion: String,
    pub estimated_savings: u64,
}

// ============================================================================
// Utility Functions
// ============================================================================

/// Parse function selector from input data
pub fn parse_selector(input: &str) -> Option<String> {
    if input.len() < 10 {
        return None;
    }
    
    let selector = &input[2..10];
    Some(format!("0x{}", selector))
}

/// Decode function signature from selector
pub fn decode_selector(_selector: &str) -> Option<String> {
    // In production, this would use a signature database
    None
}

/// Format call tree for visualization
pub fn format_call_tree(calls: &[CallTrace]) -> String {
    let mut output = String::new();
    
    for call in calls {
        let indent = "  ".repeat(call.depth as usize);
        let status = if call.revert { " [REVERTED]" } else { "" };
        
        output.push_str(&format!(
            "{}{} {} -> {} ({} gas){}\n",
            indent,
            call.call_type,
            format_short_address(&call.from),
            format_short_address(&call.to),
            call.gas_used,
            status
        ));
    }
    
    output
}

fn format_short_address(addr: &str) -> String {
    if addr.len() >= 42 {
        format!("{}...{}", &addr[0..6], &addr[38..42])
    } else {
        addr.to_string()
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_memory_inspector() {
        let mut mem = MemoryInspector::new();
        mem.load(&[1, 2, 3, 4, 5, 6, 7, 8, 9, 10]);
        
        assert_eq!(mem.read(0, 4), Some("0x01020304".to_string()));
        assert_eq!(mem.len(), 10);
    }

    #[test]
    fn test_stack_inspector() {
        let mut stack = StackInspector::new();
        stack.push("0x1");
        stack.push("0x2");
        stack.push("0x3");
        
        assert_eq!(stack.pop(), Some("0x3".to_string()));
        assert_eq!(stack.peek(0), Some(&"0x2".to_string()));
        assert_eq!(stack.len(), 2);
    }

    #[test]
    fn test_storage_inspector() {
        let mut storage = StorageInspector::new();
        storage.set("0x1234", "0x0", "0x1111");
        storage.set("0x1234", "0x1", "0x2222");
        
        assert_eq!(storage.get("0x1234", "0x0"), Some(&"0x1111".to_string()));
    }

    #[test]
    fn test_parse_selector() {
        assert_eq!(parse_selector("0x12345678"), Some("0x12345678".to_string()));
        assert_eq!(parse_selector("0x"), None);
    }
}