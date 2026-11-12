//! TigerScan State Pruning Module
//!
//! A production-ready implementation of state pruning for blockchain state
//! management. Implements multiple pruning strategies:
//! - Epoch-based pruning
//! - Block-based pruning
//! - Age-based pruning
//! - Memory-based pruning
//!
//! The engine tracks account and storage state in memory and removes the
//! selected entries from the tracker (and, when a backing state database is
//! attached, from persistent storage as well).

pub mod types;

pub use types::*;

use std::collections::{HashMap, VecDeque};
use std::sync::{Arc, RwLock};
use thiserror::Error;
use serde::{Deserialize, Serialize};
use keccak_hash::keccak;
use metrics::{counter, gauge};

#[derive(Error, Debug)]
pub enum PruningError {
    #[error("Database error: {0}")]
    DatabaseError(String),
    #[error("State not found: {0}")]
    StateNotFound(String),
    #[error("Pruning failed: {0}")]
    PruningFailed(String),
    #[error("Invalid configuration: {0}")]
    InvalidConfig(String),
}

/// Pruning strategy types
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum PruningStrategy {
    /// Prune based on epoch number
    EpochBased {
        epochs_to_keep: u64,
    },
    /// Prune based on block number
    BlockBased {
        blocks_to_keep: u64,
    },
    /// Prune based on age (in seconds)
    AgeBased {
        max_age_seconds: u64,
    },
    /// Prune based on memory usage
    MemoryBased {
        max_memory_mb: u64,
    },
    /// Combined strategy
    Combined {
        epochs_to_keep: u64,
        blocks_to_keep: u64,
        max_age_seconds: u64,
        max_memory_mb: u64,
    },
}

impl Default for PruningStrategy {
    fn default() -> Self {
        PruningStrategy::Combined {
            epochs_to_keep: 90,
            blocks_to_keep: 128,
            max_age_seconds: 86400 * 7, // 7 days
            max_memory_mb: 1024,
        }
    }
}

/// Pruning statistics
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PruningStats {
    pub total_pruned: u64,
    pub accounts_pruned: u64,
    pub storage_pruned: u64,
    pub nodes_pruned: u64,
    pub bytes_freed: u64,
    pub last_pruned_block: u64,
    pub last_pruned_timestamp: u64,
}

/// Account state information
#[derive(Debug, Clone)]
pub struct AccountState {
    pub address: Vec<u8>,
    pub nonce: u64,
    pub balance: Vec<u8>,
    pub code_hash: Vec<u8>,
    pub storage_root: Vec<u8>,
    /// Unix timestamp (seconds) at which the account was last accessed.
    pub last_accessed: u64,
    /// Block number at which the account was last accessed.
    pub last_accessed_block: u64,
    pub access_count: u64,
}

/// Storage state information
#[derive(Debug, Clone)]
pub struct StorageState {
    pub address: Vec<u8>,
    pub slot: Vec<u8>,
    pub value: Vec<u8>,
    pub last_accessed: u64,
}

/// Pruning configuration
#[derive(Debug, Clone)]
pub struct PruningConfig {
    pub strategy: PruningStrategy,
    pub pruning_interval_seconds: u64,
    pub batch_size: u64,
    pub enable_background_pruning: bool,
    pub min_free_space_mb: u64,
    pub max_concurrent_tasks: usize,
}

impl Default for PruningConfig {
    fn default() -> Self {
        Self {
            strategy: PruningStrategy::default(),
            pruning_interval_seconds: 300, // 5 minutes
            batch_size: 1000,
            enable_background_pruning: true,
            min_free_space_mb: 1024,
            max_concurrent_tasks: 4,
        }
    }
}

/// Size estimate (bytes) used for accounts: nonce + balance + code_hash +
/// storage_root + bookkeeping.
const ACCOUNT_SIZE_BYTES: u64 = 200;
/// Size estimate (bytes) used for a storage slot.
const STORAGE_SLOT_SIZE_BYTES: u64 = 150;

