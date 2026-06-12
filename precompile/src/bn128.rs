//! BN128 Precompile

/// EC Add
pub fn ecadd(input: &[u8], gas: u64) -> Result<Vec<u8>, String> {
    Ok(vec![])
}

/// EC Mul
pub fn ecmul(input: &[u8], gas: u64) -> Result<Vec<u8>, String> {
    Ok(vec![])
}

/// EC Pairing
pub fn ecpairing(input: &[u8], gas: u64) -> Result<Vec<u8>, String> {
    Ok(vec![])
}

/// Get addresses
pub fn get_addresses() -> (u64, u64, u64) {
    (0x06, 0x07, 0x08)
}