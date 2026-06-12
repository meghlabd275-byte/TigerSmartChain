//! Advanced Encryption Module for TigerScan
//! AES-256-GCM encryption, SHA-256 hashing, HMAC, and more

use aes_gcm::{
    aead::{Aead, KeyInit, OsRng},
    Aes256Gcm, Nonce,
};
use base64::{engine::general_purpose::STANDARD as BASE64, Engine};
use digest::Digest;
use hmac::{Hmac, Mac};
use rand::RngCore;
use sha2::{Sha256, Sha512};
use std::time::{SystemTime, UNIX_EPOCH};
use subtle::ConstantTimeEq;

// =============================================================================
// CONSTANTS
// =============================================================================

pub const AES_256_KEY_SIZE: usize = 32;
pub const GCM_NONCE_SIZE: usize = 12;
pub const GCM_TAG_SIZE: usize = 16;

// =============================================================================
// ERROR TYPES
// =============================================================================

#[derive(Debug, Clone)]
pub enum SecurityError {
    EncryptionError(String),
    DecryptionError(String),
    HashError(String),
    InvalidKey(String),
    InvalidData(String),
}

impl std::fmt::Display for SecurityError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SecurityError::EncryptionError(e) => write!(f, "Encryption error: {}", e),
            SecurityError::DecryptionError(e) => write!(f, "Decryption error: {}", e),
            SecurityError::HashError(e) => write!(f, "Hash error: {}", e),
            SecurityError::InvalidKey(e) => write!(f, "Invalid key: {}", e),
            SecurityError::InvalidData(e) => write!(f, "Invalid data: {}", e),
        }
    }
}

impl std::error::Error for SecurityError {}

pub type Result<T> = std::result::Result<T, SecurityError>;

// =============================================================================
// AES-256-GCM ENCRYPTION
// =============================================================================

/// Advanced Encryptor using AES-256-GCM
pub struct Encryptor {
    key: [u8; AES_256_KEY_SIZE],
}

impl Encryptor {
    /// Create a new encryptor with a 256-bit key
    pub fn new(key: &[u8; AES_256_KEY_SIZE]) -> Self {
        Self { key: *key }
    }

    /// Create a new encryptor with key generation
    pub fn generate() -> Result<(Self, Vec<u8>)> {
        let mut key = [0u8; AES_256_KEY_SIZE];
        OsRng.fill_bytes(&mut key);
        Ok((Self::new(&key), key.to_vec()))
    }

    /// Encrypt data using AES-256-GCM
    /// Returns: nonce (12 bytes) + ciphertext + tag (16 bytes)
    pub fn encrypt(&self, plaintext: &[u8]) -> Result<Vec<u8>> {
        let cipher = Aes256Gcm::new_from_slice(&self.key)
            .map_err(|e| SecurityError::EncryptionError(e.to_string()))?;

        let mut nonce_bytes = [0u8; GCM_NONCE_SIZE];
        OsRng.fill_bytes(&mut nonce_bytes);
        let nonce = Nonce::from_slice(&nonce_bytes);

        let ciphertext = cipher
            .encrypt(nonce, plaintext)
            .map_err(|e| SecurityError::EncryptionError(e.to_string()))?;

        // Prepend nonce to ciphertext
        let mut result = Vec::with_capacity(GCM_NONCE_SIZE + ciphertext.len());
        result.extend_from_slice(&nonce_bytes);
        result.extend_from_slice(&ciphertext);

        Ok(result)
    }

    /// Decrypt data using AES-256-GCM
    pub fn decrypt(&self, data: &[u8]) -> Result<Vec<u8>> {
        if data.len() < GCM_NONCE_SIZE + GCM_TAG_SIZE {
            return Err(SecurityError::InvalidData(
                "Data too short".to_string(),
            ));
        }

        let cipher = Aes256Gcm::new_from_slice(&self.key)
            .map_err(|e| SecurityError::DecryptionError(e.to_string()))?;

        let nonce = Nonce::from_slice(&data[..GCM_NONCE_SIZE]);
        let ciphertext = &data[GCM_NONCE_SIZE..];

        cipher
            .decrypt(nonce, ciphertext)
            .map_err(|e| SecurityError::DecryptionError(e.to_string()))
    }

    /// Encrypt string and return base64 encoded result
    pub fn encrypt_to_base64(&self, plaintext: &str) -> Result<String> {
        let encrypted = self.encrypt(plaintext.as_bytes())?;
        Ok(BASE64.encode(&encrypted))
    }

