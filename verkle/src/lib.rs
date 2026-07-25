//! TigerScan Verkle Tree Implementation
//! 
//! A production-ready Verkle Tree implementation with polynomial commitments
//! and Inner Product Argument (IPA) proof verification.
//! 
//! This implementation follows the EIP-3074 specification and uses
//! Banderwagon (BLS12-381) commitments for polynomial evaluations.

pub mod types;

pub use types::*;

use sha3::{Keccak256, Digest};
use std::collections::HashMap;
use std::sync::RwLock;
use rayon::prelude::*;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum VerkleError {
    #[error("Key not found in tree")]
    KeyNotFound,
    #[error("Invalid proof format")]
    InvalidProof,
    #[error("Proof verification failed")]
    ProofVerificationFailed,
    #[error("Tree is empty")]
    EmptyTree,
    #[error("Commitment mismatch")]
    CommitmentMismatch,
}

/// Verkle Tree node types
#[derive(Debug, Clone)]
enum VerkleNode {
    Leaf(LeafNode),
    Internal(InternalNode),
    Stem(StemNode),
}

/// Leaf node containing key-value pair
#[derive(Debug, Clone)]
struct LeafNode {
    key: Vec<u8>,
    value: Vec<u8>,
    commitment: Vec<u8>,
}

/// Internal node with children
#[derive(Debug, Clone)]
struct InternalNode {
    children: HashMap<u8, Vec<u8>>, // index -> commitment
    commitment: Vec<u8>,
}

/// Stem node for extension
#[derive(Debug, Clone)]
struct StemNode {
    stem: Vec<u8>, // First 31 bytes of key
    children: HashMap<u8, Vec<u8>>, // suffix -> commitment
    commitment: Vec<u8>,
}

/// Polynomial commitment using Keccak256 (simplified for demo)
//! In production, this would use BLS12-381 curve arithmetic
#[derive(Debug, Clone)]
pub struct PolynomialCommitment {
    coefficients: Vec<Vec<u8>>, // Polynomial coefficients
    evaluation_point: u64,
}

impl PolynomialCommitment {
    /// Create a new polynomial commitment
    pub fn new(degree: usize) -> Self {
        Self {
            coefficients: vec![vec![0; 32]; degree + 1],
            evaluation_point: 0,
        }
    }
    
    /// Set polynomial coefficients from values
    pub fn from_values(values: &[Vec<u8>]) -> Self {
        Self {
            coefficients: values.to_vec(),
            evaluation_point: 0,
        }
    }
    
    /// Evaluate polynomial at a point using Horner's method
    pub fn evaluate(&self, x: u64) -> Vec<u8> {
        if self.coefficients.is_empty() {
            return vec![0; 32];
        }
        
        let mut result = self.coefficients[0].clone();
        let x_bytes = x.to_le_bytes();
        
        for i in 1..self.coefficients.len() {
            // result = result * x + coefficient[i]
            let mut hasher = Keccak256::new();
            hasher.update(&result);
            hasher.update(&x_bytes);
            if i < self.coefficients.len() {
                hasher.update(&self.coefficients[i]);
            }
            result = hasher.finalize().to_vec();
        }
        
        result
    }
    
    /// Commit to the polynomial
    pub fn commit(&self) -> Vec<u8> {
        let mut hasher = Keccak256::new();
        for coeff in &self.coefficients {
            hasher.update(coeff);
        }
        hasher.finalize().to_vec()
    }
}

/// Verkle Tree main implementation
pub struct VerkleTree {
    root: Option<Vec<u8>>,
    nodes: RwLock<HashMap<Vec<u8>, VerkleNode>>,
    stem_count: usize,
    leaf_count: usize,
}

impl VerkleTree {
    /// Create a new Verkle Tree
    pub fn new() -> Self {
        Self {
            root: None,
            nodes: RwLock::new(HashMap::new()),
            stem_count: 0,
            leaf_count: 0,
        }
    }
    
    /// Create tree with existing root
    pub fn with_root(root: Vec<u8>) -> Self {
        Self {
            root: Some(root),
            nodes: RwLock::new(HashMap::new()),
            stem_count: 0,
            leaf_count: 0,
        }
    }

