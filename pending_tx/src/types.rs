//! Pending Transaction Types

use serde::{Deserialize, Serialize};

// =============================================================================
// PENDING TRANSACTION
// =============================================================================

/// Pending Transaction
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PendingTransaction {
    pub hash: String,
    pub from: String,
    pub to: String,
    pub value: u64,
    pub gas: u64,
    pub gas_price: u64,
    pub nonce: u64,
    pub input: Vec<u8>,
    pub received_at: u64,
}

/// Pending Transaction Pool
pub struct Pool {
    transactions: std::collections::HashMap<String, PendingTransaction>,
}

impl Pool {
    pub fn new() -> Self {
        Self {
            transactions: std::collections::HashMap::new(),
        }
    }

    /// Add transaction
    pub fn add(&mut self, tx: PendingTransaction) {
        self.transactions.insert(tx.hash.clone(), tx);
    }

    /// Get transaction
    pub fn get(&self, hash: &str) -> Option<&PendingTransaction> {
        self.transactions.get(hash)
    }

    /// Remove transaction
    pub fn remove(&mut self, hash: &str) {
        self.transactions.remove(hash);
    }

    /// List by sender
    pub fn list_by_sender(&self, sender: &str) -> Vec<&PendingTransaction> {
        self.transactions
            .values()
            .filter(|t| t.from == sender)
            .collect()
    }
}

impl Default for Pool {
    fn default() -> Self {
        Self::new()
    }
}