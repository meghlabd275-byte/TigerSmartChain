//! Browser Extension Types

use serde::{Deserialize, Serialize};

// =============================================================================
// BROWSER EXTENSION
// =============================================================================

/// Extension Config
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExtensionConfig {
    pub network_id: u64,
    pub rpc_url: String,
    pub chain_id: u64,
}

/// Transaction Preview
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TransactionPreview {
    pub from: String,
    pub to: String,
    pub value: u64,
    pub gas: u64,
    pub data: Vec<u8>,
}

/// Extension State
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExtensionState {
    pub connected: bool,
    pub address: Option<String>,
    pub chain_id: u64,
    pub balance: u64,
}

impl ExtensionState {
    pub fn new() -> Self {
        Self {
            connected: false,
            address: None,
            chain_id: 1,
            balance: 0,
        }
    }
}

impl Default for ExtensionState {
    fn default() -> Self {
        Self::new()
    }
}