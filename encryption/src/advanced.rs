//! Advanced Encryption - AES-256-GCM, ChaCha20-Poly1305, and ECIES implementations
//! 
//! This module provides high-security cryptographic primitives for:
//! - AES-256-GCM for symmetric encryption
//! - ChaCha20-Poly1305 for high-performance encryption
//! - ECIES (Elliptic Curve Integrated Encryption Scheme) for asymmetric encryption
//! - Key derivation using HKDF
//! - Digital signatures using ECDSA

use super::types::*;
use aes::Aes256;
use aes_gcm::{
    aead::{Aead, KeyInit, OsRng},
    Aes256Gcm, Nonce,
};
use chacha20poly1305::{
    aead::{Aead as _, KeyInit as _, OsRng as _},
    ChaCha20Poly1305, Nonce as ChaChaNonce,
};
use k256::{
    ecdsa::{
        signature::{Signer, Verifier},
        SigningKey, VerifyingKey,
    },
    Secp256k1,
};
use sha2::{Digest, Sha256, Sha384};
use hmac::{Hmac, Mac};
use ecdh::Ecdh;
use rand::RngCore;
use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64};
use hex;
use zeroize::{Zeroize, Zeroizing};
use thiserror::Error;

// =============================================================================
// ERRORS
// =============================================================================

#[derive(Error, Debug)]
pub enum EncryptionError {
    #[error("Encryption failed: {0}")]
    EncryptionFailed(String),
    #[error("Decryption failed: {0}")]
    DecryptionFailed(String),
    #[error("Invalid key length: {0}")]
    InvalidKeyLength(usize),
    #[error("Invalid ciphertext")]
    InvalidCiphertext,
    #[error("Key derivation failed: {0}")]
    KeyDerivationFailed(String),
    #[error("Signature error: {0}")]
    SignatureError(String),
}

// Aliases for cryptographic primitives
type Aes256GcmCipher = Aes256Gcm;
type ChaChaCipher = ChaCha20Poly1305;
type HmacSha256 = Hmac<Sha256>;

// =============================================================================
// ADVANCED ENCRYPTION
// =============================================================================

/// Advanced Cipher with AES-256-GCM and ChaCha20-Poly1305
pub struct AdvancedCipher {
    /// AES key (32 bytes)
    aes_key: Zeroizing<Vec<u8>>,
    /// ChaCha20 key (32 bytes)
    chacha_key: Zeroizing<Vec<u8>>,
}

impl AdvancedCipher {
    /// Create new cipher with derived keys from master key
    pub fn new(master_key: &[u8]) -> Result<Self, EncryptionError> {
        if master_key.len() < 32 {
            return Err(EncryptionError::InvalidKeyLength(master_key.len()));
        }
        
        // Derive separate keys for AES and ChaCha using HKDF-like expansion
        let aes_key = derive_key(master_key, b"AES-256-GCM", 32)?;
        let chacha_key = derive_key(master_key, b"ChaCha20-Poly1305", 32)?;
        
        Ok(Self {
            aes_key: Zeroizing::new(aes_key),
            chacha_key: Zeroizing::new(chacha_key),
        })
    }

    /// Encrypt with AES-256-GCM (AEAD)
    /// 
    /// # Arguments
    /// * `plaintext` - Data to encrypt
    /// * `aad` - Additional authenticated data (optional, can be empty)
    /// 
    /// # Returns
    /// EncryptedData containing ciphertext, nonce (12 bytes), and auth tag (16 bytes)
    pub fn encrypt_aead(&self, plaintext: &[u8], aad: &[u8]) -> Result<EncryptedData, EncryptionError> {
        // Create cipher instance
        let cipher = Aes256GcmCipher::new(self.aes_key.as_ref().into())
            .map_err(|e| EncryptionError::EncryptionFailed(e.to_string()))?;
        
        // Generate random 12-byte nonce
        let mut nonce_bytes = [0u8; 12];
        OsRng.fill_bytes(&mut nonce_bytes);
        let nonce = Nonce::from_slice(&nonce_bytes);
        
        // Encrypt with AAD
        let ciphertext = cipher
            .encrypt(nonce, aead::Payload::from_slice(plaintext, aad))
            .map_err(|e| EncryptionError::EncryptionFailed(e.to_string()))?;
        
        // Split ciphertext into actual ciphertext and tag (last 16 bytes)
        let len = ciphertext.len();
        let mut ct = ciphertext[..len - 16].to_vec();
        let tag = ciphertext[len - 16..].to_vec();
        
        Ok(EncryptedData {
            ciphertext: ct,
            nonce: nonce_bytes.to_vec(),
            tag,
        })
    }

