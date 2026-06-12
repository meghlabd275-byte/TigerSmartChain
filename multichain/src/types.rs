//! Multichain Types

use serde::{Deserialize, Serialize};

// =============================================================================
// MULTICHAIN
// =============================================================================

/// Chain
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Chain {
    pub id: u64,
    pub name: String,
    pub symbol: String,
    pub rpc_url: String,
    pub explorer_url: String,
    pub active: bool,
}

/// Chain Config
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChainConfig {
    pub chain_id: u64,
    pub name: String,
    pub rpc_url: String,
    pub ws_url: String,
    pub native_token: String,
    pub block_time: u64,
}

/// Multichain Service
pub struct Service {
    chains: std::collections::HashMap<u64, Chain>,
}

impl Service {
    pub fn new() -> Self {
        Self {
            chains: std::collections::HashMap::new(),
        }
    }

    /// Add chain
    pub fn add_chain(&mut self, chain: Chain) {
        self.chains.insert(chain.id, chain);
    }

    /// Get chain
    pub fn get_chain(&self, id: u64) -> Option<&Chain> {
        self.chains.get(&id)
    }
}

impl Default for Service {
    fn default() -> Self {
        Self::new()
    }
}