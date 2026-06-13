//! TigerScan Production Indexer Service
//! High-performance blockchain indexer with real-time indexing for blocks, transactions, tokens, NFTs, and traces

use std::sync::Arc;
use std::time::Duration;

use anyhow::Result;
use async_trait::async_trait;
use bytes::Bytes;
use chrono::{DateTime, Utc};
use ethers::providers::{Http, Provider, StreamExt, Ws};
use ethers::types::{Address, Block, Filter, H160, H256, Log, Transaction, U64};
use parking_lot::RwLock;
use regex::Regex;
use serde::{Deserialize, Serialize};
use sqlx::postgres::{PgPool, PgPoolOptions};
use sqlx::{Row, Transaction as SqlxTransaction};
use thiserror::Error;
use tokio::sync::mpsc;
use tokio::time::sleep;
use tracing::{error, info, warn};

// ============================================================================
// Error Types
// ============================================================================

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
    
    #[error("Indexing error: {0}")]
    Indexing(String),
}

// ============================================================================
// Configuration
// ============================================================================

#[derive(Debug, Clone, Deserialize)]
pub struct Config {
    /// RPC HTTP endpoint
    pub rpc_url: String,
    /// RPC WebSocket endpoint
    pub ws_url: Option<String>,
    /// Archive RPC URL for historical data
    pub archive_url: Option<String>,
    /// Database connection string
    pub database_url: String,
    /// Maximum concurrent requests
    pub max_concurrent_requests: usize,
    /// Request timeout
    pub request_timeout: Duration,
    /// Batch size for indexing
    pub batch_size: u64,
    /// Start block number
    pub start_block: u64,
    /// Confirmation blocks
    pub confirmation_blocks: u64,
    /// Index traces
    pub index_traces: bool,
    /// Index internal transactions
    pub index_internal_txs: bool,
    /// Index tokens
    pub index_tokens: bool,
    /// Index NFTs
    pub index_nfts: bool,
    /// IPFS gateway URL
    pub ipfs_gateway: Option<String>,
    /// Token addresses to index
    pub token_addresses: Vec<String>,
    /// NFT contract addresses
    pub nft_addresses: Vec<String>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            rpc_url: std::env::var("RPC_URL").unwrap_or_else(|_| "http://localhost:8545".to_string()),
            ws_url: std::env::var("WS_URL").ok(),
            archive_url: std::env::var("ARCHIVE_URL").ok(),
            database_url: std::env::var("DATABASE_URL").unwrap_or_else(|_| "postgres://localhost:5432/tigerscan".to_string()),
            max_concurrent_requests: 10,
            request_timeout: Duration::from_secs(30),
            batch_size: 100,
            start_block: 0,
            confirmation_blocks: 12,
            index_traces: true,
            index_internal_txs: true,
            index_tokens: true,
            index_nfts: true,
            ipfs_gateway: std::env::var("IPFS_GATEWAY").ok(),
            token_addresses: vec![],
            nft_addresses: vec![],
        }
    }
}