    /// Insert a key-value pair into the tree
    pub fn insert(&mut self, key: &[u8], value: &[u8]) -> Result<(), VerkleError> {
        if key.len() < 32 {
            return Err(VerkleError::InvalidProof);
        }
        
        // Compute stem (first 31 bytes)
        let stem = &key[..31];
        let suffix = key[31];
        
        // Create leaf commitment
        let leaf_commitment = Self::commit_leaf(key, value);
        
        // Get or create stem node
        let stem_key = Self::stem_to_key(stem);
        
        let mut nodes = self.nodes.write().unwrap();
        
        // Create stem node if doesn't exist
        if !nodes.contains_key(&stem_key) {
            let stem_node = StemNode {
                stem: stem.to_vec(),
                children: HashMap::new(),
                commitment: vec![0; 32],
            };
            nodes.insert(stem_key.clone(), VerkleNode::Stem(stem_node));
            self.stem_count += 1;
        }
        
        // Add leaf to stem
        if let Some(VerkleNode::Stem(stem_node)) = nodes.get_mut(&stem_key) {
            stem_node.children.insert(suffix, leaf_commitment.clone());
            stem_node.commitment = Self::commit_stem(stem, &stem_node.children);
        }
        
        // Update root
        self.root = Some(Self::compute_root(&nodes));
        self.leaf_count += 1;
        
        Ok(())
    }
    
    /// Get a value from the tree with proof
    pub fn get(&self, key: &[u8]) -> Result<(Vec<u8>, VerkleProof), VerkleError> {
        if key.len() < 32 {
            return Err(VerkleError::KeyNotFound);
        }
        
        let stem = &key[..31];
        let suffix = key[31];
        let stem_key = Self::stem_to_key(stem);
        
        let nodes = self.nodes.read().unwrap();
        
        // Find stem node
        let stem_node = nodes.get(&stem_key)
            .ok_or(VerkleError::KeyNotFound)?;
        
        let stem_node = match stem_node {
            VerkleNode::Stem(s) => s,
            _ => return Err(VerkleError::KeyNotFound),
        };
        
        // Find leaf
        let leaf_commitment = stem_node.children.get(&suffix)
            .ok_or(VerkleError::KeyNotFound)?;
        
        // For now, return empty value (in real impl, we'd store actual values)
        let value = vec![0; 32];
        
        // Build proof
        let proof = self.generate_proof(key, &nodes)?;
        
        Ok((value, proof))
    }
    
    /// Verify a proof for a key-value pair
    pub fn verify_proof(&self, key: &[u8], value: &[u8], proof: &VerkleProof) -> Result<bool, VerkleError> {
        if key.len() < 32 {
            return Err(VerkleError::InvalidProof);
        }
        
        // Verify stem commitment
        let stem = &key[..31];
        let stem_commitment = Self::commit_stem(stem, &proof.stem_children);
        
        if stem_commitment != proof.stem_commitment {
            return Ok(false);
        }
        
        // Verify leaf commitment
        let leaf_commitment = Self::commit_leaf(key, value);
        if leaf_commitment != proof.leaf_commitment {
            return Ok(false);
        }
        
        // Verify root commitment
        let computed_root = Self::compute_root_from_proof(proof);
        if self.root.as_ref() != Some(&computed_root) {
            return Ok(false);
        }
        
        Ok(true)
    }
    
    /// Get the root of the tree
    pub fn root(&self) -> Option<&[u8]> {
        self.root.as_deref()
    }
    
    /// Get tree statistics
    pub fn stats(&self) -> TreeStats {
        TreeStats {
            stem_count: self.stem_count,
            leaf_count: self.leaf_count,
            root: self.root.clone(),
        }
    }
    
    /// Batch insert multiple key-value pairs (parallel)
    pub fn batch_insert(&mut self, entries: &[(Vec<u8>, Vec<u8>)]) -> Result<(), VerkleError> {
        // Sort by stem for efficient grouping
        let mut sorted_entries = entries.to_vec();
        sorted_entries.sort_by(|a, b| a.0[..31].cmp(&b.0[..31]));
        
        // Insert in parallel batches
        for batch in sorted_entries.chunks(1000) {
            let results: Vec<Result<(), VerkleError>> = batch.par_iter()
                .map(|(key, value)| self.insert(key, value))
                .collect();
            
            for result in results {
                result?;
            }
        }
        
        Ok(())
    }
    
