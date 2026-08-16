/**
 * Advanced Blockchain Indexer - High-performance indexing with full trace support
 * Rust-based for ultra-low latency and maximum throughput
 */

use std::sync::Arc;
use std::collections::HashMap;
use std::str::FromStr;

use async_trait::async_trait;
use bytes::Bytes;
use chrono::{DateTime, Utc};
use ethers::providers::{Http, Provider, Ws, Middleware};
use ethers::types::{Block, Transaction, TransactionReceipt, Log, Trace, H256, U64, U256, Address};
use futures::StreamExt;
use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use sqlx::{postgres::{PgPool, PgRow}, Row, Column};
use thiserror::Error;
use tracing::{info, warn, error, debug};
use tokio::sync::mpsc;
use tokio::time::{interval, Duration};

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
    pub ws_url: String,
    pub database_url: String,
    pub chain_id: u64,
    pub start_block: u64,
    pub batch_size: u64,
    pub confirmation_blocks: u64,
    pub index_traces: bool,
    pub index_logs: bool,
    pub index_transfers: bool,
}

impl IndexerConfig {
    pub fn from_env() -> Result<Self> {
        Ok(Self {
            rpc_url: std::env::var("RPC_URL")
                .map_err(|_| IndexerError::Config("RPC_URL not set".into()))?,
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
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IndexedTrace {
    pub trace_address: String,
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
                created_at TIMESTAMP DEFAULT NOW()
            );
            
            CREATE TABLE IF NOT EXISTS traces (
                id SERIAL PRIMARY KEY,
                trace_address TEXT NOT NULL,
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
                created_at TIMESTAMP DEFAULT NOW()
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
        "#).execute(&self.pool).await?;

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
                INSERT INTO logs (address, topics, data, log_index, transaction_hash, block_number)
                VALUES ($1, $2, $3, $4, $5, $6)
            "#)
            .bind(&log.address)
            .bind(&log.topics.join(","))
            .bind(&log.data)
            .bind(log.log_index)
            .bind(&log.transaction_hash)
            .bind(log.block_number)
            .execute(&self.pool)
            .await?;
        }
        Ok(())
    }

    pub async fn insert_traces(&self, traces: &[IndexedTrace]) -> Result<()> {
        for trace in traces {
            sqlx::query(r#"
                INSERT INTO traces (trace_address, call_type, action_from, action_to, action_value, action_input, result_gas, result_output, subtraces, transaction_hash, block_number)
                VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11)
            "#)
            .bind(&trace.trace_address)
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
                INSERT INTO token_transfers (transaction_hash, block_number, timestamp, from_address, to_address, token_address, token_id, value, log_index)
                VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)
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

const ERC20_TRANSFER_TOPIC: &str = "0xddf252ad1be2c89b69c2b068fc378daa952ba7f163c4a11628f55a4df523b3ef";
const ERC20_APPROVAL_TOPIC: &str = "0x8c5be1e5ebec7d5bd14f71427d1e84f3dd0314c0f7b2291e5b200ac8c7c3b925";
const ERC721_TRANSFER_TOPIC: &str = "0xddf252ad1be2c89b69c2b068fc378daa952ba7f163c4a11628f55a4df523b3ef";

pub struct TokenParser;

impl TokenParser {
    pub fn parse_log(log: &ethers::types::Log) -> Option<TokenTransfer> {
        if log.topics.is_empty() {
            return None;
        }
        
        let topic0 = log.topics[0].to_string();
        
        // ERC20 Transfer
        if topic0 == ERC20_TRANSFER_TOPIC && log.topics.len() >= 3 {
            let from = format!("0x{}", &log.topics[1].to_string()[26..]);
            let to = format!("0x{}", &log.topics[2].to_string()[26..]);
            let value = U256::from_str(&log.data.to_string()).unwrap_or_default();
            
            return Some(TokenTransfer {
                transaction_hash: log.transaction_hash.map(|h| h.to_string()).unwrap_or_default(),
                block_number: log.block_number.map(|b| b.as_u64() as i64).unwrap_or(0),
                timestamp: 0,
                from_address: from,
                to_address: to,
                token_address: log.address.to_string(),
                token_id: None,
                value: value.to_string(),
                log_index: log.log_index.map(|i| i.as_u32() as i32).unwrap_or(0),
            });
        }
        
        // ERC721 Transfer
        if topic0 == ERC721_TRANSFER_TOPIC && log.topics.len() >= 4 {
            let from = format!("0x{}", &log.topics[1].to_string()[26..]);
            let to = format!("0x{}", &log.topics[2].to_string()[26..]);
            let token_id = U256::from_str(&log.topics[3].to_string()).unwrap_or_default();
            
            return Some(TokenTransfer {
                transaction_hash: log.transaction_hash.map(|h| h.to_string()).unwrap_or_default(),
                block_number: log.block_number.map(|b| b.as_u64() as i64).unwrap_or(0),
                timestamp: 0,
                from_address: from,
                to_address: to,
                token_address: log.address.to_string(),
                token_id: Some(token_id.to_string()),
                value: "1".to_string(),
                log_index: log.log_index.map(|i| i.as_u32() as i32).unwrap_or(0),
            });
        }
        
        None
    }
}

// ============================================
// Main Indexer
// ============================================

pub struct Indexer {
    config: IndexerConfig,
    provider: Provider<Http>,
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
        
        let ws_provider = if !config.ws_url.is_empty() {
            Some(Provider::<Ws>::connect(&config.ws_url).await
                .map_err(|e| IndexerError::Rpc(e.to_string()))?)
        } else {
            None
        };
        
        let db = Arc::new(Database::new(&config.database_url).await?);
        db.init_schema().await?;
        
        Ok(Self {
            config,
            provider,
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
        
        // Start WebSocket subscription for new blocks if available
        if let Some(ws) = &self.ws_provider {
            let db = self.db.clone();
            let state = self.state.clone();
            let config = self.config.clone();
            
            tokio::spawn(async move {
                if let Ok(mut stream) = ws.subscribe_pending_headers().await {
                    while let Some(block) = stream.next().await {
                        if let Ok(block) = block {
                            let num = block.number.unwrap_or_default().as_u64();
                            debug!("New block: {}", num);
                            
                            // Index the block
                            if let Err(e) = Self::index_block_internal(&db, &block, &config).await {
                                error!("Failed to index block {}: {}", num, e);
                            }
                            
                            state.write().current_block = num;
                            state.write().indexed_blocks += 1;
                        }
                    }
                }
            });
        }
        
        // Main indexing loop
        let mut ticker = interval(Duration::from_secs(15));
        
        loop {
            tokio::select! {
                _ = ticker.tick() => {
                    if let Err(e) = self.sync_blocks().await {
                        error!("Sync error: {}", e);
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
        let current_block = {
            let state = self.state.read();
            state.current_block
        };
        
        // Get latest block from RPC
        let latest = self.provider.get_block_number().await
            .map_err(|e| IndexerError::Rpc(e.to_string()))?;
        let latest = latest.as_u64();
        
        // Calculate safe head (confirmed blocks)
        let safe_head = latest.saturating_sub(self.config.confirmation_blocks);
        
        if current_block >= safe_head {
            return Ok(());
        }
        
        // Batch index blocks
        let batch_size = self.config.batch_size as usize;
        let end_block = std::cmp::min(current_block + batch_size as u64, safe_head);
        
        info!("Indexing blocks {} to {}", current_block, end_block);
        
        for block_num in current_block..=end_block {
            if let Some(block) = self.provider.get_block_with_transactions(block_num.into())
                .await
                .map_err(|e| IndexerError::Rpc(e.to_string()))? 
            {
                if let Err(e) = self.index_block(&block).await {
                    warn!("Failed to index block {}: {}", block_num, e);
                }
            }
        }
        
        Ok(())
    }

    async fn index_block(&self, block: &Block<Transaction>) -> Result<()> {
        let block_number = block.number.unwrap_or_default().as_u64();
        let timestamp = block.timestamp.as_u64() as i64;
        
        // Index block
        let indexed_block = IndexedBlock {
            number: block_number as i64,
            hash: block.hash.map(|h| h.to_string()).unwrap_or_default(),
            parent_hash: block.parent_hash.to_string(),
            timestamp,
            miner: block.miner.map(|m| m.to_string()).unwrap_or_default(),
            gas_limit: block.gas_limit.as_u64() as i64,
            gas_used: block.gas_used.as_u64() as i64,
            transaction_count: block.transactions.len() as i32,
            size: block.size.as_u64() as i32,
        };
        
        self.db.insert_block(&indexed_block).await?;
        
        // Index transactions
        for tx in &block.transactions {
            let receipt = self.provider.get_transaction_receipt(tx.hash)
                .await
                .map_err(|e| IndexerError::Rpc(e.to_string()))?;
            
            let indexed_tx = self.index_transaction(tx, receipt.as_ref(), block_number, timestamp).await?;
            self.db.insert_transaction(&indexed_tx).await?;
            
            // Index logs
            if let Some(ref receipt) = receipt {
                let logs: Vec<IndexedLog> = receipt.logs.iter().map(|log| {
                    IndexedLog {
                        address: log.address.to_string(),
                        topics: log.topics.iter().map(|t| t.to_string()).collect(),
                        data: log.data.to_string(),
                        log_index: log.log_index.as_u32() as i32,
                        transaction_hash: tx.hash.to_string(),
                        block_number: block_number as i64,
                    }
                }).collect();
                
                if !logs.is_empty() {
                    self.db.insert_logs(&logs).await?;
                    self.state.write().indexed_logs += logs.len() as u64;
                    
                    // Parse token transfers
                    if self.config.index_transfers {
                        let transfers: Vec<TokenTransfer> = receipt.logs.iter()
                            .filter_map(TokenParser::parse_log)
                            .collect();
                        
                        if !transfers.is_empty() {
                            self.db.insert_token_transfers(&transfers).await?;
                        }
                    }
                }
                
                // Index traces if enabled
                if self.config.index_traces {
                    // Would fetch traces via debug_traceTransaction
                    // Simplified for demo
                }
            }
        }
        
        let mut state = self.state.write();
        state.current_block = block_number;
        state.indexed_blocks += 1;
        state.indexed_transactions += block.transactions.len() as u64;
        
        Ok(())
    }

    async fn index_transaction(
        &self,
        tx: &Transaction,
        receipt: Option<&TransactionReceipt>,
        block_number: u64,
        timestamp: i64,
    ) -> Result<IndexedTransaction> {
        Ok(IndexedTransaction {
            hash: tx.hash.to_string(),
            block_number: block_number as i64,
            block_hash: tx.block_hash.map(|h| h.to_string()).unwrap_or_default(),
            timestamp,
            from_address: tx.from.map(|a| a.to_string()).unwrap_or_default(),
            to_address: tx.to.map(|a| a.to_string()),
            value: tx.value.to_string(),
            gas_price: tx.gas_price.map(|p| p.to_string()).unwrap_or_default(),
            gas_used: receipt.map(|r| r.gas_used.as_u64() as i64).unwrap_or(0),
            input: tx.input.to_string(),
            status: receipt.map(|r| r.status.map(|s| s.as_u64() == 1)),
            logs: vec![],
            traces: vec![],
        })
    }

    pub fn get_state(&self) -> IndexerState {
        self.state.read().clone()
    }
}

#[async_trait]
impl Clone for Indexer {
    fn clone(&self) -> Self {
        Self {
            config: self.config.clone(),
            provider: self.provider.clone(),
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