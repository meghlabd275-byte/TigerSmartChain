//! TigerScan authenticated key-value tree.
//!
//! # What this is, precisely
//!
//! This is a **Keccak-256 keyed Merkle tree** keyed by a 32-byte key. The keys
//! are split into a 31-byte *stem* and a one-byte *suffix*, which is the shape
//! a Verkle tree uses, and the tree groups leaves under stems the same way.
//! Storage, lookup and proofs are real: `get` returns the value that was
//! stored, and a proof is a set of sibling hashes that reconstructs the root.
//!
//! It is **not** a Verkle tree in the EIP-6800 sense. A real Verkle tree commits
//! to each node with a vector commitment over a Banderwagon (BLS12-381) curve
//! and proves membership with a polynomial evaluation witness, which makes
//! proofs constant-sized. A hash-based tree proves membership with O(log n)
//! sibling hashes instead. The two are not interchangeable, and this module
//! does not claim the smaller proofs.
//!
//! The previous implementation was worse than a hash tree: `get` returned
//! `vec![0; 32]` rather than the stored value, and `compute_root_from_proof`
//! hashed a different input than `compute_root`, so `verify_proof` could never
//! return true. `VerkleProof` and `TreeStats` were referenced but never
//! defined, and the manifest pointed at a benchmark file that did not exist, so
//! the crate did not compile at all.

pub mod types;

pub use types::{ProofStep, TreeStats, VerkleProof};

use std::collections::BTreeMap;
use thiserror::Error;

/// The domain separator prefixed to every hash, so a leaf hash cannot be
/// confused with an internal node hash.
const LEAF_PREFIX: u8 = 0x00;
const INTERNAL_PREFIX: u8 = 0x01;

#[derive(Error, Debug)]
pub enum VerkleError {
    #[error("key must be exactly 32 bytes, got {0}")]
    InvalidKeyLength(usize),
    #[error("key not found in tree")]
    KeyNotFound,
    #[error("invalid proof format")]
    InvalidProof,
    #[error("proof verification failed")]
    ProofVerificationFailed,
    #[error("tree is empty")]
    EmptyTree,
    #[error("commitment mismatch")]
    CommitmentMismatch,
    #[error("malformed serialized tree: {0}")]
    Malformed(String),
}

/// Hash a leaf from its key and value.
fn hash_leaf(key: &[u8], value: &[u8]) -> Vec<u8> {
    let mut buf = Vec::with_capacity(1 + 4 + key.len() + 4 + value.len());
    buf.push(LEAF_PREFIX);
    buf.extend_from_slice(&(key.len() as u32).to_be_bytes());
    buf.extend_from_slice(key);
    buf.extend_from_slice(&(value.len() as u32).to_be_bytes());
    buf.extend_from_slice(value);
    keccak_hash::keccak(&buf).0.to_vec()
}

/// Hash an internal node from its children.
///
/// A node with one child is the child itself, so an internal node always has
/// at least two children and no unary nodes are ever hashed.
fn hash_internal(left: &[u8], right: &[u8]) -> Vec<u8> {
    let mut buf = Vec::with_capacity(1 + 32 + 32);
    buf.push(INTERNAL_PREFIX);
    buf.extend_from_slice(left);
    buf.extend_from_slice(right);
    keccak_hash::keccak(&buf).0.to_vec()
}

/// A node in the tree.
#[derive(Debug, Clone)]
enum Node {
    Leaf { key: Vec<u8>, value: Vec<u8> },
    Internal { left: Box<Node>, right: Box<Node> },
}

impl Node {
    fn hash(&self) -> Vec<u8> {
        match self {
            Node::Leaf { key, value } => hash_leaf(key, value),
            Node::Internal { left, right } => hash_internal(&left.hash(), &right.hash()),
        }
    }
}

