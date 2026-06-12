//! Indexer Types for TigerScan

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

// =============================================================================
// BLOCK DATA
// =============================================================================

/// Indexed Block
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IndexedBlock {
    pub number: u64,
    pub hash: String,
    pub parent_hash: String,
    pub timestamp: i64,
    pub transactions: Vec<IndexedTransaction>,
    pub logs: Vec<IndexedLog>,
    pub internal_txs: Vec<InternalTransaction>,
    pub miner: String,
    pub difficulty: String,
    pub total_difficulty: String,
    pub size: u64,
    pub gas_used: u64,
    pub gas_limit: u64,
    pub base_fee_per_gas: Option<u64>,
}

/// Transaction
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IndexedTransaction {
    pub hash: String,
    pub block_number: u64,
    pub block_hash: String,
    pub transaction_index: u64,
    pub from: String,
    pub to: Option<String>,
    pub value: String,
    pub gas_price: u64,
    pub gas_used: u64,
    pub nonce: u64,
    pub input: String,
    pub status: TransactionStatus,
    pub logs: Vec<String>,
}

/// Transaction Status
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum TransactionStatus {
    Pending,
    Success,
    Failed,
}

/// Log Entry
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IndexedLog {
    pub address: String,
    pub topics: Vec<String>,
    pub data: String,
    pub log_index: u64,
    pub transaction_hash: String,
    pub block_number: u64,
}

/// Internal Transaction
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InternalTransaction {
    pub transaction_hash: String,
    pub block_number: u64,
    pub from: String,
    pub to: String,
    pub value: String,
    pub call_type: CallType,
    pub depth: u32,
}

/// Call Type
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum CallType {
    Call,
    CallCode,
    DelegateCall,
    StaticCall,
    Create,
    Create2,
}

// =============================================================================
// TOKEN DATA
// =============================================================================

/// Token
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IndexedToken {
    pub address: String,
    pub name: String,
    pub symbol: String,
    pub decimals: u8,
    pub total_supply: String,
    pub transfer_count: i64,
    pub holder_count: i64,
}

/// Token Transfer
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenTransfer {
    pub transaction_hash: String,
    pub block_number: u64,
    pub from: String,
    pub to: String,
    pub value: String,
    pub token_address: String,
    pub log_index: u64,
    pub timestamp: i64,
}

/// Token Holder
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenHolder {
    pub address: String,
    pub balance: String,
    pub token_address: String,
}

// =============================================================================
// NFT DATA
// =============================================================================

/// NFT
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IndexedNFT {
    pub address: String,
    pub name: String,
    pub symbol: String,
    pub token_type: NFTType,
    pub total_supply: String,
}

/// NFT Type
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum NFTType {
    ERC721,
    ERC721A,
    ERC1155,
}

/// NFT Transfer
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NFTTransfer {
    pub transaction_hash: String,
    pub block_number: u64,
    pub from: String,
    pub to: String,
    pub token_id: String,
    pub amount: String,
    pub token_address: String,
    pub log_index: u64,
}

// =============================================================================
// CONTRACT DATA
// =============================================================================

/// Contract
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IndexedContract {
    pub address: String,
    pub creator: String,
    pub creation_tx: String,
    pub bytecode: String,
    pub compiled_version: String,
    pub timestamp: i64,
}

// =============================================================================
// STATS
// =============================================================================

/// Indexer Statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IndexerStats {
    pub current_block: u64,
    pub indexed_block: u64,
    pub indexed_transactions: i64,
    pub indexed_logs: i64,
    pub indexed_tokens: i64,
    pub indexed_nfts: i64,
    pub last_update: i64,
    pub processing_rate: f64,
}

// =============================================================================
// CONFIG
// =============================================================================

/// Indexer Configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IndexerConfig {
    pub chain_id: u64,
    pub rpc_url: String,
    pub ws_url: String,
    pub start_block: u64,
    pub batch_size: usize,
    pub confirmations: u64,
    pub parallel_workers: usize,
}

impl Default for IndexerConfig {
    fn default() -> Self {
        Self {
            chain_id: 9001, // TigerChain
            rpc_url: "http://localhost:8545".to_string(),
            ws_url: "ws://localhost:8546".to_string(),
            start_block: 0,
            batch_size: 100,
            confirmations: 12,
            parallel_workers: 4,
        }
    }
}