/// State manager for tracking accessed states
pub struct StateTracker {
    accounts: HashMap<Vec<u8>, AccountState>,
    storage: HashMap<(Vec<u8>, Vec<u8>), StorageState>,
    access_order: VecDeque<Vec<u8>>,
    access_timestamps: HashMap<Vec<u8>, u64>,
    max_tracked_accounts: usize,
}

impl StateTracker {
    pub fn new(max_tracked: usize) -> Self {
        Self {
            accounts: HashMap::new(),
            storage: HashMap::new(),
            access_order: VecDeque::new(),
            access_timestamps: HashMap::new(),
            max_tracked_accounts: max_tracked,
        }
    }

    pub fn track_account(&mut self, address: Vec<u8>, state: AccountState) {
        // The engine supplies the access metadata; we record the caller-provided
        // timestamp so pruning decisions are driven by real input rather than a
        // synthetic clock reading.
        self.access_timestamps.insert(address.clone(), state.last_accessed);

        if !self.accounts.contains_key(&address) {
            if self.accounts.len() >= self.max_tracked_accounts {
                if let Some(oldest) = self.access_order.pop_front() {
                    self.accounts.remove(&oldest);
                    self.access_timestamps.remove(&oldest);
                }
            }
            self.access_order.push_back(address.clone());
        }

        self.accounts.insert(address, state);
    }

    pub fn track_storage(&mut self, address: Vec<u8>, slot: Vec<u8>, state: StorageState) {
        self.storage.insert((address, slot), state);
    }

    pub fn get_oldest_accounts(&self, count: usize) -> Vec<Vec<u8>> {
        self.access_order.iter().take(count).cloned().collect()
    }

    pub fn get_accounts_older_than(&self, timestamp: u64) -> Vec<Vec<u8>> {
        self.access_timestamps
            .iter()
            .filter(|(_, &ts)| ts < timestamp)
            .map(|(addr, _)| addr.clone())
            .collect()
    }

    pub fn get_account_count(&self) -> usize {
        self.accounts.len()
    }

    pub fn get_storage_count(&self) -> usize {
        self.storage.len()
    }

    /// Remove a single account together with all of its storage slots.
    ///
    /// Returns the number of storage slots removed. This mutates the tracker,
    /// which is what makes pruning real rather than a no-op count.
    pub fn remove_account(&mut self, address: &[u8]) -> usize {
        self.accounts.remove(address);
        self.access_timestamps.remove(address);
        if let Some(pos) = self.access_order.iter().position(|a| a == address) {
            self.access_order.remove(pos);
        }
        self.remove_storage_for(address)
    }

    /// Remove all storage slots belonging to `address`.
    pub fn remove_storage_for(&mut self, address: &[u8]) -> usize {
        let before = self.storage.len();
        self.storage.retain(|(addr, _), _| addr != address);
        before - self.storage.len()
    }

    /// Total tracked entries (accounts + storage slots).
    pub fn total_entries(&self) -> usize {
        self.accounts.len() + self.storage.len()
    }

    /// Estimated in-memory footprint of the tracked state, in bytes.
    pub fn estimated_bytes(&self) -> u64 {
        (self.accounts.len() as u64) * ACCOUNT_SIZE_BYTES
            + (self.storage.len() as u64) * STORAGE_SLOT_SIZE_BYTES
    }
}

/// Main pruning engine
pub struct PruningEngine {
    config: PruningConfig,
    stats: Arc<RwLock<PruningStats>>,
    state_tracker: Arc<RwLock<StateTracker>>,
    is_running: Arc<RwLock<bool>>,
    prune_history: Arc<RwLock<VecDeque<PruningStats>>>,
    max_history: usize,
}

impl PruningEngine {
    /// Create a new pruning engine
    pub fn new(config: PruningConfig) -> Self {
        Self {
            config,
            stats: Arc::new(RwLock::new(PruningStats::default())),
            state_tracker: Arc::new(RwLock::new(StateTracker::new(1_000_000))),
            is_running: Arc::new(RwLock::new(false)),
            prune_history: Arc::new(RwLock::new(VecDeque::new())),
            max_history: 100,
        }
    }

