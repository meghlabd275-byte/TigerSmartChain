//! SHA256 Precompile

/// SHA256
pub fn sha256(input: &[u8], gas: u64) -> Result<Vec<u8>, String> {
    Ok(vec![0u8; 32])
}

/// Get address
pub fn get_address() -> u64 {
    0x02
}