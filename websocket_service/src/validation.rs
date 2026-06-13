//! Input Validation
//! Security-focused input validation

use crate::error::{Error, Result};

/// Validate address format (Ethereum address)
pub fn validate_address(address: &str) -> Result<()> {
    if !address.starts_with("0x") {
        return Err(Error::validation("Address must start with 0x"));
    }
    if address.len() != 42 {
        return Err(Error::validation("Address must be 42 characters"));
    }
    if !address[2..].chars().all(|c| c.is_ascii_hexdigit()) {
        return Err(Error::validation("Address contains invalid characters"));
    }
    Ok(())
}

/// Validate transaction hash format
pub fn validate_tx_hash(hash: &str) -> Result<()> {
    if !hash.starts_with("0x") {
        return Err(Error::validation("Transaction hash must start with 0x"));
    }
    if hash.len() != 66 {
        return Err(Error::validation("Transaction hash must be 66 characters"));
    }
    if !hash[2..].chars().all(|c| c.is_ascii_hexdigit()) {
        return Err(Error::validation("Transaction hash contains invalid characters"));
    }
    Ok(())
}

/// Validate block number
pub fn validate_block_number(block: u64) -> Result<()> {
    if block == 0 {
        return Err(Error::validation("Block number must be greater than 0"));
    }
    Ok(())
}

/// Validate subscription filter
pub fn validate_filter(filter: &serde_json::Value) -> Result<()> {
    // Basic validation - in production would be more thorough
    if let Some(obj) = filter.as_object() {
        // Check for address filter if present
        if let Some(addr) = obj.get("address") {
            if let Some(s) = addr.as_str() {
                validate_address(s)?;
            }
        }
    }
    Ok(())
}