//! TigerSmartChain state primitives.
//!
//! Provides RLP serialisation, the Merkle Patricia Trie, and the account/world
//! state layer that computes real Ethereum state roots.

pub mod account;
pub mod rlp;
pub mod trie;
pub mod types;

pub use account::{
    account_hash, decode_account, decode_storage_value, encode_account, is_empty_account,
    storage_root, Account as StateAccount, Address as StateAddress, WorldState,
    EMPTY_CODE_HASH, EMPTY_STATE_ROOT,
};
pub use rlp::{RlpError, RlpItem};
pub use trie::{Node, Trie};
pub use types::*;