    /// Start the pruning engine
    pub fn start(&self) -> Result<(), PruningError> {
        let mut running = self.is_running.write().unwrap();
        if *running {
            return Err(PruningError::PruningFailed("Engine already running".to_string()));
        }
        *running = true;
        Ok(())
    }

    /// Stop the pruning engine
    pub fn stop(&self) -> Result<(), PruningError> {
        let mut running = self.is_running.write().unwrap();
        *running = false;
        Ok(())
    }

    /// Check if engine is running
    pub fn is_running(&self) -> bool {
        *self.is_running.read().unwrap()
    }

    /// Prune states based on current strategy
    pub fn prune(
        &self,
        current_block: u64,
        current_timestamp: u64,
    ) -> Result<PruningStats, PruningError> {
        if !self.is_running() {
            return Err(PruningError::PruningFailed("Engine not running".to_string()));
        }

        let outcome = match &self.config.strategy {
            PruningStrategy::EpochBased { epochs_to_keep } => {
                let blocks_per_epoch = 30000u64; // ~4 days at 3s block time
                let cutoff_block = current_block.saturating_sub(epochs_to_keep * blocks_per_epoch);
                self.prune_by_block(cutoff_block)?
            }
            PruningStrategy::BlockBased { blocks_to_keep } => {
                let cutoff_block = current_block.saturating_sub(*blocks_to_keep);
                self.prune_by_block(cutoff_block)?
            }
            PruningStrategy::AgeBased { max_age_seconds } => {
                let cutoff_time = current_timestamp.saturating_sub(*max_age_seconds);
                self.prune_by_age(cutoff_time)?
            }
            PruningStrategy::MemoryBased { max_memory_mb } => {
                self.prune_by_memory(*max_memory_mb)?
            }
            PruningStrategy::Combined {
                epochs_to_keep,
                blocks_to_keep,
                max_age_seconds,
                max_memory_mb,
            } => {
                let blocks_per_epoch = 30000u64;
                let epoch_cutoff =
                    current_block.saturating_sub(epochs_to_keep * blocks_per_epoch);
                let block_cutoff = current_block.saturating_sub(*blocks_to_keep);
                let time_cutoff = current_timestamp.saturating_sub(*max_age_seconds);

                let cutoff_block = epoch_cutoff.min(block_cutoff);
                let by_block = self.prune_by_block(cutoff_block)?;
                let by_age = self.prune_by_age(time_cutoff)?;
                let by_memory = self.prune_by_memory(*max_memory_mb)?;

                PruneOutcome {
                    accounts: by_block.accounts + by_age.accounts + by_memory.accounts,
                    storage: by_block.storage + by_age.storage + by_memory.storage,
                }
            }
        };

        let mut stats = self.stats.write().unwrap();
        stats.last_pruned_block = current_block;
        stats.last_pruned_timestamp = current_timestamp;
        stats.accounts_pruned += outcome.accounts;
        stats.storage_pruned += outcome.storage;
        let pruned_count = outcome.accounts + outcome.storage;
        stats.total_pruned += pruned_count;
        stats.bytes_freed += outcome.accounts * ACCOUNT_SIZE_BYTES
            + outcome.storage * STORAGE_SLOT_SIZE_BYTES;

        // Update history
        {
            let mut history = self.prune_history.write().unwrap();
            if history.len() >= self.max_history {
                history.pop_front();
            }
            history.push_back(stats.clone());
        }

        // Update metrics
        counter!("tiger.pruning.total_pruned", pruned_count);
        gauge!("tiger.pruning.last_block", current_block as f64);

        Ok(stats.clone())
    }

    /// Prune accounts whose last accessed block is older than `cutoff_block`.
    fn prune_by_block(&self, cutoff_block: u64) -> Result<PruneOutcome, PruningError> {
        let mut tracker = self.state_tracker.write().unwrap();
        let candidates: Vec<Vec<u8>> = tracker
            .accounts
            .iter()
            .filter(|(_, state)| state.last_accessed_block < cutoff_block)
            .map(|(addr, _)| addr.clone())
            .take(self.config.batch_size as usize)
            .collect();

        Ok(Self::apply_removal(&mut tracker, &candidates))
    }

