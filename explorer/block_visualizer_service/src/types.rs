//! Visualizer Types

use serde::{Deserialize, Serialize};

/// Block Visualization
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BlockVisualization {
    pub number: u64,
    pub hash: String,
    pub timestamp: i64,
    pub transactions: Vec<TxVisualization>,
    pub gas_used: u64,
    pub gas_limit: u64,
    pub miner: String,
    pub size: usize,
}

/// Transaction Visualization
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TxVisualization {
    pub hash: String,
    pub from: String,
    pub to: String,
    pub value: String,
    pub gas: u64,
    pub status: bool,
    pub call_type: String,
    pub internal_calls: Vec<InternalCall>,
}

/// Internal Call
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InternalCall {
    pub from: String,
    pub to: String,
    pub value: String,
    pub call_type: String,
    pub depth: u32,
}

/// Transaction Flow
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TransactionFlow {
    pub tx_hash: String,
    pub calls: Vec<CallNode>,
    pub flow_type: FlowType,
}

/// Call Node
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CallNode {
    pub id: u32,
    pub call_type: String,
    pub from: String,
    pub to: String,
    pub value: String,
    pub depth: u32,
    pub children: Vec<u32>,
}

/// Flow Type
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FlowType {
    Transfer,
    ContractCall,
    TokenTransfer,
    NFTTransfer,
    Swap,
    Staking,
    Governance,
}

/// Config
#[derive(Debug, Clone, Deserialize)]
pub struct Config {
    pub rpc_url: String,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            rpc_url: "http://localhost:8545".to_string(),
        }
    }
}