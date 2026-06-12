//! Account Types

use serde::{Deserialize, Serialize};

// =============================================================================
// ACCOUNT
// =============================================================================

/// Account
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Account {
    pub address: String,
    pub nonce: u64,
    pub balance: String,
    pub code_hash: String,
    pub storage_root: String,
}

impl Account {
    pub fn new(address: String) -> Self {
        Self {
            address,
            nonce: 0,
            balance: "0".to_string(),
            code_hash: "0".to_string(),
            storage_root: "0".to_string(),
        }
    }
}

/// Account State
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AccountState {
    pub address: String,
    pub balance: String,
    pub code: Vec<u8>,
    pub storage: std::collections::HashMap<String, String>,
}