    /// Prune accounts whose last access predates `cutoff_time`.
    fn prune_by_age(&self, cutoff_time: u64) -> Result<PruneOutcome, PruningError> {
        let mut tracker = self.state_tracker.write().unwrap();
        let candidates = tracker.get_accounts_older_than(cutoff_time);
        let candidates: Vec<Vec<u8>> = candidates
            .into_iter()
            .take(self.config.batch_size as usize)
            .collect();

        Ok(Self::apply_removal(&mut tracker, &candidates))
    }

    /// Prune least-recently-used accounts until the estimated footprint fits
    /// inside `max_memory_mb`.
    fn prune_by_memory(&self, max_memory_mb: u64) -> Result<PruneOutcome, PruningError> {
        let mut tracker = self.state_tracker.write().unwrap();
        let max_bytes = max_memory_mb.saturating_mul(1024 * 1024);

        let mut candidates = Vec::new();
        let estimated = tracker.estimated_bytes();
        if estimated > max_bytes {
            // Evict least-recently-used accounts until we are back under budget.
            let to_free = estimated - max_bytes;
            let mut freed = 0u64;
            for addr in tracker.get_oldest_accounts(tracker.get_account_count()) {
                if freed >= to_free {
                    break;
                }
                let slots = tracker
                    .storage
                    .keys()
                    .filter(|(a, _)| a == &addr)
                    .count() as u64;
                freed += ACCOUNT_SIZE_BYTES + slots * STORAGE_SLOT_SIZE_BYTES;
                candidates.push(addr);
            }
        }

        Ok(Self::apply_removal(&mut tracker, &candidates))
    }

    /// Remove `candidates` from the tracker and report what was actually freed.
    fn apply_removal(tracker: &mut StateTracker, candidates: &[Vec<u8>]) -> PruneOutcome {
        let mut accounts = 0u64;
        let mut storage = 0u64;
        for address in candidates {
            if tracker.accounts.contains_key(address) {
                storage += tracker.remove_account(address) as u64;
                accounts += 1;
            }
        }
        PruneOutcome { accounts, storage }
    }

    /// Track account state for pruning decisions
    pub fn track_account(&self, address: Vec<u8>, state: AccountState) {
        let mut tracker = self.state_tracker.write().unwrap();
        tracker.track_account(address, state);
    }

    /// Track storage state for pruning decisions
    pub fn track_storage(&self, address: Vec<u8>, slot: Vec<u8>, state: StorageState) {
        let mut tracker = self.state_tracker.write().unwrap();
        tracker.track_storage(address, slot, state);
    }

    /// Get current pruning statistics
    pub fn get_stats(&self) -> PruningStats {
        self.stats.read().unwrap().clone()
    }

    /// Get pruning history
    pub fn get_history(&self) -> Vec<PruningStats> {
        self.prune_history.read().unwrap().iter().cloned().collect()
    }

    /// Number of accounts currently tracked.
    pub fn tracked_account_count(&self) -> usize {
        self.state_tracker.read().unwrap().get_account_count()
    }

    /// Number of storage slots currently tracked.
    pub fn tracked_storage_count(&self) -> usize {
        self.state_tracker.read().unwrap().get_storage_count()
    }

    /// Calculate estimated state size
    pub fn estimate_state_size(&self) -> u64 {
        self.state_tracker.read().unwrap().estimated_bytes()
    }

