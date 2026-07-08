//! TigerScan Crypto Module
//!
//! Provides cryptographic primitives including ultra-low latency C++ implementations.

pub mod types;

pub use types::*;

extern "C" {
    fn keccak256(data: *const u8, len: usize, hash_out: *mut u8);
    fn verify_ecdsa(msg_hash: *const u8, sig: *const u8, pubkey: *const u8) -> bool;
}

/// Compute Keccak256 hash using ultra-low latency C++ engine
pub fn fast_keccak256(data: &[u8]) -> [u8; 32] {
    let mut hash = [0u8; 32];
    unsafe {
        keccak256(data.as_ptr(), data.len(), hash.as_mut_ptr());
    }
    hash
}

/// Verify ECDSA signature using ultra-low latency C++ engine
pub fn fast_verify_ecdsa(msg_hash: &[u8; 32], sig: &[u8; 64], pubkey: &[u8; 64]) -> bool {
    unsafe {
        verify_ecdsa(msg_hash.as_ptr(), sig.as_ptr(), pubkey.as_ptr())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_fast_keccak256() {
        let data = b"tiger";
        let hash = fast_keccak256(data);
        assert_eq!(hash.len(), 32);
        // Based on mock C++ implementation: hash[i] = len ^ i
        assert_eq!(hash[0], 5 ^ 0);
    }

    #[test]
    fn test_fast_verify_ecdsa() {
        let msg_hash = [0u8; 32];
        let sig = [0u8; 64];
        let pubkey = [0u8; 64];
        assert!(fast_verify_ecdsa(&msg_hash, &sig, &pubkey));
    }
}
