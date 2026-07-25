//! State Storage Implementation

use super::*;
use std::collections::HashMap;
use std::path::PathBuf;
use tracing::{info, debug};

/// In-memory state storage (for testing)
pub struct StateStorage {
    db_path: PathBuf,
    state_roots: HashMap<u64, H256>,
    state_nodes: HashMap<H256, Vec<Vec<u8>>>,
    snapshots: HashMap<u64, Vec<u8>>,
    memory_index: HashMap<H256, Vec<u64>>,
}

impl StateStorage {
    /// Create new storage
    pub fn new(db_path: PathBuf) -> Result<Self, PruningError> {
        Ok(Self {
            db_path,
            state_roots: HashMap::new(),
            state_nodes: HashMap::new(),
            snapshots: HashMap::new(),
            memory_index: HashMap::new(),
        })
    }
    
    /// Put state root
    pub fn put_state_root(&mut self, block: u64, root: H256) -> Result<(), PruningError> {
        self.state_roots.insert(block, root);
        
        // Update index
        self.memory_index.entry(root).or_default().push(block);
        
        Ok(())
    }
    
    /// Get state root
    pub fn get_state_root(&self, block: u64) -> Option<H256> {
        self.state_roots.get(&block).copied()
    }
    
    /// Get state roots in range
    pub fn get_state_roots(&self, from_block: u64) -> Result<Vec<(u64, H256)>, PruningError> {
        let mut roots: Vec<_> = self.state_roots
            .iter()
            .filter(|(k, _)| **k >= from_block)
            .map(|(k, v)| (*k, *v))
            .collect();
        
        roots.sort_by_key(|(k, _)| *k);
        
        Ok(roots)
    }
    
    /// Get state nodes for root
    pub fn get_state_nodes(&self, root: H256) -> Result<Vec<Vec<u8>>, PruningError> {
        Ok(self.state_nodes.get(&root).cloned().unwrap_or_default())
    }
    
    /// Delete state node
    pub fn delete_state_node(&mut self, root: H256, node: &[u8]) -> Result<(), PruningError> {
        if let Some(nodes) = self.state_nodes.get_mut(&root) {
            nodes.retain(|n| n != node);
        }
        
        // Update index
        if let Some(blocks) = self.memory_index.get_mut(&root) {
            // Clean up block references if needed
        }
        
        Ok(())
    }
    
    /// Get account state
    pub fn get_account_state(&self, root: H256, address: Address) -> Result<AccountState, PruningError> {
        // Simplified - in production would traverse trie
        Ok(AccountState {
            nonce: 0,
            balance: U256::zero(),
            storage_root: H256::zero(),
            code_hash: H256::zero(),
            code: None,
        })
    }
    
    /// Generate state proof
    pub fn generate_proof(
        &self,
        root: H256,
        address: Address,
        slots: Vec<H256>,
    ) -> Result<StateProof, PruningError> {
        // Simplified proof generation
        Ok(StateProof {
            address,
            account_state: AccountState::default(),
            storage_proofs: slots.into_iter().map(|slot| StorageProof {
                slot,
                value: H256::zero(),
                proof: vec![],
            }).collect(),
            block_number: 0,
            state_root: root,
        })
    }
    
    /// Create snapshot
    pub fn create_snapshot(&mut self, block: u64, root: H256) -> Result<(), PruningError> {
        // Get all nodes for current state
        let nodes = self.state_nodes.get(&root).cloned().unwrap_or_default();
        
        // Create snapshot (simplified - in production would write to disk)
        let snapshot_data = serde_json::to_vec(&nodes).unwrap_or_default();
        self.snapshots.insert(block, snapshot_data);
        
        debug!("Snapshot created for block {}", block);
        
        Ok(())
    }
    
    /// Estimate size
    pub fn estimate_size(&self) -> usize {
        let mut size = 0;
        
        for nodes in self.state_nodes.values() {
            for node in nodes {
                size += node.len();
            }
        }
        
        for snapshot in self.snapshots.values() {
            size += snapshot.len();
        }
        
        size
    }
}

/// State proof
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StateProof {
    pub address: Address,
    pub account_state: AccountState,
    pub storage_proofs: Vec<StorageProof>,
    pub block_number: u64,
    pub state_root: H256,
}

/// Storage proof
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StorageProof {
    pub slot: H256,
    pub value: H256,
    pub proof: Vec<Vec<u8>>,
}
