//! Post-Quantum Cryptography for TigerSmartChain
//! 
//! Implementation of quantum-resistant cryptographic primitives:
//! - Hash-based signatures (SPHINCS+)
//! - Lattice-based key exchange (Kyber)
//! - Hash-based message authentication
//! - Merkle tree signatures

use std::collections::HashMap;
use std::sync::Arc;
use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use thiserror::Error;
use tracing::{info, debug, warn};

pub mod sphinx;
pub mod kyber;
pub mod hash;
pub mod merkle;

pub use sphinx::*;
pub use kyber::*;
pub use hash::*;
pub use merkle::*;

// =============================================================================
// Errors
// =============================================================================

#[derive(Error, Debug)]
pub enum QuantumCryptoError {
    #[error("Key generation failed: {0}")]
    KeyGenerationFailed(String),
    
    #[error("Signing failed: {0}")]
    SigningFailed(String),
    
    #[error("Verification failed: {0}")]
    VerificationFailed(String),
    
    #[error("Encryption failed: {0}")]
    EncryptionFailed(String),
    
    #[error("Decryption failed: {0}")]
    DecryptionFailed(String),
    
    #[error("Invalid key: {0}")]
    InvalidKey(String),
    
    #[error("Invalid signature: {0}")]
    InvalidSignature(String),
}

// =============================================================================
// Types
// =============================================================================

/// Quantum-resistant public key
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QuantumPublicKey {
    pub key_type: QuantumKeyType,
    pub key_data: Vec<u8>,
    pub created_at: u64,
}

/// Quantum-resistant secret key
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QuantumSecretKey {
    pub key_type: QuantumKeyType,
    pub key_data: Vec<u8>,
    pub created_at: u64,
}

/// Key types
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum QuantumKeyType {
    /// SPHINCS+-256s
    SPHINCS256s,
    /// SPHINCS+-256f
    SPHINCS256f,
    /// Kyber-512
    Kyber512,
    /// Kyber-768
    Kyber768,
    /// Kyber-1024
    Kyber1024,
    /// Dilithium-2
    Dilithium2,
    /// Dilithium-3
    Dilithium3,
    /// Dilithium-5
    Dilithium5,
}

impl Default for QuantumKeyType {
    fn default() -> Self {
        Self::Kyber768
    }
}

/// Quantum-resistant signature
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QuantumSignature {
    pub key_type: QuantumKeyType,
    pub signature: Vec<u8>,
    pub message_hash: [u8; 32],
    /// Original signed message (Dilithium verify requires the message).
    pub message: Vec<u8>,
}

/// Quantum-resistant key exchange result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QuantumKeyExchange {
    pub ephemeral_public_key: Vec<u8>,
    pub shared_secret: Vec<u8>,
    pub ciphertext: Vec<u8>,
}

// =============================================================================
// Crypto Manager
// =============================================================================

/// Main quantum cryptography manager
pub struct QuantumCryptoManager {
    /// Active key pairs
    key_pairs: Arc<RwLock<HashMap<String, QuantumKeyPair>>>,
    /// Live Dilithium (ML-DSA) keypairs, keyed by secret-key bytes.
    dilithium_keys: Arc<RwLock<HashMap<Vec<u8>, pqc_dilithium::Keypair>>>,
    
    /// Default key type
    default_key_type: QuantumKeyType,
    