    /// Decrypt with AES-256-GCM (AEAD)
    /// 
    /// # Arguments
    /// * `data` - EncryptedData containing ciphertext, nonce, and tag
    /// * `aad` - Additional authenticated data (must match encryption)
    /// 
    /// # Returns
    /// Decrypted plaintext
    pub fn decrypt_aead(&self, data: &EncryptedData, aad: &[u8]) -> Result<Vec<u8>, EncryptionError> {
        if data.nonce.len() != 12 {
            return Err(EncryptionError::InvalidCiphertext);
        }
        if data.tag.len() != 16 {
            return Err(EncryptionError::InvalidCiphertext);
        }
        
        // Reconstruct ciphertext with tag appended
        let mut ciphertext = data.ciphertext.clone();
        ciphertext.extend_from_slice(&data.tag);
        
        // Create cipher instance
        let cipher = Aes256GcmCipher::new(self.aes_key.as_ref().into())
            .map_err(|e| EncryptionError::DecryptionFailed(e.to_string()))?;
        
        let nonce = Nonce::from_slice(&data.nonce);
        
        // Decrypt
        let plaintext = cipher
            .decrypt(nonce, aead::Payload::from_slice(&ciphertext, aad))
            .map_err(|e| EncryptionError::DecryptionFailed(e.to_string()))?;
        
        Ok(plaintext)
    }

    /// Encrypt with ChaCha20-Poly1305 (AEAD) - faster for large data
    pub fn encrypt_chacha(&self, plaintext: &[u8], aad: &[u8]) -> Result<EncryptedData, EncryptionError> {
        let cipher = ChaChaCipher::new(self.chacha_key.as_ref().into())
            .map_err(|e| EncryptionError::EncryptionFailed(e.to_string()))?;
        
        let mut nonce_bytes = [0u8; 12];
        OsRng.fill_bytes(&mut nonce_bytes);
        let nonce = ChaChaNonce::from_slice(&nonce_bytes);
        
        let ciphertext = cipher
            .encrypt(nonce, aead::Payload::from_slice(plaintext, aad))
            .map_err(|e| EncryptionError::EncryptionFailed(e.to_string()))?;
        
        let len = ciphertext.len();
        let mut ct = ciphertext[..len - 16].to_vec();
        let tag = ciphertext[len - 16..].to_vec();
        
        Ok(EncryptedData {
            ciphertext: ct,
            nonce: nonce_bytes.to_vec(),
            tag,
        })
    }

    /// Decrypt with ChaCha20-Poly1305
    pub fn decrypt_chacha(&self, data: &EncryptedData, aad: &[u8]) -> Result<Vec<u8>, EncryptionError> {
        if data.nonce.len() != 12 || data.tag.len() != 16 {
            return Err(EncryptionError::InvalidCiphertext);
        }
        
        let mut ciphertext = data.ciphertext.clone();
        ciphertext.extend_from_slice(&data.tag);
        
        let cipher = ChaChaCipher::new(self.chacha_key.as_ref().into())
            .map_err(|e| EncryptionError::DecryptionFailed(e.to_string()))?;
        
        let nonce = ChaChaNonce::from_slice(&data.nonce);
        
        let plaintext = cipher
            .decrypt(nonce, aead::Payload::from_slice(&ciphertext, aad))
            .map_err(|e| EncryptionError::DecryptionFailed(e.to_string()))?;
        
        Ok(plaintext)
    }

    /// Generate ECDSA key pair for ECIES
    pub fn generate_key_pair(&self) -> Result<KeyPair, EncryptionError> {
        let signing_key = SigningKey::random(&mut OsRng);
        let verifying_key = VerifyingKey::from(&signing_key);
        
        Ok(KeyPair {
            public_key: hex::encode(verifying_key.to_encoded_point(false).as_bytes()),
            private_key: hex::encode(signing_key.to_bytes().as_slice()),
        })
    }