/// The authenticated key-value tree.
///
/// Keys are 32 bytes. Values are arbitrary bytes.
pub struct VerkleTree {
    /// Ordered by the 31-byte stem, so iteration and serialization are
    /// deterministic.
    stems: BTreeMap<Vec<u8>, BTreeMap<u8, Vec<u8>>>,
    root: Option<Vec<u8>>,
}

impl Default for VerkleTree {
    fn default() -> Self {
        Self::new()
    }
}

impl VerkleTree {
    /// Create an empty tree.
    pub fn new() -> Self {
        Self {
            stems: BTreeMap::new(),
            root: None,
        }
    }

    /// Split a key into its 31-byte stem and suffix byte.
    fn split_key(key: &[u8]) -> Result<(&[u8], u8), VerkleError> {
        if key.len() != 32 {
            return Err(VerkleError::InvalidKeyLength(key.len()));
        }
        Ok((&key[..31], key[31]))
    }

    /// Insert or replace a value. Returns the previous value, if any.
    pub fn insert(&mut self, key: &[u8], value: &[u8]) -> Result<Option<Vec<u8>>, VerkleError> {
        let (stem, suffix) = Self::split_key(key)?;
        let stem = stem.to_vec();

        let previous = self
            .stems
            .entry(stem)
            .or_default()
            .insert(suffix, value.to_vec());

        self.recompute_root();
        Ok(previous)
    }

    /// Read a value back.
    pub fn get(&self, key: &[u8]) -> Result<Vec<u8>, VerkleError> {
        let (stem, suffix) = Self::split_key(key)?;

        self.stems
            .get(stem)
            .and_then(|children| children.get(&suffix))
            .cloned()
            .ok_or(VerkleError::KeyNotFound)
    }

    /// Remove a key. Returns the removed value, if any.
    pub fn remove(&mut self, key: &[u8]) -> Result<Option<Vec<u8>>, VerkleError> {
        let (stem, suffix) = Self::split_key(key)?;

        let removed = match self.stems.get_mut(stem) {
            Some(children) => {
                let removed = children.remove(&suffix);
                if children.is_empty() {
                    self.stems.remove(stem);
                }
                removed
            }
            None => None,
        };

        self.recompute_root();
        Ok(removed)
    }

    /// The current root, or `None` when the tree is empty.
    pub fn root(&self) -> Option<&[u8]> {
        self.root.as_deref()
    }

    /// Tree statistics.
    pub fn stats(&self) -> TreeStats {
        TreeStats {
            stem_count: self.stems.len(),
            leaf_count: self.stems.values().map(|c| c.len()).sum(),
            root: self.root.clone(),
        }
    }

    /// Build the tree of nodes and return the root hash.
    ///
    /// Leaves are ordered by (stem, suffix). The tree is built bottom-up in
    /// levels; an odd node at any level is promoted to the next level
    /// unchanged, so the shape depends only on the key set and not on the
    /// order of insertion.
    fn recompute_root(&mut self) {
        if self.stems.is_empty() {
            self.root = None;
            return;
        }

        let mut level: Vec<Node> = Vec::new();
        for (stem, children) in &self.stems {
            for (suffix, value) in children {
                let mut key = stem.clone();
                key.push(*suffix);
                level.push(Node::Leaf {
                    key,
                    value: value.clone(),
                });
            }
        }

        while level.len() > 1 {
            let mut next = Vec::with_capacity(level.len().div_ceil(2));
            let mut iter = level.into_iter();
            while let Some(left) = iter.next() {
                match iter.next() {
                    Some(right) => next.push(Node::Internal {
                        left: Box::new(left),
                        right: Box::new(right),
                    }),
                    None => next.push(left),
                }
            }
            level = next;
        }

        self.root = Some(level[0].hash());
    }

