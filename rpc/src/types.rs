//! RPC Types

use serde::{Deserialize, Serialize};

// =============================================================================
// REQUEST/RESPONSE
// =============================================================================

/// JSON-RPC Request
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RPCRequest {
    pub jsonrpc: String,
    pub method: String,
    pub params: Option<serde_json::Value>,
    pub id: Option<i64>,
}

/// JSON-RPC Response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RPCResponse {
    pub jsonrpc: String,
    pub result: Option<serde_json::Value>,
    pub error: Option<RPCError>,
    pub id: Option<i64>,
}

impl RPCResponse {
    pub fn success(result: serde_json::Value, id: i64) -> Self {
        Self {
            jsonrpc: "2.0".to_string(),
            result: Some(result),
            error: None,
            id: Some(id),
        }
    }

    pub fn error(code: i32, message: String, id: i64) -> Self {
        Self {
            jsonrpc: "2.0".to_string(),
            result: None,
            error: Some(RPCError { code, message }),
            id: Some(id),
        }
    }
}

/// RPC Error
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RPCError {
    pub code: i32,
    pub message: String,
}

// =============================================================================
// METHODS
// =============================================================================

/// RPC Methods
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum RPCMethod {
    // Blocks
    eth_blockNumber,
    eth_getBlockByNumber,
    eth_getBlockByHash,
    eth_getBlockTransactionCountByNumber,
    eth_getBlockTransactionCountByHash,
    
    // Transactions
    eth_getTransactionByHash,
    eth_getTransactionByBlockNumberAndIndex,
    eth_getTransactionByBlockHashAndIndex,
    eth_getTransactionReceipt,
    eth_sendRawTransaction,
    
    // State
    eth_getBalance,
    eth_getCode,
    eth_getStorageAt,
    eth_getTransactionCount,
    
    // Contract
    eth_call,
    eth_estimateGas,
    
    // Filters
    eth_newFilter,
    eth_newBlockFilter,
    eth_newPendingTransactionFilter,
    eth_getFilterChanges,
    eth_uninstallFilter,
    
    // Chain info
    eth_chainId,
    eth_gasPrice,
    eth_maxPriorityFeePerGas,
    eth_getBlockReceipts,
}

// =============================================================================
// BLOCK TYPES
// =============================================================================

/// Block
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Block {
    pub number: String,
    pub hash: String,
    pub parent_hash: String,
    pub nonce: String,
    pub sha3_uncles: String,
    pub logs_bloom: String,
    pub transactions_root: String,
    pub state_root: String,
    pub receipts_root: String,
    pub miner: String,
    pub difficulty: String,
    pub total_difficulty: String,
    pub size: String,
    pub gas_limit: String,
    pub gas_used: String,
    pub timestamp: String,
    pub extra_data: String,
    pub mix_hash: String,
    pub uncles: Vec<String>,
}

// =============================================================================
// TRANSACTION TYPES
// =============================================================================

/// Transaction
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Transaction {
    pub hash: String,
    pub nonce: String,
    pub block_hash: Option<String>,
    pub block_number: Option<String>,
    pub transaction_index: Option<String>,
    pub from: String,
    pub to: Option<String>,
    pub value: String,
    pub gas_price: String,
    pub gas: String,
    pub input: String,
}

/// Transaction Receipt
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TransactionReceipt {
    pub transaction_hash: String,
    pub block_hash: String,
    pub block_number: String,
    pub contract_address: Option<String>,
    pub cumulative_gas_used: String,
    pub gas_used: String,
    pub logs: Vec<Log>,
    pub logs_bloom: String,
    pub status: String,
}

// =============================================================================
// LOG
// =============================================================================

/// Log
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Log {
    pub address: String,
    pub topics: Vec<String>,
    pub data: String,
    pub block_number: String,
    pub transaction_hash: String,
    pub log_index: String,
    pub removed: bool,
}

/// Log filter
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LogFilter {
    pub from_block: Option<String>,
    pub to_block: Option<String>,
    pub address: Option<serde_json::Value>,
    pub topics: Option<Vec<serde_json::Value>>,
    #[serde(rename = "blockHash")]
    pub block_hash: Option<String>,
}

// =============================================================================
// TRACE TYPES - Internal Transaction Tracking
// =============================================================================

/// Trace result from trace_block/trace_transaction
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TraceResult {
    pub action: Option<TraceAction>,
    pub result: Option<TraceResultData>,
    pub tx_hash: Option<String>,
    pub subtraces: Option<u32>,
    #[serde(rename = "type")]
    pub trace_type: Option<String>,
}

/// Trace action
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TraceAction {
    #[serde(rename = "callType")]
    pub call_type: Option<String>,
    pub from: Option<String>,
    pub to: Option<String>,
    pub value: Option<String>,
    pub gas: Option<String>,
    pub input: Option<String>,
    pub init: Option<String>,
    #[serde(rename = "address")]
    pub created_contract: Option<String>,
    #[serde(rename = "balance")]
    pub created_balance: Option<String>,
    #[serde(rename = "refundAddress")]
    pub refund_address: Option<String>,
}

