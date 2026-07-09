//! Input validation module

use crate::Result;
use lazy_static::lazy_static;
use regex::Regex;

lazy_static! {
    static ref HASH_REGEX: Regex = Regex::new(r"^0x[a-fA-F0-9]{64}$").unwrap();
    static ref BLOCK_REGEX: Regex = Regex::new(r"^\d+$").unwrap();
    static ref HEX_REGEX: Regex = Regex::new(r"^0x[a-fA-F0-9]*$").unwrap();
}

/// Validates various input types
pub struct InputValidator;

impl InputValidator {
    /// Validate transaction hash
    pub fn is_valid_tx_hash(hash: &str) -> bool {
        HASH_REGEX.is_match(hash)
    }
    
    /// Validate block number
    pub fn is_valid_block_number(block: &str) -> bool {
        BLOCK_REGEX.is_match(block) && block.parse::<u64>().map(|n| n > 0).unwrap_or(false)
    }
    
    /// Validate hex string
    pub fn is_valid_hex(data: &str) -> bool {
        HEX_REGEX.is_match(data)
    }
    
    /// Validate integer string
    pub fn is_valid_uint256(value: &str) -> bool {
        // Must be valid decimal or hex number
        if value.starts_with("0x") {
            Self::is_valid_hex(value)
        } else {
            value.chars().all(|c| c.is_ascii_digit())
        }
    }
    
    /// Validate contract bytecode
    pub fn is_valid_bytecode(bytecode: &str) -> bool {
        bytecode.starts_with("0x") && bytecode.len() >= 2
    }
    
    /// Validate ABI-encoded data
    pub fn is_valid_abi_data(data: &str) -> bool {
        Self::is_valid_hex(data) && data.len() >= 2
    }
    
    /// Sanitize search query
    pub fn sanitize_search_query(query: &str) -> String {
        // Remove potentially dangerous characters
        query
            .chars()
            .filter(|c| c.is_alphanumeric() || c.is_whitespace() || *c == '-' || *c == '_')
            .collect::<String>()
            .trim()
            .to_string()
    }
}

/// Validates contract code for dangerous patterns
pub struct ContractValidator;

impl ContractValidator {
    /// Check for common exploit patterns in bytecode
    pub fn has_exploit_patterns(bytecode: &str) -> Vec<String> {
        let mut patterns = Vec::new();
        
        // Self-destruct
        if bytecode.contains("ff") {
            patterns.push("Contains self-destruct (0xff)".to_string());
        }
        
        // Delegate call
        if bytecode.contains("f4") {
            patterns.push("Contains delegatecall (0xf4)".to_string());
        }
        
        // Create2
        if bytecode.contains("f5") {
            patterns.push("Contains create2 (0xf5)".to_string());
        }
        
        patterns
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_tx_hash_validation() {
        assert!(InputValidator::is_valid_tx_hash("0x1234567890abcdef1234567890abcdef1234567890abcdef1234567890abcd"));
        assert!(!InputValidator::is_valid_tx_hash("not-a-hash"));
    }
}
