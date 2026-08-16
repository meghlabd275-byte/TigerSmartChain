//! SHA-256 Precompile (address 0x02)
//!
//! Computes SHA-256 over the input and returns a 32-byte digest.

use sha2::{Sha256, Digest};

/// SHA-256 precompile. Returns 32-byte digest.
pub fn sha256(input: &[u8], _gas: u64) -> Result<Vec<u8>, String> {
    let mut hasher = Sha256::new();
    hasher.update(input);
    let digest = hasher.finalize();
    Ok(digest.to_vec())
}

/// Get precompile address
pub fn get_address() -> u64 {
    0x02
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sha256_empty() {
        let out = sha256(&[], 0).unwrap();
        // SHA-256("") = e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855
        let expected = "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855";
        assert_eq!(hex::encode(&out), expected);
    }

    #[test]
    fn test_sha256_abc() {
        let out = sha256(b"abc", 0).unwrap();
        // SHA-256("abc") = ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad
        let expected = "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad";
        assert_eq!(hex::encode(&out), expected);
    }
}
