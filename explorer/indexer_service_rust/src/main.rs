//! TigerScan Indexer Service
//! High-performance blockchain indexer with trace support

use std::sync::Arc;
use std::time::Duration;

use anyhow::Result;
use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use sqlx::postgres::PgPoolOptions;
use thiserror::Error;
use tokio::sync::mpsc;
use tokio::time::interval;
use tracing::{error, info};

// ============================================================================
// Error Types
// ============================================================================

#[derive(Error, Debug)]
pub enum IndexerError {
    #[error("RPC error: {0}")]
    RpcError(String),
    #[error("Database error: {0}")]
    DatabaseError(String),
    #[error("Parse error: {0}")]
    ParseError(String),
}

// ============================================================================
// Data Models
// ============================================================================

/// Block data from RPC
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Block {
    pub number: u64,
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
    pub size: u64,
    pub gas_limit: u64,
    pub gas_used: u64,
    pub timestamp: u64,
    pub transactions: Vec<String>,
}

/// Transaction data
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Transaction {
    pub hash: String,
    pub block_number: u64,
    pub block_hash: String,
    pub transaction_index: u64,
    pub from: String,
    pub to: Option<String>,
    pub value: String,
    pub gas_price: String,
    pub gas: u64,
    pub nonce: u64,
    pub input: String,
    pub v: u64,
    pub r: String,
    pub s: String,
}

/// Receipt data
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Receipt {
    pub transaction_hash: String,
    pub block_number: u64,
    pub block_hash: String,
    pub contract_address: Option<String>,
    pub cumulative_gas_used: u64,
    pub gas_used: u64,
    pub logs: Vec<Log>,
    pub logs_bloom: String,
    pub status: bool,
}

/// Log data
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Log {
    pub address: String,
    pub topics: Vec<String>,
    pub data: String,
    pub log_index: u64,
}

/// Trace data
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Trace {
    pub block_number: u64,
    pub transaction_hash: String,
    pub transaction_index: u64,
    pub from: String,
    pub to: Option<String>,
    pub value: String,
    pub gas: u64,
    pub gas_used: u64,
    pub input: String,
    pub output: String,
    pub call_type: String,
    pub error: Option<String>,
    pub depth: u32,
    pub trace_index: u64,
    pub parent_index: Option<u64>,
}

// ============================================================================
// Indexer Service
// ============================================================================

pub struct IndexerService {
    pool: sqlx::PgPool,
    rpc_url: String,
    start_block: u64,
    batch_size: u64,
    last_block: Arc<RwLock<u64>>,
    running: Arc<RwLock<bool>>,
    shutdown_tx: mpsc::Sender<()>,
}

impl IndexerService {
    pub async fn new(
        rpc_url: String,
        start_block: u64,
        batch_size: u64,
    ) -> Result<Self, IndexerError> {
        let pool = PgPoolOptions::new()
            .max_connections(10)
            .connect("postgres://tigerscan:tigerscan@localhost:5432/tigerscan")
            .await
            .map_err(|e| IndexerError::DatabaseError(e.to_string()))?;

        let (shutdown_tx, _) = mpsc::channel::<()>(1);

        Ok(Self {
            pool,
            rpc_url,
            start_block,
            batch_size,
            last_block: Arc::new(RwLock::new(0)),
            running: Arc::new(RwLock::new(false)),
            shutdown_tx,
        })
    }

    /// Start the indexer
    pub async fn run(&self) -> Result<()> {
        info!("Starting indexer service");

        *self.running.write() = true;

        // Get last indexed block
        let last_indexed = self.get_last_indexed_block().await?;
        let start = if last_indexed > 0 { last_indexed + 1 } else { self.start_block };

        *self.last_block.write() = start;

        // Main indexing loop
        let pool = self.pool.clone();
        let rpc_url = self.rpc_url.clone();
        let batch_size = self.batch_size;
        let last_block = self.last_block.clone();
        let running = self.running.clone();

        tokio::spawn(async move {
            let mut timer = interval(Duration::from_secs(5));

            while *running.read() {
                timer.tick().await;

                let current_block = *last_block.read();

                // Get latest block from RPC
                let latest = match Self::get_latest_block(&rpc_url).await {
                    Ok(n) => n,
                    Err(e) => {
                        error!("Failed to get latest block: {}", e);
                        continue;
                    }
                };

                if current_block >= latest {
                    continue;
                }

                // Index batch
                let end = std::cmp::min(current_block + batch_size, latest);

                for block_num in current_block..end {
                    if let Err(e) = Self::index_block(&pool, &rpc_url, block_num).await {
                        error!("Failed to index block {}: {}", block_num, e);
                        break;
                    }

                    *last_block.write() = block_num + 1;
                }
            }
        });

        Ok(())
    }

    /// Stop the indexer
    pub fn stop(&self) {
        *self.running.write() = false;
    }

