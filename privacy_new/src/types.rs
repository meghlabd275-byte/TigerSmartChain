//! Privacy Service Types

use serde::{Deserialize, Serialize};

// =============================================================================
// PRIVACY SERVICE
// =============================================================================

/// Privacy Pool
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PrivacyPool {
    pub id: String,
    pub token: String,
    pub total_deposited: u64,
    pub members: u32,
}

/// Deposit
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Deposit {
    pub hash: String,
    pub pool: String,
    pub depositor: String,
    pub amount: u64,
    pub commitment: String,
    pub timestamp: u64,
}

/// Withdrawal
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Withdrawal {
    pub hash: String,
    pub pool: String,
    pub recipient: String,
    pub amount: u64,
    pub proof: String,
    pub timestamp: u64,
}

/// Privacy Service
pub struct Service {
    pools: std::collections::HashMap<String, PrivacyPool>,
}

impl Service {
    pub fn new() -> Self {
        Self {
            pools: std::collections::HashMap::new(),
        }
    }

    /// Add pool
    pub fn add_pool(&mut self, pool: PrivacyPool) {
        self.pools.insert(pool.id.clone(), pool);
    }

    /// Get pool
    pub fn get_pool(&self, id: &str) -> Option<&PrivacyPool> {
        self.pools.get(id)
    }
}

impl Default for Service {
    fn default() -> Self {
        Self::new()
    }
}