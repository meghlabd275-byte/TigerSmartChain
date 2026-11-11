//! Verkle types.
//!
//! The commitment this tree uses is Keccak-256 based (see the crate root), so
//! the proof carries sibling hashes rather than polynomial evaluation
//! witnesses.

use serde::{Deserialize, Serialize};

/// One step of a membership proof, from the leaf upward.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProofStep {
    /// The hash of the sibling subtree at this level, or `None` when the node
    /// was promoted unchanged because it had no sibling.
    pub sibling: Option<Vec<u8>>,
    /// Whether the node being proven was the left child at this level. Ignored
    /// when `sibling` is `None`.
    pub is_left: bool,
}

/// A membership proof for one key.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct VerkleProof {
    /// The key the proof is about.
    pub key: Vec<u8>,
    /// The value at that key.
    pub value: Vec<u8>,
    /// The hash of the leaf.
    pub leaf_hash: Vec<u8>,
    /// Sibling hashes from the leaf's parent to the root.
    pub path: Vec<ProofStep>,
    /// The root the proof is expected to reconstruct.
    pub root: Vec<u8>,
}

/// Tree statistics.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TreeStats {
    pub stem_count: usize,
    pub leaf_count: usize,
    pub root: Option<Vec<u8>>,
}