    /// Produce a membership proof for `key`.
    pub fn prove(&self, key: &[u8]) -> Result<VerkleProof, VerkleError> {
        let (stem, suffix) = Self::split_key(key)?;

        let value = self
            .stems
            .get(stem)
            .and_then(|children| children.get(&suffix))
            .cloned()
            .ok_or(VerkleError::KeyNotFound)?;

        let root = self.root.clone().ok_or(VerkleError::EmptyTree)?;

        // Rebuild the level list and record the position of the target leaf.
        let mut level: Vec<Node> = Vec::new();
        let mut target_index = None;
        for (stem, children) in &self.stems {
            for (child_suffix, child_value) in children {
                let mut leaf_key = stem.clone();
                leaf_key.push(*child_suffix);
                if leaf_key == key {
                    target_index = Some(level.len());
                }
                level.push(Node::Leaf {
                    key: leaf_key,
                    value: child_value.clone(),
                });
            }
        }

        let mut index = target_index.ok_or(VerkleError::KeyNotFound)?;
        let mut path = Vec::new();

        while level.len() > 1 {
            let mut next = Vec::with_capacity(level.len().div_ceil(2));
            let mut next_index = None;

            let mut position = 0;
            while position < level.len() {
                let left_index = position;
                let right_index = position + 1;

                if right_index < level.len() {
                    let left_hash = level[left_index].hash();
                    let right_hash = level[right_index].hash();

                    if index == left_index {
                        path.push(ProofStep {
                            sibling: Some(right_hash),
                            is_left: true,
                        });
                        next_index = Some(next.len());
                    } else if index == right_index {
                        path.push(ProofStep {
                            sibling: Some(left_hash),
                            is_left: false,
                        });
                        next_index = Some(next.len());
                    }

                    next.push(Node::Internal {
                        left: Box::new(level[left_index].clone()),
                        right: Box::new(level[right_index].clone()),
                    });
                } else {
                    // Promoted unchanged: there is no sibling to fold in.
                    if index == left_index {
                        path.push(ProofStep {
                            sibling: None,
                            is_left: true,
                        });
                        next_index = Some(next.len());
                    }
                    next.push(level[left_index].clone());
                }

                position += 2;
            }

            level = next;
            index = next_index.ok_or(VerkleError::InvalidProof)?;
        }

        Ok(VerkleProof {
            key: key.to_vec(),
            value: value.clone(),
            leaf_hash: hash_leaf(key, &value),
            path,
            root,
        })
    }

    /// Verify a membership proof.
    ///
    /// Returns `Ok(true)` when the proof reconstructs the expected root,
    /// `Ok(false)` when it does not, and `Err` only for a malformed proof.
    /// Verification needs no access to the tree, so a light client can check it
    /// against a root it already trusts.
    pub fn verify_proof(proof: &VerkleProof, expected_root: &[u8]) -> Result<bool, VerkleError> {
        if proof.key.len() != 32 {
            return Err(VerkleError::InvalidProof);
        }

        if hash_leaf(&proof.key, &proof.value) != proof.leaf_hash {
            return Ok(false);
        }

        let mut current = proof.leaf_hash.clone();
        for step in &proof.path {
            current = match &step.sibling {
                Some(sibling) => {
                    if sibling.len() != 32 {
                        return Err(VerkleError::InvalidProof);
                    }
                    if step.is_left {
                        hash_internal(&current, sibling)
                    } else {
                        hash_internal(sibling, &current)
                    }
                }
                None => current,
            };
        }

        Ok(current == expected_root)
    }

    /// Prove a key against the tree's own root.
    pub fn prove_and_verify(&self, key: &[u8]) -> Result<bool, VerkleError> {
        let proof = self.prove(key)?;
        let root = self.root().ok_or(VerkleError::EmptyTree)?;
        Self::verify_proof(&proof, root)
    }

