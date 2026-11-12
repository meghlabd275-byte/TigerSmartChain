//! Transaction pool.
//!
//! A block producer takes transactions from here, so the order it hands them
//! out matters. Two properties have to hold:
//!
//!   1. A sender's transactions are returned in nonce order. The state
//!      transition rejects a transaction whose nonce does not match the
//!      account, so a pool that returned nonce 3 before nonce 2 would have
//!      every block reject the later one and the sender would stall.
//!   2. Replacing a transaction is possible. A client that underpriced a
//!      transaction resends the same nonce with a higher price; if the pool
//!      kept both, one of them would always be rejected at execution.
//!
//! The pool is also bounded. Without a cap a peer can fill memory by
//! submitting transactions, so a full pool rejects new work instead of
//! growing without limit, and the cheapest transactions are evicted first so
//! a full pool still admits better-paying ones.

use crate::types::*;
use std::collections::HashMap;

/// Default ceiling on the number of transactions the pool holds.
pub const DEFAULT_MAX_POOL_SIZE: usize = 8192;

/// Transactions grouped by sender, each sender's group kept in nonce order.
#[derive(Default)]
struct SenderQueue {
    /// The sender's transactions, sorted by nonce.
    transactions: Vec<Transaction>,
}

impl SenderQueue {
    /// Insert `tx`, replacing any existing transaction with the same nonce.
    ///
    /// Returns the transaction that was displaced, if any.
    fn insert(&mut self, tx: Transaction) -> Option<Transaction> {
        match self.transactions.binary_search_by_key(&tx.nonce, |t| t.nonce) {
            Ok(index) => Some(std::mem::replace(&mut self.transactions[index], tx)),
            Err(index) => {
                self.transactions.insert(index, tx);
                None
            }
        }
    }

    fn remove(&mut self, hash: &str) -> Option<Transaction> {
        let index = self.transactions.iter().position(|t| t.hash == hash)?;
        Some(self.transactions.remove(index))
    }

    fn is_empty(&self) -> bool {
        self.transactions.is_empty()
    }
}

/// Transaction Pool
pub struct Mempool {
    /// Transactions by hash, for lookup by the RPC layer.
    by_hash: HashMap<String, Transaction>,
    /// Transactions grouped by sender, in nonce order.
    by_sender: HashMap<String, SenderQueue>,
    max_pool_size: usize,
}

impl Mempool {
    pub fn new() -> Self {
        Self::with_capacity(DEFAULT_MAX_POOL_SIZE)
    }

    /// A pool that holds at most `max_pool_size` transactions.
    pub fn with_capacity(max_pool_size: usize) -> Self {
        Self {
            by_hash: HashMap::new(),
            by_sender: HashMap::new(),
            max_pool_size: max_pool_size.max(1),
        }
    }

    /// The pool's capacity.
    pub fn capacity(&self) -> usize {
        self.max_pool_size
    }

    /// Add a transaction.
    ///
    /// Replaces an existing transaction from the same sender with the same
    /// nonce. When the pool is full and the transaction is not a replacement,
    /// the cheapest transaction in the pool is evicted to make room; if this
    /// transaction is not worth more than that one it is rejected instead.
    pub fn add(&mut self, tx: Transaction) -> Result<(), String> {
        // A malformed transaction cannot be executed, so it must not occupy a
        // slot. The hash is what the pool is keyed by, so it cannot be empty.
        if tx.hash.is_empty() {
            return Err("transaction has no hash".to_string());
        }
        if tx.from.is_empty() {
            return Err("transaction has no sender".to_string());
        }

        let is_replacement = self
            .by_sender
            .get(&tx.from)
            .map(|q| q.transactions.iter().any(|t| t.nonce == tx.nonce))
            .unwrap_or(false);

        if !is_replacement && self.len() >= self.max_pool_size {
            // Make room by evicting the cheapest transaction, unless this one
            // is not worth more than the cheapest.
            let cheapest = self.cheapest().map(|t| (t.hash.clone(), t.gas_price));
            match cheapest {
                Some((hash, price)) if tx.gas_price > price => {
                    self.remove(&hash);
                }
                _ => return Err("mempool is full".to_string()),
            }
        }

        // Inserting a replacement must not leave the displaced hash behind.
        if let Some(queue) = self.by_sender.get_mut(&tx.from) {
            if let Some(displaced) = queue.insert(tx.clone()) {
                self.by_hash.remove(&displaced.hash);
            }
        } else {
            let mut queue = SenderQueue::default();
            queue.insert(tx.clone());
            self.by_sender.insert(tx.from.clone(), queue);
        }

        self.by_hash.insert(tx.hash.clone(), tx);
        Ok(())
    }

    /// The transaction with the lowest gas price, if the pool is not empty.
    fn cheapest(&self) -> Option<&Transaction> {
        self.by_hash.values().min_by_key(|t| t.gas_price)
    }

    /// Look up a transaction by hash.
    pub fn get(&self, hash: &str) -> Option<&Transaction> {
        self.by_hash.get(hash)
    }

    /// Remove a transaction by hash.
    pub fn remove(&mut self, hash: &str) -> Option<Transaction> {
        let tx = self.by_hash.remove(hash)?;
        if let Some(queue) = self.by_sender.get_mut(&tx.from) {
            queue.remove(hash);
            if queue.is_empty() {
                self.by_sender.remove(&tx.from);
            }
        }
        Some(tx)
    }

