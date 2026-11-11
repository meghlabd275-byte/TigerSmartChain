//! State Pruning Module for TigerSmartChain
//! 
//! Complete implementation of state trie pruning with:
//! - Historical state access support
//! - Archive node vs pruned node support
//! - Multiple pruning strategies
//! - State proof generation

use std::collections::{HashMap, VecDeque};
use std::sync::Arc;
use std::path::PathBuf;
use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use thiserror::Error;
use tracing::{info, debug};

// Re-export types
pub mod storage;
pub mod proof;
pub mod strategy;

pub use storage::*;
pub use proof::*;
pub use strategy::*;

// =============================================================================
// Errors
// =============================================================================

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
    
    #[error("IO error: {0}")]
    IoError(#[from] std::io::Error),
}

// =============================================================================
// Constants
// =============================================================================

/// Default pruning interval in blocks
const DEFAULT_PRUNING_INTERVAL: u64 = 10000;

// =============================================================================
// Configuration
// =============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PruningConfig {
    /// Enable state pruning
    pub enabled: bool,
    
    /// Pruning strategy
    pub strategy: PruningStrategy,
    
    /// Number of blocks to keep in full (no pruning)
    pub full_blocks: u64,
    
    /// Number of blocks to keep in archive mode
    pub archive_blocks: u64,
    
    /// Pruning interval in blocks
    pub pruning_interval: u64,
    
    /// Enable background pruning
    pub background_pruning: bool,
    
    /// Maximum disk space for state storage (bytes)
    pub max_state_size: u64,
    
    /// Enable state snapshotting
    pub snapshot_enabled: bool,
    
    /// Snapshot interval in blocks
    pub snapshot_interval: u64,
}

impl Default for PruningConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            strategy: PruningStrategy::Interval,
            full_blocks: 128,
            archive_blocks: 90000,
            pruning_interval: DEFAULT_PRUNING_INTERVAL,
            background_pruning: true,
            max_state_size: 500 * 1024 * 1024 * 1024, // 500 GB
            snapshot_enabled: true,
            snapshot_interval: 30000,
        }
    }
}

// =============================================================================
// Pruning Strategy
// =============================================================================

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PruningStrategy {
    /// No pruning - keep all state
    None,
    
    /// Interval-based pruning
    Interval,
    
    /// Absolute pruning - keep only N blocks
    Absolute,
    
    /// Bi-mode: archive vs quick
    BiMode,
    
    /// Hybrid pruning
    Hybrid,
}

impl Default for PruningStrategy {
    fn default() -> Self {
        Self::Interval
    }
}

// =============================================================================
// State Manager
// =============================================================================

/// Main state pruning manager
pub struct StatePruner {
    config: PruningConfig,
    storage: Arc<RwLock<StateStorage>>,
    
    /// Current block number
    current_block: u64,
    
    /// Last pruning block
    last_pruning_block: u64,
    
    /// Last snapshot block
    last_snapshot_block: u64,
    
    /// Pruned state roots
    pruned_roots: HashMap<u64, H256>,
    
    /// Active state roots by block
    state_roots: VecDeque<StateRootInfo>,
    
    /// Statistics
    stats: PruningStats,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StateRootInfo {
    pub block_number: u64,
    pub state_root: H256,
    pub timestamp: u64,
    pub size_estimate: u64,
}

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct PruningStats {
    pub total_pruned_nodes: u64,
    pub total_pruned_bytes: u64,
    pub total_snapshots: u64,
    pub last_pruning_time_ms: u64,
    pub last_pruning_block: u64,
    pub current_state_size: u64,
    pub nodes_in_memory: u64,
}

impl StatePruner {
    /// Create a new state pruner
    pub fn new(config: PruningConfig, db_path: PathBuf) -> Result<Self, PruningError> {
        let storage = StateStorage::new(db_path)?;
        
        Ok(Self {
            config,
            storage: Arc::new(RwLock::new(storage)),
            current_block: 0,
            last_pruning_block: 0,
            last_snapshot_block: 0,
            pruned_roots: HashMap::new(),
            state_roots: VecDeque::new(),
            stats: PruningStats::default(),
        })
    }
    
    /// Create with existing storage
    pub fn with_storage(config: PruningConfig, storage: StateStorage) -> Self {
        Self {
            config,
            storage: Arc::new(RwLock::new(storage)),
            current_block: 0,
            last_pruning_block: 0,
            last_snapshot_block: 0,
            pruned_roots: HashMap::new(),
            state_roots: VecDeque::new(),
            stats: PruningStats::default(),
        }
    }
    
