//! ENS Cryptographic Functions
//! Name hashing and signature verification

use ethers::core::utils::id;
use sha2::{Sha256, Digest};

/// Compute name hash (namehash)
pub fn compute_name_hash(name: &str) -> [u8; 32] {
    let mut hash = [0u8; 32];
    
    if name.is_empty() {
        return hash;
    }
    
    // Split name into labels
    let labels: Vec<&str> = name.split('.').collect();
    let mut current_label = labels.last().unwrap_or(&name);
    
    // Hash last label
    let mut hasher = Sha256::new();
    hasher.update(current_label.as_bytes());
    let result = hasher.finalize();
    hash.copy_from_slice(&result[..32]);
    
    // Hash remaining labels in reverse
    for label in labels.iter().rev().skip(1) {
        let mut hasher = Sha256::new();
        hasher.update(&hash);
        hasher.update(label.as_bytes());
        let result = hasher.finalize();
        hash.copy_from_slice(&result[..32]);
    }
    
    hash
}

/// Compute label hash
pub fn compute_label_hash(label: &str) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(label.as_bytes());
    let result = hasher.finalize();
    let mut hash = [0u8; 32];
    hash.copy_from_slice(&result[..32]);
    hash
}

/// Validate ENS name format
pub fn validate_name(name: &str) -> bool {
    if name.is_empty() || name.len() > 255 {
        return false;
    }
    
    // Must end with .eth for 2LD or be root
    if name != "eth" && !name.ends_with(".eth") {
        return false;
    }
    
    // Check for invalid characters
    for c in name.chars() {
        if !c.is_ascii_alphanumeric() && c != '-' && c != '.' && c != '_' {
            return false;
        }
    }
    
    true
}

/// Validate Ethereum address format
pub fn validate_address(address: &str) -> bool {
    if !address.starts_with("0x") {
        return false;
    }
    if address.len() != 42 {
        return false;
    }
    address[2..].chars().all(|c| c.is_ascii_hexdigit())
}