    /// Encrypt with ECIES (Elliptic Curve Integrated Encryption Scheme)
    /// Combines ECDH key exchange with AES-256-GCM
    pub fn encrypt_ecies(&self, plaintext: &[u8], recipient_pubkey: &[u8]) -> Result<EncryptedData, EncryptionError> {
        // Generate ephemeral key pair
        let ephemeral_sk = SigningKey::random(&mut OsRng);
        let ephemeral_pk = VerifyingKey::from(&ephemeral_sk);
        
        // Import recipient public key
        let recipient_vk = VerifyingKey::from_sec1_bytes(recipient_pubkey)
            .map_err(|e| EncryptionError::EncryptionFailed(e.to_string()))?;
        
        // Perform ECDH
        let ecdh = ecdh::Ecdh::new();
        let shared_secret = ecdh
            .diffie_hellman(ephemeral_sk.to_bytes(), recipient_vk.to_encoded_point(false).as_bytes().to_vec())
            .map_err(|e| EncryptionError::KeyDerivationFailed(e.to_string()))?;
        
        // Derive encryption key from shared secret
        let enc_key = derive_key(&shared_secret, b"ECIES-AES256", 32)?;
        
        // Encrypt with derived key
        let cipher = AdvancedCipher::new(&enc_key)?;
        let encrypted = cipher.encrypt_aead(plaintext, &[])?;
        
        // Prepend ephemeral public key to ciphertext
        let mut full_ct = Vec::from(ephemeral_pk.to_encoded_point(false).as_bytes());
        full_ct.extend_from_slice(&encrypted.ciphertext);
        full_ct.extend_from_slice(&encrypted.nonce);
        full_ct.extend_from_slice(&encrypted.tag);
        
        Ok(EncryptedData {
            ciphertext: full_ct,
            nonce: vec![],
            tag: vec![],
        })
    }

    /// Decrypt with ECIES
    pub fn decrypt_ecies(&self, data: &EncryptedData, private_key: &[u8]) -> Result<Vec<u8>, EncryptionError> {
        // Parse ephemeral public key (65 bytes for uncompressed)
        if data.ciphertext.len() < 65 {
            return Err(EncryptionError::InvalidCiphertext);
        }
        
        let ephemeral_pubkey = &data.ciphertext[..65];
        let remaining = &data.ciphertext[65..];
        
        // Extract nonce and tag from end (12 + 16 = 28 bytes)
        if remaining.len() < 28 {
            return Err(EncryptionError::InvalidCiphertext);
        }
        
        let ct_len = remaining.len() - 28;
        let ciphertext = &remaining[..ct_len];
        let nonce = &remaining[ct_len..ct_len + 12];
        let tag = &remaining[ct_len + 12..];
        
        // Import private key
        let priv_key_bytes: [u8; 32] = private_key.try_into()
            .map_err(|_| EncryptionError::InvalidKeyLength(private_key.len()))?;
        let signing_key = SigningKey::from_bytes(&priv_key_bytes.into())
            .map_err(|e| EncryptionError::SignatureError(e.to_string()))?;
        
        // Import ephemeral public key
        let ephemeral_vk = VerifyingKey::from_sec1_bytes(ephemeral_pubkey)
            .map_err(|e| EncryptionError::EncryptionFailed(e.to_string()))?;
        
        // ECDH
        let ecdh = ecdh::Ecdh::new();
        let shared_secret = ecdh
            .diffie_hellman(signing_key.to_bytes(), ephemeral_pubkey.to_vec())
            .map_err(|e| EncryptionError::KeyDerivationFailed(e.to_string()))?;
        
        // Derive encryption key
        let enc_key = derive_key(&shared_secret, b"ECIES-AES256", 32)?;
        let cipher = AdvancedCipher::new(&enc_key)?;
        
        // Decrypt
        let enc_data = EncryptedData {
            ciphertext: ciphertext.to_vec(),
            nonce: nonce.to_vec(),
            tag: tag.to_vec(),
        };
        
        cipher.decrypt_aead(&enc_data, &[])
    }
}

// =============================================================================
// KEY DERIVATION
// =============================================================================

