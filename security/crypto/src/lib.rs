//! Cryptographic primitives for TigerSmartChain
//! 
//! This module provides secure, memory-safe cryptographic operations
//! for the TigerSmartChain blockchain platform.

pub mod ecdsa;
pub mod ed25519;
pub mod hashing;
pub mod key_derivation;
pub mod signing;
pub mod verification;

/// Result type for cryptographic operations.
pub type Result<T> = core::result::Result<T, Error>;

/// Error type for cryptographic operations.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("invalid key: {0}")]
    InvalidKey(String),
    
    #[error("invalid signature: {0}")]
    InvalidSignature(String),
    
    #[error("signing failed: {0}")]
    SigningFailed(String),
    
    #[error("verification failed: {0}")]
    VerificationFailed(String),
    
    #[error("encryption failed: {0}")]
    EncryptionFailed(String),
    
    #[error("decryption failed: {0}")]
    DecryptionFailed(String),
    
    #[error("derivation failed: {0}")]
    DerivationFailed(String),
    
    #[error("invalid data: {0}")]
    InvalidData(String),
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_ecdsa_sign_verify() {
        let (private_key, public_key) = ecdsa::generate_key_pair();
        
        let message = b"Hello, TigerSmartChain!";
        let signature = ecdsa::sign(&private_key, message).unwrap();
        
        assert!(ecdsa::verify(&public_key, message, &signature).is_ok());
    }
    
    #[test]
    fn test_ed25519_sign_verify() {
        let private_key = ed25519::generate_key_pair();
        
        let message = b"Hello, TigerSmartChain!";
        let signature = ed25519::sign(&private_key, message).unwrap();
        
        assert!(ed25519::verify(&private_key.public_key(), message, &signature).is_ok());
    }
    
    #[test]
    fn test_hash() {
        let data = b"Hello, TigerSmartChain!";
        let hash = hashing::keccak256(data);
        
        assert_eq!(hash.len(), 32);
        assert!(!hash.iter().all(|&b| b == 0));
    }
    
    #[test]
    fn test_key_derivation() {
        let entropy = [0u8; 32];
        let mnemonic = key_derivation::mnemonic_from_entropy(&entropy).unwrap();
        let seed = key_derivation::seed_from_mnemonic(&mnemonic, "").unwrap();
        
        assert_eq!(seed.len(), 64);
    }
}