/// Trace result data
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TraceResultData {
    pub gas: Option<String>,
    pub return_value: Option<String>,
    pub address: Option<String>,
}

/// Trace replay result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TraceReplayResult {
    #[serde(rename = "txHash")]
    pub tx_hash: String,
    pub trace: Option<Vec<TraceResult>>,
    pub state_diff: Option<StateDiff>,
    pub vm_trace: Option<VmTrace>,
}

/// State diff
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StateDiff {
    pub from: String,
    pub to: String,
    pub before: String,
    pub after: String,
}

// =============================================================================
// DEBUG TYPES - Block Tracing & Inspection
// =============================================================================

/// Debug trace options
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DebugTraceOptions {
    #[serde(rename = "disableStorage")]
    pub disable_storage: Option<bool>,
    #[serde(rename = "disableStack")]
    pub disable_stack: Option<bool>,
    #[serde(rename = "enableMemory")]
    pub enable_memory: Option<bool>,
    #[serde(rename = "enableReturnData")]
    pub enable_return_data: Option<bool>,
    pub tracer: Option<String>,
    #[serde(rename = "tracerConfig")]
    pub tracer_config: Option<serde_json::Value>,
}

impl Default for DebugTraceOptions {
    fn default() -> Self {
        Self {
            disable_storage: Some(false),
            disable_stack: Some(false),
            enable_memory: Some(false),
            enable_return_data: Some(false),
            tracer: Some("callTracer".to_string()),
            tracer_config: None,
        }
    }
}

/// Debug trace
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DebugTrace {
    #[serde(rename = "type")]
    pub trace_type: String,
    pub from: String,
    pub to: String,
    pub value: String,
    pub gas: String,
    pub gas_used: String,
    pub input: String,
    pub output: String,
    pub calls: Vec<DebugCall>,
}

/// Debug call
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DebugCall {
    #[serde(rename = "type")]
    pub call_type: String,
    pub from: String,
    pub to: String,
    pub value: String,
    pub gas: String,
    pub gas_used: String,
    pub input: String,
    pub output: String,
    pub calls: Vec<DebugCall>,
}

/// Vm trace
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VmTrace {
    pub code: String,
    pub ops: Vec<VmOp>,
}

/// Vm operation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VmOp {
    pub pc: u32,
    pub cost: u32,
    pub stack: Vec<String>,
    pub memory: Option<String>,
    pub return_data: Option<String>,
    pub op: String,
}

// =============================================================================
// PARITY TYPES - OpenEthereum Compatibility
// =============================================================================

/// Parity trace (OpenEthereum style)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ParityTrace {
    #[serde(rename = "action")]
    pub action: ParityAction,
    pub error: Option<String>,
    #[serde(rename = "result")]
    pub result: Option<ParityResult>,
}

/// Parity action
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ParityAction {
    #[serde(rename = "callType")]
    pub call_type: Option<String>,
    pub from: Option<String>,
    pub to: Option<String>,
    pub value: Option<String>,
    pub gas: Option<String>,
    pub input: Option<String>,
    #[serde(rename = "init")]
    pub init: Option<String>,
    #[serde(rename = "address")]
    pub created_contract: Option<String>,
    #[serde(rename = "balance")]
    pub created_balance: Option<String>,
}

/// Parity result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ParityResult {
    pub gas: Option<String>,
    #[serde(rename = "returnValue")]
    pub return_value: Option<String>,
    pub address: Option<String>,
}

/// Parity receipt
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ParityReceipt {
    #[serde(rename = "transactionHash")]
    pub transaction_hash: String,
    pub block_hash: String,
    pub block_number: u64,
    #[serde(rename = "contractAddress")]
    pub contract_address: Option<String>,
    #[serde(rename = "cumulativeGasUsed")]
    pub cumulative_gas_used: String,
    #[serde(rename = "gasUsed")]
    pub gas_used: String,
    pub logs: Vec<Log>,
    #[serde(rename = "logsBloom")]
    pub logs_bloom: String,
    #[serde(rename = "status")]
    pub status: String,
}

/// Receipt (for eth_getBlockReceipts)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Receipt {
    #[serde(rename = "transactionHash")]
    pub transaction_hash: String,
    #[serde(rename = "blockHash")]
    pub block_hash: String,
    #[serde(rename = "blockNumber")]
    pub block_number: u64,
    #[serde(rename = "contractAddress")]
    pub contract_address: Option<String>,
    #[serde(rename = "cumulativeGasUsed")]
    pub cumulative_gas_used: String,
    #[serde(rename = "gasUsed")]
    pub gas_used: String,
    pub logs: Vec<Log>,
    #[serde(rename = "logsBloom")]
    pub logs_bloom: String,
    pub status: String,
    #[serde(rename = "type")]
    pub tx_type: String,
}

/// Call request
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CallRequest {
    pub from: Option<String>,
    pub to: Option<String>,
    pub gas: Option<String>,
    #[serde(rename = "gasPrice")]
    pub gas_price: Option<String>,
    pub value: Option<String>,
    pub data: Option<String>,
}