    /// Get latest block from RPC
    async fn get_latest_block(rpc_url: &str) -> Result<u64, IndexerError> {
        let client = reqwest::Client::new();

        let request = serde_json::json!({
            "jsonrpc": "2.0",
            "method": "eth_blockNumber",
            "params": [],
            "id": 1
        });

        let response = client
            .post(rpc_url)
            .json(&request)
            .send()
            .await
            .map_err(|e| IndexerError::RpcError(e.to_string()))?;

        let result: serde_json::Value = response
            .json()
            .await
            .map_err(|e| IndexerError::RpcError(e.to_string()))?;

        let block_hex = result["result"]
            .as_str()
            .ok_or_else(|| IndexerError::ParseError("Invalid block number".to_string()))?;

        let block_num = u64::from_str_radix(block_hex.trim_start_matches("0x"), 16)
            .map_err(|e| IndexerError::ParseError(e.to_string()))?;

        Ok(block_num)
    }

    /// Index a single block
    async fn index_block(
        pool: &sqlx::PgPool,
        rpc_url: &str,
        block_num: u64,
    ) -> Result<(), IndexerError> {
        // Get block data
        let block: Block = Self::rpc_call(
            rpc_url,
            "eth_getBlockByNumber",
            serde_json::json!([format!("0x{:x}", block_num), true]),
        )
        .await?;

        // Insert block
        sqlx::query(
            "INSERT INTO blocks (number, hash, parent_hash, nonce, sha3_uncles, logs_bloom, transactions_root, state_root, receipts_root, miner, difficulty, total_difficulty, size, gas_limit, gas_used, timestamp, transactions) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16, $17) ON CONFLICT (number) DO NOTHING",
        )
        .bind(block.number)
        .bind(&block.hash)
        .bind(&block.parent_hash)
        .bind(&block.nonce)
        .bind(&block.sha3_uncles)
        .bind(&block.logs_bloom)
        .bind(&block.transactions_root)
        .bind(&block.state_root)
        .bind(&block.receipts_root)
        .bind(&block.miner)
        .bind(&block.difficulty)
        .bind(&block.total_difficulty)
        .bind(block.size)
        .bind(block.gas_limit)
        .bind(block.gas_used)
        .bind(block.timestamp)
        .bind(serde_json::json!(&block.transactions))
        .execute(pool)
        .await
        .map_err(|e| IndexerError::DatabaseError(e.to_string()))?;

        // Index transactions
        for (idx, tx_hash) in block.transactions.iter().enumerate() {
            if let Ok(tx) = Self::get_transaction(rpc_url, tx_hash).await {
                Self::insert_transaction(pool, &tx).await?;
            }

            // Get receipt
            if let Ok(receipt) = Self::get_receipt(rpc_url, tx_hash).await {
                Self::insert_receipt(pool, &receipt).await?;

                // Index logs
                for log in receipt.logs {
                    Self::insert_log(pool, &log, block_num, idx as u64).await?;
                }
            }

            // Get traces
            if let Ok(traces) = Self::get_traces(rpc_url, tx_hash).await {
                for trace in traces {
                    Self::insert_trace(pool, &trace).await?;
                }
            }
        }

        Ok(())
    }

    /// Generic RPC call
    async fn rpc_call<T: for<'de> Deserialize<'de>>(
        rpc_url: &str,
        method: &str,
        params: serde_json::Value,
    ) -> Result<T, IndexerError> {
        let client = reqwest::Client::new();

        let request = serde_json::json!({
            "jsonrpc": "2.0",
            "method": method,
            "params": params,
            "id": 1
        });

        let response = client
            .post(rpc_url)
            .json(&request)
            .send()
            .await
            .map_err(|e| IndexerError::RpcError(e.to_string()))?;

        let result: serde_json::Value = response
            .json()
            .await
            .map_err(|e| IndexerError::RpcError(e.to_string()))?;

        serde_json::from_value(result["result"].clone())
            .map_err(|e| IndexerError::ParseError(e.to_string()))
    }

    /// Get transaction
    async fn get_transaction(rpc_url: &str, hash: &str) -> Result<Transaction, IndexerError> {
        Self::rpc_call(rpc_url, "eth_getTransactionByHash", serde_json::json!([hash])).await
    }

    /// Get receipt
    async fn get_receipt(rpc_url: &str, hash: &str) -> Result<Receipt, IndexerError> {
        Self::rpc_call(rpc_url, "eth_getTransactionReceipt", serde_json::json!([hash])).await
    }

    /// Get traces
    async fn get_traces(rpc_url: &str, hash: &str) -> Result<Vec<Trace>, IndexerError> {
        #[derive(Deserialize)]
        struct TraceResult {
            #[serde(rename = "result")]
            traces: Vec<Trace>,
        }

        let result: TraceResult = Self::rpc_call(rpc_url, "trace_transaction", serde_json::json!([hash])).await?;
        Ok(result.traces)
    }

