//! TigerScan privacy module.
//!
//! A shielded commitment pool built on real Ristretto255 arithmetic. See
//! [`pool`] for the exact security properties this provides and the ones it
//! deliberately does not claim.

pub mod crypto;
pub mod merkle;
pub mod pool;

pub use crypto::{BlindingProof, Commitment, POINT_LEN, SCALAR_LEN};
pub use merkle::CommitmentTree;
pub use pool::{EncryptedNote, Note, NoteOpening, PoolStats, PrivacyError, ShieldedPool, Spend};
