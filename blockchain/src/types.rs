//! Blockchain Types

use serde::{Deserialize, Serialize};

// =============================================================================
// BLOCK
// =============================================================================

/// Block Header
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BlockHeader {
    pub parent_hash: String,
    pub sha3_uncles: String,
    pub miner: String,
    pub state_root: String,
    pub transactions_root: String,
    pub receipts_root: String,
    pub logs_bloom: String,
    pub difficulty: String,
    pub number: u64,
    pub gas_limit: u64,
    pub gas_used: u64,
    pub timestamp: u64,
    pub extra_data: String,
    pub mix_hash: String,
    pub nonce: String,
    pub base_fee_per_gas: Option<u64>,
    pub withdrawals_root: Option<String>,
}

/// Block
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Block {
    pub header: BlockHeader,
    pub transactions: Vec<Transaction>,
    pub uncles: Vec<String>,
}

// =============================================================================
// TRANSACTION
// =============================================================================

/// Transaction
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Transaction {
    pub hash: String,
    pub nonce: u64,
    pub block_hash: Option<String>,
    pub block_number: Option<u64>,
    pub transaction_index: Option<u64>,
    pub from: String,
    pub to: Option<String>,
    pub value: String,
    pub gas_price: u64,
    pub gas: u64,
    pub input: String,
    pub v: String,
    pub r: String,
    pub s: String,
}

/// Transaction Status
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TransactionStatus {
    pub transaction_hash: String,
    pub block_hash: String,
    pub block_number: u64,
    pub contract_address: Option<String>,
    pub cumulative_gas_used: u64,
    pub gas_used: u64,
    pub logs: Vec<Log>,
    pub logs_bloom: String,
    pub status: bool,
}

/// Log
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Log {
    pub address: String,
    pub topics: Vec<String>,
    pub data: String,
    pub log_index: u64,
    pub transaction_index: u64,
    pub transaction_hash: String,
    pub block_number: u64,
    pub block_hash: String,
    pub removed: bool,
}

// =============================================================================
// CHAIN
// =============================================================================

/// Chain Config
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChainConfig {
    pub chain_id: u64,
    pub homestead_block: Option<u64>,
    pub dao_fork_block: Option<u64>,
    pub eip150_block: Option<u64>,
    pub eip150_hash: Option<String>,
    pub eip155_block: Option<u64>,
    pub eip158_block: Option<u64>,
    pub byzantium_block: Option<u64>,
    pub constantinople_block: Option<u64>,
    pub petersburg_block: Option<u64>,
    pub istanbul_block: Option<u64>,
    pub eip1283_block: Option<u64>,
    pub berlin_block: Option<u64>,
    pub london_block: Option<u64>,
    pub merge_fork_block: Option<u64>,
    pub shanghai_block: Option<u64>,
    pub cancel_deneb: Option<u64>,
}

impl Default for ChainConfig {
    fn default() -> Self {
        Self {
            chain_id: 1,
            homestead_block: Some(0),
            dao_fork_block: None,
            eip150_block: Some(0),
            eip150_hash: None,
            eip155_block: Some(0),
            eip158_block: Some(0),
            byzantium_block: Some(0),
            constantinople_block: Some(0),
            petersburg_block: Some(0),
            istanbul_block: Some(0),
            eip1283_block: None,
            berlin_block: Some(0),
            london_block: Some(0),
            merge_fork_block: None,
            shanghai_block: None,
            cancel_deneb: None,
        }
    }
}

// =============================================================================
// GENESIS
// =============================================================================

/// Genesis
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Genesis {
    pub config: ChainConfig,
    pub nonce: u64,
    pub timestamp: String,
    pub extra_data: String,
    pub gas_limit: String,
    pub difficulty: String,
    pub mix_hash: String,
    pub coinbase: String,
    pub alloc: std::collections::HashMap<String, Account>,
}

/// Account
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Account {
    pub balance: String,
    pub nonce: Option<u64>,
    pub code: Option<String>,
    pub storage: std::collections::HashMap<String, String>,
}

// =============================================================================
// HEADER
// =============================================================================

/// Block Id
#[derive(Debug, Clone)]
pub enum BlockId {
    Number(u64),
    Hash(String),
    Latest,
    Earliest,
    Pending,
}