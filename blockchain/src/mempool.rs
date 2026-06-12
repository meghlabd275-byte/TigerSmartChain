//! Mempool

use crate::types::*;
use std::collections::{HashMap, VecDeque};

// =============================================================================
// MEMPOOL
// =============================================================================

/// Transaction Pool
pub struct Mempool {
    pending: HashMap<String, Transaction>,
    queued: HashMap<String, Transaction>,
    max_pending: usize,
    max_queued: usize,
}

impl Mempool {
    pub fn new() -> Self {
        Self {
            pending: HashMap::new(),
            queued: HashMap::new(),
            max_pending: 4096,
            max_queued: 8192,
        }
    }

    /// Add transaction
    pub fn add(&mut self, tx: Transaction) -> Result<(), String> {
        if self.pending.len() >= self.max_pending {
            return Err("Mempool full".to_string());
        }
        self.pending.insert(tx.hash.clone(), tx);
        Ok(())
    }

    /// Get transaction
    pub fn get(&self, hash: &str) -> Option<&Transaction> {
        self.pending.get(hash).or_else(|| self.queued.get(hash))
    }

    /// Remove transaction
    pub fn remove(&mut self, hash: &str) -> Option<Transaction> {
        self.pending.remove(hash).or_else(|| self.queued.remove(hash))
    }

    /// Get pending transactions
    pub fn pending(&self) -> Vec<&Transaction> {
        self.pending.values().collect()
    }

    /// Get queued transactions
    pub fn queued(&self) -> Vec<&Transaction> {
        self.queued.values().collect()
    }

    /// Get count
    pub fn count(&self) -> usize {
        self.pending.len() + self.queued.len()
    }

    /// Clear
    pub fn clear(&mut self) {
        self.pending.clear();
        self.queued.clear();
    }
}

impl Default for Mempool {
    fn default() -> Self {
        Self::new()
    }
}

// =============================================================================
// TX POOL
// =============================================================================

/// Transaction Pool Manager
pub struct TxPool {
    mempool: Mempool,
    by_nonce: HashMap<String, VecDeque<Transaction>>,
    by_price: HashMap<String, VecDeque<Transaction>>,
}

impl TxPool {
    pub fn new() -> Self {
        Self {
            mempool: Mempool::new(),
            by_nonce: HashMap::new(),
            by_price: HashMap::new(),
        }
    }

    /// Add transaction
    pub fn add(&mut self, tx: Transaction) -> Result<(), String> {
        self.mempool.add(tx)
    }

    /// Get by nonce
    pub fn get_by_nonce(&self, from: &str) -> Vec<&Transaction> {
        self.by_nonce.get(from).map(|q| q.iter().collect()).unwrap_or_default()
    }

    /// Get by price
    pub fn get_by_price(&self, from: &str) -> Vec<&Transaction> {
        self.by_price.get(from).map(|q| q.iter().collect()).unwrap_or_default()
    }

    /// Get pending
    pub fn pending(&self) -> Vec<&Transaction> {
        self.mempool.pending()
    }
}

impl Default for TxPool {
    fn default() -> Self {
        Self::new()
    }
}