    /// Initialize from existing state
    pub fn initialize(&mut self, current_block: u64, state_root: H256) -> Result<(), PruningError> {
        self.current_block = current_block;
        
        // Load state roots from storage
        {
            let storage = self.storage.read();
            if let Ok(roots) = storage.get_state_roots(current_block.saturating_sub(self.config.full_blocks)) {
                for (block, root) in roots {
                    self.state_roots.push_back(StateRootInfo {
                        block_number: block,
                        state_root: root,
                        timestamp: 0,
                        size_estimate: 0,
                    });
                }
            }
        }
        
        // Add current state root
        self.state_roots.push_back(StateRootInfo {
            block_number: current_block,
            state_root,
            timestamp: 0,
            size_estimate: 0,
        });
        
        info!("State pruner initialized at block {}", current_block);
        Ok(())
    }
    
    /// Add new state root
    pub fn add_state_root(&mut self, block: u64, state_root: H256, timestamp: u64) -> Result<(), PruningError> {
        self.current_block = block;
        
        // Add to state roots
        self.state_roots.push_back(StateRootInfo {
            block_number: block,
            state_root,
            timestamp,
            size_estimate: 0,
        });
        
        // Save to storage
        {
            let mut storage = self.storage.write();
            storage.put_state_root(block, state_root)?;
        }
        
        // Check if pruning is needed
        if self.should_prune() {
            self.prune()?;
        }
        
        // Check if snapshot is needed
        if self.config.snapshot_enabled && self.should_snapshot() {
            self.create_snapshot()?;
        }
        
        Ok(())
    }
    
    /// Check if pruning should run
    fn should_prune(&self) -> bool {
        if !self.config.enabled {
            return false;
        }
        
        if !self.config.background_pruning {
            return false;
        }
        
        self.current_block.saturating_sub(self.last_pruning_block) >= self.config.pruning_interval
    }
    
    /// Check if snapshot should be created
    fn should_snapshot(&self) -> bool {
        if !self.config.snapshot_enabled {
            return false;
        }
        
        self.current_block.saturating_sub(self.last_snapshot_block) >= self.config.snapshot_interval
    }
    
    /// Perform pruning
    pub fn prune(&mut self) -> Result<(), PruningError> {
        let start = std::time::Instant::now();
        
        debug!("Starting pruning at block {}", self.current_block);
        
        match self.config.strategy {
            PruningStrategy::None => {
                return Ok(());
            }
            PruningStrategy::Interval => {
                self.prune_interval()?;
            }
            PruningStrategy::Absolute => {
                self.prune_absolute()?;
            }
            PruningStrategy::BiMode => {
                self.prune_bimode()?;
            }
            PruningStrategy::Hybrid => {
                self.prune_hybrid()?;
            }
        }
        
        self.last_pruning_block = self.current_block;
        
        let elapsed = start.elapsed().as_millis() as u64;
        self.stats.last_pruning_time_ms = elapsed;
        self.stats.last_pruning_block = self.current_block;
        
        info!(
            "Pruning completed in {}ms, pruned {} nodes",
            elapsed,
            self.stats.total_pruned_nodes
        );
        
        Ok(())
    }
    
    /// Interval-based pruning
    fn prune_interval(&mut self) -> Result<(), PruningError> {
        let prune_threshold = self.current_block.saturating_sub(self.config.full_blocks);
        
        // Collect the roots to prune first, so the loop does not hold a
        // borrow of `self` while pruning mutates it.
        let to_prune: Vec<(u64, H256)> = self
            .state_roots
            .iter()
            .filter(|root_info| root_info.block_number < prune_threshold)
            .map(|root_info| (root_info.block_number, root_info.state_root))
            .collect();

        for (block, state_root) in to_prune {
            self.prune_state_root(state_root)?;
            self.pruned_roots.insert(block, state_root);
        }
        
        // Clean up state roots
        self.state_roots.retain(|r| r.block_number >= prune_threshold);
        
        Ok(())
    }
    
    /// Absolute pruning
    fn prune_absolute(&mut self) -> Result<(), PruningError> {
        let keep_blocks = self.config.full_blocks;
        let prune_threshold = self.current_block.saturating_sub(keep_blocks);
        
        self.state_roots.retain(|r| r.block_number >= prune_threshold);
        
        Ok(())
    }
    