    /// Serialize the tree.
    ///
    /// Format: `root(32) || stem_count(u32) || leaf_count(u32) || per stem:
    /// stem_len(u8) || stem || suffix_count(u32) || per leaf: suffix(u8) ||
    /// value_len(u32) || value`.
    pub fn serialize(&self) -> Vec<u8> {
        let mut out = Vec::new();
        out.extend_from_slice(&self.root.clone().unwrap_or_else(|| vec![0u8; 32]));

        let stem_count = self.stems.len() as u32;
        let leaf_count: u32 = self.stems.values().map(|c| c.len() as u32).sum();
        out.extend_from_slice(&stem_count.to_be_bytes());
        out.extend_from_slice(&leaf_count.to_be_bytes());

        for (stem, children) in &self.stems {
            out.push(stem.len() as u8);
            out.extend_from_slice(stem);
            out.extend_from_slice(&(children.len() as u32).to_be_bytes());
            for (suffix, value) in children {
                out.push(*suffix);
                out.extend_from_slice(&(value.len() as u32).to_be_bytes());
                out.extend_from_slice(value);
            }
        }

        out
    }

    /// Deserialize a tree produced by [`VerkleTree::serialize`].
    pub fn deserialize(data: &[u8]) -> Result<Self, VerkleError> {
        let mut cursor = 0usize;

        let take = |cursor: &mut usize, n: usize| -> Result<&[u8], VerkleError> {
            if *cursor + n > data.len() {
                return Err(VerkleError::Malformed("unexpected end of input".to_string()));
            }
            let slice = &data[*cursor..*cursor + n];
            *cursor += n;
            Ok(slice)
        };

        let root = take(&mut cursor, 32)?.to_vec();
        let stem_count =
            u32::from_be_bytes(take(&mut cursor, 4)?.try_into().unwrap()) as usize;
        let _leaf_count =
            u32::from_be_bytes(take(&mut cursor, 4)?.try_into().unwrap()) as usize;

        let mut stems: BTreeMap<Vec<u8>, BTreeMap<u8, Vec<u8>>> = BTreeMap::new();

        for _ in 0..stem_count {
            let stem_len = take(&mut cursor, 1)?[0] as usize;
            let stem = take(&mut cursor, stem_len)?.to_vec();
            let child_count =
                u32::from_be_bytes(take(&mut cursor, 4)?.try_into().unwrap()) as usize;

            let mut children = BTreeMap::new();
            for _ in 0..child_count {
                let suffix = take(&mut cursor, 1)?[0];
                let value_len =
                    u32::from_be_bytes(take(&mut cursor, 4)?.try_into().unwrap()) as usize;
                let value = take(&mut cursor, value_len)?.to_vec();
                children.insert(suffix, value);
            }

            stems.insert(stem, children);
        }

        let mut tree = Self {
            stems,
            root: None,
        };
        tree.recompute_root();

        // The recomputed root must match the stored one, otherwise the bytes
        // were not a faithful serialization of this key set.
        if tree.root.as_deref() != Some(root.as_slice()) && !(tree.root.is_none() && root == vec![0u8; 32])
        {
            return Err(VerkleError::CommitmentMismatch);
        }

        Ok(tree)
    }

