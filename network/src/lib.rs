//! Peer-to-peer networking for TigerSmartChain.
//!
//! The crate provides three things:
//!
//!   * [`transport`] — a mutually authenticated, forward-secret, encrypted
//!     connection to a single peer, carrying length-prefixed frames.
//!   * [`types`] — the wire messages and the shared network types.
//!   * [`peer`] — the peer set: who is connected, what they claim their chain
//!     looks like, and which of them to ask for blocks.

pub mod peer;
pub mod transport;
pub mod types;

pub use peer::{PeerConnection, PeerManager};
pub use transport::{Identity, Session, MAX_FRAME_SIZE};
pub use types::*;

/// Re-exported so a consumer can build and inspect blocks without depending on
/// the blockchain crate directly.
pub use tiger_blockchain::types::{Block, Transaction};
