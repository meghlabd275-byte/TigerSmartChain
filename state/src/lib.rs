//! TigerSmartChain state primitives.
//!
//! Provides RLP serialisation and the Merkle Patricia Trie that underpin
//! state roots, transaction roots and receipt roots.

pub mod rlp;
pub mod trie;
pub mod types;

pub use rlp::{RlpError, RlpItem};
pub use trie::{Node, Trie};
pub use types::*;
