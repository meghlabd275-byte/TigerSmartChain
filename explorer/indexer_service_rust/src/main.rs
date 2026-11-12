//! TigerScan Indexer Service
//! High-performance blockchain indexer with trace support

use std::sync::Arc;
use std::time::Duration;

use anyhow::Result;
use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use sqlx::postgres::PgPoolOptions;
use thiserror::Error;
use tokio::time::interval;
use tracing::{error, info};

// ============================================================================
// Error Types
// ============================================================================

/// Transaction status, matching the `tx_status` Postgres enum.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TxStatus {
    Pending,
    Success,
    Failed,
}

impl TxStatus {
    fn as_str(self) -> &'static str {
        match self {
            TxStatus::Pending => "pending",
            TxStatus::Success => "success",
            TxStatus::Failed => "failed",
        }
    }
}

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
    #[serde(deserialize_with = "de_u64_from_str")]
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
    #[serde(deserialize_with = "de_u64_from_str")]
    pub size: u64,
    #[serde(deserialize_with = "de_u64_from_str")]
    pub gas_limit: u64,
    #[serde(deserialize_with = "de_u64_from_str")]
    pub gas_used: u64,
    #[serde(deserialize_with = "de_u64_from_str")]
    pub timestamp: u64,
    #[serde(deserialize_with = "de_tx_hashes")]
    pub transactions: Vec<String>,
}

/// Postgres has no unsigned integers, so every `u64` read from JSON-RPC is
/// converted to the `i64` the driver can bind. Values above `i64::MAX` cannot
/// occur on chain (they would exceed the total wei supply), but the conversion
/// is checked rather than wrapping.
fn to_i64(value: u64) -> Result<i64, IndexerError> {
    i64::try_from(value)
        .map_err(|_| IndexerError::ParseError(format!("value {value} exceeds i64 range")))
}

/// JSON-RPC returns every numeric quantity as a `0x`-prefixed hex string.
fn de_u64_from_str<'de, D>(deserializer: D) -> Result<u64, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let value = serde_json::Value::deserialize(deserializer)?;
    match value {
        serde_json::Value::String(s) => {
            let trimmed = s.strip_prefix("0x").unwrap_or(&s);
            u64::from_str_radix(trimmed, 16).map_err(serde::de::Error::custom)
        }
        serde_json::Value::Number(n) => n
            .as_u64()
            .ok_or_else(|| serde::de::Error::custom("expected unsigned integer")),
        other => Err(serde::de::Error::custom(format!(
            "expected hex string or number, got {other}"
        ))),
    }
}

/// `eth_getBlockByNumber` returns `transactions` as a list of hashes unless
/// full objects are requested; accept both so the type cannot silently fail.
fn de_tx_hashes<'de, D>(deserializer: D) -> Result<Vec<String>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let value = serde_json::Value::deserialize(deserializer)?;
    let items = value
        .as_array()
        .ok_or_else(|| serde::de::Error::custom("expected an array of transactions"))?;
    items
        .iter()
        .map(|item| match item {
            serde_json::Value::String(hash) => Ok(hash.clone()),
            serde_json::Value::Object(obj) => obj
                .get("hash")
                .and_then(|h| h.as_str())
                .map(str::to_owned)
                .ok_or_else(|| serde::de::Error::custom("transaction object has no hash")),
            _ => Err(serde::de::Error::custom("unexpected transaction entry")),
        })
        .collect()
}

/// Transaction data
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Transaction {
    pub hash: String,
    #[serde(deserialize_with = "de_u64_from_str")]
    pub block_number: u64,
    pub block_hash: String,
    #[serde(deserialize_with = "de_u64_from_str")]
    pub transaction_index: u64,
    pub from: String,
    pub to: Option<String>,
    pub value: String,
    pub gas_price: String,
    #[serde(deserialize_with = "de_u64_from_str")]
    pub gas: u64,
    #[serde(deserialize_with = "de_u64_from_str")]
    pub nonce: u64,
    pub input: String,
    #[serde(deserialize_with = "de_u64_from_str")]
    pub v: u64,
    pub r: String,
    pub s: String,
}

/// Receipt data
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Receipt {
    pub transaction_hash: String,
    #[serde(deserialize_with = "de_u64_from_str")]
    pub block_number: u64,
    pub block_hash: String,
    pub contract_address: Option<String>,
    #[serde(deserialize_with = "de_u64_from_str")]
    pub cumulative_gas_used: u64,
    #[serde(deserialize_with = "de_u64_from_str")]
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
    #[serde(deserialize_with = "de_u64_from_str")]
    pub log_index: u64,
    /// Filled in from the receipt; `eth_getTransactionReceipt` does not repeat
    /// the hash on each log entry.
    #[serde(default)]
    pub transaction_hash: String,
    /// Block timestamp, injected by the indexer from the parent block.
    #[serde(default, deserialize_with = "de_opt_u64_lenient")]
    pub timestamp: u64,
}

