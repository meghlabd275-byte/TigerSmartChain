//! RPC Handler

use crate::types::*;
use std::sync::Arc;

// =============================================================================
// HANDLER
// =============================================================================

/// RPC Handler
pub struct RPCHandler {
    block_number: u64,
}

impl RPCHandler {
    pub fn new() -> Self {
        Self { block_number: 0 }
    }

    /// Handle request
    pub fn handle(&self, request: &RPCRequest) -> RPCResponse {
        let id = request.id.unwrap_or(0);
        
        match request.method.as_str() {
            "eth_blockNumber" => self.eth_block_number(id),
            "eth_getBlockByNumber" => self.eth_get_block_by_number(request, id),
            "eth_getBlockByHash" => self.eth_get_block_by_hash(request, id),
            "eth_getTransactionByHash" => self.eth_get_transaction_by_hash(request, id),
            "eth_getBalance" => self.eth_get_balance(request, id),
            "eth_getTransactionCount" => self.eth_get_transaction_count(request, id),
            "eth_call" => self.eth_call(request, id),
            "eth_estimateGas" => self.eth_estimate_gas(request, id),
            "eth_chainId" => self.eth_chain_id(id),
            "eth_gasPrice" => self.eth_gas_price(id),
            "eth_sendRawTransaction" => self.eth_send_raw_transaction(request, id),
            _ => RPCResponse::error(-32601, "Method not found".to_string(), id),
        }
    }

    fn eth_block_number(&self, id: i64) -> RPCResponse {
        RPCResponse::success(serde_json::json!(format!("0x{:x}", self.block_number)), id)
    }

    fn eth_get_block_by_number(&self, request: &RPCRequest, id: i64) -> RPCResponse {
        let block = Block {
            number: format!("0x{:x}", self.block_number),
            hash: "0x0000".to_string(),
            parent_hash: "0x0000".to_string(),
            nonce: "0x0000000000000000".to_string(),
            sha3_uncles: "0x0000".to_string(),
            logs_bloom: "0x0000".to_string(),
            transactions_root: "0x0000".to_string(),
            state_root: "0x0000".to_string(),
            receipts_root: "0x0000".to_string(),
            miner: "0x0000".to_string(),
            difficulty: "0x0".to_string(),
            total_difficulty: "0x0".to_string(),
            size: "0x0".to_string(),
            gas_limit: "0x0".to_string(),
            gas_used: "0x0".to_string(),
            timestamp: "0x0".to_string(),
            extra_data: "0x".to_string(),
            mix_hash: "0x0000".to_string(),
            uncles: vec![],
        };
        RPCResponse::success(serde_json::to_value(block).unwrap(), id)
    }

    fn eth_get_block_by_hash(&self, request: &RPCRequest, id: i64) -> RPCResponse {
        self.eth_get_block_by_number(request, id)
    }

    fn eth_get_transaction_by_hash(&self, request: &RPCRequest, id: i64) -> RPCResponse {
        let tx = Transaction {
            hash: "0x0000".to_string(),
            nonce: "0x0".to_string(),
            block_hash: Some("0x0000".to_string()),
            block_number: Some("0x0".to_string()),
            transaction_index: Some("0x0".to_string()),
            from: "0x0000".to_string(),
            to: Some("0x0000".to_string()),
            value: "0x0".to_string(),
            gas_price: "0x0".to_string(),
            gas: "0x0".to_string(),
            input: "0x".to_string(),
        };
        RPCResponse::success(serde_json::to_value(tx).unwrap(), id)
    }

    fn eth_get_balance(&self, request: &RPCRequest, id: i64) -> RPCResponse {
        RPCResponse::success(serde_json::json!("0x0"), id)
    }

    fn eth_get_transaction_count(&self, request: &RPCRequest, id: i64) -> RPCResponse {
        RPCResponse::success(serde_json::json!("0x0"), id)
    }

    fn eth_call(&self, request: &RPCRequest, id: i64) -> RPCResponse {
        RPCResponse::success(serde_json::json!("0x"), id)
    }

    fn eth_estimate_gas(&self, request: &RPCRequest, id: i64) -> RPCResponse {
        RPCResponse::success(serde_json::json!("0x5208"), id)
    }

    fn eth_chain_id(&self, id: i64) -> RPCResponse {
        RPCResponse::success(serde_json::json!("0x1"), id)
    }

    fn eth_gas_price(&self, id: i64) -> RPCResponse {
        RPCResponse::success(serde_json::json!("0x4", ), id)
    }

    fn eth_send_raw_transaction(&self, request: &RPCRequest, id: i64) -> RPCResponse {
        RPCResponse::success(serde_json::json!("0x0000"), id)
    }
}

impl Default for RPCHandler {
    fn default() -> Self {
        Self::new()
    }
}