// ============================================================================
// Database Types
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BlockRecord {
    pub number: i64,
    pub hash: String,
    pub parent_hash: String,
    pub nonce: Option<String>,
    pub sha3_uncles: Option<String>,
    pub logs_bloom: Option<String>,
    pub transactions_root: Option<String>,
    pub state_root: Option<String>,
    pub receipts_root: Option<String>,
    pub miner: String,
    pub difficulty: Option<String>,
    pub total_difficulty: Option<String>,
    pub gas_limit: i64,
    pub gas_used: i64,
    pub timestamp: i64,
    pub size: i64,
    pub extra_data: Option<String>,
    pub base_fee_per_gas: Option<i64>,
    pub tx_count: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TransactionRecord {
    pub hash: String,
    pub nonce: i64,
    pub block_hash: Option<String>,
    pub block_number: Option<i64>,
    pub transaction_index: Option<i32>,
    pub from_address: String,
    pub to_address: Option<String>,
    pub value: String,
    pub gas_price: i64,
    pub gas_limit: i64,
    pub gas_used: Option<i64>,
    pub input: Option<String>,
    pub v: i64,
    pub r: Option<String>,
    pub s: Option<String>,
    pub chain_id: Option<i64>,
    pub transaction_type: Option<String>,
    pub status: String,
    pub cumulative_gas_used: Option<i64>,
    pub effective_gas_price: Option<i64>,
    pub logs: Option<String>,
    pub contract_address: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenTransfer {
    pub token_address: String,
    pub from_address: String,
    pub to_address: String,
    pub value: String,
    pub transaction_hash: String,
    pub block_number: i64,
    pub log_index: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NftTransfer {
    pub token_address: String,
    pub from_address: String,
    pub to_address: String,
    pub token_id: String,
    pub value: Option<String>,
    pub transaction_hash: String,
    pub block_number: i64,
    pub log_index: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Contract {
    pub address: String,
    pub contract_name: Option<String>,
    pub source_code: String,
    pub bytecode: Option<String>,
    pub abi: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Token {
    pub address: String,
    pub name: String,
    pub symbol: String,
    pub decimals: i32,
    pub total_supply: Option<String>,
    pub is_verified: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NftCollection {
    pub address: String,
    pub name: String,
    pub symbol: Option<String>,
    pub contract_type: String,
    pub total_supply: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Nft {
    pub token_address: String,
    pub token_id: String,
    pub owner: String,
    pub uri: Option<String>,
    pub metadata: Option<String>,
    pub name: Option<String>,
    pub description: Option<String>,
    pub image_url: Option<String>,
    pub attributes: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InternalTransaction {
    pub transaction_hash: String,
    pub block_number: i64,
    pub transaction_index: i32,
    pub depth: i32,
    pub call_type: String,
    pub from_address: String,
    pub to_address: String,
    pub value: String,
    pub gas: i64,
    pub input: Option<String>,
    pub output: Option<String>,
    pub revert: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Trace {
    pub transaction_hash: String,
    pub block_number: i64,
    pub transaction_index: i32,
    pub from_address: String,
    pub to_address: Option<String>,
    pub call_type: String,
    pub value: Option<String>,
    pub gas: Option<i64>,
    pub input: Option<String>,
    pub output: Option<String>,
    pub revert: bool,
    pub error: Option<String>,
    pub depth: i32,
}

// ============================================================================
// Indexer Service
// ============================================================================

pub struct IndexerService {
    config: Config,
    db: PgPool,
    rpc: Provider<Http>,
    ws: Option<Provider<Ws>>,
    state: Arc<RwLock<IndexerState>>,
    shutdown_tx: Option<mpsc::Sender<()>>,
}

#[derive(Debug, Clone)]
pub struct IndexerState {
    pub current_block: u64,
    pub last_indexed_block: u64,
    pub last_confirmed_block: u64,
    pub is_running: bool,
    pub errors: Vec<String>,
}

impl Default for IndexerState {
    fn default() -> Self {
        Self {
            current_block: 0,
            last_indexed_block: 0,
            last_confirmed_block: 0,
            is_running: false,
            errors: vec![],
        }
    }
}

impl IndexerService {
    /// Create a new indexer service
    pub async fn new(config: Config) -> Result<Self, IndexerError> {
        // Initialize database pool
        let db = PgPoolOptions::new()
            .max_connections(20)
            .min_connections(5)
            .acquire_timeout(Duration::from_secs(30))
            .connect(&config.database_url)
            .await?;

        // Initialize RPC client
        let rpc = Provider::<Http>::try_from(config.rpc_url.clone())
            .map_err(|e| IndexerError::Rpc(e.to_string()))?
            .interval(config.request_timeout);

        // Initialize WebSocket client
        let ws = if let Some(ws_url) = &config.ws_url {
            Some(
                Provider::<Ws>::connect(ws_url.clone(), None)
                    .await
                    .map_err(|e| IndexerError::Rpc(e.to_string()))?,
            )
        } else {
            None
        };

        Ok(Self {
            config,
            db,
            rpc,
            ws,
            state: Arc::new(RwLock::new(IndexerState::default())),
            shutdown_tx: None,
        })
    }

    /// Start indexing
    pub async fn start(&mut self) -> Result<(), IndexerError> {
        info!("Starting indexer service");

        // Get current block number
        let current_block = self.get_current_block().await?;
        
        // Get last indexed block from database
        let last_indexed = self.get_last_indexed_block().await.unwrap_or(self.config.start_block);

        {
            let mut state = self.state.write();
            state.current_block = current_block;
            state.last_indexed_block = last_indexed;
            state.last_confirmed_block = current_block.saturating_sub(self.config.confirmation_blocks);
            state.is_running = true;
        }

        info!(
            "Current block: {}, Last indexed: {}, Confirmed: {}",
            current_block, last_indexed, self.state.read().last_confirmed_block
        );

        // Start block indexing
        self.index_blocks_loop().await?;

        // Start log indexing if WebSocket is available
        if self.ws.is_some() {
            self.index_logs_loop().await?;
        }

        Ok(())
    }

    /// Stop indexing
    pub async fn stop(&mut self) {
        info!("Stopping indexer service");
        
        let mut state = self.state.write();
        state.is_running = false;

        if let Some(tx) = self.shutdown_tx.take() {
            let _ = tx.send(()).await;
        }
    }

    /// Get current block number
    async fn get_current_block(&self) -> Result<u64, IndexerError> {
        let block_num = self
            .rpc
            .get_block_number()
            .await
            .map_err(|e| IndexerError::Rpc(e.to_string()))?;
        
        Ok(block_num.as_u64())
    }

    /// Get last indexed block from database
    async fn get_last_indexed_block(&self) -> Result<u64, IndexerError> {
        let result: Result<(i64,), _> = sqlx::query_as(
            "SELECT COALESCE(MAX(number), 0) FROM blocks"
        )
        .fetch_one(&self.db)
        .await;

        match result {
            Ok(row) => Ok(row.0 as u64),
            Err(_) => Ok(self.config.start_block),
        }
    }

    /// Index blocks in a loop
    async fn index_blocks_loop(&self) -> Result<(), IndexerError> {
        let mut interval = tokio::time::interval(Duration::from_secs(12));
        
        loop {
            interval.tick().await;
            
            let state = self.state.clone();
            let state_read = state.read();
            
            if !state_read.is_running {
                break;
            }

            let last_indexed = state_read.last_indexed_block;
            let last_confirmed = state_read.last_confirmed_block;
            
            if last_indexed >= last_confirmed {
                sleep(Duration::from_secs(5)).await;
                continue;
            }

            let end_block = std::cmp::min(
                last_indexed + self.config.batch_size,
                last_confirmed,
            );

            // Index batch of blocks
            if let Err(e) = self.index_blocks(last_indexed + 1, end_block).await {
                error!("Error indexing blocks: {}", e);
                state.write().errors.push(e.to_string());
            }

            state.write().last_indexed_block = end_block;
        }

        Ok(())
    }

    /// Index a range of blocks
    async fn index_blocks(&self, start: u64, end: u64) -> Result<(), IndexerError> {
        info!("Indexing blocks {} to {}", start, end);

        for block_num in start..=end {
            // Get block with transactions
            let block = self.rpc.get_block_with_txs(block_num.into())
                .await
                .map_err(|e| IndexerError::Rpc(e.to_string()))?;

            if let Some(block) = block {
                // Insert block
                self.insert_block(&block).await?;

                // Index transactions
                for tx in block.transactions {
                    self.insert_transaction(&tx, block.number.as_u64()).await?;
                }

                // Update state
                self.state.write().current_block = block.number.as_u64();
            }
        }

        info!("Completed indexing blocks {} to {}", start, end);
        Ok(())
    }

    /// Insert a block into the database
    async fn insert_block(&self, block: &Block<Transaction>) -> Result<(), IndexerError> {
        let record = BlockRecord {
            number: block.number.as_u64() as i64,
            hash: block.hash.to_string(),
            parent_hash: block.parent_hash.to_string(),
            nonce: block.nonce.map(|n| format!("0x{}", hex::encode(n.0))),
            sha3_uncles: Some(block.uncles_hash.to_string()),
            logs_bloom: Some(format!("0x{}", hex::encode(block.logs_bloom.0))),
            transactions_root: Some(block.transactions_root.to_string()),
            state_root: Some(block.state_root.to_string()),
            receipts_root: Some(block.receipts_root.to_string()),
            miner: block.author.to_string(),
            difficulty: Some(block.difficulty.to_string()),
            total_difficulty: block.total_difficulty.map(|d| d.to_string()),
            gas_limit: block.gas_limit.as_u64() as i64,
            gas_used: block.gas_used.as_u64() as i64,
            timestamp: block.timestamp.as_u64() as i64,
            size: block.size.as_u64() as i64,
            extra_data: Some(format!("0x{}", hex::encode(block.extra_data.clone()))),
            base_fee_per_gas: block.base_fee_per_gas.map(|b| b.as_u64() as i64),
            tx_count: block.transactions.len() as i32,
        };

        sqlx::query(
            r#"
            INSERT INTO blocks (
                number, hash, parent_hash, nonce, sha3_uncles, logs_bloom,
                transactions_root, state_root, receipts_root, miner,
                difficulty, total_difficulty, gas_limit, gas_used, timestamp,
                size, extra_data, base_fee_per_gas, tx_count
            ) VALUES (
                $1, $2, $3, $4, $5, $6, $7, $8, $9, $10,
                $11, $12, $13, $14, $15, $16, $17, $18, $19
            )
            ON CONFLICT (number) DO UPDATE SET
                hash = EXCLUDED.hash,
                gas_used = EXCLUDED.gas_used,
                tx_count = EXCLUDED.tx_count,
                updated_at = NOW()
            "#,
        )
        .bind(record.number)
        .bind(record.hash)
        .bind(record.parent_hash)
        .bind(record.nonce)
        .bind(record.sha3_uncles)
        .bind(record.logs_bloom)
        .bind(record.transactions_root)
        .bind(record.state_root)
        .bind(record.receipts_root)
        .bind(record.miner)
        .bind(record.difficulty)
        .bind(record.total_difficulty)
        .bind(record.gas_limit)
        .bind(record.gas_used)
        .bind(record.timestamp)
        .bind(record.size)
        .bind(record.extra_data)
        .bind(record.base_fee_per_gas)
        .bind(record.tx_count)
        .execute(&self.db)
        .await?;

        Ok(())
    }

    /// Insert a transaction into the database
    async fn insert_transaction(&self, tx: &Transaction, block_number: u64) -> Result<(), IndexerError> {
        let record = TransactionRecord {
            hash: tx.hash.to_string(),
            nonce: tx.nonce.as_u64() as i64,
            block_hash: tx.block_hash.map(|h| h.to_string()),
            block_number: tx.block_number.map(|b| b.as_u64() as i64),
            transaction_index: tx.transaction_index.map(|i| i.as_u64() as i32),
            from_address: tx.from.to_string(),
            to_address: tx.to.map(|a| a.to_string()),
            value: tx.value.to_string(),
            gas_price: tx.gas_price.as_u64() as i64,
            gas_limit: tx.gas.as_u64() as i64,
            gas_used: None,
            input: Some(format!("0x{}", hex::encode(tx.input.clone()))),
            v: tx.v.as_u64() as i64,
            r: Some(tx.r.to_string()),
            s: Some(tx.s.to_string()),
            chain_id: tx.chain_id.map(|c| c.as_u64() as i64),
            transaction_type: Some("legacy".to_string()),
            status: "pending".to_string(),
            cumulative_gas_used: None,
            effective_gas_price: None,
            logs: None,
            contract_address: None,
        };

        sqlx::query(
            r#"
            INSERT INTO transactions (
                hash, nonce, block_hash, block_number, transaction_index,
                from_address, to_address, value, gas_price, gas_limit,
                input, v, r, s, chain_id, transaction_type, status
            ) VALUES (
                $1, $2, $3, $4, $5, $6, $7, $8, $9, $10,
                $11, $12, $13, $14, $15, $16, $17
            )
            ON CONFLICT (hash) DO UPDATE SET
                block_hash = EXCLUDED.block_hash,
                block_number = EXCLUDED.block_number,
                status = EXCLUDED.status
            "#,
        )
        .bind(record.hash)
        .bind(record.nonce)
        .bind(record.block_hash)
        .bind(record.block_number)
        .bind(record.transaction_index)
        .bind(record.from_address)
        .bind(record.to_address)
        .bind(record.value)
        .bind(record.gas_price)
        .bind(record.gas_limit)
        .bind(record.input)
        .bind(record.v)
        .bind(record.r)
        .bind(record.s)
        .bind(record.chain_id)
        .bind(record.transaction_type)
        .bind(record.status)
        .execute(&self.db)
        .await?;

        // Index token transfers
        self.index_token_transfers(tx, block_number).await?;

        // Index NFT transfers
        self.index_nft_transfers(tx, block_number).await?;

        Ok(())
    }

    /// Index token transfers from a transaction
    async fn index_token_transfers(&self, tx: &Transaction, block_number: u64) -> Result<(), IndexerError> {
        // Get transaction receipt for logs
        let receipt = self.rpc
            .get_transaction_receipt(tx.hash)
            .await
            .map_err(|e| IndexerError::Rpc(e.to_string()))?;

        let Some(receipt) = receipt else {
            return Ok(());
        };

        // Parse Transfer events (TEP20)
        let transfer_signature = H256::from_slice(&hex::decode("0xddf252ad1be2c89b69c2b068fc378da9529521f1c5d5ba481f6d2a9c92955c5be").unwrap());
        
        for (log_index, log) in receipt.logs.iter().enumerate() {
            if log.topics.len() != 4 {
                continue;
            }
            
            // Check if it's a Transfer event
            if log.topics[0] != transfer_signature {
                continue;
            }

            // Parse Transfer event
            let token_address = log.address.to_string();
            let from_address = ethers::types::H160::from(log.topics[1]).to_string();
            let to_address = ethers::types::H160::from(log.topics[2]).to_string();
            let value = ethers::types::U256::from(log.data.0).to_string();

            let transfer = TokenTransfer {
                token_address,
                from_address,
                to_address,
                value,
                transaction_hash: tx.hash.to_string(),
                block_number: block_number as i64,
                log_index: log_index as i32,
            };

            sqlx::query(
                r#"
                INSERT INTO token_transfers (
                    token_address, from_address, to_address, value,
                    transaction_hash, block_number, log_index
                ) VALUES ($1, $2, $3, $4, $5, $6, $7)
                "#,
            )
            .bind(transfer.token_address)
            .bind(transfer.from_address)
            .bind(transfer.to_address)
            .bind(transfer.value)
            .bind(transfer.transaction_hash)
            .bind(transfer.block_number)
            .bind(transfer.log_index)
            .execute(&self.db)
            .await?;
        }

        Ok(())
    }

    /// Index NFT transfers from a transaction
    async fn index_nft_transfers(&self, tx: &Transaction, block_number: u64) -> Result<(), IndexerError> {
        // Get transaction receipt for logs
        let receipt = self.rpc
            .get_transaction_receipt(tx.hash)
            .await
            .map_err(|e| IndexerError::Rpc(e.to_string()))?;

        let Some(receipt) = receipt else {
            return Ok(());
        };

        // Parse Transfer events (TEP721/TEP1155)
        let transfer_signature_721 = H256::from_slice(&hex::decode("0xddf252ad1be2c89b69c2b068fc378da9529521f1c5d5ba481f6d2a9c92955c5be").unwrap());
        
        for (log_index, log) in receipt.logs.iter().enumerate() {
            if log.topics.len() != 4 {
                continue;
            }
            
            // Check if it's a Transfer event
            if log.topics[0] != transfer_signature_721 {
                continue;
            }

            // Parse Transfer event
            let token_address = log.address.to_string();
            let from_address = ethers::types::H160::from(log.topics[1]).to_string();
            let to_address = ethers::types::H160::from(log.topics[2]).to_string();
            let token_id = ethers::types::U256::from(log.topics[3].0).to_string();
            let value = if log.data.0.is_empty() {
                "1".to_string()
            } else {
                ethers::types::U256::from(log.data.0).to_string()
            };

            let transfer = NftTransfer {
                token_address,
                from_address,
                to_address,
                token_id,
                value: Some(value),
                transaction_hash: tx.hash.to_string(),
                block_number: block_number as i64,
                log_index: log_index as i32,
            };

            sqlx::query(
                r#"
                INSERT INTO nft_transfers (
                    token_address, from_address, to_address, token_id, value,
                    transaction_hash, block_number, log_index
                ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
                "#,
            )
            .bind(transfer.token_address)
            .bind(transfer.from_address)
            .bind(transfer.to_address)
            .bind(transfer.token_id)
            .bind(transfer.value)
            .bind(transfer.transaction_hash)
            .bind(transfer.block_number)
            .bind(transfer.log_index)
            .execute(&self.db)
            .await?;
        }

        Ok(())
    }

    /// Index logs in real-time using WebSocket
    async fn index_logs_loop(&self) -> Result<(), IndexerError> {
        let Some(ws) = &self.ws else {
            return Ok(());
        };

        // Subscribe to logs
        let filter = Filter::new().topic0(ethers::types::H256::from_slice(&hex::decode("0xddf252ad1be2c89b69c2b068fc378da9529521f1c5d5ba481f6d2a9c92955c5be").unwrap()));
        
        let mut stream = ws.subscribe_pending_txs().await
            .map_err(|e| IndexerError::Rpc(e.to_string()))?;

        info!("Starting log indexing via WebSocket");

        loop {
            tokio::select! {
                _ = async {
                    if let Some(tx) = stream.next().await {
                        // Get receipt
                        if let Ok(Some(receipt)) = self.rpc.get_transaction_receipt(tx).await {
                            let block_number = receipt.block_number.unwrap_or(U64::zero()).as_u64();
                            
                            // Index token transfers
                            for (log_index, log) in receipt.logs.iter().enumerate() {
                                // Process log
                                info!("Processed log {} from tx {}", log_index, tx);
                            }
                        }
                    }
                } => {},
            }
        }
    }

    /// Get indexer state
    pub fn get_state(&self) -> IndexerState {
        self.state.read().clone()
    }

    /// Get metrics
    pub async fn get_metrics(&self) -> Result<IndexerMetrics, IndexerError> {
        let blocks_count: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM blocks")
            .fetch_one(&self.db)
            .await
            .map_err(|e| IndexerError::Database(e))?;

        let transactions_count: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM transactions")
            .fetch_one(&self.db)
            .await
            .map_err(|e| IndexerError::Database(e))?;

        let tokens_count: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM tokens")
            .fetch_one(&self.db)
            .await
            .map_err(|e| IndexerError::Database(e))?;

        let state = self.state.read();

        Ok(IndexerMetrics {
            blocks_count: blocks_count.0 as u64,
            transactions_count: transactions_count.0 as u64,
            tokens_count: tokens_count.0 as u64,
            current_block: state.current_block,
            last_indexed_block: state.last_indexed_block,
            last_confirmed_block: state.last_confirmed_block,
            is_running: state.is_running,
            errors_count: state.errors.len() as u64,
        })
    }
}

// ============================================================================
// Metrics
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IndexerMetrics {
    pub blocks_count: u64,
    pub transactions_count: u64,
    pub tokens_count: u64,
    pub current_block: u64,
    pub last_indexed_block: u64,
    pub last_confirmed_block: u64,
    pub is_running: bool,
    pub errors_count: u64,
}

// ============================================================================
// Helper Functions
// ============================================================================

/// Parse an Ethereum address from string
pub fn parse_address(s: &str) -> Option<Address> {
    let s = s.strip_prefix("0x").unwrap_or(s);
    if s.len() != 40 {
        return None;
    }
    
    let bytes = hex::decode(s).ok()?;
    if bytes.len() != 20 {
        return None;
    }
    
    Some(Address::from_slice(&bytes))
}

/// Parse an Ethereum hash from string
pub fn parse_hash(s: &str) -> Option<H256> {
    let s = s.strip_prefix("0x").unwrap_or(s);
    if s.len() != 64 {
        return None;
    }
    
    let bytes = hex::decode(s).ok()?;
    if bytes.len() != 32 {
        return None;
    }
    
    Some(H256::from_slice(&bytes))
}

/// Format address to checksum
pub fn to_checksum(address: &str) -> String {
    if let Some(addr) = parse_address(address) {
        format!("{:?}", addr)
    } else {
        address.to_string()
    }
}

/// Parse IPFS URI
pub fn parse_ipfs_uri(uri: &str) -> Option<String> {
    let ipfs_gateway = "https://ipfs.io/ipfs/";
    
    if uri.starts_with("ipfs://") {
        let hash = uri.strip_prefix("ipfs://").unwrap_or(uri);
        Some(format!("{}{}", ipfs_gateway, hash))
    } else if uri.starts_with("ipfs://") {
        Some(uri.to_string())
    } else {
        None
    }
}

/// Detect contract type from bytecode
pub fn detect_contract_type(bytecode: &str) -> &'static str {
    if bytecode.is_empty() || bytecode == "0x" {
        return "EOA";
    }

    // Simple heuristics
    if bytecode.contains("ddf252ad") {
        return "Token";
    }
    
    if bytecode.contains("a22cb465") || bytecode.contains("42842e0e") {
        return "NFT";
    }
    
    "Contract"
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_address() {
        let addr = "0x742d35Cc6634C0532925a3b8D3812e09e48F2F0504";
        let parsed = parse_address(addr);
        assert!(parsed.is_some());
    }

    #[test]
    fn test_parse_hash() {
        let hash = "0xddf252ad1be2c89b69c2b068fc378da9529521f1c5d5ba481f6d2a9c92955c5be";
        let parsed = parse_hash(hash);
        assert!(parsed.is_some());
    }

    #[test]
    fn test_detect_contract_type() {
        assert_eq!(detect_contract_type("0x"), "EOA");
        assert_eq!(detect_contract_type(""), "EOA");
    }
}