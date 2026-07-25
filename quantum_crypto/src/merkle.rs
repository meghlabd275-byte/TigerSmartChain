//! Merkle Tree Implementation

use super::*;

/// Merkle tree for quantum-resistant signatures
pub struct MerkleTree {
    leaves: Vec<[u8; 32]>,
    nodes: Vec<[u8; 32]>,
}

impl MerkleTree {
    /// Create new merkle tree
    pub fn new(leaves: Vec<[u8; 32]>) -> Self {
        let mut tree = Self {
            leaves: leaves.clone(),
            nodes: Vec::new(),
        };
        
        tree.build();
        
        tree
    }
    
    /// Build tree
    fn build(&mut self) {
        let mut current_level = self.leaves.clone();
        
        while current_level.len() > 1 {
            let mut next_level = Vec::new();
            
            for i in (0..current_level.len()).step_by(2) {
                if i + 1 < current_level.len() {
                    let combined = [current_level[i], current_level[i + 1]].concat();
                    let hash = sha3_hash(&combined);
                    next_level.push(hash);
                } else {
                    // Duplicate last element
                    let combined = [current_level[i], current_level[i]].concat();
                    let hash = sha3_hash(&combined);
                    next_level.push(hash);
                }
            }
            
            self.nodes.extend(current_level);
            current_level = next_level;
        }
        
        // Add root
        self.nodes.extend(current_level);
    }
    
    /// Get root
    pub fn root(&self) -> Option<[u8; 32]> {
        self.nodes.last().copied()
    }
    
    /// Get proof
    pub fn get_proof(&self, index: usize) -> Vec<[u8; 32]> {
        let mut proof = Vec::new();
        
        let mut current_index = index;
        
        while self.nodes.len() > 1 {
            let level_size = self.leaves.len().next_power_of_two();
            let sibling = if current_index % 2 == 0 {
                current_index + 1
            } else {
                current_index - 1
            };
            
            if sibling < level_size && sibling < self.nodes.len() {
                proof.push(self.nodes[sibling]);
            }
            
            current_index /= 2;
        }
        
        proof
    }
    
    /// Verify proof
    pub fn verify_proof(leaf: [u8; 32], proof: &[&[u8; 32]], root: [u8; 32]) -> bool {
        let mut current = leaf;
        
        for sibling in proof {
            let mut combined = vec![];
            
            // Determine order
            if current < **sibling {
                combined.extend_from_slice(&current);
                combined.extend_from_slice(sibling);
            } else {
                combined.extend_from_slice(sibling);
                combined.extend_from_slice(&current);
            }
            
            current = sha3_hash(&combined);
        }
        
        current == root
    }
}