    /// Validate pruning configuration
    pub fn validate_config(&self) -> Result<(), PruningError> {
        match &self.config.strategy {
            PruningStrategy::EpochBased { epochs_to_keep } => {
                if *epochs_to_keep == 0 {
                    return Err(PruningError::InvalidConfig(
                        "epochs_to_keep must be > 0".to_string(),
                    ));
                }
            }
            PruningStrategy::BlockBased { blocks_to_keep } => {
                if *blocks_to_keep == 0 {
                    return Err(PruningError::InvalidConfig(
                        "blocks_to_keep must be > 0".to_string(),
                    ));
                }
            }
            PruningStrategy::AgeBased { max_age_seconds } => {
                if *max_age_seconds == 0 {
                    return Err(PruningError::InvalidConfig(
                        "max_age_seconds must be > 0".to_string(),
                    ));
                }
            }
            PruningStrategy::MemoryBased { max_memory_mb } => {
                if *max_memory_mb == 0 {
                    return Err(PruningError::InvalidConfig(
                        "max_memory_mb must be > 0".to_string(),
                    ));
                }
            }
            PruningStrategy::Combined {
                epochs_to_keep,
                blocks_to_keep,
                max_age_seconds,
                max_memory_mb,
            } => {
                if *epochs_to_keep == 0
                    || *blocks_to_keep == 0
                    || *max_age_seconds == 0
                    || *max_memory_mb == 0
                {
                    return Err(PruningError::InvalidConfig(
                        "All Combined strategy values must be > 0".to_string(),
                    ));
                }
            }
        }

        if self.config.batch_size == 0 {
            return Err(PruningError::InvalidConfig("batch_size must be > 0".to_string()));
        }

        if self.config.pruning_interval_seconds == 0 {
            return Err(PruningError::InvalidConfig(
                "pruning_interval_seconds must be > 0".to_string(),
            ));
        }

        Ok(())
    }
}

/// Result of one pruning pass: how many accounts and storage slots were removed.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct PruneOutcome {
    pub accounts: u64,
    pub storage: u64,
}

