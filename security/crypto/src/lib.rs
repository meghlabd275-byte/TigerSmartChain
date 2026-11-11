//! Cryptographic primitives for TigerSmartChain
//! 
//! This module provides secure, memory-safe cryptographic operations
//! for the TigerSmartChain blockchain platform.

pub mod ed25519;
pub mod encryption;
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
    fn ecdsa_sign_verify() {
        let (private_key, public_key) = signing::generate_key_pair();

        let digest = hashing::keccak256(b"Hello, TigerSmartChain!");
        let signature = signing::sign(&private_key, &digest).unwrap();

        assert!(signing::verify(&public_key, &digest, &signature).is_ok());
    }

    #[test]
    fn ed25519_sign_verify() {
        let (private_key, public_key) = ed25519::generate_key_pair();

        let message = b"Hello, TigerSmartChain!";
        let signature = ed25519::sign(&private_key, message).unwrap();

        assert!(ed25519::verify(&public_key, message, &signature).is_ok());
    }

    #[test]
    fn hash() {
        let hash = hashing::keccak256(b"Hello, TigerSmartChain!");

        assert_eq!(hash.len(), 32);
        assert!(!hash.iter().all(|&b| b == 0));
    }

    #[test]
    fn key_derivation() {
        let mnemonic =
            key_derivation::generate_mnemonic(key_derivation::EntropyStrength::Bits128).unwrap();
        let seed = key_derivation::seed_from_mnemonic(&mnemonic, "").unwrap();

        assert_eq!(seed.as_bytes().len(), 64);

        let key = key_derivation::derive_ethereum_key(&seed, 0).unwrap();
        assert_ne!(key.private_key_bytes(), [0u8; 32]);
    }

    #[test]
    fn encryption_round_trip() {
        let key = encryption::generate_key();
        let encrypted = encryption::encrypt(&key, b"secret").unwrap();

        assert_eq!(encryption::decrypt(&key, &encrypted).unwrap(), b"secret");
    }

    #[test]
    fn end_to_end_wallet_flow() {
        // Mnemonic to key to address to signature to verified message, which
        // is the path every wallet takes.
        let mnemonic =
            key_derivation::generate_mnemonic(key_derivation::EntropyStrength::Bits128).unwrap();
        let seed = key_derivation::seed_from_mnemonic(&mnemonic, "").unwrap();
        let extended = key_derivation::derive_ethereum_key(&seed, 0).unwrap();
        let private_key = extended.to_private_key().unwrap();
        let public_key = private_key.public_key();

        let address = hashing::checksum_address(&public_key.to_address());
        let message = b"authenticate";
        let signature =
            signing::sign(&private_key, &verification::personal_message_digest(message)).unwrap();

        assert!(verification::verify_message(&address, message, &signature).is_ok());
    }
}
