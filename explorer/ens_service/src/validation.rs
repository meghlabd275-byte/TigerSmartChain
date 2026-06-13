//! ENS Input Validation
//! Security-focused input validation

use crate::errors::{Error, Result};

/// Validate ENS name
pub fn validate_ens_name(name: &str) -> Result<()> {
    if name.is_empty() {
        return Err(Error::invalid_name("Name cannot be empty"));
    }
    
    if name.len() > 255 {
        return Err(Error::invalid_name("Name too long (max 255 characters)"));
    }
    
    // Check for valid characters
    for c in name.chars() {
        if !c.is_ascii_alphanumeric() && c != '-' && c != '.' && c != '_' {
            return Err(Error::invalid_name(format!("Invalid character: {}", c)));
        }
    }
    
    Ok(())
}

/// Validate Ethereum address
pub fn validate_ethereum_address(address: &str) -> Result<()> {
    if !address.starts_with("0x") {
        return Err(Error::invalid_address("Must start with 0x"));
    }
    
    if address.len() != 42 {
        return Err(Error::invalid_address("Must be 42 characters"));
    }
    
    if !address[2..].chars().all(|c| c.is_ascii_hexdigit()) {
        return Err(Error::invalid_address("Invalid hex characters"));
    }
    
    Ok(())
}