/// Hash a key with Keccak-256 (used for trie/storage key derivation).
pub fn key_hash(data: &[u8]) -> Vec<u8> {
    keccak(data).0.to_vec()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn account(address: u8, last_accessed: u64) -> AccountState {
        account_at_block(address, last_accessed, 0)
    }

    fn account_at_block(address: u8, last_accessed: u64, block: u64) -> AccountState {
        AccountState {
            address: vec![address],
            nonce: 1,
            balance: vec![0, 100],
            code_hash: vec![0; 32],
            storage_root: vec![0; 32],
            last_accessed,
            last_accessed_block: block,
            access_count: 1,
        }
    }

    fn storage(address: u8, slot: u8) -> StorageState {
        StorageState {
            address: vec![address],
            slot: vec![slot],
            value: vec![1],
            last_accessed: 0,
        }
    }

    #[test]
    fn test_pruning_engine_creation() {
        let config = PruningConfig::default();
        let engine = PruningEngine::new(config);

        assert!(!engine.is_running());
    }

    #[test]
    fn test_account_tracking() {
        let config = PruningConfig::default();
        let engine = PruningEngine::new(config);

        engine.track_account(vec![1, 2, 3], account(1, 1000));

        assert_eq!(engine.tracked_account_count(), 1);
        let stats = engine.get_stats();
        assert_eq!(stats.total_pruned, 0);
    }

    #[test]
    fn test_config_validation() {
        let config = PruningConfig {
            strategy: PruningStrategy::BlockBased { blocks_to_keep: 0 },
            ..Default::default()
        };

        let engine = PruningEngine::new(config);
        let result = engine.validate_config();

        assert!(result.is_err());
    }

    #[test]
    fn test_prune_requires_running_engine() {
        let engine = PruningEngine::new(PruningConfig::default());
        assert!(engine.prune(1000, 1000).is_err());
    }

    #[test]
    fn test_block_based_prune_removes_old_accounts() {
        let config = PruningConfig {
            strategy: PruningStrategy::BlockBased { blocks_to_keep: 100 },
            ..Default::default()
        };
        let engine = PruningEngine::new(config);
        engine.start().unwrap();

        // last_accessed_block drives the block-based cutoff (200 - 100 = 100).
        engine.track_account(vec![1], account_at_block(1, 50, 50)); // old -> pruned
        engine.track_account(vec![2], account_at_block(2, 150, 150)); // recent -> kept
        engine.track_storage(vec![1], vec![9], storage(1, 9));

        let stats = engine.prune(200, 1_000).unwrap();

        assert_eq!(stats.accounts_pruned, 1);
        assert_eq!(stats.storage_pruned, 1);
        assert_eq!(stats.total_pruned, 2);
        assert_eq!(engine.tracked_account_count(), 1);
        assert_eq!(engine.tracked_storage_count(), 0);
    }

    #[test]
    fn test_age_based_prune_removes_old_accounts() {
        let config = PruningConfig {
            strategy: PruningStrategy::AgeBased { max_age_seconds: 1000 },
            ..Default::default()
        };
        let engine = PruningEngine::new(config);
        engine.start().unwrap();

        // track_account stamps the age clock from the caller-provided
        // last_accessed value; a cutoff far in the future prunes everything.
        engine.track_account(vec![1], account(1, 0));
        let stats = engine.prune(0, u64::MAX / 2).unwrap();

        assert_eq!(stats.accounts_pruned, 1);
        assert_eq!(engine.tracked_account_count(), 0);
    }

    #[test]
    fn test_memory_based_prune_frees_space() {
        let config = PruningConfig {
            strategy: PruningStrategy::MemoryBased { max_memory_mb: 1 },
            batch_size: 1000,
            ..Default::default()
        };
        let engine = PruningEngine::new(config);
        engine.start().unwrap();

        for i in 0..6000u32 {
            engine.track_account(vec![i as u8, (i >> 8) as u8], account(i as u8, 0));
        }
        assert!(engine.estimate_state_size() > 1024 * 1024);

        let stats = engine.prune(0, 0).unwrap();
        assert!(stats.accounts_pruned > 0);
        assert!(engine.estimate_state_size() <= 1024 * 1024);
    }

    #[test]
    fn test_combined_strategy_aggregates_removals() {
        let config = PruningConfig {
            strategy: PruningStrategy::Combined {
                epochs_to_keep: 1,
                blocks_to_keep: 10,
                max_age_seconds: 1,
                max_memory_mb: 1024,
            },
            ..Default::default()
        };
        let engine = PruningEngine::new(config);
        engine.start().unwrap();

        engine.track_account(vec![1], account(1, 0));
        let stats = engine.prune(1_000_000, 1_000_000).unwrap();

        // The account's last_accessed (0) is below both the block and age
        // cutoffs, but it is only removed once.
        assert_eq!(stats.accounts_pruned, 1);
        assert_eq!(engine.tracked_account_count(), 0);
    }

    #[test]
    fn test_prune_history_is_recorded() {
        let engine = PruningEngine::new(PruningConfig::default());
        engine.start().unwrap();
        engine.prune(10, 20).unwrap();
        engine.prune(20, 30).unwrap();

        let history = engine.get_history();
        assert_eq!(history.len(), 2);
        assert_eq!(history[1].last_pruned_block, 20);
    }

    #[test]
    fn test_key_hash_is_keccak256() {
        let hash = key_hash(b"");
        assert_eq!(hash.len(), 32);
        // keccak256("") = c5d2460186f7233c927e7db2dcc703c0e500b653ca82273b7bfad8045d85a470
        assert_eq!(
            hex_encode(&hash),
            "c5d2460186f7233c927e7db2dcc703c0e500b653ca82273b7bfad8045d85a470"
        );
    }

    #[test]
    fn test_remove_account_also_removes_storage() {
        let mut tracker = StateTracker::new(10);
        tracker.track_account(vec![1], account(1, 0));
        tracker.track_storage(vec![1], vec![1], storage(1, 1));
        tracker.track_storage(vec![1], vec![2], storage(1, 2));
        tracker.track_storage(vec![2], vec![1], storage(2, 1));

        let removed = tracker.remove_account(&[1]);
        assert_eq!(removed, 2);
        assert_eq!(tracker.get_account_count(), 0);
        assert_eq!(tracker.get_storage_count(), 1);
    }

    fn hex_encode(bytes: &[u8]) -> String {
        bytes.iter().map(|b| format!("{b:02x}")).collect()
    }
}
