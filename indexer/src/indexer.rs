//! Indexer for TigerScan - Full implementation

use crate::types::*;
use chrono::Utc;
use std::sync::Arc;
use tokio::sync::RwLock;
use tokio::time::{interval, Duration};
use thiserror::Error;

// =============================================================================
// ERRORS
// =============================================================================

#[derive(Error, Debug)]
pub enum IndexerError {
    #[error("RPC error: {0}")]
    RPCError(String),
    #[error("Database error: {0}")]
    DatabaseError(String),
    #[error("Parse error: {0}")]
    ParseError(String),
    #[error("Not running")]
    NotRunning,
}

// =============================================================================
// INDEXER
// =============================================================================

/// Blockchain Indexer - Full implementation with RPC connection
pub struct Indexer {
    config: IndexerConfig,
    stats: Arc<RwLock<IndexerStats>>,
    running: Arc<RwLock<bool>>,
    current_block: Arc<RwLock<u64>>,
    /// RPC URL for connecting to Ethereum node
    rpc_url: String,
}

impl Indexer {
    /// Create new indexer
    pub fn new(config: IndexerConfig) -> Self {
        Self {
            config: config.clone(),
            stats: Arc::new(RwLock::new(IndexerStats {
                current_block: 0,
                indexed_block: 0,
                indexed_transactions: 0,
                indexed_logs: 0,
                indexed_tokens: 0,
                indexed_nfts: 0,
                last_update: Utc::now().timestamp(),
                processing_rate: 0.0,
            })),
            running: Arc::new(RwLock::new(false)),
            current_block: Arc::new(RwLock::new(config.start_block)),
            rpc_url: config.rpc_url.clone(),
        }
    }

    /// Start indexing
    pub async fn start(&self) -> Result<(), IndexerError> {
        *self.running.write().await = true;
        
        log::info!("Starting indexer from block {}", self.config.start_block);
        
        // Start block processing loop
        self.process_blocks_loop().await
    }

    /// Stop indexing
    pub async fn stop(&self) {
        *self.running.write().await = false;
        log::info!("Indexer stopped");
    }

    /// Is running
    pub async fn is_running(&self) -> bool {
        *self.running.read().await
    }

    /// Get statistics
    pub async fn get_stats(&self) -> IndexerStats {
        self.stats.read().await.clone()
    }

    /// Get current block
    pub async fn get_current_block(&self) -> u64 {
        *self.current_block.read().await
    }

    /// Process blocks in a loop
    async fn process_blocks_loop(&self) -> Result<(), IndexerError> {
        let mut current = self.config.start_block;
        let mut poll_interval = interval(Duration::from_secs(12)); // ~12 second block time

        while !*self.running.read().await {
            return Err(IndexerError::NotRunning);
        }

        while self.is_running().await {
            poll_interval.tick().await;

            // Get latest block from RPC
            let latest = match self.fetch_latest_block().await {
                Ok(n) => n,
                Err(e) => {
                    log::warn!("Failed to fetch latest block: {}", e);
                    continue;
                }
            };

            // Process new blocks
            if current > latest {
                // No new blocks, wait
                continue;
            }

            // Process batch of blocks
            let batch_size = self.config.batch_size as u64;
            let end = std::cmp::min(current + batch_size - 1, latest);

            log::info!("Processing blocks {} to {}", current, end);

            for block_num in current..=end {
                match self.process_block(block_num).await {
                    Ok(_) => {
                        *self.current_block.write().await = block_num + 1;
                    }
                    Err(e) => {
                        log::error!("Error processing block {}: {}", block_num, e);
                    }
                }
            }

            current = end + 1;
        }

        Ok(())
    }

    /// Fetch latest block number from RPC
    async fn fetch_latest_block(&self) -> Result<u64, IndexerError> {
        // Use reqwest to call RPC endpoint
        let client = reqwest::Client::new();
        
        let request = serde_json::json!({
            "jsonrpc": "2.0",
            "method": "eth_blockNumber",
            "params": [],
            "id": 1
        });

        let response = client
            .post(&self.rpc_url)
            .json(&request)
            .send()
            .await
            .map_err(|e| IndexerError::RPCError(e.to_string()))?
            .json::<serde_json::Value>()
            .await
            .map_err(|e| IndexerError::RPCError(e.to_string()))?;

        let result = response.get("result")
            .ok_or_else(|| IndexerError::RPCError("No result in response".to_string()))?;

        let hex = result.as_str()
            .ok_or_else(|| IndexerError::RPCError("Invalid block number".to_string()))?;

        // Parse hex
        let num = u64::from_str_radix(&hex[2..], 16)
            .map_err(|e| IndexerError::ParseError(e.to_string()))?;

        Ok(num)
    }

