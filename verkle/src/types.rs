//! Verkle Types

// =============================================================================
// VERKLE TREE
// =============================================================================

/// Verkle Tree
pub struct VerkleTree {
    pub(crate) root: Option<Vec<u8>>,
}

impl Default for VerkleTree {
    fn default() -> Self {
        Self { root: None }
    }
}
