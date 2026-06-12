//! Transient Storage Opcodes

use std::collections::HashMap;

// =============================================================================
// TRANSIENT STORE
// =============================================================================

/// Transient Storage
pub struct TransientStorage {
    data: HashMap<(String, Vec<u8>), Vec<u8>>,
}

impl TransientStorage {
    pub fn new() -> Self {
        Self {
            data: HashMap::new(),
        }
    }

    /// TLOAD
    pub fn tload(&self, key: &[u8]) -> Option<&Vec<u8>> {
        self.data.get(&(String::new(), key.to_vec()))
    }

    /// TSTORE
    pub fn tstore(&mut self, key: Vec<u8>, value: Vec<u8>) {
        self.data.insert((String::new(), key), value);
    }
}

impl Default for TransientStorage {
    fn default() -> Self {
        Self::new()
    }
}