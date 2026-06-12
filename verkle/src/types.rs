//! Verkle Types

use serde::{Deserialize, Serialize};

// =============================================================================
// VERKLE TREE
// =============================================================================

/// Verkle Tree
pub struct VerkleTree {
    root: Option<Vec<u8>>,
}

impl VerkleTree {
    pub fn new() -> Self {
        Self { root: None }
    }

    /// Insert
    pub fn insert(&mut self, key: &[u8], value: &[u8]) {}

    /// Get
    pub fn get(&self, key: &[u8]) -> Option<Vec<u8>> {
        None
    }

    /// Delete
    pub fn delete(&mut self, key: &[u8]) -> bool {
        false
    }

    /// Get root
    pub fn root(&self) -> Option<&[u8]> {
        self.root.as_deref()
    }
}

impl Default for VerkleTree {
    fn default() -> Self {
        Self::new()
    }
}