    /// Insert transaction
    async fn insert_transaction(pool: &sqlx::PgPool, tx: &Transaction) -> Result<(), IndexerError> {
        sqlx::query(
            "INSERT INTO transactions (hash, block_number, block_hash, transaction_index, from_address, to_address, value, gas_price, gas, nonce, input, v, r, s, status) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15) ON CONFLICT (hash) DO NOTHING",
        )
        .bind(&tx.hash)
        .bind(tx.block_number)
        .bind(&tx.block_hash)
        .bind(tx.transaction_index)
        .bind(&tx.from)
        .bind(&tx.to)
        .bind(&tx.value)
        .bind(&tx.gas_price)
        .bind(tx.gas)
        .bind(tx.nonce)
        .bind(&tx.input)
        .bind(tx.v)
        .bind(&tx.r)
        .bind(&tx.s)
        .bind(if tx.status { "success" } else { "failure" })
        .execute(pool)
        .await
        .map_err(|e| IndexerError::DatabaseError(e.to_string()))?;

        Ok(())
    }

    /// Insert receipt
    async fn insert_receipt(pool: &sqlx::PgPool, receipt: &Receipt) -> Result<(), IndexerError> {
        sqlx::query(
            "INSERT INTO transaction_receipts (transaction_hash, block_number, block_hash, contract_address, cumulative_gas_used, gas_used, logs_bloom, status) VALUES ($1, $2, $3, $4, $5, $6, $7, $8) ON CONFLICT (transaction_hash) DO NOTHING",
        )
        .bind(&receipt.transaction_hash)
        .bind(receipt.block_number)
        .bind(&receipt.block_hash)
        .bind(&receipt.contract_address)
        .bind(receipt.cumulative_gas_used)
        .bind(receipt.gas_used)
        .bind(&receipt.logs_bloom)
        .bind(if receipt.status { "0x1" } else { "0x0" })
        .execute(pool)
        .await
        .map_err(|e| IndexerError::DatabaseError(e.to_string()))?;

        Ok(())
    }

    /// Insert log
    async fn insert_log(
        pool: &sqlx::PgPool,
        log: &Log,
        block_number: u64,
        tx_index: u64,
    ) -> Result<(), IndexerError> {
        sqlx::query(
            "INSERT INTO logs (block_number, transaction_hash, log_index, address, topics, data) VALUES ($1, $2, $3, $4, $5, $6) ON CONFLICT DO NOTHING",
        )
        .bind(block_number)
        .bind(&log.address)
        .bind(log.log_index)
        .bind(&log.address)
        .bind(serde_json::json!(&log.topics))
        .bind(&log.data)
        .execute(pool)
        .await
        .map_err(|e| IndexerError::DatabaseError(e.to_string()))?;

        Ok(())
    }

    /// Insert trace
    async fn insert_trace(pool: &sqlx::PgPool, trace: &Trace) -> Result<(), IndexerError> {
        sqlx::query(
            "INSERT INTO traces (block_number, transaction_hash, transaction_index, from_address, to_address, value, gas, gas_used, input, output, call_type, error, depth, trace_index, parent_index) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15) ON CONFLICT DO NOTHING",
        )
        .bind(trace.block_number)
        .bind(&trace.transaction_hash)
        .bind(trace.transaction_index)
        .bind(&trace.from)
        .bind(&trace.to)
        .bind(&trace.value)
        .bind(trace.gas)
        .bind(trace.gas_used)
        .bind(&trace.input)
        .bind(&trace.output)
        .bind(&trace.call_type)
        .bind(&trace.error)
        .bind(trace.depth)
        .bind(trace.trace_index)
        .bind(&trace.parent_index)
        .execute(pool)
        .await
        .map_err(|e| IndexerError::DatabaseError(e.to_string()))?;

        Ok(())
    }

    /// Get last indexed block
    async fn get_last_indexed_block(&self) -> Result<u64, IndexerError> {
        let result: Option<(i64,)> = sqlx::query_as(
            "SELECT MAX(number) FROM blocks",
        )
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| IndexerError::DatabaseError(e.to_string()))?;

        Ok(result.map(|(n,)| n as u64).unwrap_or(0))
    }
}

// ============================================================================
// Main
// ============================================================================

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt::init();
    info!("Starting TigerScan Indexer Service");

    let rpc_url = std::env::var("RPC_URL")
        .unwrap_or_else(|_| "http://localhost:8545".to_string());
    let start_block = std::env::var("START_BLOCK")
        .unwrap_or_else(|_| "0".to_string())
        .parse()
        .unwrap_or(0);
    let batch_size = std::env::var("BATCH_SIZE")
        .unwrap_or_else(|_| "10".to_string())
        .parse()
        .unwrap_or(10);

    let service = IndexerService::new(rpc_url, start_block, batch_size).await?;
    service.run().await?;

    // Wait for shutdown
    tokio::signal::ctrl_c().await?;

    service.stop();

    Ok(())
}