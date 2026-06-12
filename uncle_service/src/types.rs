//! Uncle Service Types

use serde::{Deserialize, Serialize};

// =============================================================================
// UNCLE SERVICE
// =============================================================================

/// Uncle Block
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UncleBlock {
    pub hash: String,
    pub number: u64,
    pub miner: String,
    pub timestamp: u64,
    pub gas_used: u64,
}

/// Uncle Service
pub struct Service {
    uncles: std::collections::HashMap<String, UncleBlock>,
}

impl Service {
    pub fn new() -> Self {
        Self {
            uncles: std::collections::HashMap::new(),
        }
    }

    /// Add uncle
    pub fn add_uncle(&mut self, hash: String, uncle: UncleBlock) {
        self.uncles.insert(hash, uncle);
    }

    /// Get uncle
    pub fn get_uncle(&self, hash: &str) -> Option<&UncleBlock> {
        self.uncles.get(hash)
    }
}

impl Default for Service {
    fn default() -> Self {
        Self::new()
    }
}