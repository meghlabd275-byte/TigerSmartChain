//! Address validation and verification module

use crate::Result;
use lazy_static::lazy_static;
use regex::Regex;
use sha3::{Digest, Keccak256};

lazy_static! {
    static ref ADDRESS_REGEX: Regex = Regex::new(r"^0x[a-fA-F0-9]{40}$").unwrap();
    static ref CONTRACT_REGEX: Regex = Regex::new(r"^0x[a-fA-F0-9]{40}$").unwrap();
}

/// Validates and verifies blockchain addresses
pub struct AddressValidator;

impl AddressValidator {
    /// Validate Ethereum-style address format
    pub fn is_valid(address: &str) -> bool {
        ADDRESS_REGEX.is_match(address)
    }
    
    /// Verify address checksum (EIP-55)
    pub fn verify_checksum(address: &str) -> bool {
        if !ADDRESS_REGEX.is_match(address) {
            return false;
        }
        
        let addr = address.trim_start_matches("0x");
        let hash = Self::keccak256(&addr.to_lowercase());
        
        for (i, c) in addr.chars().enumerate() {
            let hash_char = hash.chars().nth(i / 2).unwrap_or('0');
            let hash_val = hex_char_to_int(hash_char);
            
            if (hash_val > 7 && c.is_uppercase()) || (hash_val <= 7 && c.is_lowercase()) {
                return false;
            }
        }
        
        true
    }
    
    /// Convert address to checksum format
    pub fn to_checksum(address: &str) -> String {
        let addr = address.trim_start_matches("0x");
        let hash = Self::keccak256(&addr.to_lowercase());
        
        let mut result = String::from("0x");
        
        for (i, c) in addr.chars().enumerate() {
            let hash_char = hash.chars().nth(i / 2).unwrap_or('0');
            let hash_val = hex_char_to_int(hash_char);
            
            if hash_val > 7 && c.is_ascii_lowercase() {
                result.push(c.to_ascii_uppercase());
            } else {
                result.push(c);
            }
        }
        
        result
    }
    
    /// Check if address is a contract (has code)
    pub fn is_contract(&self, _address: &str) -> bool {
        // This would check via RPC if the address has code
        // For now, we return false as this requires external data
        false
    }
    
    /// Generate address from public key
    pub fn from_public_key(public_key: &[u8]) -> String {
        let hash = Keccak256::digest(public_key);
        let address_bytes = &hash[12..];
        format!("0x{}", hex::encode(address_bytes))
    }
    
    /// Generate address from private key
    pub fn from_private_key(private_key: &[u8]) -> Option<String> {
        use secp256k1::{Secp256k1, SecretKey};
        
        let secp = Secp256k1::new();
        let secret = SecretKey::from_slice(private_key).ok()?;
        let public_key = secp256k1::PublicKey::from_secret_key(&secp, &secret);
        
        let uncompressed = public_key.serialize_uncompressed();
        // Skip first byte (0x04) for uncompressed key
        Some(Self::from_public_key(&uncompressed[1..]))
    }
    
    fn keccak256(data: &str) -> String {
        let hash = Keccak256::digest(data.as_bytes());
        hex::encode(hash)
    }
}

/// Check if address is a commonly used contract
pub fn is_known_contract(address: &str) -> bool {
    let known_addresses = [
        // Common tokens
        "0xbb4cdb9cbd36b01bd1cbaebf2de08d9173b095c", // WBNB
        "0x55d398326f99059ff775485246999027b3197955", // USDT
        "0xe9e7cea3dedca5984780bafc599bd69add087d56", // BUSD
        "0x8ba1f109551bd432803012645ac136ddd64dba72", // BNB
        // Exchanges
        "0x10ed43c718714eb63d5aa57b78b54704e256024e", // PancakeSwap
        "0xd9e2889b4c3c2d803e6dc3b4e7e7a3c8f8e9a1b", // Uniswap
    ];
    
    let addr = address.trim_start_matches("0x").to_lowercase();
    known_addresses.contains(&addr.as_str())
}

fn hex_char_to_int(c: char) -> u8 {
    match c {
        '0'..='9' => c as u8 - b'0',
        'a'..='f' => c as u8 - b'a' + 10,
        'A'..='F' => c as u8 - b'A' + 10,
        _ => 0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_address_validation() {
        assert!(AddressValidator::is_valid("0x742d35Cc6634C0532925a3b844Bc9e7595f0eB1E"));
        assert!(!AddressValidator::is_valid("0x742d35Cc6634C0532925a3b844Bc9e7595f0eB1"));
        assert!(!AddressValidator::is_valid("not-an-address"));
    }
    
    #[test]
    fn test_checksum() {
        let addr = "0x742d35Cc6634C0532925a3b844Bc9e7595f0eB1E";
        let checksum = AddressValidator::to_checksum(addr);
        assert!(AddressValidator::verify_checksum(&checksum));
    }
}