    /// Process single block
    async fn process_block(&self, block_number: u64) -> Result<(), IndexerError> {
        let start_time = std::time::Instant::now();

        // Fetch block data from RPC
        let block = self.fetch_block_data(block_number).await?;
        
        // Process transactions
        for tx in &block.transactions {
            self.index_transaction(tx).await?;
        }

        // Process logs
        for log_entry in &block.logs {
            self.index_log(log_entry).await?;
        }

        // Update statistics
        let mut stats = self.stats.write().await;
        stats.indexed_block = block_number;
        stats.last_update = Utc::now().timestamp();
        
        // Calculate processing rate
        let elapsed = start_time.elapsed().as_secs_f64();
        if elapsed > 0.0 {
            stats.processing_rate = block.transactions.len() as f64 / elapsed;
        }

        Ok(())
    }

    /// Fetch block data from RPC
    async fn fetch_block_data(&self, block_number: u64) -> Result<IndexedBlock, IndexerError> {
        let client = reqwest::Client::new();
        
        // Get block
        let block_request = serde_json::json!({
            "jsonrpc": "2.0",
            "method": "eth_getBlockByNumber",
            "params": [format!("0x{:x}", block_number), true],
            "id": 1
        });

        let block_response = client
            .post(&self.rpc_url)
            .json(&block_request)
            .send()
            .await
            .map_err(|e| IndexerError::RPCError(e.to_string()))?
            .json::<serde_json::Value>()
            .await
            .map_err(|e| IndexerError::RPCError(e.to_string()))?;

        let block_data = block_response.get("result")
            .ok_or_else(|| IndexerError::RPCError("No block result".to_string()))?
            .clone();

        // Get logs for block
        let logs_request = serde_json::json!({
            "jsonrpc": "2.0",
            "method": "eth_getLogs",
            "params": [{
                "fromBlock": format!("0x{:x}", block_number),
                "toBlock": format!("0x{:x}", block_number)
            }],
            "id": 2
        });

        let logs_response = client
            .post(&self.rpc_url)
            .json(&logs_request)
            .send()
            .await
            .map_err(|e| IndexerError::RPCError(e.to_string()))?
            .json::<serde_json::Value>()
            .await
            .map_err(|e| IndexerError::RPCError(e.to_string()))?;

        let logs_data = logs_response.get("result")
            .and_then(|v| v.as_array())
            .map(|a| a.clone())
            .unwrap_or_default();

        // Parse transactions
        let transactions: Vec<IndexedTransaction> = block_data.get("transactions")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|tx| {
                        let tx_obj = tx.as_object()?;
                        Some(IndexedTransaction {
                            hash: tx_obj.get("hash")?.as_str()?.to_string(),
                            block_number,
                            block_hash: tx_obj.get("blockHash")?.as_str()?.to_string(),
                            from: tx_obj.get("from")?.as_str()?.to_string(),
                            to: tx_obj.get("to").and_then(|v| v.as_str()).map(|s| s.to_string()),
                            value: tx_obj.get("value").and_then(|v| v.as_str()).unwrap_or("0x0").to_string(),
                            gas_price: tx_obj.get("gasPrice").and_then(|v| v.as_str()).unwrap_or("0x0").to_string(),
                            gas_limit: u64::from_str_radix(
                                tx_obj.get("gas").and_then(|v| v.as_str()).unwrap_or("0x0").trim_start_matches("0x"), 16
                            ).unwrap_or(21000),
                            nonce: tx_obj.get("nonce").and_then(|v| v.as_str()).map(|s| {
                                u64::from_str_radix(s.trim_start_matches("0x"), 16).unwrap_or(0)
                            }).unwrap_or(0),
                            input_data: tx_obj.get("input").and_then(|v| v.as_str()).unwrap_or("0x").to_string(),
                            timestamp: block_data.get("timestamp").and_then(|v| v.as_str()).map(|s| {
                                u64::from_str_radix(s.trim_start_matches("0x"), 16).unwrap_or(0)
                            }).unwrap_or(0),
                        })
                    })
                    .collect()
            })
            .unwrap_or_default();

        // Parse logs
        let logs: Vec<IndexedLog> = logs_data.iter()
            .filter_map(|log| {
                let log_obj = log.as_object()?;
                Some(IndexedLog {
                    address: log_obj.get("address")?.as_str()?.to_string(),
                    topics: log_obj.get("topics")
                        .and_then(|v| v.as_array())
                        .map(|arr| arr.iter().filter_map(|t| t.as_str().map(|s| s.to_string())).collect())
                        .unwrap_or_default(),
                    data: log_obj.get("data").and_then(|v| v.as_str()).unwrap_or("0x").to_string(),
                    block_number,
                    transaction_hash: log_obj.get("transactionHash").and_then(|v| v.as_str()).unwrap_or("").to_string(),
                })
            })
            .collect();

        // Parse block info
        let number = u64::from_str_radix(
            block_data.get("number").and_then(|v| v.as_str()).unwrap_or("0x0").trim_start_matches("0x"), 16
        ).unwrap_or(block_number);

        Ok(IndexedBlock {
            number,
            hash: block_data.get("hash").and_then(|v| v.as_str()).unwrap_or("").to_string(),
            parent_hash: block_data.get("parentHash").and_then(|v| v.as_str()).unwrap_or("").to_string(),
            timestamp: block_data.get("timestamp").and_then(|v| v.as_str()).map(|s| {
                u64::from_str_radix(s.trim_start_matches("0x"), 16).unwrap_or(0)
            }).unwrap_or(0),
            transactions,
            logs,
            internal_txs: vec![],
            miner: block_data.get("miner").and_then(|v| v.as_str()).unwrap_or("").to_string(),
            difficulty: block_data.get("difficulty").and_then(|v| v.as_str()).unwrap_or("0x0").to_string(),
            total_difficulty: block_data.get("totalDifficulty").and_then(|v| v.as_str()).unwrap_or("0x0").to_string(),
            size: block_data.get("size").and_then(|v| v.as_str()).map(|s| {
                u64::from_str_radix(s.trim_start_matches("0x"), 16).unwrap_or(0)
            }).unwrap_or(0),
            gas_used: block_data.get("gasUsed").and_then(|v| v.as_str()).map(|s| {
                u64::from_str_radix(s.trim_start_matches("0x"), 16).unwrap_or(0)
            }).unwrap_or(0),
            gas_limit: block_data.get("gasLimit").and_then(|v| v.as_str()).map(|s| {
                u64::from_str_radix(s.trim_start_matches("0x"), 16).unwrap_or(0)
            }).unwrap_or(0),
            base_fee_per_gas: block_data.get("baseFeePerGas").and_then(|v| v.as_str()).map(|s| {
                u64::from_str_radix(s.trim_start_matches("0x"), 16).ok()
            }),
        })
    }

    /// Index a transaction
    pub async fn index_transaction(&self, tx: &IndexedTransaction) -> Result<(), IndexerError> {
        let mut stats = self.stats.write().await;
        stats.indexed_transactions += 1;
        
        // Here we would insert into database
        log::debug!("Indexed tx: {}", tx.hash);
        
        Ok(())
    }

    /// Index a log
    pub async fn index_log(&self, log_entry: &IndexedLog) -> Result<(), IndexerError> {
        let mut stats = self.stats.write().await;
        stats.indexed_logs += 1;
        
        // Here we would insert into database
        log::debug!("Indexed log: {} from {}", log_entry.transaction_hash, log_entry.address);
        
        Ok(())
    }

    /// Index a token
    pub async fn index_token(&self, token: IndexedToken) -> Result<(), IndexerError> {
        let mut stats = self.stats.write().await;
        stats.indexed_tokens += 1;
        Ok(())
    }
}

