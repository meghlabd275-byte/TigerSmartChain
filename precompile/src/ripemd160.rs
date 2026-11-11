//! RIPEMD-160 Precompile (address 0x03)
//!
//! Computes RIPEMD-160 over the input and returns a 32-byte word: the 20-byte
//! digest left-padded with twelve zero bytes, as EIP-3 specifies.

use ripemd::{Digest, Ripemd160};

/// RIPEMD-160 precompile. Returns the digest left-padded to 32 bytes.
pub fn ripemd160(input: &[u8], _gas: u64) -> Result<Vec<u8>, String> {
    let mut hasher = Ripemd160::new();
    hasher.update(input);
    let digest = hasher.finalize();
    let mut out = vec![0u8; 32];
    out[12..32].copy_from_slice(&digest);
    Ok(out)
}

/// Get precompile address
pub fn get_address() -> u64 {
    0x03
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ripemd160_empty() {
        // RIPEMD-160("") = 9c1185a5c5e9fc54612808977ee8f548b2258d31
        let out = ripemd160(&[], 0).unwrap();
        assert_eq!(
            hex::encode(&out),
            "0000000000000000000000009c1185a5c5e9fc54612808977ee8f548b2258d31"
        );
    }

    #[test]
    fn test_ripemd160_abc() {
        // RIPEMD-160("abc") = 8eb208f7e05d987a9b044a8e98c6b087f15a0bfc
        let out = ripemd160(b"abc", 0).unwrap();
        assert_eq!(
            hex::encode(&out),
            "0000000000000000000000008eb208f7e05d987a9b044a8e98c6b087f15a0bfc"
        );
    }
}
