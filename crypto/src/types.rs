//! Crypto Types

use serde::{Deserialize, Serialize};

// =============================================================================
// KEY
// =============================================================================

/// Private Key
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PrivateKey(pub [u8; 32]);

/// Public Key
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PublicKey(pub [u8; 64]);

/// Address
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Address(pub [u8; 20]);

impl Address {
    pub fn from_slice(s: &[u8]) -> Self {
        let mut addr = [0u8; 20];
        addr.copy_from_slice(s);
        Self(addr)
    }
}

// =============================================================================
// SIGNATURE
// =============================================================================

/// Signature
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Signature {
    pub v: u8,
    pub r: [u8; 32],
    pub s: [u8; 32],
}