fn de_opt_u64_lenient<'de, D>(deserializer: D) -> Result<u64, D::Error>
where
    D: serde::Deserializer<'de>,
{
    de_opt_u64_from_str(deserializer).map(|v| v.unwrap_or(0))
}

/// Trace data
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Trace {
    #[serde(deserialize_with = "de_u64_from_str")]
    pub block_number: u64,
    pub transaction_hash: String,
    #[serde(deserialize_with = "de_u64_from_str")]
    pub transaction_index: u64,
    pub from: String,
    pub to: Option<String>,
    pub value: String,
    #[serde(deserialize_with = "de_u64_from_str")]
    pub gas: u64,
    #[serde(deserialize_with = "de_u64_from_str")]
    pub gas_used: u64,
    pub input: String,
    pub output: String,
    pub call_type: String,
    pub error: Option<String>,
    #[serde(deserialize_with = "de_u32_from_str")]
    pub depth: u32,
    #[serde(deserialize_with = "de_u64_from_str")]
    pub trace_index: u64,
    #[serde(default, deserialize_with = "de_opt_u64_from_str")]
    pub parent_index: Option<u64>,
}

fn de_u32_from_str<'de, D>(deserializer: D) -> Result<u32, D::Error>
where
    D: serde::Deserializer<'de>,
{
    u32::try_from(de_u64_from_str(deserializer)?)
        .map_err(|_| serde::de::Error::custom("value exceeds u32 range"))
}

