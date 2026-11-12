//! RPC Client for Indexer

use crate::indexer::parse_hex_u64;
use crate::types::*;

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
        
        parse_hex_u64(number).map_err(|e| e.to_string())
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
        // Every quantity below is a `0x` hex string in the JSON-RPC reply and is
        // decoded through `parse_hex_u64`, which checks the prefix first; the
        // previous `[2..]` slice panicked on any value that lacked it.
        let result = &data["result"];
        let quantity = |field: &str| -> Result<u64, String> {
            parse_hex_u64(result[field].as_str().unwrap_or("0x0")).map_err(|e| e.to_string())
        };

        Ok(IndexedBlock {
            number,
            hash: result["hash"].as_str().unwrap_or("").to_string(),
            parent_hash: result["parentHash"].as_str().unwrap_or("").to_string(),
            timestamp: quantity("timestamp")? as i64,
            transactions: vec![],
            logs: vec![],
            internal_txs: vec![],
            miner: result["miner"].as_str().unwrap_or("").to_string(),
            difficulty: result["difficulty"].as_str().unwrap_or("0x0").to_string(),
            total_difficulty: result["totalDifficulty"].as_str().unwrap_or("0x0").to_string(),
            size: quantity("size")?,
            gas_used: quantity("gasUsed")?,
            gas_limit: quantity("gasLimit")?,
            base_fee_per_gas: result["baseFeePerGas"]
                .as_str()
                .map(parse_hex_u64)
                .transpose()
                .map_err(|e| e.to_string())?,
        })
    }

    /// Get a transaction by hash.
    ///
    /// A receipt reports the gas *used*, not the gas *limit* the sender set;
    /// the limit only appears on the transaction itself.
    pub async fn get_transaction(&self, hash: &str) -> Result<IndexedTransaction, String> {
        let body = serde_json::json!({
            "jsonrpc": "2.0",
            "method": "eth_getTransactionByHash",
            "params": [hash],
            "id": 1
        });

        let response = self
            .client
            .post(&self.url)
            .json(&body)
            .send()
            .await
            .map_err(|e| e.to_string())?;

        let data: serde_json::Value = response.json().await.map_err(|e| e.to_string())?;

        let result = data["result"]
            .as_object()
            .ok_or_else(|| format!("transaction {hash} is not known to the node"))?;

        let result = serde_json::Value::Object(result.clone());
        let quantity = |field: &str| -> Result<u64, String> {
            parse_hex_u64(result[field].as_str().unwrap_or("0x0")).map_err(|e| e.to_string())
        };

        Ok(IndexedTransaction {
            hash: result["hash"].as_str().unwrap_or(hash).to_string(),
            block_number: quantity("blockNumber")?,
            block_hash: result["blockHash"].as_str().unwrap_or("").to_string(),
            transaction_index: quantity("transactionIndex")?,
            from: result["from"].as_str().unwrap_or("").to_string(),
            to: result["to"].as_str().map(|s| s.to_string()),
            value: result["value"].as_str().unwrap_or("0x0").to_string(),
            gas_price: quantity("gasPrice")?,
            gas_used: quantity("gas")?,
            nonce: quantity("nonce")?,
            input: result["input"].as_str().unwrap_or("0x").to_string(),
            status: TransactionStatus::Pending,
            logs: Vec::new(),
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

        let quantity = |field: &str| -> Result<u64, String> {
            parse_hex_u64(result[field].as_str().unwrap_or("0x0")).map_err(|e| e.to_string())
        };

        Ok(IndexedTransaction {
            hash: hash.to_string(),
            block_number: quantity("blockNumber")?,
            block_hash: result["blockHash"].as_str().unwrap_or("").to_string(),
            transaction_index: quantity("transactionIndex")?,
            from: result["from"].as_str().unwrap_or("").to_string(),
            to: result["to"].as_str().map(|s| s.to_string()),
            value: result["value"].as_str().unwrap_or("0x0").to_string(),
            gas_price: quantity("effectiveGasPrice")?,
            gas_used: quantity("gasUsed")?,
            nonce: quantity("nonce")?,
            input: result["input"].as_str().unwrap_or("").to_string(),
            status,
            logs: vec![],
        })
    }
}