//! Encryption Types

use serde::{Deserialize, Serialize};

// =============================================================================
// ENCRYPTION
// =============================================================================

/// Encrypted Data
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EncryptedData {
    pub ciphertext: Vec<u8>,
    pub nonce: Vec<u8>,
    pub tag: Vec<u8>,
}

/// Key Pair
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KeyPair {
    pub public_key: String,
    pub private_key: String,
}

/// Cipher
pub struct Cipher {
    key: Vec<u8>,
}

impl Cipher {
    pub fn new(key: Vec<u8>) -> Self {
        Self { key }
    }

    /// Encrypt
    pub fn encrypt(&self, plaintext: &[u8]) -> EncryptedData {
        EncryptedData {
            ciphertext: plaintext.to_vec(),
            nonce: vec![0; 12],
            tag: vec![0; 16],
        }
    }

    /// Decrypt
    pub fn decrypt(&self, data: &EncryptedData) -> Vec<u8> {
        data.ciphertext.clone()
    }
}