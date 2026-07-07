//! Crypto Types

use serde::{Deserialize, Serialize};

// =============================================================================
// KEY
// =============================================================================

/// Private Key
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PrivateKey(pub Vec<u8>);

/// Public Key
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PublicKey(pub Vec<u8>);

/// Address
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Address(pub Vec<u8>);

impl Address {
    pub fn from_slice(s: &[u8]) -> Self {
        Self(s.to_vec())
    }
}

// =============================================================================
// SIGNATURE
// =============================================================================

/// Signature
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Signature {
    pub v: u8,
    pub r: Vec<u8>,
    pub s: Vec<u8>,
}
