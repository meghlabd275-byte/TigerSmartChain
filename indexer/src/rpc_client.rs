//! RPC Client for Indexer

use crate::types::*;
use std::sync::Arc;

// =============================================================================
// RPC CLIENT
// =============================================================================

/// RPC Client
pub struct RPCClient {
    url: String,
    client: reqwest::Client,
}

impl RPCClient {
    /// Create new RPC client
    pub fn new(url: &str) -> Self {
        Self {
            url: url.to_string(),
            client: reqwest::Client::new(),
        }
    }

    /// Get latest block number
    pub async fn get_block_number(&self) -> Result<u64, String> {
        let body = serde_json::json!({
            "jsonrpc": "2.0",
            "method": "eth_blockNumber",
            "params": [],
            "id": 1
        });

        let response = self.client
            .post(&self.url)
            .json(&body)
            .send()
            .await
            .map_err(|e| e.to_string())?;

        let data: serde_json::Value = response.json().await.map_err(|e| e.to_string())?;
        
        let number = data["result"]
            .as_str()
            .ok_or("Invalid response")?;
        
        Ok(u64::from_str_radix(&number[2..], 16).map_err(|e| e.to_string())?)
    }

    /// Get block by number
    pub async fn get_block_by_number(&self, number: u64) -> Result<IndexedBlock, String> {
        let hex = format!("0x{:x}", number);
        
        let body = serde_json::json!({
            "jsonrpc": "2.0",
            "method": "eth_getBlockByNumber",
            "params": [hex, true],
            "id": 1
        });

        let response = self.client
            .post(&self.url)
            .json(&body)
            .send()
            .await
            .map_err(|e| e.to_string())?;

        let data: serde_json::Value = response.json().await.map_err(|e| e.to_string())?;
        
        // Parse block data
        Ok(IndexedBlock {
            number,
            hash: data["result"]["hash"].as_str().unwrap_or("").to_string(),
            parent_hash: data["result"]["parentHash"].as_str().unwrap_or("").to_string(),
            timestamp: i64::from_str_radix(
                data["result"]["timestamp"].as_str().unwrap_or("0x0")[2..], 16
            ).unwrap_or(0) as i64,
            transactions: vec![],
            logs: vec![],
            internal_txs: vec![],
            miner: data["result"]["miner"].as_str().unwrap_or("").to_string(),
            difficulty: data["result"]["difficulty"].as_str().unwrap_or("0").to_string(),
            total_difficulty: data["result"]["totalDifficulty"].as_str().unwrap_or("0").to_string(),
            size: data["result"]["size"].as_str().unwrap_or("0x0")[2..].parse().unwrap_or(0),
            gas_used: data["result"]["gasUsed"].as_str().unwrap_or("0x0")[2..].parse().unwrap_or(0),
            gas_limit: data["result"]["gasLimit"].as_str().unwrap_or("0x0")[2..].parse().unwrap_or(0),
            base_fee_per_gas: data["result"]["baseFeePerGas"]
                .as_str()
                .map(|s| u64::from_str_radix(&s[2..], 16).ok())
                .flatten(),
        })
    }

    /// Get transaction receipt
    pub async fn get_transaction_receipt(&self, hash: &str) -> Result<IndexedTransaction, String> {
        let body = serde_json::json!({
            "jsonrpc": "2.0",
            "method": "eth_getTransactionReceipt",
            "params": [hash],
            "id": 1
        });

        let response = self.client
            .post(&self.url)
            .json(&body)
            .send()
            .await
            .map_err(|e| e.to_string())?;

        let data: serde_json::Value = response.json().await.map_err(|e| e.to_string())?;
        
        let result = data["result"].as_object().ok_or("Transaction not found")?;
        
        let status = if result["status"].as_str() == Some("0x1") {
            TransactionStatus::Success
        } else {
            TransactionStatus::Failed
        };

        Ok(IndexedTransaction {
            hash: hash.to_string(),
            block_number: u64::from_str_radix(
                result["blockNumber"].as_str().unwrap_or("0x0")[2..], 16
            ).unwrap_or(0),
            block_hash: result["blockHash"].as_str().unwrap_or("").to_string(),
            transaction_index: u64::from_str_radix(
                result["transactionIndex"].as_str().unwrap_or("0x0")[2..], 16
            ).unwrap_or(0),
            from: result["from"].as_str().unwrap_or("").to_string(),
            to: result["to"].as_str().map(|s| s.to_string()),
            value: result["value"].as_str().unwrap_or("0x0").to_string(),
            gas_price: u64::from_str_radix(
                result["gasPrice"].as_str().unwrap_or("0x0")[2..], 16
            ).unwrap_or(0),
            gas_used: u64::from_str_radix(
                result["gasUsed"].as_str().unwrap_or("0x0")[2..], 16
            ).unwrap_or(0),
            nonce: u64::from_str_radix(
                result["nonce"].as_str().unwrap_or("0x0")[2..], 16
            ).unwrap_or(0),
            input: result["input"].as_str().unwrap_or("").to_string(),
            status,
            logs: vec![],
        })
    }
}