    /// Bi-mode pruning
    fn prune_bimode(&mut self) -> Result<(), PruningError> {
        let archive_threshold = self.current_block.saturating_sub(self.config.archive_blocks);
        let full_threshold = self.current_block.saturating_sub(self.config.full_blocks);
        
        // Blocks below the archive threshold keep their full state; blocks
        // between the archive and full thresholds are pruned. Roots are
        // collected first so the loop does not borrow `self`.
        let to_prune: Vec<H256> = self
            .state_roots
            .iter()
            .filter(|root_info| {
                root_info.block_number >= archive_threshold
                    && root_info.block_number < full_threshold
            })
            .map(|root_info| root_info.state_root)
            .collect();

        for state_root in to_prune {
            self.prune_state_root(state_root)?;
        }
        
        Ok(())
    }
    
    /// Hybrid pruning
    fn prune_hybrid(&mut self) -> Result<(), PruningError> {
        // Use combination of strategies
        self.prune_interval()?;
        self.prune_bimode()?;
        
        Ok(())
    }
    
    /// Prune specific state root
    fn prune_state_root(&mut self, state_root: H256) -> Result<(), PruningError> {
        let mut storage = self.storage.write();
        
        // Get nodes to prune
        let nodes = storage.get_state_nodes(state_root)?;
        
        let node_count = nodes.len();
        let total_size: usize = nodes.iter().map(|n| n.len()).sum();
        
        // Delete nodes
        for node in &nodes {
            storage.delete_state_node(state_root, node)?;
        }
        
        self.stats.total_pruned_nodes += node_count as u64;
        self.stats.total_pruned_bytes += total_size as u64;
        
        Ok(())
    }
    
    /// Create state snapshot
    pub fn create_snapshot(&mut self) -> Result<(), PruningError> {
        let start = std::time::Instant::now();
        
        info!("Creating state snapshot at block {}", self.current_block);
        
        // Get current state root
        let current_root = self.state_roots
            .iter()
            .find(|r| r.block_number == self.current_block)
            .map(|r| r.state_root)
            .ok_or_else(|| PruningError::StateNotFound("Current state root".to_string()))?;
        
        // Create snapshot: the trie nodes for this root are the snapshot.
        {
            let storage = self.storage.read();
            storage.create_snapshot(self.current_block, current_root)?;
        }
        
        self.last_snapshot_block = self.current_block;
        self.stats.total_snapshots += 1;
        
        info!("Snapshot created in {}ms", start.elapsed().as_millis());
        
        Ok(())
    }
    
    /// Get state at specific block.
    ///
    /// Returns `None` when the account is absent at that block, which is a real
    /// answer. The previous version returned a default `AccountState` for every
    /// request, so a missing account and a zero-balance account were
    /// indistinguishable.
    pub fn get_state_at(
        &self,
        address: Address,
        block: u64,
    ) -> Result<Option<AccountState>, PruningError> {
        let state_root = self.find_state_root(block)?;

        let storage = self.storage.read();
        storage.get_account_state(state_root, address)
    }

    /// Record a state root together with the trie that produced it, so the
    /// pruner can delete the right nodes and serve real proofs.
    pub fn put_state(
        &mut self,
        block: u64,
        state_root: H256,
        trie: &tiger_state::trie::Trie,
    ) -> Result<(), PruningError> {
        {
            let mut storage = self.storage.write();
            storage.put_trie(state_root, trie)?;
        }
        self.add_state_root(block, state_root, now_unix())
    }

    /// Find state root for block
    fn find_state_root(&self, block: u64) -> Result<H256, PruningError> {
        // First check if we have it in memory
        for root_info in self.state_roots.iter().rev() {
            if root_info.block_number <= block {
                return Ok(root_info.state_root);
            }
        }
        
        // Then check pruned roots
        if let Some(root) = self.pruned_roots.get(&block) {
            return Ok(*root);
        }
        
        // Finally check storage
        let storage = self.storage.read();
        storage.get_state_root(block)
            .ok_or_else(|| PruningError::StateNotFound(format!("State root for block {}", block)))
    }
    
    /// Generate state proof
    pub fn generate_proof(
        &self,
        address: Address,
        block: u64,
        slots: Vec<H256>,
    ) -> Result<StateProof, PruningError> {
        let state_root = self.find_state_root(block)?;
        
        let storage = self.storage.read();
        storage.generate_proof(state_root, address, slots)
    }
    
