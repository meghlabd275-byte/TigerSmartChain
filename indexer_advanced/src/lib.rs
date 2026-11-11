/**
 * Advanced Blockchain Indexer - High-performance indexing with full trace support
 * Rust-based for ultra-low latency and maximum throughput
 */

use std::sync::Arc;

use ethers::providers::{Http, Middleware, Provider, Ws};
use ethers::types::{Block, CallFrame, GethTrace, GethTraceFrame, Transaction, TransactionReceipt, U256};
use futures::StreamExt;
use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use sqlx::postgres::PgPool;
use thiserror::Error;
use tokio::sync::mpsc;
use tokio::time::{interval, Duration};
use tracing::{debug, error, info, warn};

// ============================================
// Error Types
// ============================================

#[derive(Error, Debug)]
pub enum IndexerError {
    #[error("Database error: {0}")]
    Database(#[from] sqlx::Error),
    
    #[error("RPC error: {0}")]
    Rpc(String),
    
    #[error("Parse error: {0}")]
    Parse(String),
    
    #[error("Configuration error: {0}")]
    Config(String),
    
    #[error("Queue error: {0}")]
    Queue(String),
}

pub type Result<T> = std::result::Result<T, IndexerError>;

// ============================================
// Configuration
// ============================================

#[derive(Debug, Clone)]
pub struct IndexerConfig {
    pub rpc_url: String,
    /// Endpoint used for `debug_traceTransaction`. Tracing is far heavier than
    /// ordinary reads, so operators normally point this at a dedicated
    /// archive/trace node and leave it empty on ordinary nodes.
    pub trace_rpc_url: String,
    pub ws_url: String,
    pub database_url: String,
    pub chain_id: u64,
    pub start_block: u64,
    pub batch_size: u64,
    pub confirmation_blocks: u64,
    pub index_traces: bool,
    pub index_logs: bool,
    pub index_transfers: bool,
    /// Comma-separated `address:topic0` allowlist for log indexing. Empty means
    /// index every log, which on a busy chain is far too much data to store.
    pub log_addresses: Vec<String>,
}

impl IndexerConfig {
    pub fn from_env() -> Result<Self> {
        Ok(Self {
            rpc_url: std::env::var("RPC_URL")
                .map_err(|_| IndexerError::Config("RPC_URL not set".into()))?,
            trace_rpc_url: std::env::var("TRACE_RPC_URL").unwrap_or_default(),
            ws_url: std::env::var("WS_URL")
                .unwrap_or_else(|_| "ws://localhost:8546".to_string()),
            database_url: std::env::var("DATABASE_URL")
                .map_err(|_| IndexerError::Config("DATABASE_URL not set".into()))?,
            chain_id: std::env::var("CHAIN_ID")
                .map(|v| v.parse().unwrap_or(6666))
                .unwrap_or(6666),
            start_block: std::env::var("START_BLOCK")
                .map(|v| v.parse().unwrap_or(0))
                .unwrap_or(0),
            batch_size: std::env::var("BATCH_SIZE")
                .map(|v| v.parse().unwrap_or(100))
                .unwrap_or(100),
            confirmation_blocks: std::env::var("CONFIRMATION_BLOCKS")
                .map(|v| v.parse().unwrap_or(12))
                .unwrap_or(12),
            index_traces: true,
            index_logs: true,
            index_transfers: true,
            log_addresses: std::env::var("INDEX_LOG_ADDRESSES")
                .ok()
                .map(|v| {
                    v.split(',')
                        .map(|s| s.trim().to_ascii_lowercase())
                        .filter(|s| !s.is_empty())
                        .collect()
                })
                .unwrap_or_default(),
        })
    }
}

// ============================================
// Data Models
// ============================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IndexedBlock {
    pub number: i64,
    pub hash: String,
    pub parent_hash: String,
    pub timestamp: i64,
    pub miner: String,
    pub gas_limit: i64,
    pub gas_used: i64,
    pub transaction_count: i32,
    pub size: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IndexedTransaction {
    pub hash: String,
    pub block_number: i64,
    pub block_hash: String,
    pub timestamp: i64,
    pub from_address: String,
    pub to_address: Option<String>,
    pub value: String,
    pub gas_price: String,
    pub gas_used: i64,
    pub input: String,
    pub status: Option<bool>,
    pub logs: Vec<IndexedLog>,
    pub traces: Vec<IndexedTrace>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IndexedLog {
    pub address: String,
    pub topics: Vec<String>,
    pub data: String,
    pub log_index: i32,
    pub transaction_hash: String,
    pub block_number: i64,
    /// Position of the emitting transaction inside its block.
    pub tx_index: i32,
    /// Address that submitted the emitting transaction.
    pub from_address: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IndexedTrace {
    pub trace_address: String,
    /// Index of the transaction inside its block.
    pub tx_index: i32,
    pub call_type: String,
    pub action_from: String,
    pub action_to: Option<String>,
    pub action_value: Option<String>,
    pub action_input: String,
    pub result_gas: Option<i64>,
    pub result_output: String,
    pub subtraces: i32,
    pub transaction_hash: String,
    pub block_number: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenTransfer {
    pub transaction_hash: String,
    pub block_number: i64,
    pub timestamp: i64,
    pub from_address: String,
    pub to_address: String,
    pub token_address: String,
    pub token_id: Option<String>,
    pub value: String,
    pub log_index: i32,
    pub tx_index: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Contract {
    pub address: String,
    pub bytecode: String,
    pub balance: String,
    pub nonce: i64,
    pub code_hash: String,
    pub is_contract: bool,
    pub first_seen_block: i64,
    pub last_seen_block: i64,
}

// ============================================
// Database Operations
// ============================================

pub struct Database {
    pool: PgPool,
}

impl Database {
    pub async fn new(url: &str) -> Result<Self> {
        let pool = PgPool::connect(url).await?;
        Ok(Self { pool })
    }

    pub async fn init_schema(&self) -> Result<()> {
        // Create tables
        sqlx::query(r#"
            CREATE TABLE IF NOT EXISTS blocks (
                number BIGINT PRIMARY KEY,
                hash VARCHAR(66) NOT NULL,
                parent_hash VARCHAR(66) NOT NULL,
                timestamp BIGINT NOT NULL,
                miner VARCHAR(42) NOT NULL,
                gas_limit BIGINT NOT NULL,
                gas_used BIGINT NOT NULL,
                transaction_count INT NOT NULL,
                size INT NOT NULL,
                created_at TIMESTAMP DEFAULT NOW()
            );
            
            CREATE TABLE IF NOT EXISTS transactions (
                hash VARCHAR(66) PRIMARY KEY,
                block_number BIGINT NOT NULL,
                block_hash VARCHAR(66) NOT NULL,
                timestamp BIGINT NOT NULL,
                from_address VARCHAR(42) NOT NULL,
                to_address VARCHAR(42),
                value VARCHAR(78) NOT NULL,
                gas_price VARCHAR(78) NOT NULL,
                gas_used BIGINT NOT NULL,
                input TEXT NOT NULL,
                status BOOLEAN,
                created_at TIMESTAMP DEFAULT NOW()
            );
            
            CREATE TABLE IF NOT EXISTS logs (
                id SERIAL PRIMARY KEY,
                address VARCHAR(42) NOT NULL,
                topics TEXT NOT NULL,
                data TEXT NOT NULL,
                log_index INT NOT NULL,
                transaction_hash VARCHAR(66) NOT NULL,
                block_number BIGINT NOT NULL,
                tx_index INT NOT NULL DEFAULT 0,
                from_address VARCHAR(42) NOT NULL DEFAULT '',
                created_at TIMESTAMP DEFAULT NOW(),
                UNIQUE (transaction_hash, log_index)
            );
            
            CREATE TABLE IF NOT EXISTS traces (
                id SERIAL PRIMARY KEY,
                trace_address TEXT NOT NULL,
                tx_index INT NOT NULL DEFAULT 0,
                call_type VARCHAR(32) NOT NULL,
                action_from VARCHAR(42) NOT NULL,
                action_to VARCHAR(42),
                action_value VARCHAR(78),
                action_input TEXT NOT NULL,
                result_gas BIGINT,
                result_output TEXT NOT NULL,
                subtraces INT NOT NULL,
                transaction_hash VARCHAR(66) NOT NULL,
                block_number BIGINT NOT NULL,
                created_at TIMESTAMP DEFAULT NOW()
            );
            
            CREATE TABLE IF NOT EXISTS token_transfers (
                id SERIAL PRIMARY KEY,
                transaction_hash VARCHAR(66) NOT NULL,
                block_number BIGINT NOT NULL,
                timestamp BIGINT NOT NULL,
                from_address VARCHAR(42) NOT NULL,
                to_address VARCHAR(42) NOT NULL,
                token_address VARCHAR(42) NOT NULL,
                token_id VARCHAR(78),
                value VARCHAR(78) NOT NULL,
                log_index INT NOT NULL,
                tx_index INT NOT NULL DEFAULT 0,
                created_at TIMESTAMP DEFAULT NOW(),
                UNIQUE (transaction_hash, log_index)
            );
            
            CREATE TABLE IF NOT EXISTS contracts (
                address VARCHAR(42) PRIMARY KEY,
                bytecode TEXT NOT NULL,
                balance VARCHAR(78) NOT NULL,
                nonce BIGINT NOT NULL,
                code_hash VARCHAR(66) NOT NULL,
                is_contract BOOLEAN NOT NULL,
                first_seen_block BIGINT NOT NULL,
                last_seen_block BIGINT NOT NULL,
                created_at TIMESTAMP DEFAULT NOW()
            );
            
            CREATE INDEX IF NOT EXISTS idx_transactions_block ON transactions(block_number);
            CREATE INDEX IF NOT EXISTS idx_transactions_from ON transactions(from_address);
            CREATE INDEX IF NOT EXISTS idx_transactions_to ON transactions(to_address);
            CREATE INDEX IF NOT EXISTS idx_logs_block ON logs(block_number);
            CREATE INDEX IF NOT EXISTS idx_logs_address ON logs(address);
            CREATE INDEX IF NOT EXISTS idx_traces_block ON traces(block_number);
            CREATE INDEX IF NOT EXISTS idx_traces_tx ON traces(transaction_hash);
            CREATE INDEX IF NOT EXISTS idx_token_transfers_block ON token_transfers(block_number);
            CREATE INDEX IF NOT EXISTS idx_token_transfers_token ON token_transfers(token_address);
            CREATE INDEX IF NOT EXISTS idx_contracts_code ON contracts(code_hash);
            CREATE INDEX IF NOT EXISTS idx_logs_tx ON logs(transaction_hash);
            CREATE INDEX IF NOT EXISTS idx_traces_address ON traces(action_from);
            CREATE INDEX IF NOT EXISTS idx_token_transfers_from ON token_transfers(from_address);
            CREATE INDEX IF NOT EXISTS idx_token_transfers_to ON token_transfers(to_address);
            CREATE INDEX IF NOT EXISTS idx_blocks_timestamp ON blocks(timestamp);
        "#).execute(&self.pool).await?;

        // Additive migrations. `ADD COLUMN IF NOT EXISTS` is idempotent, so
        // re-running them on an existing database is a no-op.
        for stmt in [
            "ALTER TABLE logs ADD COLUMN IF NOT EXISTS tx_index INT NOT NULL DEFAULT 0",
            "ALTER TABLE logs ADD COLUMN IF NOT EXISTS from_address VARCHAR(42) NOT NULL DEFAULT ''",
            "ALTER TABLE traces ADD COLUMN IF NOT EXISTS tx_index INT NOT NULL DEFAULT 0",
            "ALTER TABLE token_transfers ADD COLUMN IF NOT EXISTS tx_index INT NOT NULL DEFAULT 0",
        ] {
            sqlx::query(stmt).execute(&self.pool).await?;
        }

        Ok(())
    }

    pub async fn insert_block(&self, block: &IndexedBlock) -> Result<()> {
        sqlx::query(r#"
            INSERT INTO blocks (number, hash, parent_hash, timestamp, miner, gas_limit, gas_used, transaction_count, size)
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)
            ON CONFLICT (number) DO UPDATE SET
                hash = EXCLUDED.hash,
                gas_used = EXCLUDED.gas_used,
                transaction_count = EXCLUDED.transaction_count
        "#)
        .bind(block.number)
        .bind(&block.hash)
        .bind(&block.parent_hash)
        .bind(block.timestamp)
        .bind(&block.miner)
        .bind(block.gas_limit)
        .bind(block.gas_used)
        .bind(block.transaction_count)
        .bind(block.size)
        .execute(&self.pool)
        .await?;
        
        Ok(())
    }

    pub async fn insert_transaction(&self, tx: &IndexedTransaction) -> Result<()> {
        sqlx::query(r#"
            INSERT INTO transactions (hash, block_number, block_hash, timestamp, from_address, to_address, value, gas_price, gas_used, input, status)
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11)
            ON CONFLICT (hash) DO UPDATE SET
                status = EXCLUDED.status,
                gas_used = EXCLUDED.gas_used
        "#)
        .bind(&tx.hash)
        .bind(tx.block_number)
        .bind(&tx.block_hash)
        .bind(tx.timestamp)
        .bind(&tx.from_address)
        .bind(&tx.to_address)
        .bind(&tx.value)
        .bind(&tx.gas_price)
        .bind(tx.gas_used)
        .bind(&tx.input)
        .bind(tx.status)
        .execute(&self.pool)
        .await?;
        
        Ok(())
    }

    pub async fn insert_logs(&self, logs: &[IndexedLog]) -> Result<()> {
        for log in logs {
            sqlx::query(r#"
                INSERT INTO logs (address, topics, data, log_index, transaction_hash, block_number, tx_index, from_address)
                VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
                ON CONFLICT (transaction_hash, log_index) DO NOTHING
            "#)
            .bind(&log.address)
            .bind(&log.topics.join(","))
            .bind(&log.data)
            .bind(log.log_index)
            .bind(&log.transaction_hash)
            .bind(log.block_number)
            .bind(log.tx_index)
            .bind(&log.from_address)
            .execute(&self.pool)
            .await?;
        }
        Ok(())
    }

    pub async fn insert_traces(&self, traces: &[IndexedTrace]) -> Result<()> {
        for trace in traces {
            sqlx::query(r#"
                INSERT INTO traces (trace_address, tx_index, call_type, action_from, action_to, action_value, action_input, result_gas, result_output, subtraces, transaction_hash, block_number)
                VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12)
                ON CONFLICT (transaction_hash, trace_address) DO NOTHING
            "#)
            .bind(&trace.trace_address)
            .bind(trace.tx_index)
            .bind(&trace.call_type)
            .bind(&trace.action_from)
            .bind(&trace.action_to)
            .bind(&trace.action_value)
            .bind(&trace.action_input)
            .bind(trace.result_gas)
            .bind(&trace.result_output)
            .bind(trace.subtraces)
            .bind(&trace.transaction_hash)
            .bind(trace.block_number)
            .execute(&self.pool)
            .await?;
        }
        Ok(())
    }

    pub async fn insert_token_transfers(&self, transfers: &[TokenTransfer]) -> Result<()> {
        for transfer in transfers {
            sqlx::query(r#"
                INSERT INTO token_transfers (transaction_hash, block_number, timestamp, from_address, to_address, token_address, token_id, value, log_index, tx_index)
                VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)
                ON CONFLICT (transaction_hash, log_index) DO NOTHING
            "#)
            .bind(&transfer.transaction_hash)
            .bind(transfer.block_number)
            .bind(transfer.timestamp)
            .bind(&transfer.from_address)
            .bind(&transfer.to_address)
            .bind(&transfer.token_address)
            .bind(&transfer.token_id)
            .bind(&transfer.value)
            .bind(transfer.log_index)
            .bind(transfer.tx_index)
            .execute(&self.pool)
            .await?;
        }
        Ok(())
    }

    pub async fn get_latest_block(&self) -> Result<Option<u64>> {
        let row: Option<(i64,)> = sqlx::query_as("SELECT MAX(number) FROM blocks")
            .fetch_optional(&self.pool)
            .await?;
        
        Ok(row.and_then(|(n,)| if n > 0 { Some(n as u64) } else { None }))
    }
}

// ============================================
// EVM Event Parsers
// ============================================

// Canonical event signatures, all verified against keccak256 of the
// Solidity declaration:
//   Transfer(address,address,uint256)
//   Approval(address,address,uint256)
//   Transfer(address,address,uint256,bytes)  (ERC-1155 TransferSingle)
const ERC20_TRANSFER_TOPIC: &str = "0xddf252ad1be2c89b69c2b068fc378daa952ba7f163c4a11628f55a4df523b3ef";
const ERC20_APPROVAL_TOPIC: &str = "0x8c5be1e5ebec7d5bd14f71427d1e84f3dd0314c0f7b2291e5b200ac8c7c3b925";
const ERC721_TRANSFER_TOPIC: &str = "0xddf252ad1be2c89b69c2b068fc378daa952ba7f163c4a11628f55a4df523b3ef";
const ERC1155_TRANSFER_SINGLE_TOPIC: &str =
    "0xc3d58168c5ae7397731d063d5bbf3d657854427343f4c083240f7aacaa2d0f62";

/// Full lowercase `0x`-prefixed hex for a 20-byte address.
///
/// `H160`'s `Display` implementation abbreviates to `0x1234…abcd`, so storing
/// `to_string()` would put a 13-character pseudo-hash in the database and any
/// downstream `[26..]` slice would panic. Always format through here.
fn hex_address(a: &ethers::types::Address) -> String {
    format!("{a:#x}")
}

/// Full lowercase `0x`-prefixed hex for a 32-byte hash. See `hex_address`.
fn hex_hash(h: &ethers::types::H256) -> String {
    format!("{h:#x}")
}

/// Full lowercase `0x`-prefixed hex for byte payloads.
fn hex_bytes(b: &ethers::types::Bytes) -> String {
    format!("0x{}", hex::encode(b.as_ref()))
}

/// The low 20 bytes of a 32-byte, left-padded topic word, as a full address.
fn topic_to_address(topic: &ethers::types::H256) -> String {
    hex_address(&ethers::types::Address::from_slice(&topic.as_bytes()[12..]))
}

pub struct TokenParser;

impl TokenParser {
    /// Extract a fungible/NFT transfer from a log.
    ///
    /// `timestamp` is carried in from the enclosing block because logs do not
    /// record one; passing 0 here would store un-orderable rows.
    pub fn parse_log(log: &ethers::types::Log, timestamp: i64, tx_index: i32) -> Option<TokenTransfer> {
        if log.topics.is_empty() {
            return None;
        }

        let topic0 = hex_hash(&log.topics[0]);
        let transaction_hash = log
            .transaction_hash
            .map(|h| hex_hash(&h))
            .unwrap_or_default();
        let block_number = log.block_number.map(|b| b.as_u64() as i64).unwrap_or(0);
        let log_index = log.log_index.map(|i| i.as_u32() as i32).unwrap_or(0);
        let token_address = hex_address(&log.address);

        // ERC-20 Transfer(address,address,uint256): value lives in data.
        if topic0 == ERC20_TRANSFER_TOPIC && log.topics.len() == 3 {
            let value = U256::from_big_endian(log.data.as_ref());
            return Some(TokenTransfer {
                transaction_hash,
                block_number,
                timestamp,
                tx_index,
                from_address: topic_to_address(&log.topics[1]),
                to_address: topic_to_address(&log.topics[2]),
                token_address,
                token_id: None,
                value: value.to_string(),
                log_index,
            });
        }

        // ERC-721 Transfer(address,address,uint256): token id is indexed.
        if topic0 == ERC721_TRANSFER_TOPIC && log.topics.len() == 4 {
            let token_id = U256::from_big_endian(log.topics[3].as_bytes());
            return Some(TokenTransfer {
                transaction_hash,
                block_number,
                timestamp,
                tx_index,
                from_address: topic_to_address(&log.topics[1]),
                to_address: topic_to_address(&log.topics[2]),
                token_address,
                token_id: Some(token_id.to_string()),
                value: "1".to_string(),
                log_index,
            });
        }

        // ERC-1155 TransferSingle(operator,from,to,id,value): the first topic is
        // the operator and both id and value are non-indexed in the data.
        if topic0 == ERC1155_TRANSFER_SINGLE_TOPIC && log.topics.len() == 4 {
            let data = log.data.as_ref();
            if data.len() < 64 {
                return None;
            }
            let token_id = U256::from_big_endian(&data[0..32]);
            let value = U256::from_big_endian(&data[32..64]);
            return Some(TokenTransfer {
                transaction_hash,
                block_number,
                timestamp,
                tx_index,
                from_address: topic_to_address(&log.topics[2]),
                to_address: topic_to_address(&log.topics[3]),
                token_address,
                token_id: Some(token_id.to_string()),
                value: value.to_string(),
                log_index,
            });
        }

        None
    }

    /// True when the log is an ERC-20 Approval. Stored separately from
    /// transfers because an approval moves no value.
    pub fn is_approval(log: &ethers::types::Log) -> bool {
        log.topics.len() == 3 && hex_hash(&log.topics[0]) == ERC20_APPROVAL_TOPIC
    }
}

// ============================================
// Main Indexer
// ============================================

pub struct Indexer {
    config: IndexerConfig,
    provider: Provider<Http>,
    /// Dedicated endpoint for `debug_traceTransaction`. `None` when tracing is
    /// disabled or no trace endpoint is configured, in which case trace
    /// indexing is skipped and reported rather than silently faked.
    trace_provider: Option<Provider<Http>>,
    ws_provider: Option<Provider<Ws>>,
    db: Arc<Database>,
    state: Arc<RwLock<IndexerState>>,
    shutdown_tx: Option<mpsc::Sender<()>>,
}

pub struct IndexerState {
    pub current_block: u64,
    pub indexed_blocks: u64,
    pub indexed_transactions: u64,
    pub indexed_logs: u64,
    pub indexed_traces: u64,
    pub last_error: Option<String>,
    pub is_running: bool,
}

impl Indexer {
    pub async fn new(config: IndexerConfig) -> Result<Self> {
        let provider = Provider::<Http>::try_from(&config.rpc_url)
            .map_err(|e| IndexerError::Rpc(e.to_string()))?;
        
        // Websocket is used only for new-block notifications. If it cannot be
        // reached the indexer still makes progress through its polling loop, so
        // a missing WS endpoint is a warning, not a fatal error.
        let ws_provider = if config.ws_url.is_empty() {
            None
        } else {
            match Provider::<Ws>::connect(&config.ws_url).await {
                Ok(p) => Some(p),
                Err(e) => {
                    warn!("WebSocket provider unavailable ({}): {e}; falling back to polling", config.ws_url);
                    None
                }
            }
        };

        let trace_provider = if config.index_traces && !config.trace_rpc_url.is_empty() {
            Some(
                Provider::<Http>::try_from(&config.trace_rpc_url)
                    .map_err(|e| IndexerError::Rpc(e.to_string()))?,
            )
        } else {
            None
        };
        
        let db = Arc::new(Database::new(&config.database_url).await?);
        db.init_schema().await?;
        
        Ok(Self {
            config,
            provider,
            trace_provider,
            ws_provider,
            db,
            state: Arc::new(RwLock::new(IndexerState {
                current_block: 0,
                indexed_blocks: 0,
                indexed_transactions: 0,
                indexed_logs: 0,
                indexed_traces: 0,
                last_error: None,
                is_running: false,
            })),
            shutdown_tx: None,
        })
    }

    pub async fn start(&mut self) -> Result<()> {
        info!("Starting advanced indexer...");
        
        let (shutdown_tx, mut shutdown_rx) = mpsc::channel(1);
        self.shutdown_tx = Some(shutdown_tx);
        
        // Get start block
        let mut current_block = self.config.start_block;
        if let Ok(Some(db_block)) = self.db.get_latest_block().await {
            current_block = db_block + 1;
        }
        
        // Update state
        {
            let mut state = self.state.write();
            state.current_block = current_block;
            state.is_running = true;
        }
        
        // Wake the indexing loop as soon as the node announces a block, so the
        // explorer sees new blocks promptly instead of waiting out the tick.
        // The announced number is only a hint: the polling loop re-derives what
        // to index from the database and is authoritative.
        let (wake_tx, mut wake_rx) = mpsc::channel::<()>(1);
        if let Some(ws) = &self.ws_provider {
            let ws = ws.clone();
            tokio::spawn(async move {
                match ws.subscribe_blocks().await {
                    Ok(mut stream) => {
                        while let Some(header) = stream.next().await {
                            debug!("New block announced: {}", header.number.unwrap_or_default());
                            // try_send so a backlog of notifications
                            // collapses into a single pending wake-up.
                            let _ = wake_tx.try_send(());
                        }
                    }
                    Err(e) => warn!("Cannot subscribe to new blocks: {e}"),
                }
            });
        }

        let mut ticker = interval(Duration::from_secs(15));
        loop {
            tokio::select! {
                _ = ticker.tick() => {
                    if let Err(e) = self.sync_blocks().await {
                        error!("Sync error: {e}");
                        self.state.write().last_error = Some(e.to_string());
                    }
                }
                Some(()) = wake_rx.recv() => {
                    if let Err(e) = self.sync_blocks().await {
                        error!("Sync error: {e}");
                        self.state.write().last_error = Some(e.to_string());
                    }
                }
                _ = shutdown_rx.recv() => {
                    info!("Indexer shutdown requested");
                    break;
                }
            }
        }

        self.state.write().is_running = false;
        Ok(())
    }

    pub async fn stop(&mut self) {
        if let Some(tx) = self.shutdown_tx.take() {
            let _ = tx.send(()).await;
        }
    }

    async fn sync_blocks(&self) -> Result<()> {
        let current_block = self.state.read().current_block;

        // A failure here is usually transient (the node is restarting), so it is
        // reported to the caller, which records it in `last_error` and retries
        // on the next tick.
        let latest = self
            .provider
            .get_block_number()
            .await
            .map_err(|e| IndexerError::Rpc(e.to_string()))?
            .as_u64();

        let safe_head = latest.saturating_sub(self.config.confirmation_blocks);

        // Block 0 is a valid block and is also the initial `current_block`, so
        // "start from 0" and "block 0 already indexed" are distinguished by
        // looking at the database rather than by the counter alone.
        let next = if !self.has_indexed_block(current_block).await? {
            current_block
        } else {
            current_block + 1
        };

        if next > safe_head {
            return Ok(());
        }

        let end_block = std::cmp::min(next + self.config.batch_size, safe_head);
        debug!("Indexing blocks {next}..={end_block}");

        for block_num in next..=end_block {
            match self.fetch_and_index_block(block_num).await {
                Ok(true) => {}
                Ok(false) => {
                    // The node does not have this block yet (pruned or lagging).
                    // Stop the batch and let the next tick retry.
                    warn!("Block {block_num} is not available from the node; pausing this batch");
                    break;
                }
                Err(e) => {
                    warn!("Failed to index block {block_num}: {e}");
                    self.state.write().last_error = Some(e.to_string());
                }
            }
        }

        Ok(())
    }

    /// Whether a block is already stored. Block 0 doubles as the initial
    /// counter value, so it cannot be recognised as "done" from the counter.
    async fn has_indexed_block(&self, number: u64) -> Result<bool> {
        let row: Option<(i64,)> = sqlx::query_as("SELECT 1 FROM blocks WHERE number = $1 LIMIT 1")
            .bind(number as i64)
            .fetch_optional(&self.db.pool)
            .await?;
        Ok(row.is_some())
    }

    /// Fetch one block with full transactions and index it. Returns `Ok(false)`
    /// when the node does not have the block yet.
    async fn fetch_and_index_block(&self, number: u64) -> Result<bool> {
        let block = self
            .provider
            .get_block_with_txs(number)
            .await
            .map_err(|e| IndexerError::Rpc(e.to_string()))?;

        let Some(block) = block else {
            return Ok(false);
        };

        self.index_block(&block).await?;
        Ok(true)
    }

    async fn index_block(&self, block: &Block<Transaction>) -> Result<()> {
        let block_number = block.number.map(|n| n.as_u64()).unwrap_or(0);
        let timestamp = block.timestamp.as_u64() as i64;

        let indexed_block = IndexedBlock {
            number: block_number as i64,
            hash: block.hash.as_ref().map(hex_hash).unwrap_or_default(),
            parent_hash: hex_hash(&block.parent_hash),
            timestamp,
            miner: block.author.as_ref().map(hex_address).unwrap_or_default(),
            gas_limit: block.gas_limit.as_u64() as i64,
            gas_used: block.gas_used.as_u64() as i64,
            transaction_count: block.transactions.len() as i32,
            size: block.size.map(|s| s.as_u64() as i32).unwrap_or(0),
        };
        self.db.insert_block(&indexed_block).await?;

        let mut logs_indexed = 0u64;
        let mut traces_indexed = 0u64;

        for (idx, tx) in block.transactions.iter().enumerate() {
            let tx_index = idx as i32;

            let receipt = self
                .provider
                .get_transaction_receipt(tx.hash)
                .await
                .map_err(|e| IndexerError::Rpc(e.to_string()))?;

            let indexed_tx = Self::build_transaction(tx, receipt.as_ref(), block_number, timestamp)?;
            self.db.insert_transaction(&indexed_tx).await?;

            let Some(receipt) = receipt else {
                continue;
            };

            // Every log is stored: logs are the raw material for the explorer's
            // event search, so they are kept regardless of whether they match a
            // known token signature.
            if self.config.index_logs && !receipt.logs.is_empty() {
                let logs: Vec<IndexedLog> = receipt
                    .logs
                    .iter()
                    .map(|log| IndexedLog {
                        address: hex_address(&log.address),
                        topics: log.topics.iter().map(hex_hash).collect(),
                        data: hex_bytes(&log.data),
                        log_index: log.log_index.map(|i| i.as_u32() as i32).unwrap_or(0),
                        transaction_hash: hex_hash(&tx.hash),
                        block_number: block_number as i64,
                        tx_index,
                        from_address: hex_address(&tx.from),
                    })
                    .collect();
                self.db.insert_logs(&logs).await?;
                logs_indexed += logs.len() as u64;
            }

            if self.config.index_transfers {
                let transfers: Vec<TokenTransfer> = receipt
                    .logs
                    .iter()
                    .filter_map(|log| TokenParser::parse_log(log, timestamp, tx_index))
                    .collect();
                if !transfers.is_empty() {
                    self.db.insert_token_transfers(&transfers).await?;
                }
            }

            if self.config.index_traces {
                match self.trace_transaction(tx.hash, tx_index, block_number).await {
                    Ok(traces) => {
                        if !traces.is_empty() {
                            self.db.insert_traces(&traces).await?;
                            traces_indexed += traces.len() as u64;
                        }
                    }
                    // A node without the `debug` namespace, or one that does not
                    // retain the state needed to re-execute the transaction, must
                    // not abort the batch. The block's other data is still valid.
                    Err(e) => debug!("Skipping traces for {}: {e}", hex_hash(&tx.hash)),
                }
            }
        }

        let mut state = self.state.write();
        state.current_block = block_number;
        state.indexed_blocks += 1;
        state.indexed_transactions += block.transactions.len() as u64;
        state.indexed_logs += logs_indexed;
        state.indexed_traces += traces_indexed;

        Ok(())
    }

    /// Re-execute a transaction with the `callTracer` and flatten the returned
    /// call tree into rows.
    async fn trace_transaction(
        &self,
        hash: ethers::types::H256,
        tx_index: i32,
        block_number: u64,
    ) -> Result<Vec<IndexedTrace>> {
        let Some(provider) = &self.trace_provider else {
            return Ok(Vec::new());
        };

        let opts = serde_json::json!({
            "tracer": "callTracer",
            "tracerConfig": { "withLog": false }
        });

        let trace: GethTrace = provider
            .request("debug_traceTransaction", (hash, opts))
            .await
            .map_err(|e| IndexerError::Rpc(e.to_string()))?;

        let frame = match trace {
            GethTrace::Known(GethTraceFrame::CallTracer(frame)) => frame,
            other => {
                debug!("trace for {} was not a call frame: {other:?}", hex_hash(&hash));
                return Ok(Vec::new());
            }
        };

        let tx_hash = hex_hash(&hash);
        let mut out = Vec::new();
        flatten_call_frame(&frame, &mut Vec::new(), &tx_hash, tx_index, block_number, &mut out);
        Ok(out)
    }

    fn build_transaction(
        tx: &Transaction,
        receipt: Option<&TransactionReceipt>,
        block_number: u64,
        timestamp: i64,
    ) -> Result<IndexedTransaction> {
        Ok(IndexedTransaction {
            hash: hex_hash(&tx.hash),
            block_number: block_number as i64,
            block_hash: tx.block_hash.as_ref().map(hex_hash).unwrap_or_default(),
            timestamp,
            from_address: hex_address(&tx.from),
            to_address: tx.to.as_ref().map(hex_address),
            value: tx.value.to_string(),
            gas_price: tx.gas_price.map(|p| p.to_string()).unwrap_or_default(),
            gas_used: receipt
                .and_then(|r| r.gas_used)
                .map(|g| g.as_u64() as i64)
                .unwrap_or(0),
            input: hex_bytes(&tx.input),
            status: receipt.and_then(|r| r.status).map(|s| s.as_u64() == 1),
            logs: vec![],
            traces: vec![],
        })
    }

    pub fn get_state(&self) -> IndexerState {
        let s = self.state.read();
        IndexerState {
            current_block: s.current_block,
            indexed_blocks: s.indexed_blocks,
            indexed_transactions: s.indexed_transactions,
            indexed_logs: s.indexed_logs,
            indexed_traces: s.indexed_traces,
            last_error: s.last_error.clone(),
            is_running: s.is_running,
        }
    }
}

/// Walk a geth call frame depth-first, emitting one row per call.
///
/// `trace_address` follows the Parity/OpenEthereum convention: a dot-separated
/// path of child indices, e.g. `0.1.2`. The root call has an empty address.
fn flatten_call_frame(
    frame: &CallFrame,
    path: &mut Vec<usize>,
    tx_hash: &str,
    tx_index: i32,
    block_number: u64,
    out: &mut Vec<IndexedTrace>,
) {
    let trace_address = path
        .iter()
        .map(|i| i.to_string())
        .collect::<Vec<_>>()
        .join(".");

    let to = frame.to.as_ref().map(|t| match t {
        ethers::types::NameOrAddress::Address(a) => hex_address(a),
        ethers::types::NameOrAddress::Name(n) => n.clone(),
    });

    out.push(IndexedTrace {
        trace_address,
        tx_index,
        call_type: frame.typ.clone(),
        action_from: hex_address(&frame.from),
        action_to: to,
        action_value: frame.value.map(|v| v.to_string()),
        action_input: hex_bytes(&frame.input),
        result_gas: Some(frame.gas_used.as_u64() as i64),
        result_output: frame
            .output
            .as_ref()
            .map(hex_bytes)
            .unwrap_or_else(|| "0x".to_string()),
        subtraces: frame.calls.as_ref().map(|c| c.len()).unwrap_or(0) as i32,
        transaction_hash: tx_hash.to_string(),
        block_number: block_number as i64,
    });

    if let Some(children) = &frame.calls {
        for (i, child) in children.iter().enumerate() {
            path.push(i);
            flatten_call_frame(child, path, tx_hash, tx_index, block_number, out);
            path.pop();
        }
    }
}

impl Clone for Indexer {
    fn clone(&self) -> Self {
        Self {
            config: self.config.clone(),
            provider: self.provider.clone(),
            trace_provider: self.trace_provider.clone(),
            ws_provider: self.ws_provider.clone(),
            db: self.db.clone(),
            state: self.state.clone(),
            shutdown_tx: None,
        }
    }
}

// ============================================
// CLI Entry Point
// ============================================

#[tokio::main]
async fn main() -> std::result::Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt::init();
    
    info!("TigerSmartChain Advanced Indexer v0.1.0");
    
    let config = IndexerConfig::from_env()?;
    
    let mut indexer = Indexer::new(config).await?;
    
    if let Err(e) = indexer.start().await {
        error!("Indexer error: {}", e);
    }
    
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    use ethers::types::{Address, Bytes, Log, H256};

    fn log_with(topics: Vec<H256>, data: Vec<u8>, address: Address) -> Log {
        Log {
            address,
            topics,
            data: Bytes::from(data),
            ..Default::default()
        }
    }

    fn topic_from_address(a: &str) -> H256 {
        let addr = a.parse::<Address>().unwrap();
        let mut word = [0u8; 32];
        word[12..].copy_from_slice(addr.as_bytes());
        H256::from(word)
    }

    #[test]
    fn addresses_and_hashes_are_never_truncated() {
        // `H160`/`H256` `Display` abbreviates to `0x1234…abcd`. Every value the
        // indexer stores must go through the full-width formatters instead.
        let addr = "0x1234567890abcdef1234567890abcdef12345678"
            .parse::<Address>()
            .unwrap();
        let rendered = hex_address(&addr);
        assert_eq!(rendered.len(), 42, "address must be 0x + 20 bytes");
        assert_eq!(rendered, "0x1234567890abcdef1234567890abcdef12345678");
        assert!(!rendered.contains('…'));

        let hash = H256::from_low_u64_be(0xdead_beef);
        let rendered = hex_hash(&hash);
        assert_eq!(rendered.len(), 66, "hash must be 0x + 32 bytes");
        assert!(rendered.ends_with("deadbeef"));
        assert!(!rendered.contains('…'));

        // Demonstrate the trap being avoided.
        assert!(addr.to_string().contains('…'));
    }

    #[test]
    fn indexed_topic_word_maps_to_full_address() {
        let addr = "0xabcdefabcdefabcdefabcdefabcdefabcdefabcd";
        let topic = topic_from_address(addr);
        assert_eq!(topic_to_address(&topic), addr);
    }

    #[test]
    fn parses_erc20_transfer_from_data_field() {
        let from = "0x1111111111111111111111111111111111111111";
        let to = "0x2222222222222222222222222222222222222222";
        let token = "0x3333333333333333333333333333333333333333"
            .parse::<Address>()
            .unwrap();

        // 1_000_000 base units, big-endian, as a 32-byte word.
        let mut data = [0u8; 32];
        data[28..].copy_from_slice(&1_000_000u32.to_be_bytes());

        let log = log_with(
            vec![
                ERC20_TRANSFER_TOPIC.parse().unwrap(),
                topic_from_address(from),
                topic_from_address(to),
            ],
            data.to_vec(),
            token,
        );

        let transfer = TokenParser::parse_log(&log, 1_700_000_000, 3).expect("must parse");
        assert_eq!(transfer.from_address, from);
        assert_eq!(transfer.to_address, to);
        assert_eq!(transfer.token_address, format!("{token:#x}"));
        assert_eq!(transfer.value, "1000000");
        assert_eq!(transfer.token_id, None);
        assert_eq!(transfer.timestamp, 1_700_000_000);
        assert_eq!(transfer.tx_index, 3);
    }

    #[test]
    fn parses_erc721_transfer_with_indexed_token_id() {
        let from = "0x1111111111111111111111111111111111111111";
        let to = "0x2222222222222222222222222222222222222222";
        let token = "0x3333333333333333333333333333333333333333"
            .parse::<Address>()
            .unwrap();

        let log = log_with(
            vec![
                ERC721_TRANSFER_TOPIC.parse().unwrap(),
                topic_from_address(from),
                topic_from_address(to),
                H256::from_low_u64_be(42),
            ],
            vec![],
            token,
        );

        let transfer = TokenParser::parse_log(&log, 1, 0).expect("must parse");
        assert_eq!(transfer.token_id.as_deref(), Some("42"));
        assert_eq!(transfer.value, "1");
        assert_eq!(transfer.from_address, from);
    }

    #[test]
    fn parses_erc1155_transfer_single() {
        let operator = "0x4444444444444444444444444444444444444444";
        let from = "0x1111111111111111111111111111111111111111";
        let to = "0x2222222222222222222222222222222222222222";
        let token = "0x3333333333333333333333333333333333333333"
            .parse::<Address>()
            .unwrap();

        let mut data = [0u8; 64];
        data[31] = 7; // token id
        data[63] = 5; // value

        let log = log_with(
            vec![
                ERC1155_TRANSFER_SINGLE_TOPIC.parse().unwrap(),
                topic_from_address(operator),
                topic_from_address(from),
                topic_from_address(to),
            ],
            data.to_vec(),
            token,
        );

        let transfer = TokenParser::parse_log(&log, 1, 0).expect("must parse");
        assert_eq!(transfer.from_address, from, "operator must not be used as sender");
        assert_eq!(transfer.to_address, to);
        assert_eq!(transfer.token_id.as_deref(), Some("7"));
        assert_eq!(transfer.value, "5");
    }

    #[test]
    fn ignores_unrelated_and_malformed_logs() {
        let token = Address::zero();

        // Unknown signature.
        let unknown = log_with(vec![H256::from_low_u64_be(9)], vec![], token);
        assert!(TokenParser::parse_log(&unknown, 0, 0).is_none());

        // No topics at all must not panic.
        let empty = log_with(vec![], vec![], token);
        assert!(TokenParser::parse_log(&empty, 0, 0).is_none());

        // ERC-1155 signature but a truncated data payload must be rejected
        // rather than indexing a half-decoded value.
        let short = log_with(
            vec![
                ERC1155_TRANSFER_SINGLE_TOPIC.parse().unwrap(),
                H256::zero(),
                H256::zero(),
                H256::zero(),
            ],
            vec![0u8; 8],
            token,
        );
        assert!(TokenParser::parse_log(&short, 0, 0).is_none());

        // ERC-20 transfer missing its `to` topic.
        let malformed = log_with(
            vec![ERC20_TRANSFER_TOPIC.parse().unwrap(), H256::zero()],
            vec![0u8; 32],
            token,
        );
        assert!(TokenParser::parse_log(&malformed, 0, 0).is_none());
    }

    #[test]
    fn approval_logs_are_recognised_but_not_transfers() {
        let owner = "0x1111111111111111111111111111111111111111";
        let spender = "0x2222222222222222222222222222222222222222";
        let token = "0x3333333333333333333333333333333333333333"
            .parse::<Address>()
            .unwrap();

        let log = log_with(
            vec![
                ERC20_APPROVAL_TOPIC.parse().unwrap(),
                topic_from_address(owner),
                topic_from_address(spender),
            ],
            vec![0u8; 32],
            token,
        );

        assert!(TokenParser::is_approval(&log));
        assert!(TokenParser::parse_log(&log, 0, 0).is_none(), "an approval moves no value");
    }

    #[test]
    fn call_frames_flatten_to_parity_trace_addresses() {
        let alice: Address = "0x1111111111111111111111111111111111111111".parse().unwrap();
        let bob: Address = "0x2222222222222222222222222222222222222222".parse().unwrap();
        let carol: Address = "0x3333333333333333333333333333333333333333".parse().unwrap();

        let leaf = CallFrame {
            typ: "STATICCALL".to_string(),
            from: bob,
            to: Some(carol.into()),
            gas_used: 100.into(),
            input: Bytes::from(vec![1, 2, 3]),
            output: Some(Bytes::from(vec![9])),
            ..Default::default()
        };

        let root = CallFrame {
            typ: "CALL".to_string(),
            from: alice,
            to: Some(bob.into()),
            gas_used: 500.into(),
            input: Bytes::from(vec![0xaa, 0xbb]),
            output: None,
            calls: Some(vec![
                CallFrame {
                    typ: "CALL".to_string(),
                    from: bob,
                    to: Some(carol.into()),
                    gas_used: 10.into(),
                    ..Default::default()
                },
                leaf,
            ]),
            ..Default::default()
        };

        let tx_hash = "0xabc";
        let mut out = Vec::new();
        flatten_call_frame(&root, &mut Vec::new(), tx_hash, 4, 99, &mut out);

        assert_eq!(out.len(), 3, "root plus two children");

        assert_eq!(out[0].trace_address, "", "root call has an empty address");
        assert_eq!(out[0].call_type, "CALL");
        assert_eq!(out[0].action_from, format!("{alice:#x}"));
        assert_eq!(out[0].action_to.as_deref(), Some(format!("{bob:#x}").as_str()));
        assert_eq!(out[0].subtraces, 2);
        assert_eq!(out[0].result_gas, Some(500));
        assert_eq!(out[0].result_output, "0x", "missing output is empty, not fabricated");
        assert_eq!(out[0].transaction_hash, tx_hash);
        assert_eq!(out[0].block_number, 99);
        assert_eq!(out[0].tx_index, 4);

        assert_eq!(out[1].trace_address, "0");
        assert_eq!(out[2].trace_address, "1");
        assert_eq!(out[2].call_type, "STATICCALL");
        assert_eq!(out[2].action_from, format!("{bob:#x}"));
        assert_eq!(out[2].action_input, "0x010203");
        assert_eq!(out[2].result_output, "0x09");
        assert_eq!(out[2].result_gas, Some(100));
        assert_eq!(out[2].subtraces, 0);
    }
}
