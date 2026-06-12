//! Privacy Types

use serde::{Deserialize, Serialize};

// =============================================================================
// SHIELDED TRANSACTION
// =============================================================================

/// Shielded Transaction
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShieldedTransaction {
    pub proof: Vec<u8>,
    pub commitment: String,
    pub nullifier: String,
}

/// Shielded Pool
pub struct ShieldedPool {
    transactions: Vec<ShieldedTransaction>,
}

impl ShieldedPool {
    pub fn new() -> Self {
        Self {
            transactions: vec![],
        }
    }

    /// Add transaction
    pub fn add(&mut self, tx: ShieldedTransaction) {
        self.transactions.push(tx);
    }

    /// Get transactions
    pub fn transactions(&self) -> &Vec<ShieldedTransaction> {
        &self.transactions
    }
}

impl Default for ShieldedPool {
    fn default() -> Self {
        Self::new()
    }
}