    /// Get a storage slot at a specific block.
    ///
    /// Returns `None` when the account or the slot is absent. The previous
    /// version returned the account's storage *root* as if it were the slot
    /// value, which is a different quantity entirely.
    pub fn get_storage_at(
        &self,
        address: Address,
        slot: H256,
        block: u64,
    ) -> Result<Option<H256>, PruningError> {
        let state_root = self.find_state_root(block)?;

        let account = {
            let storage = self.storage.read();
            match storage.get_account_state(state_root, address)? {
                Some(account) => account,
                None => return Ok(None),
            }
        };

        // The slot lives in the account's own storage trie.
        let storage = self.storage.read();
        match storage.get_storage_slot(account.storage_root, slot) {
            Ok(value) => Ok(value),
            // A storage trie that was pruned is not a zero value; it is unknown.
            Err(PruningError::StateNotFound(_)) => Ok(None),
            Err(e) => Err(e),
        }
    }

    /// State root for a block, if it is still held.
    pub fn state_root_at(&self, block: u64) -> Result<H256, PruningError> {
        self.find_state_root(block)
    }

    /// Get current stats
    pub fn get_stats(&self) -> PruningStats {
        self.stats.clone()
    }
    
    /// Get config
    pub fn config(&self) -> &PruningConfig {
        &self.config
    }
    
    /// Update config
    pub fn update_config(&mut self, config: PruningConfig) {
        self.config = config;
    }
    
    /// Get memory usage estimate
    pub fn estimate_memory_usage(&self) -> usize {
        let storage = self.storage.read();
        storage.estimate_size()
    }
}

// =============================================================================
// Types
// =============================================================================

/// Ethereum address
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
pub struct Address(pub [u8; 20]);

impl Address {
    pub fn zero() -> Self {
        Self([0u8; 20])
    }
    
    pub fn from_slice(slice: &[u8]) -> Self {
        let mut addr = [0u8; 20];
        let len = std::cmp::min(slice.len(), 20);
        addr[..len].copy_from_slice(&slice[..len]);
        Self(addr)
    }
    
    pub fn as_bytes(&self) -> &[u8] {
        &self.0
    }
}

impl std::fmt::Display for Address {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "0x{}", hex::encode(self.0))
    }
}

/// Keccak-256 hash
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
pub struct H256(pub [u8; 32]);

impl H256 {
    pub fn zero() -> Self {
        Self([0u8; 32])
    }
    
    pub fn from_slice(slice: &[u8]) -> Self {
        let mut hash = [0u8; 32];
        let len = std::cmp::min(slice.len(), 32);
        hash[..len].copy_from_slice(&slice[..len]);
        Self(hash)
    }
    
    pub fn as_bytes(&self) -> &[u8] {
        &self.0
    }
}

impl std::fmt::Display for H256 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "0x{}", hex::encode(self.0))
    }
}

/// Account state
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AccountState {
    pub nonce: u64,
    pub balance: U256,
    pub storage_root: H256,
    pub code_hash: H256,
    pub code: Option<Vec<u8>>,
}

/// Unsigned 256-bit integer
#[derive(Debug, Clone, Copy, Serialize, Deserialize, Default)]
pub struct U256(pub [u64; 4]);

impl U256 {
    pub fn zero() -> Self {
        Self([0u64; 4])
    }
    
    pub fn from_u64(val: u64) -> Self {
        let mut result = Self::zero();
        result.0[0] = val;
        result
    }

    /// From the canonical big-endian 32-byte form used on the wire.
    pub fn from_be_bytes(bytes: [u8; 32]) -> Self {
        let mut limbs = [0u64; 4];
        for (i, limb) in limbs.iter_mut().enumerate() {
            let mut chunk = [0u8; 8];
            chunk.copy_from_slice(&bytes[i * 8..i * 8 + 8]);
            *limb = u64::from_be_bytes(chunk);
        }
        Self(limbs)
    }

    /// The canonical big-endian 32-byte form.
    pub fn to_be_bytes(self) -> [u8; 32] {
        let mut out = [0u8; 32];
        for (i, limb) in self.0.iter().enumerate() {
            out[i * 8..i * 8 + 8].copy_from_slice(&limb.to_be_bytes());
        }
        out
    }
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_pruning_config_default() {
        let config = PruningConfig::default();
        assert!(config.enabled);
        assert_eq!(config.strategy, PruningStrategy::Interval);
    }
    
    #[test]
    fn test_address_display() {
        let addr = Address::zero();
        assert_eq!(addr.to_string(), "0x0000000000000000000000000000000000000000");
    }
    
    fn state_account(nonce: u64, balance: u64) -> AccountState {
        AccountState {
            nonce,
            balance: U256::from_u64(balance),
            storage_root: H256(tiger_state::account::EMPTY_STATE_ROOT),
            code_hash: H256(tiger_state::account::EMPTY_CODE_HASH),
            code: None,
        }
    }

