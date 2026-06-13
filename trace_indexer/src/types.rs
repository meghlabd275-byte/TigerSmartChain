//! Trace Indexer Types for TigerScan

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

// =============================================================================
// TRACE DATA
// =============================================================================

/// Indexed Trace - internal transaction
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IndexedTrace {
    pub id: String,
    pub transaction_hash: String,
    pub block_number: u64,
    pub subtrace_index: u32,
    /// Call type: call, callcode, delegatecall, staticcall, create, create2, suicide
    pub call_type: String,
    /// Address that executed this call
    pub from: String,
    /// Address that received this call
    pub to: String,
    /// Value transferred (in wei)
    pub value: String,
    /// Gas provided
    pub gas: String,
    /// Gas used
    pub gas_used: Option<String>,
    /// Input data
    pub input: String,
    /// Output data
    pub output: Option<String>,
    /// Error if any
    pub error: Option<String>,
    /// Depth in call stack
    pub depth: u32,
    /// Parent trace index
    pub parent_index: Option<u32>,
    /// Trace type: call, create, suicide, reward
    pub trace_type: String,
    /// Transaction hash (additional field for convenience)
    #[serde(default)]
    pub tx_hash: String,
}

impl IndexedTrace {
    pub fn new(
        tx_hash: String,
        block_number: u64,
        subtrace_index: u32,
        call_type: String,
        from: String,
        to: String,
        value: String,
    ) -> Self {
        Self {
            id: format!("{}-{}-{}", tx_hash, block_number, subtrace_index),
            transaction_hash: tx_hash,
            block_number,
            subtrace_index,
            call_type,
            from,
            to,
            value,
            gas: "0x0".to_string(),
            gas_used: None,
            input: String::new(),
            output: None,
            error: None,
            depth: 0,
            parent_index: None,
            trace_type: "call".to_string(),
        }
    }
}

// =============================================================================
// STATE DIFF
// =============================================================================

/// State Diff - balance or storage change
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IndexedStateDiff {
    pub id: String,
    pub transaction_hash: String,
    pub block_number: u64,
    /// Address where change occurred
    pub address: String,
    /// Key for storage (empty for balance)
    pub storage_key: Option<String>,
    /// Previous value
    pub previous: String,
    /// New value
    pub current: String,
    /// Type of change
    pub diff_type: StateDiffType,
}

/// State Diff Type
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum StateDiffType {
    Balance,
    Code,
    Storage,
    Nonce,
}

// =============================================================================
// CONTRACT CREATION
// =============================================================================

/// Contract Creation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContractCreation {
    pub id: String,
    pub transaction_hash: String,
    pub block_number: u64,
    /// Address where the new contract was created
    pub address: String,
    /// Creator address
    pub creator: String,
    /// Initial balance
    pub balance: String,
    /// Contract code (if available)
    pub code: Option<String>,
    /// Code hash
    pub code_hash: Option<String>,
    /// Constructor input
    pub init: String,
}

impl ContractCreation {
    pub fn new(tx_hash: String, block_number: u64, address: String, creator: String) -> Self {
        Self {
            id: format!("{}-{}", tx_hash, address),
            transaction_hash: tx_hash,
            block_number,
            address,
            creator,
            balance: "0x0".to_string(),
            code: None,
            code_hash: None,
            init: String::new(),
        }
    }
}

// =============================================================================
// SELF DESTRUCT
// =============================================================================

/// Self Destruct - contract self-destructed
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SelfDestruct {
    pub id: String,
    pub transaction_hash: String,
    pub block_number: u64,
    /// Contract address that was destroyed
    pub address: String,
    /// Address that received remaining balance
    pub refund_address: String,
    /// Balance at time of destruction
    pub balance: String,
}

// =============================================================================
// TRACE STATS
// =============================================================================

/// Trace Statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TraceStats {
    pub current_block: u64,
    pub indexed_traces: u64,
    pub indexed_state_diffs: u64,
    pub indexed_creations: u64,
    pub indexed_selfdestructs: u64,
    pub last_update: i64,
    pub processing_rate: f64,
}

impl Default for TraceStats {
    fn default() -> Self {
        Self {
            current_block: 0,
            indexed_traces: 0,
            indexed_state_diffs: 0,
            indexed_creations: 0,
            indexed_selfdestructs: 0,
            last_update: Utc::now().timestamp(),
            processing_rate: 0.0,
        }
    }
}

// =============================================================================
// CONFIG
// =============================================================================

/// Trace Indexer Configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TraceIndexerConfig {
    /// RPC URL for connecting to Ethereum node
    pub rpc_url: String,
    /// Starting block for indexing
    pub start_block: u64,
    /// Batch size for processing
    pub batch_size: u32,
    /// Request timeout in seconds
    pub timeout_secs: u64,
    /// Enable trace indexing
    pub enable_traces: bool,
    /// Enable state diff indexing
    pub enable_state_diffs: bool,
    /// Enable contract creation indexing
    pub enable_creations: bool,
    /// Enable self-destruct indexing
    pub enable_selfdestructs: bool,
    /// Trace types to index
    pub trace_types: Vec<String>,
}

impl Default for TraceIndexerConfig {
    fn default() -> Self {
        Self {
            rpc_url: "http://localhost:8545".to_string(),
            start_block: 0,
            batch_size: 10,
            timeout_secs: 30,
            enable_traces: true,
            enable_state_diffs: true,
            enable_creations: true,
            enable_selfdestructs: true,
            trace_types: vec![
                "call".to_string(),
                "create".to_string(),
                "create2".to_string(),
                "suicide".to_string(),
                "reward".to_string(),
            ],
        }
    }
}

// =============================================================================
// TRACE BLOCK RESULT
// =============================================================================

/// Block Trace Result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BlockTraceResult {
    pub block_number: u64,
    pub traces: Vec<IndexedTrace>,
    pub state_diffs: Vec<IndexedStateDiff>,
    pub creations: Vec<ContractCreation>,
    pub selfdestructs: Vec<SelfDestruct>,
}

/// Transaction Trace Result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TransactionTraceResult {
    pub transaction_hash: String,
    pub block_number: u64,
    pub traces: Vec<IndexedTrace>,
    pub state_diffs: Vec<IndexedStateDiff>,
    pub creations: Vec<ContractCreation>,
    pub selfdestructs: Vec<SelfDestruct>,
    /// Call tree for visualization
    pub call_tree: Vec<CallTreeNode>,
}

/// Call Tree Node for visualization
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CallTreeNode {
    pub index: u32,
    pub call_type: String,
    pub from: String,
    pub to: String,
    pub value: String,
    pub gas: String,
    pub input: String,
    pub output: String,
    pub children: Vec<CallTreeNode>,
    pub error: Option<String>,
}