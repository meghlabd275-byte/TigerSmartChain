//! Receipt Types

use serde::{Deserialize, Serialize};

// =============================================================================
// RECEIPT
// =============================================================================

/// Transaction Receipt
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Receipt {
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

/// Bloom Filter
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Bloom {
    pub data: [u8; 256],
}