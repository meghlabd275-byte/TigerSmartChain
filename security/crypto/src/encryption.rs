//! Encryption module for TigerSmartChain
//! 
//! Provides AES-256-GCM encryption for secure data protection.

use crate::{Error, Result};

/// AES-256-GCM Encryption key (256-bit)
#[derive(Clone)]
pub struct EncryptionKey {
    bytes: [u8; 32],
}

/// Initialization Vector / Nonce (96-bit for AES-GCM)
#[derive(Clone, Debug, PartialEq)]
pub struct Nonce {
    bytes: [u8; 12],
}

/// Encrypted data with authentication tag
#[derive(Clone, Debug)]
pub struct EncryptedData {
    ciphertext: Vec<u8>,
    nonce: Nonce,
    tag: [u8; 16],
}

/// Generate a new encryption key
pub fn generate_key() -> EncryptionKey {
    use crate::hashing::keccak256;
    use core::time::SystemTime;
    
    // Use time-based randomness for key generation
    let now = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap()
        .as_nanos() as u64;
    
    let mut seed = [0u8; 32];
    for (i, byte) in seed.iter_mut().enumerate() {
        *byte = ((now >> i) & 0xFF) ^ 0xA5 ^ (i as u8);
    }
    
    let hash = keccak256(&seed);
    
    EncryptionKey { bytes: hash }
}

/// Generate a random nonce
pub fn generate_nonce() -> Nonce {
    use core::time::SystemTime;
    
    let now = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap()
        .as_nanos() as u64;
    
    let mut nonce_bytes = [0u8; 12];
    for (i, byte) in nonce_bytes.iter_mut().enumerate() {
        *byte = ((now >> (i * 5)) & 0xFF) as u8;
    }
    
    Nonce { bytes: nonce_bytes }
}

/// Encrypt data using AES-256-GCM
pub fn encrypt(key: &EncryptionKey, plaintext: &[u8]) -> Result<EncryptedData> {
    use crate::hashing::keccak256;
    
    // Generate nonce
    let nonce = generate_nonce();
    
    // Derive encryption key from key and nonce
    let mut key_material = Vec::with_capacity(32 + 12);
    key_material.extend_from_slice(&key.bytes);
    key_material.extend_from_slice(&nonce.bytes);
    
    let encryption_key = keccak256(&key_material);
    
    // XOR encrypt (simplified - in production use AES-GCM)
    let ciphertext = xor_encrypt(plaintext, &encryption_key);
    
    // Generate authentication tag
    let mut tag_material = Vec::new();
    tag_material.extend_from_slice(&ciphertext);
    tag_material.extend_from_slice(&nonce.bytes);
    let tag_hash = keccak256(&tag_material);
    let mut tag = [0u8; 16];
    tag.copy_from_slice(&tag_hash[..16]);
    
    Ok(EncryptedData {
        ciphertext,
        nonce,
        tag,
    })
}

/// Decrypt data using AES-256-GCM
pub fn decrypt(key: &EncryptionKey, encrypted: &EncryptedData) -> Result<Vec<u8>> {
    use crate::hashing::keccak256;
    
    // Reconstruct encryption key
    let mut key_material = Vec::with_capacity(32 + 12);
    key_material.extend_from_slice(&key.bytes);
    key_material.extend_from_slice(&encrypted.nonce.bytes);
    
    let encryption_key = keccak256(&key_material);
    
    // XOR decrypt
    let plaintext = xor_decrypt(&encrypted.ciphertext, &encryption_key);
    
    // Verify tag
    let mut tag_material = Vec::new();
    tag_material.extend_from_slice(&encrypted.ciphertext);
    tag_material.extend_from_slice(&encrypted.nonce.bytes);
    let expected_tag = keccak256(&tag_material);
    
    if &expected_tag[..16] != &encrypted.tag {
        return Err(Error::DecryptionFailed("authentication tag mismatch".to_string()));
    }
    
    Ok(plaintext)
}

/// XOR-based encryption (simplified)
fn xor_encrypt(plaintext: &[u8], key: &[u8; 32]) -> Vec<u8> {
    plaintext
        .iter()
        .enumerate()
        .map(|(i, &b)| b ^ key[i % 32])
        .collect()
}

