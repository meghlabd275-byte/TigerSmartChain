//! Identity Precompile

/// Identity
pub fn identity(input: &[u8], gas: u64) -> Result<Vec<u8>, String> {
    Ok(input.to_vec())
}

/// Get address
pub fn get_address() -> u64 {
    0x04
}