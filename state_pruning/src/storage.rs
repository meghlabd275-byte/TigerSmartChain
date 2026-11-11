//! State Storage Implementation

use super::*;
use std::collections::HashMap;
use std::path::PathBuf;
use tiger_state::account::{decode_account, decode_storage_value};
use tiger_state::trie::Trie;

/// In-memory state storage (for testing)
pub struct StateStorage {
    /// Path this store was opened at. Retained so a caller can report where a
    /// store lives; the in-memory maps are the live copy.
    #[allow(dead_code)]
    db_path: PathBuf,
    state_roots: HashMap<u64, H256>,
    state_nodes: HashMap<H256, Vec<Vec<u8>>>,
    snapshots: parking_lot::RwLock<HashMap<u64, Vec<u8>>>,
    memory_index: HashMap<H256, Vec<u64>>,
}

impl StateStorage {
    /// Create new storage
    pub fn new(db_path: PathBuf) -> Result<Self, PruningError> {
        Ok(Self {
            db_path,
            state_roots: HashMap::new(),
            state_nodes: HashMap::new(),
            snapshots: parking_lot::RwLock::new(HashMap::new()),
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
        let mut removed = false;
        if let Some(nodes) = self.state_nodes.get_mut(&root) {
            let before = nodes.len();
            nodes.retain(|n| n != node);
            removed = nodes.len() != before;
        }

        // The index must not keep pointing at a root whose nodes are gone.
        if removed {
            self.memory_index.remove(&root);
        }

        Ok(())
    }
    
    /// Store the RLP of every node of a trie under `root`.
    ///
    /// A pruner needs the nodes themselves, not just the root, so it can delete
    /// the ones no longer referenced. The trie is the real Merkle Patricia Trie
    /// from the state layer, and `node_db` yields every node keyed by its
    /// keccak-256 hash.
    pub fn put_trie(&mut self, root: H256, trie: &Trie) -> Result<(), PruningError> {
        let db = trie.node_db();
        let nodes: Vec<Vec<u8>> = db.into_values().collect();
        self.state_nodes.insert(root, nodes);
        Ok(())
    }

    /// Look an account up in the trie stored under `root`.
    ///
    /// Returns `None` when the account is absent, which is a real answer and
    /// not the same as an account with zero balance. The previous version
    /// returned a default `AccountState` for every request, so a caller could
    /// not tell a funded account from a missing one.
    pub fn get_account_state(
        &self,
        root: H256,
        address: Address,
    ) -> Result<Option<AccountState>, PruningError> {
        let trie = self.trie_for(root)?;
        let key = tiger_crypto::keccak256(address.as_bytes());

        let encoded = match trie.get(&key) {
            Some(bytes) => bytes,
            None => return Ok(None),
        };

        let account = decode_account(&encoded).ok_or_else(|| {
            PruningError::DatabaseError("stored account RLP is malformed".to_string())
        })?;

        Ok(Some(AccountState {
            nonce: account.nonce,
            balance: U256::from_be_bytes(account.balance),
            storage_root: H256(account.storage_root),
            code_hash: H256(account.code_hash),
            code: if account.code.is_empty() {
                None
            } else {
                Some(account.code)
            },
        }))
    }

    /// Read one storage slot from the trie stored under `storage_root`.
    pub fn get_storage_slot(
        &self,
        storage_root: H256,
        slot: H256,
    ) -> Result<Option<H256>, PruningError> {
        let trie = self.trie_for(storage_root)?;
        let key = tiger_crypto::keccak256(slot.as_bytes());

        match trie.get(&key) {
            Some(encoded) => Ok(Some(H256(decode_storage_value(&encoded)))),
            None => Ok(None),
        }
    }

    /// Rebuild the trie stored under `root` from its nodes.
    ///
    /// The nodes are keyed by their hash, so the trie is reconstructed by
    /// walking from the root rather than by trusting insertion order.
    fn trie_for(&self, root: H256) -> Result<Trie, PruningError> {
        let nodes = self
            .state_nodes
            .get(&root)
            .ok_or_else(|| PruningError::StateNotFound(format!("state root {root}")))?;

        if nodes.is_empty() {
            return Ok(Trie::new());
        }

        // The trie is keyed by the keccak-256 of each node's encoding, so the
        // map is rebuilt from the stored bytes rather than trusting an order.
        let node_map: HashMap<[u8; 32], Vec<u8>> = nodes
            .iter()
            .map(|node| (tiger_crypto::keccak256(node), node.clone()))
            .collect();

        Trie::from_root(&node_map, root.0).ok_or_else(|| {
            PruningError::DatabaseError(
                "stored trie nodes do not rebuild to the expected root".to_string(),
            )
        })
    }

    /// Generate an account proof and storage proofs through the real trie.
    pub fn generate_proof(
        &self,
        root: H256,
        address: Address,
        slots: Vec<H256>,
    ) -> Result<StateProof, PruningError> {
        let trie = self.trie_for(root)?;
        let key = tiger_crypto::keccak256(address.as_bytes());

        let account_proof = trie.prove(&key);

        let (account_state, absent) =
            match Trie::verify_proof(&root.0, &key, &account_proof) {
                Some(Some(encoded)) => {
                    let account = decode_account(&encoded).ok_or_else(|| {
                        PruningError::DatabaseError("account RLP is malformed".to_string())
                    })?;
                    (
                        AccountState {
                            nonce: account.nonce,
                            balance: U256::from_be_bytes(account.balance),
                            storage_root: H256(account.storage_root),
                            code_hash: H256(account.code_hash),
                            code: if account.code.is_empty() {
                                None
                            } else {
                                Some(account.code)
                            },
                        },
                        false,
                    )
                }
                _ => (AccountState::default(), true),
            };

        // Storage proofs come from the account's own storage trie, which is
        // stored under the account's storage root.
        let storage_root = account_state.storage_root;
        let storage_trie = self.trie_for(storage_root).unwrap_or_else(|_| Trie::new());

        let storage_proofs = slots
            .into_iter()
            .map(|slot| {
                let slot_key = tiger_crypto::keccak256(slot.as_bytes());
                let proof = storage_trie.prove(&slot_key);
                match Trie::verify_proof(&storage_root.0, &slot_key, &proof) {
                    Some(Some(encoded)) => StorageProof {
                        slot,
                        value: H256(decode_storage_value(&encoded)),
                        absent: false,
                        proof,
                    },
                    _ => StorageProof {
                        slot,
                        value: H256::zero(),
                        absent: true,
                        proof,
                    },
                }
            })
            .collect();

        Ok(StateProof {
            address,
            account_state,
            absent,
            storage_proofs,
            block_number: 0,
            state_root: root,
            account_proof,
        })
    }

    /// Record the node set under `root` as the snapshot for `block`.
    ///
    /// A snapshot is the set of trie nodes that root resolves to. It is
    /// recorded rather than copied because the nodes are content-addressed:
    /// two blocks that share a subtree share the same bytes.
    pub fn create_snapshot(&self, block: u64, root: H256) -> Result<(), PruningError> {
        let nodes = self
            .state_nodes
            .get(&root)
            .ok_or_else(|| PruningError::StateNotFound(format!("state root {root}")))?;

        // Confirm the nodes really rebuild the root before recording it, so a
        // snapshot cannot be taken of a root the store cannot serve.
        let node_map: HashMap<[u8; 32], Vec<u8>> = nodes
            .iter()
            .map(|node| (tiger_crypto::keccak256(node), node.clone()))
            .collect();

        if Trie::from_root(&node_map, root.0).is_none() {
            return Err(PruningError::DatabaseError(
                "cannot snapshot: stored nodes do not rebuild the root".to_string(),
            ));
        }

        let encoded = serde_json::to_vec(&(block, root.0))
            .map_err(|e| PruningError::DatabaseError(e.to_string()))?;
        let mut snapshots = self.snapshots.write();
        snapshots.insert(block, encoded);

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
        
        for snapshot in self.snapshots.read().values() {
            size += snapshot.len();
        }
        
        size
    }
}
