//! Hashing operations for TigerSmartChain.

use sha2::{Sha256, Sha512, Digest};
use sha3::{Keccak256, Shake256};
use keccak_hash::keccak;
use blake3::Hasher as Blake3Hasher;

/// Compute SHA-256 hash.
pub fn sha256(data: &[u8]) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(data);
    let result = hasher.finalize();
    let mut hash = [0u8; 32];
    hash.copy_from_slice(&result);
    hash
}

/// Compute SHA-512 hash.
pub fn sha512(data: &[u8]) -> [u8; 64] {
    let mut hasher = Sha512::new();
    hasher.update(data);
    let result = hasher.finalize();
    let mut hash = [0u8; 64];
    hash.copy_from_slice(&result);
    hash
}

/// Compute Keccak-256 hash (Ethereum standard).
pub fn keccak256(data: &[u8]) -> [u8; 32] {
    let hash = keccak(data);
    let mut result = [0u8; 32];
    result.copy_from_slice(&hash);
    result
}

/// Compute Keccak-512 hash.
pub fn keccak512(data: &[u8]) -> [u8; 64] {
    let mut hasher = Keccak256::default();
    hasher.update(data);
    let result = hasher.finalize();
    let mut hash = [0u8; 64];
    hash.copy_from_slice(&result);
    hash
}

/// Compute BLAKE3 hash.
pub fn blake3(data: &[u8]) -> [u8; 32] {
    let mut hasher = Blake3Hasher::new();
    hasher.update(data);
    let mut hash = [0u8; 32];
    hasher.finalize_xof().fill(&mut hash);
    hash
}

/// Compute BLAKE3 hash with key.
pub fn blake3_with_key(data: &[u8], key: &[u8]) -> [u8; 32] {
    let mut hasher = Blake3Hasher::new_keyed(key);
    hasher.update(data);
    let mut hash = [0u8; 32];
    hasher.finalize_xof().fill(&mut hash);
    hash
}

/// Compute RIPEMD-160 hash (for address generation).
pub fn ripemd160(data: &[u8]) -> [u8; 20] {
    let mut hasher = ripemd::Ripemd160::new();
    hasher.update(data);
    let result = hasher.finalize();
    let mut hash = [0u8; 20];
    hash.copy_from_slice(&result);
    hash
}

/// Compute combined hash (SHA-256 of RIPEMD-160).
pub fn hash160(data: &[u8]) -> [u8; 20] {
    let sha = sha256(data);
    ripemd160(&sha)
}

/// Compute address from public key.
pub fn public_key_to_address(public_key: &[u8]) -> [u8; 20] {
    let hash = keccak256(public_key);
    let mut address = [0u8; 20];
    address.copy_from_slice(&hash[12..32]);
    address
}