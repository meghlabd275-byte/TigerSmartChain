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