    /// Decrypt base64 encoded data
    pub fn decrypt_from_base64(&self, data: &str) -> Result<String> {
        let decoded = BASE64
            .decode(data)
            .map_err(|e| SecurityError::InvalidData(e.to_string()))?;
        
        let decrypted = self.decrypt(&decoded)?;
        
        String::from_utf8(decrypted)
            .map_err(|e| SecurityError::InvalidData(e.to_string()))
    }
}

// =============================================================================
// KEY DERIVATION
// =============================================================================

/// Derive a key from password using PBKDF2
pub fn derive_key(password: &str, salt: &[u8], iterations: u32) -> Result<[u8; 32]> {
    use std::num::NonZeroU32;
    
    let password_bytes = password.as_bytes();
    
    // Use SHA-256 for key derivation (simplified PBKDF2)
    let mut result = Vec::new();
    let mut block = Vec::new();
    
    let num_blocks = (AES_256_KEY_SIZE + 31) / 32;
    
    for block_num in 1..=num_blocks {
        block.clear();
        
        // INT(i)
        block.extend_from_slices(&[
            ((block_num >> 24) & 0xff) as u8,
            ((block_num >> 16) & 0xff) as u8,
            ((block_num >> 8) & 0xff) as u8,
            (block_num & 0xff) as u8,
        ]);
        
        // U1 = PRF(Password, Salt || INT(i))
        let mut mac: Hmac<Sha256> = Mac::new_from_slice(password_bytes)
            .map_err(|e| SecurityError::HashError(e.to_string()))?;
        mac.update(salt);
        mac.update(&block);
        let mut u = mac.finalize().into_bytes().to_vec();
        result.extend_from_slice(&u);
        
        // U2..Uc
        for _ in 1..iterations {
            mac = Hmac::<Sha256>::new_from_slice(password_bytes)
                .map_err(|e| SecurityError::HashError(e.to_string()))?;
            mac.update(&u);
            u = mac.finalize().into_bytes().to_vec();
            
            // XOR
            for (i, byte) in result.iter_mut().enumerate() {
                if i < u.len() {
                    *byte ^= u[i];
                }
            }
        }
    }
    
    let mut key = [0u8; AES_256_KEY_SIZE];
    key.copy_from_slice(&result[..AES_256_KEY_SIZE]);
    Ok(key)
}

// =============================================================================
// PASSWORD HASHING
// =============================================================================

/// Secure password hashing with salt
pub struct PasswordHasher {
    iterations: u32,
    salt_size: usize,
}

impl Default for PasswordHasher {
    fn default() -> Self {
        Self::new()
    }
}

impl PasswordHasher {
    pub fn new() -> Self {
        Self {
            iterations: 100_000,
            salt_size: 32,
        }
    }

    /// Hash a password with automatic salt generation
    pub fn hash(&self, password: &str) -> Result<String> {
        let mut salt = vec![0u8; self.salt_size];
        OsRng.fill_bytes(&mut salt);
        
        self.hash_with_salt(password, &salt)
    }

    /// Hash a password with provided salt
    pub fn hash_with_salt(&self, password: &str, salt: &[u8]) -> Result<String> {
        let key = derive_key(password, salt, self.iterations)?;
        
        // Format: base64(salt + hash)
        let mut combined = Vec::with_capacity(salt.len() + 32);
        combined.extend_from_slice(salt);
        combined.extend_from_slice(&key);
        
        Ok(BASE64.encode(&combined))
    }

    /// Verify a password against a stored hash
    pub fn verify(&self, password: &str, stored_hash: &str) -> Result<bool> {
        let combined = BASE64
            .decode(stored_hash)
            .map_err(|e| SecurityError::InvalidData(e.to_string()))?;
        
        if combined.len() < self.salt_size + 32 {
            return Ok(false);
        }
        
        let salt = &combined[..self.salt_size];
        let stored_key = &combined[self.salt_size..self.salt_size + 32];
        
        let computed_key = derive_key(password, salt, self.iterations)?;
        
        // Constant-time comparison
        let result = computed_key.ct_eq(stored_key).unwrap_u8();
        
        Ok(result == 1)
    }
}

// =============================================================================
// HMAC
// =============================================================================