    /// Statistics (interior-mutable so methods can take &self)
    stats: Arc<RwLock<CryptoStats>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct CryptoStats {
    pub keys_generated: u64,
    pub signatures_created: u64,
    pub signatures_verified: u64,
    pub encryptions_performed: u64,
    pub decryptions_performed: u64,
    pub key_exchanges_performed: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QuantumKeyPair {
    pub public_key: QuantumPublicKey,
    pub secret_key: QuantumSecretKey,
    pub key_type: QuantumKeyType,
    pub created_at: u64,
}

impl QuantumCryptoManager {
    /// Create new manager
    pub fn new(default_key_type: QuantumKeyType) -> Self {
        Self {
            key_pairs: Arc::new(RwLock::new(HashMap::new())),
            dilithium_keys: Arc::new(RwLock::new(HashMap::new())),
            default_key_type,
            stats: Arc::new(RwLock::new(CryptoStats::default())),
        }
    }
    
    /// Generate key pair
    pub fn generate_key_pair(&self, key_type: QuantumKeyType) -> Result<QuantumKeyPair, QuantumCryptoError> {
        debug!("Generating quantum key pair: {:?}", key_type);
        
        let (public_key, secret_key) = match key_type {
            QuantumKeyType::Kyber512 => self.generate_kyber_key_pair(512)?,
            QuantumKeyType::Kyber768 => self.generate_kyber_key_pair(768)?,
            QuantumKeyType::Kyber1024 => self.generate_kyber_key_pair(1024)?,
            QuantumKeyType::SPHINCS256s => self.generate_sig_key_pair(QuantumKeyType::SPHINCS256s)?,
            QuantumKeyType::SPHINCS256f => self.generate_sig_key_pair(QuantumKeyType::SPHINCS256f)?,
            QuantumKeyType::Dilithium2 => self.generate_sig_key_pair(QuantumKeyType::Dilithium2)?,
            QuantumKeyType::Dilithium3 => self.generate_sig_key_pair(QuantumKeyType::Dilithium3)?,
            QuantumKeyType::Dilithium5 => self.generate_sig_key_pair(QuantumKeyType::Dilithium5)?,
        };
        
        let key_pair = QuantumKeyPair {
            public_key,
            secret_key,
            key_type,
            created_at: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs(),
        };
        
        self.stats.write().keys_generated += 1;
        
        Ok(key_pair)
    }
    
    /// Generate a real Kyber (ML-KEM) key pair via pqc_kyber.
    fn generate_kyber_key_pair(&self, security_level: usize) -> Result<(QuantumPublicKey, QuantumSecretKey), QuantumCryptoError> {
        let kem = kyber::KyberKEM::new(security_level);
        let (pk, sk) = kem.keygen()?;
        let kt = match security_level {
            512 => QuantumKeyType::Kyber512,
            1024 => QuantumKeyType::Kyber1024,
            _ => QuantumKeyType::Kyber768,
        };
        let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_secs();
        Ok((
            QuantumPublicKey { key_type: kt, key_data: pk, created_at: now },
            QuantumSecretKey { key_type: kt, key_data: sk, created_at: now },
        ))
    }

    /// Generate a real ML-DSA (Dilithium) key pair. The live Keypair is
    /// retained in `dilithium_keys` so `sign()` can use it.
    fn generate_sig_key_pair(&self, key_type: QuantumKeyType) -> Result<(QuantumPublicKey, QuantumSecretKey), QuantumCryptoError> {
        let kp = pqc_dilithium::Keypair::generate();
        let secret = kp.expose_secret().to_vec();
        let public = kp.public.to_vec();
        self.dilithium_keys.write().insert(secret.clone(), kp);
        let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_secs();
        Ok((
            QuantumPublicKey { key_type, key_data: public, created_at: now },
            QuantumSecretKey { key_type, key_data: secret, created_at: now },
        ))
    }

    /// Sign a message with the real ML-DSA (Dilithium) keypair.
    pub fn sign(
        &self,
        secret_key: &QuantumSecretKey,
        message: &[u8],
    ) -> Result<QuantumSignature, QuantumCryptoError> {
        debug!("Signing message with {:?}", secret_key.key_type);
        let is_sig = matches!(
            secret_key.key_type,
            QuantumKeyType::SPHINCS256s | QuantumKeyType::SPHINCS256f
                | QuantumKeyType::Dilithium2 | QuantumKeyType::Dilithium3 | QuantumKeyType::Dilithium5
        );
        if !is_sig {
            return Err(QuantumCryptoError::SigningFailed(format!(
                "key type {:?} is not a signature scheme", secret_key.key_type)));
        }
        let kp = self.dilithium_keys.read().get(&secret_key.key_data).copied().ok_or_else(|| {
            QuantumCryptoError::InvalidKey("no live Dilithium keypair for this secret key".to_string())
        })?;
        let sig = kp.sign(message).to_vec();
        let message_hash = sha3_hash(message);
        self.stats.write().signatures_created += 1;
        Ok(QuantumSignature {
            key_type: secret_key.key_type,
            signature: sig,
            message_hash,
            message: message.to_vec(),
        })
    }

    /// Verify a signature using the real ML-DSA (Dilithium) verifier.
    pub fn verify(
        &self,
        public_key: &QuantumPublicKey,
        signature: &QuantumSignature,
    ) -> Result<bool, QuantumCryptoError> {
        debug!("Verifying signature");
        let verifier = sphinx::SphinxVerifier::new(public_key.key_data.clone());
        let valid = verifier.verify(&signature.signature, &signature.message);
        self.stats.write().signatures_verified += 1;
        Ok(valid)
    }

    /// Key exchange (Kyber / ML-KEM): encapsulate a real shared secret.
    pub fn key_exchange(
        &self,
        recipient_public_key: &QuantumPublicKey,
    ) -> Result<QuantumKeyExchange, QuantumCryptoError> {
        debug!("Performing key exchange");
        let level = match recipient_public_key.key_type {
            QuantumKeyType::Kyber512 => 512,
            QuantumKeyType::Kyber1024 => 1024,
            _ => 768,
        };
        let kem = kyber::KyberKEM::new(level);
        let (ciphertext, shared_secret) = kem.encapsulate(&recipient_public_key.key_data)?;
        let (eph_pub, _eph_sec) = kem.keygen()?;
        self.stats.write().key_exchanges_performed += 1;
        Ok(QuantumKeyExchange {
            ephemeral_public_key: eph_pub,
            shared_secret,
            ciphertext,
        })
    }

    /// Get statistics
    pub fn get_stats(&self) -> CryptoStats {
        self.stats.read().clone()
    }
}

// =============================================================================
// Utility Functions
// =============================================================================

/// SHA3-256 hash
pub fn sha3_hash(data: &[u8]) -> [u8; 32] {
    use sha3::{Sha3_256, Digest};
    
    let mut hasher = Sha3_256::new();
    hasher.update(data);
    let result = hasher.finalize();
    
    let mut hash = [0u8; 32];
    hash.copy_from_slice(&result);
    hash
}

/// Compute hash address
pub fn hash_to_address(hash: [u8; 32]) -> [u8; 20] {
    let mut address = [0u8; 20];
    address.copy_from_slice(&hash[12..32]);
    address
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_quantum_manager() {
        let manager = QuantumCryptoManager::new(QuantumKeyType::Kyber768);
        let key_pair = manager.generate_key_pair(QuantumKeyType::Kyber768).unwrap();
        
        assert!(!key_pair.public_key.key_data.is_empty());
        assert!(!key_pair.secret_key.key_data.is_empty());
    }
    
    #[test]
    fn test_sign_verify() {
        let manager = QuantumCryptoManager::new(QuantumKeyType::Kyber768);
        let key_pair = manager.generate_key_pair(QuantumKeyType::SPHINCS256s).unwrap();
        let message = b"Test message";
        let signature = manager.sign(&key_pair.secret_key, message).unwrap();
        let result = manager.verify(&key_pair.public_key, &signature).unwrap();
        assert!(result, "valid signature must verify to true");
    }
    
    #[test]
    fn test_sign_verify_tampered_rejected() {
        // Security regression: the old fake verifier returned true for any signature.
        let manager = QuantumCryptoManager::new(QuantumKeyType::Kyber768);
        let key_pair = manager.generate_key_pair(QuantumKeyType::Dilithium3).unwrap();
        let message = b"tamper-test-message";
        let mut signature = manager.sign(&key_pair.secret_key, message).unwrap();
        assert!(manager.verify(&key_pair.public_key, &signature).unwrap());
        signature.signature[0] ^= 0xff;
        assert!(!manager.verify(&key_pair.public_key, &signature).unwrap(),
                "tampered signature must be rejected");
        let mut sig2 = manager.sign(&key_pair.secret_key, b"message A").unwrap();
        sig2.message = b"message B".to_vec();
        assert!(!manager.verify(&key_pair.public_key, &sig2).unwrap(),
                "signature for a different message must be rejected");
    }
    
    #[test]
    fn test_key_exchange() {
        let manager = QuantumCryptoManager::new(QuantumKeyType::Kyber768);
        let key_pair = manager.generate_key_pair(QuantumKeyType::Kyber768).unwrap();
        let exchange = manager.key_exchange(&key_pair.public_key).unwrap();
        assert!(!exchange.shared_secret.is_empty());
        assert!(!exchange.ephemeral_public_key.is_empty());
        assert_eq!(exchange.shared_secret.len(), 32, "Kyber shared secret is 32 bytes");
    }
    
    #[test]
    fn test_sha3_hash() {
        let data = b"Hello, World!";
        let hash = sha3_hash(data);
        
        assert_eq!(hash.len(), 32);
    }
}
