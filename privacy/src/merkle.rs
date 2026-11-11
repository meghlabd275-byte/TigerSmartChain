//! The commitment tree and its membership proofs.
//!
//! The tree is a fixed-depth binary Merkle tree over commitment encodings.
//! Empty subtrees are filled with a placeholder hash so the root of a tree
//! with `n` leaves does not reveal `n`, and so a membership proof has the same
//! length for every leaf. The previous version built a variable-height tree
//! whose `proof` method returned placeholder zeros for every unset sibling,
//! and nothing ever verified a proof.

use crate::crypto::{encode_point, Commitment};
use sha2::{Digest, Sha256};

/// A placeholder leaf that stands in for every empty slot.
pub fn empty_leaf() -> [u8; 32] {
    hash_leaf(b"TigerSmartChain/Privacy/EmptyLeaf/v1")
}

fn hash_leaf(data: &[u8]) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(b"TigerSmartChain/Privacy/Leaf/v1");
    hasher.update(data);
    hasher.finalize().into()
}

/// The leaf hash of a commitment.
///
/// Exposed so a verifier hashes the commitment exactly the way the tree does;
/// duplicating the recipe elsewhere would let the two drift apart.
pub fn leaf_hash(commitment: &Commitment) -> [u8; 32] {
    hash_leaf(&encode_point(&commitment.0))
}

/// Hash an internal node. The prefix distinguishes internal hashes from leaf
/// hashes so a leaf cannot be presented as an internal node.
fn hash_internal(left: &[u8; 32], right: &[u8; 32]) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(b"TigerSmartChain/Privacy/Node/v1");
    hasher.update(left);
    hasher.update(right);
    hasher.finalize().into()
}

/// A Merkle tree over note commitments.
#[derive(Debug, Clone)]
pub struct CommitmentTree {
    depth: usize,
    leaves: Vec<[u8; 32]>,
    /// Cached level-by-level nodes; level 0 is the leaves.
    levels: Vec<Vec<[u8; 32]>>,
}

impl CommitmentTree {
    /// Create a tree that holds up to `2^depth` commitments.
    pub fn new(depth: usize) -> Self {
        assert!(depth > 0 && depth < 64, "depth out of range");
        let mut tree = Self {
            depth,
            leaves: Vec::new(),
            levels: Vec::new(),
        };
        tree.rebuild();
        tree
    }

    /// The tree depth.
    pub fn depth(&self) -> usize {
        self.depth
    }

    /// How many commitments have been inserted.
    pub fn len(&self) -> usize {
        self.leaves.len()
    }

    /// Whether the tree holds no commitments.
    pub fn is_empty(&self) -> bool {
        self.leaves.is_empty()
    }

    /// Maximum number of leaves.
    pub fn capacity(&self) -> usize {
        1usize << self.depth
    }

    /// Insert a commitment and return its leaf index.
    pub fn insert(&mut self, commitment: &Commitment) -> Result<usize, String> {
        if self.leaves.len() >= self.capacity() {
            return Err("commitment tree is full".to_string());
        }

        let index = self.leaves.len();
        self.leaves.push(leaf_hash(commitment));
        self.rebuild();
        Ok(index)
    }

    /// The current root.
    pub fn root(&self) -> [u8; 32] {
        self.levels[self.depth][0]
    }

    /// Build the leaf-to-root path for `index`.
    ///
    /// Each entry is the sibling hash at that level, bottom first.
    pub fn proof(&self, index: usize) -> Result<Vec<[u8; 32]>, String> {
        if index >= self.leaves.len() {
            return Err(format!("leaf {index} does not exist"));
        }

        let mut proof = Vec::with_capacity(self.depth);
        let mut position = index;

        for level in 0..self.depth {
            let sibling = if position % 2 == 0 {
                position + 1
            } else {
                position - 1
            };
            proof.push(self.levels[level][sibling]);
            position /= 2;
        }

        Ok(proof)
    }

    /// Verify a path against a root.
    ///
    /// `index` decides, at every level, whether the running hash is the left or
    /// the right child. A caller must therefore supply the true index, which is
    /// the standard Merkle-proof contract.
    pub fn verify_proof(
        leaf: &[u8; 32],
        index: usize,
        proof: &[[u8; 32]],
        root: &[u8; 32],
    ) -> bool {
        let mut current = *leaf;
        let mut position = index;

        for sibling in proof {
            current = if position % 2 == 0 {
                hash_internal(&current, sibling)
            } else {
                hash_internal(sibling, &current)
            };
            position /= 2;
        }

        &current == root
    }

    fn rebuild(&mut self) {
        let mut level0: Vec<[u8; 32]> = Vec::with_capacity(self.capacity());
        level0.extend_from_slice(&self.leaves);
        level0.resize(self.capacity(), empty_leaf());

        let mut levels = vec![level0];

        for _ in 0..self.depth {
            let previous = levels.last().unwrap();
            let mut next = Vec::with_capacity(previous.len() / 2);
            for pair in previous.chunks(2) {
                next.push(hash_internal(&pair[0], &pair[1]));
            }
            levels.push(next);
        }

        self.levels = levels;
    }
}