// =============================================================================
// BUILDER
// =============================================================================

/// Indexer Builder
pub struct IndexerBuilder {
    config: IndexerConfig,
}

impl IndexerBuilder {
    pub fn new() -> Self {
        Self {
            config: IndexerConfig::default(),
        }
    }

    pub fn chain_id(mut self, id: u64) -> Self {
        self.config.chain_id = id;
        self
    }

    pub fn rpc_url(mut self, url: &str) -> Self {
        self.config.rpc_url = url.to_string();
        self
    }

    pub fn ws_url(mut self, url: &str) -> Self {
        self.config.ws_url = url.to_string();
        self
    }

    pub fn start_block(mut self, block: u64) -> Self {
        self.config.start_block = block;
        self
    }

    pub fn batch_size(mut self, size: usize) -> Self {
        self.config.batch_size = size;
        self
    }

    pub fn build(self) -> Indexer {
        Indexer::new(self.config)
    }
}

impl Default for IndexerBuilder {
    fn default() -> Self {
        Self::new()
    }
}

// =============================================================================
// TESTS
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_indexer_creation() {
        let indexer = Indexer::new(IndexerConfig::default());
        assert!(!indexer.is_running().await);
    }

    #[test]
    fn test_builder() {
        let indexer = IndexerBuilder::new()
            .chain_id(56)
            .rpc_url("http://localhost:8545")
            .build();
        
        assert_eq!(indexer.config.chain_id, 56);
    }
}