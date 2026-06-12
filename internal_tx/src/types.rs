//! Internal Transaction Types

use serde::{Deserialize, Serialize};

// =============================================================================
// INTERNAL TRANSACTION
// =============================================================================

/// Internal Transaction
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InternalTransaction {
    pub hash: String,
    pub from: String,
    pub to: String,
    pub value: u64,
    pub input: Vec<u8>,
    pub call_type: String,
    pub depth: u32,
}

/// Trace
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Trace {
    pub tx_hash: String,
    pub internal_txs: Vec<InternalTransaction>,
}