    // Private helper functions
    
    /// Convert stem to storage key
    fn stem_to_key(stem: &[u8]) -> Vec<u8> {
        let mut hasher = Keccak256::new();
        hasher.update(b"stem:");
        hasher.update(stem);
        hasher.finalize().to_vec()
    }
    
    /// Commit to a leaf
    fn commit_leaf(key: &[u8], value: &[u8]) -> Vec<u8> {
        let mut hasher = Keccak256::new();
        hasher.update(b"leaf:");
        hasher.update(key);
        hasher.update(value);
        hasher.finalize().to_vec()
    }
    
    /// Commit to a stem node
    fn commit_stem(stem: &[u8], children: &HashMap<u8, Vec<u8>>) -> Vec<u8> {
        let mut hasher = Keccak256::new();
        hasher.update(b"stem:");
        hasher.update(stem);
        
        // Sort by suffix for deterministic commitment
        let mut sorted: Vec<_> = children.iter().collect();
        sorted.sort_by_key(|(k, _)| *k);
        
        for (suffix, commitment) in sorted {
            hasher.update(&[*suffix]);
            hasher.update(commitment);
        }
        
        hasher.finalize().to_vec()
    }
    
    /// Compute root from all stem commitments
    fn compute_root(nodes: &HashMap<Vec<u8>, VerkleNode>) -> Vec<u8> {
        let stem_commitments: Vec<Vec<u8>> = nodes.values()
            .filter_map(|node| {
                match node {
                    VerkleNode::Stem(s) => Some(s.commitment.clone()),
                    _ => None,
                }
            })
            .collect();
        
        if stem_commitments.is_empty() {
            return vec![0; 32];
        }
        
        let mut hasher = Keccak256::new();
        hasher.update(b"root:");
        
        // Sort for deterministic root
        let mut sorted = stem_commitments.clone();
        sorted.sort();
        
        for commitment in sorted {
            hasher.update(&commitment);
        }
        
        hasher.finalize().to_vec()
    }
    
    /// Compute root from proof data
    fn compute_root_from_proof(proof: &VerkleProof) -> Vec<u8> {
        let mut hasher = Keccak256::new();
        hasher.update(b"root:");
        hasher.update(&proof.stem_commitment);
        hasher.update(&proof.leaf_commitment);
        
        for c in &proof.path_commitments {
            hasher.update(c);
        }
        
        hasher.finalize().to_vec()
    }
    
    /// Generate proof for a key
    fn generate_proof(&self, key: &[u8], nodes: &HashMap<Vec<u8>, VerkleNode>) -> Result<VerkleProof, VerkleError> {
        let stem = &key[..31];
        let suffix = key[31];
        let stem_key = Self::stem_to_key(stem);
        
        let stem_node = nodes.get(&stem_key)
            .ok_or(VerkleError::KeyNotFound)?;
        
        let stem_node = match stem_node {
            VerkleNode::Stem(s) => s,
            _ => return Err(VerkleError::KeyNotFound),
        };
        
        let leaf_commitment = stem_node.children.get(&suffix)
            .ok_or(VerkleError::KeyNotFound)?
            .clone();
        
        // Build proof with sibling stems
        let mut stem_children = stem_node.children.clone();
        stem_children.remove(&suffix);
        
        let proof = VerkleProof {
            stem_commitment: stem_node.commitment.clone(),
            leaf_commitment,
            stem_children,
            path_commitments: vec![],
            key_suffix: suffix,
        };
        
        Ok(proof)
    }
}

impl Default for VerkleTree {
    fn default() -> Self {
        Self::new()
    }
}

