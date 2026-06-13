//! Encryption Types

use serde::{Deserialize, Serialize};

// =============================================================================
// ENCRYPTION
// =============================================================================

/// Encrypted Data - result of AEAD encryption
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EncryptedData {
    pub ciphertext: Vec<u8>,
    pub nonce: Vec<u8>,
    pub tag: Vec<u8>,
}

/// Key Pair for asymmetric encryption
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KeyPair {
    pub public_key: String,
    pub private_key: String,
}

/// Simple symmetric cipher
pub struct Cipher {
    key: Vec<u8>,
}

impl Cipher {
    pub fn new(key: Vec<u8>) -> Self {
        Self { key }
    }

    /// Encrypt (simple XOR - not secure, use AdvancedCipher for production)
    pub fn encrypt(&self, plaintext: &[u8]) -> EncryptedData {
        let mut ciphertext = Vec::with_capacity(plaintext.len());
        for (i, byte) in plaintext.iter().enumerate() {
            ciphertext.push(byte ^ self.key[i % self.key.len()]);
        }
        EncryptedData {
            ciphertext,
            nonce: vec![0; 12],
            tag: vec![0; 16],
        }
    }

    /// Decrypt
    pub fn decrypt(&self, data: &EncryptedData) -> Vec<u8> {
        // Same operation for XOR
        let mut plaintext = Vec::with_capacity(data.ciphertext.len());
        for (i, byte) in data.ciphertext.iter().enumerate() {
            plaintext.push(byte ^ self.key[i % self.key.len()]);
        }
        plaintext
    }
}