/// Compute HMAC-SHA256
pub fn hmac_sha256(key: &[u8], data: &[u8]) -> Vec<u8> {
    let mut mac = Hmac::<Sha256>::new_from_slice(key)
        .expect("HMAC can take key of any size");
    mac.update(data);
    mac.finalize().into_bytes().to_vec()
}

/// Compute HMAC-SHA512
pub fn hmac_sha512(key: &[u8], data: &[u8]) -> Vec<u8> {
    let mut mac = Hmac::<Sha512>::new_from_slice(key)
        .expect("HMAC can take key of any size");
    mac.update(data);
    mac.finalize().into_bytes().to_vec()
}

/// Verify HMAC
pub fn verify_hmac(key: &[u8], data: &[u8], expected: &[u8]) -> bool {
    let computed = hmac_sha256(key, data);
    computed.ct_eq(expected).unwrap_u8() == 1
}

// =============================================================================
// SHA HASHING
// =============================================================================

/// SHA-256 hash
pub fn sha256(data: &[u8]) -> Vec<u8> {
    let mut hasher = Sha256::new();
    hasher.update(data);
    hasher.finalize().to_vec()
}

/// SHA-512 hash
pub fn sha512(data: &[u8]) -> Vec<u8> {
    let mut hasher = Sha512::new();
    hasher.update(data);
    hasher.finalize().to_vec()
}

/// SHA-256 hash as hex string
pub fn sha256_hex(data: &[u8]) -> String {
    hex::encode(sha256(data))
}

// =============================================================================
// SECURE RANDOM
// =============================================================================

/// Generate secure random bytes
pub fn secure_random(size: usize) -> Vec<u8> {
    let mut bytes = vec![0u8; size];
    OsRng.fill_bytes(&mut bytes);
    bytes
}

/// Generate secure random hex string
pub fn secure_random_hex(size: usize) -> String {
    let bytes = secure_random(size);
    hex::encode(bytes)
}

// =============================================================================
// TOKEN GENERATION
// =============================================================================

/// Generate a cryptographically secure token
pub fn generate_token(length: usize) -> String {
    secure_random_hex(length)
}

/// Generate timestamp-based token
pub fn generate_timestamp_token() -> String {
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    
    let mut data = timestamp.to_le_bytes().to_vec();
    let random = secure_random(16);
    data.extend_from_slice(&random);
    
    sha256_hex(&data)
}

// =============================================================================
// CONSTANT-TIME COMPARISON
// =============================================================================

/// Constant-time string comparison (for passwords, tokens, etc.)
pub fn constant_time_eq(a: &str, b: &str) -> bool {
    a.as_bytes().ct_eq(b.as_bytes()).unwrap_u8() == 1
}

/// Constant-time slice comparison
pub fn constant_time_slice_eq(a: &[u8], b: &[u8]) -> bool {
    a.ct_eq(b).unwrap_u8() == 1
}

// =============================================================================
// TESTS
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_encryption_decryption() {
        let (encryptor, key) = Encryptor::generate().unwrap();
        
        let plaintext = "Hello, TigerScan!";
        let encrypted = encryptor.encrypt(plaintext.as_bytes()).unwrap();
        let decrypted = encryptor.decrypt(&encrypted).unwrap();
        
        assert_eq!(plaintext.as_bytes(), decrypted.as_slice());
        
        // Verify base64
        let b64 = encryptor.encrypt_to_base64(plaintext).unwrap();
        let decrypted_b64 = encryptor.decrypt_from_base64(&b64).unwrap();
        assert_eq!(plaintext, decrypted_b64);
    }

    #[test]
    fn test_password_hashing() {
        let hasher = PasswordHasher::new();
        
        let password = "secure_password_123";
        let hash = hasher.hash(password).unwrap();
        
        assert!(hasher.verify(password, &hash).unwrap());
        assert!(!hasher.verify("wrong_password", &hash).unwrap());
    }

    #[test]
    fn test_hmac() {
        let key = b"secret_key";
        let data = b"Hello, World!";
        
        let signature = hmac_sha256(key, data);
        assert!(verify_hmac(key, data, &signature));
    }

    #[test]
    fn test_sha256() {
        let data = b"test data";
        let hash = sha256(data);
        
        assert_eq!(hash.len(), 32);
        assert_eq!(sha256_hex(data).len(), 64);
    }

    #[test]
    fn test_constant_time_eq() {
        assert!(constant_time_eq("test", "test"));
        assert!(!constant_time_eq("test", "Test"));
        assert!(!constant_time_eq("test", "test1"));
    }
}