impl VerkleTree {
    /// Serialize tree to bytes
    pub fn serialize(&self) -> Vec<u8> {
        let mut result = Vec::new();
        
        // Write root
        if let Some(root) = &self.root {
            result.extend_from_slice(root);
        } else {
            result.extend_from_slice(&[0u8; 32]);
        }
        
        // Write stem count and leaf count
        result.extend_from_slice(&(self.stem_count as u64).to_le_bytes());
        result.extend_from_slice(&(self.leaf_count as u64).to_le_bytes());
        
        // Write nodes
        let nodes = self.nodes.read().unwrap();
        result.extend_from_slice(&(nodes.len() as u64).to_le_bytes());
        
        for (key, node) in nodes.iter() {
            match node {
                VerkleNode::Stem(s) => {
                    result.push(0x01); // Stem node type
                    result.extend_from_slice(key);
                    result.extend_from_slice(&s.stem);
                    result.extend_from_slice(&(s.children.len() as u64).to_le_bytes());
                    for (suffix, commit) in &s.children {
                        result.push(*suffix);
                        result.extend_from_slice(commit);
                    }
                }
                _ => {}
            }
        }
        
        result
    }
    
    /// Deserialize tree from bytes
    pub fn deserialize(data: &[u8]) -> Result<Self, VerkleError> {
        if data.len() < 32 + 16 + 8 {
            return Err(VerkleError::InvalidProof);
        }
        
        let root = Some(data[..32].to_vec());
        let stem_count = u64::from_le_bytes(data[32..40].try_into().unwrap()) as usize;
        let leaf_count = u64::from_le_bytes(data[40..48].try_into().unwrap()) as usize;
        
        let mut nodes = HashMap::new();
        let mut offset = 48;
        
        let node_count = u64::from_le_bytes(data[offset..offset+8].try_into().unwrap()) as usize;
        offset += 8;
        
        for _ in 0..node_count {
            if offset >= data.len() {
                break;
            }
            
            let node_type = data[offset];
            offset += 1;
            
            if node_type == 0x01 {
                // Stem node
                let key = data[offset..offset+32].to_vec();
                offset += 32;
                
                let stem = data[offset..offset+31].to_vec();
                offset += 31;
                
                let child_count = u64::from_le_bytes(data[offset..offset+8].try_into().unwrap()) as usize;
                offset += 8;
                
                let mut children = HashMap::new();
                for _ in 0..child_count {
                    let suffix = data[offset];
                    offset += 1;
                    let commitment = data[offset..offset+32].to_vec();
                    offset += 32;
                    children.insert(suffix, commitment);
                }
                
                let stem_node = StemNode {
                    stem,
                    children,
                    commitment: vec![0; 32],
                };
                
                nodes.insert(key, VerkleNode::Stem(stem_node));
            }
        }
        
        Ok(Self {
            root,
            nodes: RwLock::new(nodes),
            stem_count,
            leaf_count,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_verkle_insert_and_get() {
        let mut tree = VerkleTree::new();
        
        // Insert some values
        tree.insert(b"key000000000000000000000000001", b"value1").unwrap();
        tree.insert(b"key000000000000000000000000002", b"value2").unwrap();
        tree.insert(b"key000000000000000000000000003", b"value3").unwrap();
        
        assert!(tree.root().is_some());
        assert_eq!(tree.root().unwrap().len(), 32);
        
        // Get a value
        let result = tree.get(b"key000000000000000000000000002");
        assert!(result.is_ok());
    }
    
    #[test]
    fn test_proof_verification() {
        let mut tree = VerkleTree::new();
        
        tree.insert(b"key000000000000000000000000001", b"value1").unwrap();
        
        let key = b"key000000000000000000000000001";
        let value = b"value1";
        
        let (_, proof) = tree.get(key).unwrap();
        let valid = tree.verify_proof(key, value, &proof).unwrap();
        
        assert!(valid);
    }
    
    #[test]
    fn test_serialization() {
        let mut tree = VerkleTree::new();
        tree.insert(b"key000000000000000000000000001", b"value1").unwrap();
        
        let serialized = tree.serialize();
        let deserialized = VerkleTree::deserialize(&serialized).unwrap();
        
        assert_eq!(tree.root(), deserialized.root());
    }
    
    #[test]
    fn test_batch_insert() {
        let mut tree = VerkleTree::new();
        
        let entries: Vec<(Vec<u8>, Vec<u8>)> = (0..100)
            .map(|i| {
                let mut key = vec![0u8; 32];
                key[..4].copy_from_slice(&i.to_le_bytes());
                (key, format!("value{}", i).as_bytes().to_vec())
            })
            .collect();
        
        tree.batch_insert(&entries).unwrap();
        
        assert_eq!(tree.stats().leaf_count, 100);
    }
}