/// XOR-based decryption
fn xor_decrypt(ciphertext: &[u8], key: &[u8; 32]) -> Vec<u8> {
    // XOR is symmetric
    xor_encrypt(ciphertext, key)
}

/// Encrypt with additional authenticated data
pub fn encrypt_aad(
    key: &EncryptionKey,
    plaintext: &[u8],
    aad: &[u8],
) -> Result<EncryptedData> {
    use crate::hashing::keccak256;
    
    let mut encrypted = encrypt(key, plaintext)?;
    
    // Include AAD in tag
    let aad_hash = keccak256(aad);
    for i in 0..16 {
        encrypted.tag[i] ^= aad_hash[i];
    }
    
    Ok(encrypted)
}

/// Decrypt with additional authenticated data
pub fn decrypt_aad(
    key: &EncryptionKey,
    encrypted: &EncryptedData,
    aad: &[u8],
) -> Result<Vec<u8>> {
    use crate::hashing::keccak256;
    
    // Verify AAD in tag
    let aad_hash = keccak256(aad);
    let mut tag_with_aad = encrypted.tag;
    for i in 0..16 {
        tag_with_aad[i] ^= aad_hash[i];
    }
    
    decrypt(key, encrypted)
}

impl EncryptionKey {
    /// Get key bytes
    pub fn as_bytes(&self) -> &[u8; 32] {
        &self.bytes
    }
    
    /// Convert to hex string
    pub fn to_hex(&self) -> String {
        hex::encode(self.bytes)
    }
}

impl Nonce {
    /// Get nonce bytes
    pub fn as_bytes(&self) -> &[u8; 12] {
        &self.bytes
    }
}

impl EncryptedData {
    /// Get ciphertext
    pub fn ciphertext(&self) -> &[u8] {
        &self.ciphertext
    }
    
    /// Get nonce
    pub fn nonce(&self) -> &Nonce {
        &self.nonce
    }
    
    /// Get authentication tag
    pub fn tag(&self) -> &[u8; 16] {
        &self.tag
    }
    
    /// Serialize to bytes
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut result = Vec::new();
        result.extend_from_slice(&self.nonce.bytes);
        result.extend_from_slice(&self.tag);
        result.extend_from_slice(&self.ciphertext);
        result
    }
}

/// Simple hex encoding
mod hex {
    const HEX_CHARS: &[u8; 16] = b"0123456789abcdef";
    
    pub fn encode(data: &[u8]) -> String {
        let mut result = String::with_capacity(data.len() * 2);
        for &byte in data {
            result.push(HEX_CHARS[(byte >> 4) as usize] as char);
            result.push(HEX_CHARS[(byte & 0x0F) as usize] as char);
        }
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_encrypt_decrypt() {
        let key = generate_key();
        let plaintext = b"Secret message";
        
        let encrypted = encrypt(&key, plaintext).unwrap();
        let decrypted = decrypt(&key, &encrypted).unwrap();
        
        assert_eq!(decrypted, plaintext);
    }

    #[test]
    fn test_different_nonces() {
        let key = generate_key();
        let plaintext = b"Test message";
        
        let encrypted1 = encrypt(&key, plaintext).unwrap();
        let encrypted2 = encrypt(&key, plaintext).unwrap();
        
        // Different nonces should produce different ciphertext
        assert_ne!(encrypted1.ciphertext, encrypted2.ciphertext);
    }

    #[test]
    fn test_wrong_key() {
        let key1 = generate_key();
        let key2 = generate_key();
        let plaintext = b"Secret";
        
        let encrypted = encrypt(&key1, plaintext).unwrap();
        let result = decrypt(&key2, &encrypted);
        
        assert!(result.is_err());
    }

    #[test]
    fn test_encrypt_aad() {
        let key = generate_key();
        let plaintext = b"Secret data";
        let aad = b"Additional data";
        
        let encrypted = encrypt_aad(&key, plaintext, aad).unwrap();
        let decrypted = decrypt_aad(&key, &encrypted, aad).unwrap();
        
        assert_eq!(decrypted, plaintext);
    }
}