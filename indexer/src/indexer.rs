//! Indexer for TigerScan - Full implementation

use crate::types::*;
use chrono::Utc;
use sqlx::postgres::PgPool;
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


/// Decode a `0x`-prefixed hex quantity.
///
/// Every numeric field in a JSON-RPC reply arrives as hex, and the previous
/// code indexed `[2..]` without checking the prefix, so a value that did not
/// start with `0x` panicked instead of being rejected.
pub(crate) fn parse_hex_u64(value: &str) -> Result<u64, IndexerError> {
    let trimmed = value
        .strip_prefix("0x")
        .or_else(|| value.strip_prefix("0X"))
        .unwrap_or(value);

    u64::from_str_radix(trimmed, 16)
        .map_err(|e| IndexerError::ParseError(format!("{value} is not a hex quantity: {e}")))
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
    /// Database pool. `None` until `connect` succeeds, so a misconfigured
    /// database is reported once instead of failing every insert.
    pool: Option<PgPool>,
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
            pool: None,
        }
    }

    /// Attach a database pool.
    ///
    /// Without a pool the indexer can read the chain but cannot store anything,
    /// so `start` fails rather than reporting progress it is not making.
    pub fn with_pool(mut self, pool: PgPool) -> Self {
        self.pool = Some(pool);
        self
    }

    /// Connect to the database named by `DATABASE_URL`.
    pub async fn connect_database(&mut self) -> Result<(), IndexerError> {
        let url = std::env::var("DATABASE_URL").map_err(|_| {
            IndexerError::DatabaseError("DATABASE_URL is not set".to_string())
        })?;

        let pool = sqlx::postgres::PgPoolOptions::new()
            .max_connections(10)
            .connect(&url)
            .await
            .map_err(|e| IndexerError::DatabaseError(e.to_string()))?;

        self.pool = Some(pool);
        Ok(())
    }

    /// Start indexing
    pub async fn start(&self) -> Result<(), IndexerError> {
        if self.pool.is_none() {
            return Err(IndexerError::DatabaseError(
                "no database pool; call connect_database() first".to_string(),
            ));
        }

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
        let mut poll_interval = interval(Duration::from_secs(12));

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
        parse_hex_u64(hex)
    }

    /// Process single block
    async fn process_block(&self, block_number: u64) -> Result<(), IndexerError> {
        let start_time = std::time::Instant::now();

        // Fetch block data from RPC
        let block = self.fetch_block_data(block_number).await?;
        
        // Persist the block first so the transactions it owns have a parent row.
        self.index_block(&block).await?;

        // Process transactions
        for tx in &block.transactions {
            self.index_transaction(tx).await?;
        }

        // Process logs
        for log_entry in &block.logs {
            self.index_log(log_entry, block.timestamp).await?;
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
                            transaction_index: parse_hex_u64(
                                tx_obj.get("transactionIndex").and_then(|v| v.as_str()).unwrap_or("0x0"),
                            )
                            .unwrap_or(0),
                            value: tx_obj.get("value").and_then(|v| v.as_str()).unwrap_or("0x0").to_string(),
                            gas_price: parse_hex_u64(
                                tx_obj.get("gasPrice").and_then(|v| v.as_str()).unwrap_or("0x0"),
                            )
                            .unwrap_or(0),
                            gas_used: parse_hex_u64(
                                tx_obj.get("gas").and_then(|v| v.as_str()).unwrap_or("0x0"),
                            )
                            .unwrap_or(0),
                            nonce: parse_hex_u64(
                                tx_obj.get("nonce").and_then(|v| v.as_str()).unwrap_or("0x0"),
                            )
                            .unwrap_or(0),
                            input: tx_obj.get("input").and_then(|v| v.as_str()).unwrap_or("0x").to_string(),
                            // The receipt carries the final status; a transaction
                            // seen inside a block is not pending.
                            status: TransactionStatus::Success,
                            logs: Vec::new(),
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
                    log_index: parse_hex_u64(
                        log_obj.get("logIndex").and_then(|v| v.as_str()).unwrap_or("0x0"),
                    )
                    .unwrap_or(0),
                    block_number,
                    transaction_hash: log_obj.get("transactionHash").and_then(|v| v.as_str()).unwrap_or("").to_string(),
                })
            })
            .collect();

        // Parse block info
        let number = block_data
            .get("number")
            .and_then(|v| v.as_str())
            .map(parse_hex_u64)
            .transpose()?
            .unwrap_or(block_number);

        Ok(IndexedBlock {
            number,
            hash: block_data.get("hash").and_then(|v| v.as_str()).unwrap_or("").to_string(),
            parent_hash: block_data.get("parentHash").and_then(|v| v.as_str()).unwrap_or("").to_string(),
            timestamp: block_data
                .get("timestamp")
                .and_then(|v| v.as_str())
                .map(parse_hex_u64)
                .transpose()?
                .unwrap_or(0) as i64,
            transactions,
            logs,
            internal_txs: vec![],
            miner: block_data.get("miner").and_then(|v| v.as_str()).unwrap_or("").to_string(),
            difficulty: block_data.get("difficulty").and_then(|v| v.as_str()).unwrap_or("0x0").to_string(),
            total_difficulty: block_data.get("totalDifficulty").and_then(|v| v.as_str()).unwrap_or("0x0").to_string(),
            size: block_data
                .get("size")
                .and_then(|v| v.as_str())
                .map(parse_hex_u64)
                .transpose()?
                .unwrap_or(0),
            gas_used: block_data
                .get("gasUsed")
                .and_then(|v| v.as_str())
                .map(parse_hex_u64)
                .transpose()?
                .unwrap_or(0),
            gas_limit: block_data
                .get("gasLimit")
                .and_then(|v| v.as_str())
                .map(parse_hex_u64)
                .transpose()?
                .unwrap_or(0),
            base_fee_per_gas: block_data
                .get("baseFeePerGas")
                .and_then(|v| v.as_str())
                .map(parse_hex_u64)
                .transpose()?,
        })
    }

    /// Persist a block.
    ///
    /// The block is written before its transactions because `transactions`
    /// references `blocks(number)`; writing the children first would violate
    /// the foreign key.
    async fn index_block(&self, block: &IndexedBlock) -> Result<(), IndexerError> {
        let pool = self.pool.as_ref().ok_or(IndexerError::NotRunning)?;

        sqlx::query(
            "INSERT INTO blocks (number, hash, parent_hash, miner, difficulty, \
             total_difficulty, size, gas_limit, gas_used, timestamp, base_fee_per_gas, tx_count) \
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12) \
             ON CONFLICT (number) DO UPDATE SET \
                 hash = EXCLUDED.hash, gas_used = EXCLUDED.gas_used, \
                 tx_count = EXCLUDED.tx_count, updated_at = NOW()",
        )
        .bind(block.number as i64)
        .bind(&block.hash)
        .bind(&block.parent_hash)
        .bind(&block.miner)
        .bind(&block.difficulty)
        .bind(&block.total_difficulty)
        .bind(block.size as i64)
        .bind(block.gas_limit as i64)
        .bind(block.gas_used as i64)
        .bind(block.timestamp)
        .bind(block.base_fee_per_gas.map(|v| v as i64))
        .bind(block.transactions.len() as i32)
        .execute(pool)
        .await
        .map_err(|e| IndexerError::DatabaseError(e.to_string()))?;

        Ok(())
    }

    /// Persist a transaction.
    pub async fn index_transaction(&self, tx: &IndexedTransaction) -> Result<(), IndexerError> {
        let pool = self.pool.as_ref().ok_or(IndexerError::NotRunning)?;

        let status = match tx.status {
            TransactionStatus::Pending => "pending",
            TransactionStatus::Success => "success",
            TransactionStatus::Failed => "failed",
        };

        sqlx::query(
            "INSERT INTO transactions (hash, nonce, block_hash, block_number, transaction_index, \
             from_address, to_address, value, gas_price, gas_limit, gas_used, input, status) \
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13) \
             ON CONFLICT (hash) DO UPDATE SET block_number = EXCLUDED.block_number, \
                 status = EXCLUDED.status",
        )
        .bind(&tx.hash)
        .bind(tx.nonce as i64)
        .bind(&tx.block_hash)
        .bind(tx.block_number as i64)
        .bind(tx.transaction_index as i32)
        .bind(&tx.from)
        .bind(&tx.to)
        .bind(&tx.value)
        .bind(tx.gas_price as i64)
        .bind(tx.gas_used as i64)
        .bind(tx.gas_used as i64)
        .bind(&tx.input)
        .bind(status)
        .execute(pool)
        .await
        .map_err(|e| IndexerError::DatabaseError(e.to_string()))?;

        let mut stats = self.stats.write().await;
        stats.indexed_transactions += 1;

        Ok(())
    }

    /// Persist a log.
    pub async fn index_log(
        &self,
        log_entry: &IndexedLog,
        block_timestamp: i64,
    ) -> Result<(), IndexerError> {
        let pool = self.pool.as_ref().ok_or(IndexerError::NotRunning)?;

        sqlx::query(
            "INSERT INTO logs (transaction_hash, block_number, address, topics, data, log_index, timestamp) \
             VALUES ($1, $2, $3, $4, $5, $6, $7) \
             ON CONFLICT (transaction_hash, log_index) DO NOTHING",
        )
        .bind(&log_entry.transaction_hash)
        .bind(log_entry.block_number as i64)
        .bind(&log_entry.address)
        .bind(serde_json::json!(&log_entry.topics))
        .bind(&log_entry.data)
        .bind(log_entry.log_index as i32)
        .bind(block_timestamp)
        .execute(pool)
        .await
        .map_err(|e| IndexerError::DatabaseError(e.to_string()))?;

        let mut stats = self.stats.write().await;
        stats.indexed_logs += 1;

        Ok(())
    }

    /// Persist a token.
    pub async fn index_token(&self, token: IndexedToken) -> Result<(), IndexerError> {
        let pool = self.pool.as_ref().ok_or(IndexerError::NotRunning)?;

        sqlx::query(
            "INSERT INTO tokens (address, name, symbol, decimals, total_supply) \
             VALUES ($1, $2, $3, $4, $5) ON CONFLICT (address) DO NOTHING",
        )
        .bind(&token.address)
        .bind(&token.name)
        .bind(&token.symbol)
        .bind(token.decimals as i16)
        .bind(&token.total_supply)
        .execute(pool)
        .await
        .map_err(|e| IndexerError::DatabaseError(e.to_string()))?;

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
    fn parses_prefixed_hex_quantities() {
        assert_eq!(parse_hex_u64("0x0").unwrap(), 0);
        assert_eq!(parse_hex_u64("0x1").unwrap(), 1);
        assert_eq!(parse_hex_u64("0xde0b6b3a7640000").unwrap(), 1_000_000_000_000_000_000);
        assert_eq!(parse_hex_u64("0xffffffffffffffff").unwrap(), u64::MAX);
    }

    #[test]
    fn rejects_values_that_are_not_quantities() {
        // An empty quantity, a non-hex digit and a value wider than u64 all have
        // to be reported; silently substituting a default is what hid these
        // fields before.
        assert!(parse_hex_u64("0x").is_err());
        assert!(parse_hex_u64("0xzz").is_err());
        assert!(parse_hex_u64("0x1ffffffffffffffff").is_err());
    }

    #[test]
    fn accepts_quantities_without_a_prefix() {
        assert_eq!(parse_hex_u64("ff").unwrap(), 255);
    }

    #[test]
    fn default_chain_id_is_tigersmartchain() {
        assert_eq!(IndexerConfig::default().chain_id, 6666);
    }

    #[test]
    fn builder_overrides_configuration() {
        let indexer = IndexerBuilder::new()
            .chain_id(6666)
            .rpc_url("http://localhost:8545")
            .start_block(42)
            .batch_size(10)
            .build();

        assert_eq!(indexer.config.chain_id, 6666);
        assert_eq!(indexer.config.start_block, 42);
        assert_eq!(indexer.config.batch_size, 10);
    }

    #[tokio::test]
    async fn a_new_indexer_is_not_running() {
        let indexer = Indexer::new(IndexerConfig::default());
        assert!(!indexer.is_running().await);
    }

    #[tokio::test]
    async fn starting_without_a_database_is_refused() {
        // Reporting progress without anywhere to write it is worse than failing,
        // so `start` must refuse rather than pretend.
        let indexer = Indexer::new(IndexerConfig::default());
        let error = indexer.start().await.expect_err("start must fail without a pool");

        assert!(matches!(error, IndexerError::DatabaseError(_)));
    }
}
