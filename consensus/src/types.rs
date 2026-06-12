//! Consensus Types

use serde::{Deserialize, Serialize};

// =============================================================================
// CONSENSUS
// =============================================================================

/// Validator
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Validator {
    pub address: String,
    pub stake: u64,
    pub delegated: u64,
    pub commission: u8,
    pub active: bool,
}

/// Block
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Block {
    pub hash: String,
    pub number: u64,
    pub proposer: String,
    pub transactions: Vec<String>,
    pub receipts_root: String,
    pub state_root: String,
}

/// Vote
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Vote {
    pub validator: String,
    pub block_hash: String,
    pub block_number: u64,
    pub signature: Vec<u8>,
}