    /// The transactions to include in the next block.
    ///
    /// Each sender's transactions appear in nonce order. Senders are ordered
    /// by their best gas price, so a block producer that stops early has taken
    /// the best-paying work.
    pub fn pending(&self) -> Vec<&Transaction> {
        let mut senders: Vec<&SenderQueue> = self.by_sender.values().collect();
        senders.sort_by(|a, b| {
            let a_best = a.transactions.first().map(|t| t.gas_price).unwrap_or(0);
            let b_best = b.transactions.first().map(|t| t.gas_price).unwrap_or(0);
            b_best.cmp(&a_best)
        });
        senders
            .into_iter()
            .flat_map(|q| q.transactions.iter())
            .collect()
    }

    /// A sender's transactions, in nonce order.
    pub fn pending_from(&self, sender: &str) -> Vec<&Transaction> {
        self.by_sender
            .get(sender)
            .map(|q| q.transactions.iter().collect())
            .unwrap_or_default()
    }

    /// The number of transactions in the pool.
    pub fn len(&self) -> usize {
        self.by_hash.len()
    }

    pub fn is_empty(&self) -> bool {
        self.by_hash.is_empty()
    }

    /// The number of distinct senders.
    pub fn sender_count(&self) -> usize {
        self.by_sender.len()
    }

    /// Discard every transaction.
    pub fn clear(&mut self) {
        self.by_hash.clear();
        self.by_sender.clear();
    }
}

impl Default for Mempool {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tx(hash: &str, from: &str, nonce: u64, gas_price: u64) -> Transaction {
        Transaction {
            hash: hash.to_string(),
            nonce,
            block_hash: None,
            block_number: None,
            transaction_index: None,
            from: from.to_string(),
            to: Some("0x1111111111111111111111111111111111111111".to_string()),
            value: "0x0".to_string(),
            gas_price,
            gas: 21_000,
            input: "0x".to_string(),
            v: "0x1".to_string(),
            r: "0x1".to_string(),
            s: "0x1".to_string(),
            tx_type: 0,
            access_list: None,
            max_priority_fee_per_gas: None,
            chain_id: None,
        }
    }

    #[test]
    fn a_senders_transactions_come_out_in_nonce_order() {
        let mut pool = Mempool::new();
        // Insert out of order on purpose.
        pool.add(tx("0xc", "alice", 3, 1)).unwrap();
        pool.add(tx("0xa", "alice", 1, 1)).unwrap();
        pool.add(tx("0xb", "alice", 2, 1)).unwrap();

        let nonces: Vec<u64> = pool.pending().iter().map(|t| t.nonce).collect();
        assert_eq!(nonces, vec![1, 2, 3]);
    }

    #[test]
    fn replacing_a_nonce_keeps_only_the_new_transaction() {
        let mut pool = Mempool::new();
        pool.add(tx("0xold", "alice", 1, 10)).unwrap();
        pool.add(tx("0xnew", "alice", 1, 20)).unwrap();

        assert_eq!(pool.len(), 1, "a replacement must not add a second entry");
        assert!(pool.get("0xold").is_none(), "the replaced hash must be gone");
        let kept = pool.get("0xnew").expect("the new transaction must be present");
        assert_eq!(kept.gas_price, 20);
        assert_eq!(pool.pending()[0].hash, "0xnew");
    }

    #[test]
    fn a_full_pool_evicts_the_cheapest_and_rejects_cheaper_arrivals() {
        let mut pool = Mempool::with_capacity(2);
        pool.add(tx("0xa", "alice", 1, 100)).unwrap();
        pool.add(tx("0xb", "bob", 1, 50)).unwrap();

        // A better-paying transaction evicts the cheapest (bob at 50).
        pool.add(tx("0xc", "carol", 1, 75)).unwrap();
        assert_eq!(pool.len(), 2);
        assert!(pool.get("0xb").is_none(), "the cheapest must be evicted");
        assert!(pool.get("0xc").is_some());

        // A transaction that cannot outbid the cheapest is rejected.
        let err = pool.add(tx("0xd", "dave", 1, 10)).unwrap_err();
        assert!(err.contains("full"), "got: {err}");
        assert_eq!(pool.len(), 2);
    }

    #[test]
    fn removal_clears_both_indexes() {
        let mut pool = Mempool::new();
        pool.add(tx("0xa", "alice", 1, 1)).unwrap();
        pool.add(tx("0xb", "alice", 2, 1)).unwrap();

        let removed = pool.remove("0xa").expect("must be present");
        assert_eq!(removed.nonce, 1);
        assert!(pool.get("0xa").is_none());
        assert_eq!(pool.pending_from("alice").len(), 1);
        assert_eq!(pool.pending_from("alice")[0].nonce, 2);

        // Removing the sender's last transaction drops the sender entirely.
        pool.remove("0xb");
        assert_eq!(pool.sender_count(), 0);
        assert!(pool.is_empty());
    }

    #[test]
    fn a_malformed_transaction_is_rejected() {
        let mut pool = Mempool::new();
        let no_hash = tx("", "alice", 1, 1);
        assert!(pool.add(no_hash).is_err());

        let no_sender = tx("0xa", "", 1, 1);
        assert!(pool.add(no_sender).is_err());
        assert!(pool.is_empty());
    }

    #[test]
    fn the_best_paying_sender_is_served_first() {
        let mut pool = Mempool::new();
        pool.add(tx("0xa", "alice", 1, 10)).unwrap();
        pool.add(tx("0xb", "bob", 1, 90)).unwrap();

        let pending = pool.pending();
        assert_eq!(pending[0].from, "bob", "the higher-paying sender comes first");
    }
}