/// Derive a key from input using HMAC-SHA256
fn derive_key(input: &[u8], context: &[u8], length: usize) -> Result<Vec<u8>, EncryptionError> {
    let mut mac = HmacSha256::new_from_slice(input)
        .map_err(|e| EncryptionError::KeyDerivationFailed(e.to_string()))?;
    mac.update(context);
    let result = mac.finalize().into_bytes();
    
    // Expand to desired length if needed
    let mut key = result.to_vec();
    while key.len() < length {
        let mut mac = HmacSha256::new_from_slice(&key)
            .map_err(|e| EncryptionError::KeyDerivationFailed(e.to_string()))?;
        mac.update(input);
        mac.update(&(key.len() as u32).to_be_bytes());
        let next = mac.finalize().into_bytes();
        key.extend_from_slice(&next);
    }
    
    key.truncate(length);
    Ok(key)
}

/// Hash data using SHA-256
pub fn hash_sha256(data: &[u8]) -> Vec<u8> {
    let mut hasher = Sha256::new();
    hasher.update(data);
    hasher.finalize().to_vec()
}

/// Hash data using SHA-384
pub fn hash_sha384(data: &[u8]) -> Vec<u8> {
    let mut hasher = Sha384::new();
    hasher.update(data);
    hasher.finalize().to_vec()
}

/// Compute HMAC-SHA256
pub fn hmac_sha256(key: &[u8], data: &[u8]) -> Vec<u8> {
    let mut mac = HmacSha256::new_from_slice(key).unwrap();
    mac.update(data);
    mac.finalize().into_bytes().to_vec()
}

// =============================================================================
// UTILITY FUNCTIONS
// =============================================================================

/// Encode encrypted data to base64 for transmission
pub fn encode_encrypted(data: &EncryptedData) -> String {
    let mut combined = data.ciphertext.clone();
    combined.extend_from_slice(&data.nonce);
    combined.extend_from_slice(&data.tag);
    BASE64.encode(&combined)
}

/// Decode encrypted data from base64
pub fn decode_encrypted(encoded: &str) -> Result<EncryptedData, EncryptionError> {
    let combined = BASE64.decode(encoded)
        .map_err(|e| EncryptionError::DecryptionFailed(e.to_string()))?;
    
    if combined.len() < 28 {
        return Err(EncryptionError::InvalidCiphertext);
    }
    
    let ct_len = combined.len() - 28;
    let ciphertext = &combined[..ct_len];
    let nonce = &combined[ct_len..ct_len + 12];
    let tag = &combined[ct_len + 12..];
    
    Ok(EncryptedData {
        ciphertext: ciphertext.to_vec(),
        nonce: nonce.to_vec(),
        tag: tag.to_vec(),
    })
}

// =============================================================================
// TESTS
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_aes_encrypt_decrypt() {
        let master_key = b"0123456789abcdef0123456789abcdef";
        let cipher = AdvancedCipher::new(master_key).unwrap();
        
        let plaintext = b"Hello, TigerScan!";
        let aad = b"additional data";
        
        let encrypted = cipher.encrypt_aead(plaintext, aad).unwrap();
        let decrypted = cipher.decrypt_aead(&encrypted, aad).unwrap();
        
        assert_eq!(plaintext.as_slice(), decrypted.as_slice());
    }

    #[test]
    fn test_chacha_encrypt_decrypt() {
        let master_key = b"0123456789abcdef0123456789abcdef";
        let cipher = AdvancedCipher::new(master_key).unwrap();
        
        let plaintext = b"Hello, ChaCha!";
        let aad = b"additional data";
        
        let encrypted = cipher.encrypt_chacha(plaintext, aad).unwrap();
        let decrypted = cipher.decrypt_chacha(&encrypted, aad).unwrap();
        
        assert_eq!(plaintext.as_slice(), decrypted.as_slice());
    }

    #[test]
    fn test_key_derivation() {
        let input = b"test input";
        let context = b"test context";
        
        let key1 = derive_key(input, context, 32).unwrap();
        let key2 = derive_key(input, context, 32).unwrap();
        
        assert_eq!(key1, key2);
    }

    #[test]
    fn test_hashing() {
        let data = b"test data";
        
        let hash256 = hash_sha256(data);
        let hash384 = hash_sha384(data);
        
        assert_eq!(hash256.len(), 32);
        assert_eq!(hash384.len(), 48);
    }

    #[test]
    fn test_hmac() {
        let key = b"test key";
        let data = b"test data";
        
        let hmac = hmac_sha256(key, data);
        
        assert_eq!(hmac.len(), 32);
    }
}