fn de_opt_u64_from_str<'de, D>(deserializer: D) -> Result<Option<u64>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let value = Option::<serde_json::Value>::deserialize(deserializer)?;
    match value {
        None | Some(serde_json::Value::Null) => Ok(None),
        Some(serde_json::Value::String(s)) => {
            let trimmed = s.strip_prefix("0x").unwrap_or(&s);
            u64::from_str_radix(trimmed, 16)
                .map(Some)
                .map_err(serde::de::Error::custom)
        }
        Some(serde_json::Value::Number(n)) => n
            .as_u64()
            .map(Some)
            .ok_or_else(|| serde::de::Error::custom("expected unsigned integer")),
        Some(other) => Err(serde::de::Error::custom(format!(
            "expected hex string or number, got {other}"
        ))),
    }
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

        Ok(Self {
            pool,
            rpc_url,
            start_block,
            batch_size,
            last_block: Arc::new(RwLock::new(0)),
            running: Arc::new(RwLock::new(false)),
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
        .bind(to_i64(block.number)?)
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
        .bind(to_i64(block.size)?)
        .bind(to_i64(block.gas_limit)?)
        .bind(to_i64(block.gas_used)?)
        .bind(to_i64(block.timestamp)?)
        .bind(serde_json::json!(&block.transactions))
        .execute(pool)
        .await
        .map_err(|e| IndexerError::DatabaseError(e.to_string()))?;

        // Index transactions
        for (idx, tx_hash) in block.transactions.iter().enumerate() {
            let tx_index = idx as u64;

            let tx = Self::get_transaction(rpc_url, tx_hash).await?;

            // The receipt carries the status and gas actually consumed. A
            // transaction with no receipt cannot be written with a status, so
            // it is skipped rather than stored as "pending" forever.
            let Some(receipt) = Self::get_receipt(rpc_url, tx_hash).await.ok() else {
                error!("No receipt for transaction {}; skipping", tx_hash);
                continue;
            };

            let status = if receipt.status {
                TxStatus::Success
            } else {
                TxStatus::Failed
            };

            Self::insert_transaction(pool, &tx, &receipt, status).await?;
            Self::insert_receipt(pool, &receipt).await?;

            // `eth_getTransactionReceipt` does not repeat the transaction hash
            // or the block timestamp on each log, so both are filled in here.
            for log in &receipt.logs {
                let mut log = log.clone();
                log.transaction_hash = tx_hash.clone();
                log.timestamp = block.timestamp;
                Self::insert_log(pool, &log, block_num, tx_index).await?;
            }

            // Traces come from the `trace` namespace, which is optional: a node
            // without it returns an error, and that must not abort the block.
            match Self::get_traces(rpc_url, tx_hash).await {
                Ok(traces) => {
                    for trace in &traces {
                        Self::insert_trace(pool, trace).await?;
                    }
                }
                Err(e) => {
                    info!("Traces unavailable for {}: {}", tx_hash, e);
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
    async fn insert_transaction(
        pool: &sqlx::PgPool,
        tx: &Transaction,
        receipt: &Receipt,
        status: TxStatus,
    ) -> Result<(), IndexerError> {
        sqlx::query(
            "INSERT INTO transactions (hash, nonce, block_hash, block_number, transaction_index, \
             from_address, to_address, value, gas_price, gas_limit, gas_used, input, v, r, s, \
             status, cumulative_gas_used, contract_address) \
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16, $17, $18) \
             ON CONFLICT (hash) DO NOTHING",
        )
        .bind(&tx.hash)
        .bind(to_i64(tx.nonce)?)
        .bind(&tx.block_hash)
        .bind(to_i64(tx.block_number)?)
        .bind(i32::try_from(tx.transaction_index).unwrap_or(i32::MAX))
        .bind(&tx.from)
        .bind(&tx.to)
        .bind(&tx.value)
        .bind(&tx.gas_price)
        .bind(to_i64(tx.gas)?)
        .bind(to_i64(receipt.gas_used)?)
        .bind(&tx.input)
        .bind(to_i64(tx.v)?)
        .bind(&tx.r)
        .bind(&tx.s)
        .bind(status.as_str())
        .bind(to_i64(receipt.cumulative_gas_used)?)
        .bind(&receipt.contract_address)
        .execute(pool)
        .await
        .map_err(|e| IndexerError::DatabaseError(e.to_string()))?;

        Ok(())
    }

    /// Insert receipt
    ///
    /// The schema stores receipt fields on `transactions` rather than in a
    /// separate table, so this updates the row written by `insert_transaction`.
    async fn insert_receipt(pool: &sqlx::PgPool, receipt: &Receipt) -> Result<(), IndexerError> {
        sqlx::query(
            "UPDATE transactions SET gas_used = $2, cumulative_gas_used = $3, \
             status = $4, contract_address = $5 WHERE hash = $1",
        )
        .bind(&receipt.transaction_hash)
        .bind(to_i64(receipt.gas_used)?)
        .bind(to_i64(receipt.cumulative_gas_used)?)
        .bind(if receipt.status { "success" } else { "failed" })
        .bind(&receipt.contract_address)
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
            "INSERT INTO logs (transaction_hash, block_number, address, topics, data, log_index, timestamp) \
             VALUES ($1, $2, $3, $4, $5, $6, $7) ON CONFLICT (transaction_hash, log_index) DO NOTHING",
        )
        .bind(&log.transaction_hash)
        .bind(to_i64(block_number)?)
        .bind(&log.address)
        .bind(serde_json::json!(&log.topics))
        .bind(&log.data)
        .bind(i32::try_from(log.log_index).unwrap_or(i32::MAX))
        .bind(to_i64(log.timestamp)?)
        .execute(pool)
        .await
        .map_err(|e| IndexerError::DatabaseError(e.to_string()))?;

        let _ = tx_index;
        Ok(())
    }

    /// Insert trace
    ///
    /// `traces` has no unique constraint, so repeated indexing of the same
    /// block would duplicate rows; the delete below keeps it idempotent.
    async fn insert_trace(pool: &sqlx::PgPool, trace: &Trace) -> Result<(), IndexerError> {
        sqlx::query(
            "DELETE FROM traces WHERE transaction_hash = $1 AND depth = $2 AND trace_index = $3",
        )
        .bind(&trace.transaction_hash)
        .bind(i32::try_from(trace.depth).unwrap_or(i32::MAX))
        .bind(to_i64(trace.trace_index)?)
        .execute(pool)
        .await
        .map_err(|e| IndexerError::DatabaseError(e.to_string()))?;

        sqlx::query(
            "INSERT INTO traces (transaction_hash, block_number, transaction_index, from_address, \
             to_address, call_type, value, gas, input, output, error, depth, trace_index) \
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13)",
        )
        .bind(&trace.transaction_hash)
        .bind(to_i64(trace.block_number)?)
        .bind(i32::try_from(trace.transaction_index).unwrap_or(i32::MAX))
        .bind(&trace.from)
        .bind(&trace.to)
        .bind(&trace.call_type)
        .bind(&trace.value)
        .bind(to_i64(trace.gas)?)
        .bind(&trace.input)
        .bind(&trace.output)
        .bind(&trace.error)
        .bind(i32::try_from(trace.depth).unwrap_or(i32::MAX))
        .bind(to_i64(trace.trace_index)?)
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