    fn account_trie(entries: &[(Address, AccountState)]) -> tiger_state::trie::Trie {
        let mut trie = tiger_state::trie::Trie::new();
        for (addr, state) in entries {
            let account = tiger_state::StateAccount {
                nonce: state.nonce,
                balance: state.balance.to_be_bytes(),
                storage_root: state.storage_root.0,
                code_hash: state.code_hash.0,
                code: Vec::new(),
                storage: Default::default(),
            };
            trie.insert(
                &tiger_crypto::keccak256(addr.as_bytes()),
                &tiger_state::account::encode_account(&account),
            );
        }
        trie
    }

    #[test]
    fn state_recorded_for_a_block_can_be_read_back() {
        let mut pruner = StatePruner::new(
            PruningConfig::default(),
            PathBuf::from("/tmp/state_pruning_test_a"),
        )
        .unwrap();

        let entries = vec![
            (Address::from_slice(&[1u8; 20]), state_account(1, 1000)),
            (Address::from_slice(&[2u8; 20]), state_account(2, 2000)),
        ];
        let trie = account_trie(&entries);
        let root = H256(trie.root_hash());

        pruner.put_state(1, root, &trie).unwrap();

        let found = pruner
            .get_state_at(Address::from_slice(&[1u8; 20]), 1)
            .unwrap()
            .expect("account 1 must be present");
        assert_eq!(found.nonce, 1);
        assert_eq!(found.balance.to_be_bytes(), U256::from_u64(1000).to_be_bytes());

        // A missing account reads as absent, not as a zero-balance account.
        assert!(pruner
            .get_state_at(Address::from_slice(&[9u8; 20]), 1)
            .unwrap()
            .is_none());
    }

    #[test]
    fn a_generated_proof_verifies_and_detects_tampering() {
        let mut pruner = StatePruner::new(
            PruningConfig::default(),
            PathBuf::from("/tmp/state_pruning_test_b"),
        )
        .unwrap();

        let addr = Address::from_slice(&[7u8; 20]);
        let entries = vec![(addr, state_account(5, 5000))];
        let trie = account_trie(&entries);
        let root = H256(trie.root_hash());
        pruner.put_state(10, root, &trie).unwrap();

        let proof = pruner.generate_proof(addr, 10, vec![]).unwrap();
        assert!(!proof.absent);
        assert!(ProofVerifier::verify_state_proof(&proof).unwrap());

        let mut tampered = proof.clone();
        tampered.account_state.nonce = 999;
        assert!(!ProofVerifier::verify_state_proof(&tampered).unwrap());
    }

    #[test]
    fn a_root_that_was_never_recorded_is_reported_as_missing() {
        let pruner = StatePruner::new(
            PruningConfig::default(),
            PathBuf::from("/tmp/state_pruning_test_c"),
        )
        .unwrap();

        // No state at all was recorded, so there is no root to resolve.
        assert!(pruner
            .get_state_at(Address::from_slice(&[1u8; 20]), 1)
            .is_err());
    }

    #[test]
    fn pruning_removes_old_nodes_and_keeps_recent_ones() {
        let mut config = PruningConfig::default();
        config.full_blocks = 2;
        config.pruning_interval = 0;
        config.background_pruning = true;
        config.snapshot_enabled = false;

        let mut pruner =
            StatePruner::new(config, PathBuf::from("/tmp/state_pruning_test_d")).unwrap();

        let addr = Address::from_slice(&[3u8; 20]);
        let mut roots = Vec::new();

        // Record five blocks, each with its own trie.
        for block in 1..=5u64 {
            let entries = vec![(addr, state_account(block, block * 100))];
            let trie = account_trie(&entries);
            let root = H256(trie.root_hash());
            roots.push(root);
            pruner.put_state(block, root, &trie).unwrap();
        }

        // Blocks below `current - full_blocks` are pruned.
        let old_root = roots[0];
        {
            let storage = pruner.storage.read();
            let nodes = storage.get_state_nodes(old_root).unwrap();
            assert!(nodes.is_empty(), "the oldest root's nodes must be pruned");
        }

        // The most recent block's state is still readable.
        let recent = pruner
            .get_state_at(addr, 5)
            .unwrap()
            .expect("the newest account must survive pruning");
        assert_eq!(recent.nonce, 5);
    }

    #[test]
    fn test_h256_display() {
        let hash = H256::zero();
        assert_eq!(hash.to_string(), "0x0000000000000000000000000000000000000000000000000000000000000000");
    }
}

/// Seconds since the Unix epoch.
fn now_unix() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}
