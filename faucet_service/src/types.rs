//! Faucet Service Types

use serde::{Deserialize, Serialize};

// =============================================================================
// FAUCET SERVICE
// =============================================================================

/// Faucet
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Faucet {
    pub token: String,
    pub balance: u64,
    pub drip_amount: u64,
    pub cooldown: u64,
}

/// Drip
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Drip {
    pub recipient: String,
    pub amount: u64,
    pub tx_hash: String,
    pub timestamp: u64,
}

/// Faucet Service
pub struct Service {
    faucets: std::collections::HashMap<String, Faucet>,
    drips: Vec<Drip>,
}

impl Service {
    pub fn new() -> Self {
        Self {
            faucets: std::collections::HashMap::new(),
            drips: vec![],
        }
    }

    /// Add faucet
    pub fn add_faucet(&mut self, token: String, faucet: Faucet) {
        self.faucets.insert(token, faucet);
    }

    /// Get faucet
    pub fn get_faucet(&self, token: &str) -> Option<&Faucet> {
        self.faucets.get(token)
    }

    /// Record drip
    pub fn record_drip(&mut self, drip: Drip) {
        self.drips.push(drip);
    }
}

impl Default for Service {
    fn default() -> Self {
        Self::new()
    }
}