//! State DB Types

use serde::{Deserialize, Serialize};
use rocksdb::DB;

// =============================================================================
// STATE DATABASE
// =============================================================================

/// State Database
pub struct StateDB {
    pub db: Option<DB>,
    // Memory cache
    pub(crate) accounts: std::collections::HashMap<String, Vec<u8>>,
    pub(crate) code: std::collections::HashMap<String, Vec<u8>>,
    pub(crate) storage: std::collections::HashMap<(String, Vec<u8>), Vec<u8>>,
}

impl StateDB {
    pub fn new() -> Self {
        Self {
            db: None,
            accounts: std::collections::HashMap::new(),
            code: std::collections::HashMap::new(),
            storage: std::collections::HashMap::new(),
        }
    }

    /// Get account from memory
    pub fn get_account(&self, address: &str) -> Option<&Vec<u8>> {
        self.accounts.get(address)
    }

    /// Set account in memory
    pub fn set_account(&mut self, address: String, data: Vec<u8>) {
        self.accounts.insert(address, data);
    }

    /// Get code from memory
    pub fn get_code(&self, address: &str) -> Option<&Vec<u8>> {
        self.code.get(address)
    }

    /// Set code in memory
    pub fn set_code(&mut self, address: String, code: Vec<u8>) {
        self.code.insert(address, code);
    }

    /// Get storage from memory
    pub fn get_storage(&self, address: &str, key: &[u8]) -> Option<&Vec<u8>> {
        self.storage.get(&(address.to_string(), key.to_vec()))
    }

    /// Set storage in memory
    pub fn set_storage(&mut self, address: String, key: Vec<u8>, value: Vec<u8>) {
        self.storage.insert((address, key), value);
    }
}

impl Default for StateDB {
    fn default() -> Self {
        Self::new()
    }
}
