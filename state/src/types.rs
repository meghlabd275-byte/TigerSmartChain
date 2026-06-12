//! State Types

use serde::{Deserialize, Serialize};

// =============================================================================
// ACCOUNT
// =============================================================================

/// Account
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Account {
    pub nonce: u64,
    pub balance: String,
    pub code_hash: String,
    pub storage_root: String,
}

impl Default for Account {
    fn default() -> Self {
        Self {
            nonce: 0,
            balance: "0".to_string(),
            code_hash: "0".to_string(),
            storage_root: "0".to_string(),
        }
    }
}

// =============================================================================
// STATE
// =============================================================================

/// State
#[derive(Debug, Clone)]
pub struct State {
    accounts: std::collections::HashMap<String, Account>,
}

impl State {
    pub fn new() -> Self {
        Self {
            accounts: std::collections::HashMap::new(),
        }
    }

    /// Get account
    pub fn get_account(&self, address: &str) -> Option<&Account> {
        self.accounts.get(address)
    }

    /// Set account
    pub fn set_account(&mut self, address: String, account: Account) {
        self.accounts.insert(address, account);
    }

    /// Delete account
    pub fn delete_account(&mut self, address: &str) {
        self.accounts.remove(address);
    }
}

impl Default for State {
    fn default() -> Self {
        Self::new()
    }
}

// =============================================================================
// STATE SNAPSHOT
// =============================================================================

/// State Snapshot
#[derive(Debug, Clone)]
pub struct StateSnapshot {
    pub root: String,
    pub accounts: std::collections::HashMap<String, Account>,
}