    /// Insert many entries.
    pub fn batch_insert(&mut self, entries: &[(Vec<u8>, Vec<u8>)]) -> Result<(), VerkleError> {
        for (key, value) in entries {
            self.insert(key, value)?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(n: u8) -> Vec<u8> {
        let mut k = vec![0u8; 32];
        k[0] = n;
        k
    }

    #[test]
    fn values_round_trip() {
        let mut tree = VerkleTree::new();
        tree.insert(&key(1), b"one").unwrap();
        tree.insert(&key(2), b"two").unwrap();

        // The old implementation returned zeros here.
        assert_eq!(tree.get(&key(1)).unwrap(), b"one");
        assert_eq!(tree.get(&key(2)).unwrap(), b"two");
        assert!(matches!(
            tree.get(&key(3)),
            Err(VerkleError::KeyNotFound)
        ));
    }

    #[test]
    fn insert_returns_the_previous_value_and_replaces_it() {
        let mut tree = VerkleTree::new();
        assert_eq!(tree.insert(&key(1), b"first").unwrap(), None);
        assert_eq!(
            tree.insert(&key(1), b"second").unwrap(),
            Some(b"first".to_vec())
        );
        assert_eq!(tree.get(&key(1)).unwrap(), b"second");
        assert_eq!(tree.stats().leaf_count, 1);
    }

    #[test]
    fn keys_sharing_a_stem_are_both_reachable() {
        // Same first 31 bytes, different suffix.
        let mut a = vec![0u8; 32];
        a[30] = 7;
        a[31] = 1;
        let mut b = a.clone();
        b[31] = 2;

        let mut tree = VerkleTree::new();
        tree.insert(&a, b"a").unwrap();
        tree.insert(&b, b"b").unwrap();

        assert_eq!(tree.get(&a).unwrap(), b"a");
        assert_eq!(tree.get(&b).unwrap(), b"b");
        assert_eq!(tree.stats().stem_count, 1);
        assert_eq!(tree.stats().leaf_count, 2);
    }

    #[test]
    fn proof_verifies_for_every_key() {
        let mut tree = VerkleTree::new();
        for i in 0..37u8 {
            tree.insert(&key(i), format!("value-{i}").as_bytes())
                .unwrap();
        }

        let root = tree.root().unwrap().to_vec();

        for i in 0..37u8 {
            let proof = tree.prove(&key(i)).unwrap();
            assert!(
                VerkleTree::verify_proof(&proof, &root).unwrap(),
                "proof for key {i} did not verify"
            );
        }
    }

    #[test]
    fn a_proof_with_a_wrong_value_is_rejected() {
        let mut tree = VerkleTree::new();
        tree.insert(&key(1), b"real").unwrap();
        let root = tree.root().unwrap().to_vec();

        let mut proof = tree.prove(&key(1)).unwrap();
        proof.value = b"forged".to_vec();

        assert!(!VerkleTree::verify_proof(&proof, &root).unwrap());
    }

    #[test]
    fn a_proof_for_another_root_is_rejected() {
        let mut tree = VerkleTree::new();
        tree.insert(&key(1), b"one").unwrap();
        let proof = tree.prove(&key(1)).unwrap();

        let mut other = VerkleTree::new();
        other.insert(&key(9), b"nine").unwrap();

        assert!(!VerkleTree::verify_proof(&proof, other.root().unwrap()).unwrap());
    }

    #[test]
    fn a_proof_from_a_tree_missing_the_key_does_not_verify() {
        let mut full = VerkleTree::new();
        full.insert(&key(1), b"one").unwrap();
        full.insert(&key(2), b"two").unwrap();
        let proof = full.prove(&key(1)).unwrap();

        let mut partial = VerkleTree::new();
        partial.insert(&key(1), b"one").unwrap();

        // The proof was built against a different key set, so its sibling
        // hashes cannot reconstruct the partial tree's root.
        assert!(!VerkleTree::verify_proof(&proof, partial.root().unwrap()).unwrap());
    }

    #[test]
    fn tampering_with_a_sibling_hash_is_rejected() {
        let mut tree = VerkleTree::new();
        for i in 0..5u8 {
            tree.insert(&key(i), b"v").unwrap();
        }
        let root = tree.root().unwrap().to_vec();

        let mut proof = tree.prove(&key(0)).unwrap();
        // Flip a bit in the first sibling hash.
        if let Some(step) = proof.path.iter_mut().find(|s| s.sibling.is_some()) {
            let sibling = step.sibling.as_mut().unwrap();
            sibling[0] ^= 0x01;
        }

        assert!(!VerkleTree::verify_proof(&proof, &root).unwrap());
    }

    #[test]
    fn the_root_is_order_independent() {
        let mut a = VerkleTree::new();
        a.insert(&key(1), b"one").unwrap();
        a.insert(&key(2), b"two").unwrap();
        a.insert(&key(3), b"three").unwrap();

        let mut b = VerkleTree::new();
        b.insert(&key(3), b"three").unwrap();
        b.insert(&key(1), b"one").unwrap();
        b.insert(&key(2), b"two").unwrap();

        assert_eq!(a.root(), b.root());
    }

    #[test]
    fn the_root_changes_when_a_value_changes() {
        let mut tree = VerkleTree::new();
        tree.insert(&key(1), b"one").unwrap();
        let before = tree.root().unwrap().to_vec();

        tree.insert(&key(1), b"ONE").unwrap();
        assert_ne!(tree.root().unwrap(), before.as_slice());
    }

    #[test]
    fn remove_updates_the_root_and_lookup() {
        let mut tree = VerkleTree::new();
        tree.insert(&key(1), b"one").unwrap();
        tree.insert(&key(2), b"two").unwrap();
        let with_two = tree.root().unwrap().to_vec();

        assert_eq!(tree.remove(&key(2)).unwrap(), Some(b"two".to_vec()));
        assert!(matches!(tree.get(&key(2)), Err(VerkleError::KeyNotFound)));
        assert_ne!(tree.root().unwrap(), with_two.as_slice());
        assert_eq!(tree.stats().leaf_count, 1);
    }

    #[test]
    fn serialization_round_trips_and_preserves_proofs() {
        let mut tree = VerkleTree::new();
        for i in 0..20u8 {
            tree.insert(&key(i), format!("v{i}").as_bytes()).unwrap();
        }

        let bytes = tree.serialize();
        let restored = VerkleTree::deserialize(&bytes).unwrap();

        assert_eq!(tree.root(), restored.root());
        assert_eq!(restored.stats().leaf_count, 20);
        assert_eq!(restored.get(&key(7)).unwrap(), b"v7");

        // A proof from the restored tree must verify against the same root.
        let proof = restored.prove(&key(7)).unwrap();
        assert!(VerkleTree::verify_proof(&proof, tree.root().unwrap()).unwrap());
    }

    #[test]
    fn a_corrupted_serialization_is_rejected() {
        let mut tree = VerkleTree::new();
        tree.insert(&key(1), b"one").unwrap();

        let mut bytes = tree.serialize();
        // Corrupt a byte in the value payload.
        let last = bytes.len() - 1;
        bytes[last] ^= 0xff;

        assert!(matches!(
            VerkleTree::deserialize(&bytes),
            Err(VerkleError::CommitmentMismatch)
        ));
    }

    #[test]
    fn a_truncated_serialization_is_rejected() {
        let mut tree = VerkleTree::new();
        tree.insert(&key(1), b"one").unwrap();

        let bytes = tree.serialize();
        assert!(VerkleTree::deserialize(&bytes[..bytes.len() - 2]).is_err());
    }

    #[test]
    fn wrong_key_lengths_are_rejected() {
        let mut tree = VerkleTree::new();
        assert!(matches!(
            tree.insert(b"short", b"v"),
            Err(VerkleError::InvalidKeyLength(5))
        ));
        assert!(tree.get(b"short").is_err());
    }

    #[test]
    fn an_empty_tree_has_no_root_and_no_proofs() {
        let tree = VerkleTree::new();
        assert!(tree.root().is_none());
        assert!(matches!(tree.prove(&key(1)), Err(VerkleError::KeyNotFound)));
    }

    #[test]
    fn batch_insert_matches_individual_inserts() {
        let entries: Vec<(Vec<u8>, Vec<u8>)> = (0..50u8)
            .map(|i| (key(i), format!("v{i}").into_bytes()))
            .collect();

        let mut batched = VerkleTree::new();
        batched.batch_insert(&entries).unwrap();

        let mut one_by_one = VerkleTree::new();
        for (k, v) in &entries {
            one_by_one.insert(k, v).unwrap();
        }

        assert_eq!(batched.root(), one_by_one.root());
        assert_eq!(batched.stats().leaf_count, 50);
    }
}
