//! SDK Service Types

use serde::{Deserialize, Serialize};

// =============================================================================
// SDK SERVICE
// =============================================================================

/// Client
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Client {
    pub rpc_url: String,
    pub chain_id: u64,
    pub address: Option<String>,
}

/// Provider
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Provider {
    pub name: String,
    pub rpc_url: String,
    pub chain_id: u64,
}

/// Wallet
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Wallet {
    pub address: String,
    pub private_key: String,
}

/// SDK Service
pub struct Service {
    providers: std::collections::HashMap<u64, Provider>,
}

impl Service {
    pub fn new() -> Self {
        Self {
            providers: std::collections::HashMap::new(),
        }
    }

    /// Add provider
    pub fn add_provider(&mut self, provider: Provider) {
        self.providers.insert(provider.chain_id, provider);
    }

    /// Get provider
    pub fn get_provider(&self, chain_id: u64) -> Option<&Provider> {
        self.providers.get(&chain_id)
    }
}

impl Default for Service {
    fn default() -> Self {
        Self::new()
    }
}