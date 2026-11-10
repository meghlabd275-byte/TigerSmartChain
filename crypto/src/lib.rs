//! TigerSmartChain cryptographic primitives.
//!
//! Hashing (Keccak-256/512, SHA3-256) and secp256k1 ECDSA verification and
//! public-key recovery are implemented in C++ (`crypto_cpp/`) for latency and
//! bound here over a small C ABI. The C++ side is covered by known-answer
//! tests (`crypto_cpp/crypto_test.cpp`) using published NIST, Keccak and
//! RFC 6979 vectors.

use std::fmt;

pub mod types;

pub use types::*;

// The C++ symbols share their names with the Rust wrappers below, so they are
// bound under distinct local identifiers via `link_name`.
#[link(name = "tiger_crypto")]
extern "C" {
    #[link_name = "keccak256"]
    fn c_keccak256(data: *const u8, len: usize, hash_out: *mut u8);
    #[link_name = "keccak512"]
    fn c_keccak512(data: *const u8, len: usize, hash_out: *mut u8);
    #[link_name = "sha3_256"]
    fn c_sha3_256(data: *const u8, len: usize, hash_out: *mut u8);
    #[link_name = "verify_ecdsa"]
    fn c_verify_ecdsa(msg_hash: *const u8, sig: *const u8, pubkey: *const u8) -> i32;
    #[link_name = "recover_pubkey"]
    fn c_recover_pubkey(
        msg_hash: *const u8,
        sig: *const u8,
        recovery_id: u8,
        pubkey_out: *mut u8,
    ) -> i32;
    #[link_name = "pubkey_to_address"]
    fn c_pubkey_to_address(pubkey: *const u8, address_out: *mut u8);
}

/// Errors surfaced by the cryptographic primitives.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CryptoError {
    /// The input had the wrong length for the requested operation.
    InvalidLength { expected: usize, got: usize },
    /// The signature could not be recovered or verified.
    InvalidSignature,
}

impl fmt::Display for CryptoError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CryptoError::InvalidLength { expected, got } => {
                write!(f, "invalid length: expected {expected} bytes, got {got}")
            }
            CryptoError::InvalidSignature => write!(f, "invalid signature"),
        }
    }
}

impl std::error::Error for CryptoError {}

/// Compute the Ethereum Keccak-256 digest of `data`.
pub fn keccak256(data: &[u8]) -> [u8; 32] {
    let mut hash = [0u8; 32];
    // Safety: `data` is valid for `data.len()` bytes and `hash` is a distinct
    // 32-byte output buffer.
    unsafe {
        c_keccak256(data.as_ptr(), data.len(), hash.as_mut_ptr());
    }
    hash
}

/// Compute the Keccak-512 digest of `data`.
pub fn keccak512(data: &[u8]) -> [u8; 64] {
    let mut hash = [0u8; 64];
    unsafe {
        c_keccak512(data.as_ptr(), data.len(), hash.as_mut_ptr());
    }
    hash
}

/// Compute the FIPS-202 SHA3-256 digest of `data`.
///
/// This is deliberately different from [`keccak256`]: Ethereum uses the
/// original Keccak padding, FIPS-202 uses 0x06.
pub fn sha3_256(data: &[u8]) -> [u8; 32] {
    let mut hash = [0u8; 32];
    unsafe {
        c_sha3_256(data.as_ptr(), data.len(), hash.as_mut_ptr());
    }
    hash
}

/// Derive the 20-byte Ethereum address for an uncompressed public key.
///
/// `pubkey` is the 64-byte `x || y` encoding, without the `0x04` prefix.
pub fn pubkey_to_address(pubkey: &[u8]) -> Result<[u8; 20], CryptoError> {
    if pubkey.len() != 64 {
        return Err(CryptoError::InvalidLength {
            expected: 64,
            got: pubkey.len(),
        });
    }
    let mut address = [0u8; 20];
    unsafe {
        c_pubkey_to_address(pubkey.as_ptr(), address.as_mut_ptr());
    }
    Ok(address)
}

/// Verify a secp256k1 ECDSA signature.
///
/// * `msg_hash` -- the 32-byte digest that was signed.
/// * `signature` -- 64 bytes, `r || s`, big-endian.
/// * `pubkey` -- 64 bytes, `x || y`, big-endian, no `0x04` prefix.
pub fn verify_ecdsa(
    msg_hash: &[u8; 32],
    signature: &[u8],
    pubkey: &[u8],
) -> Result<bool, CryptoError> {
    if signature.len() != 64 {
        return Err(CryptoError::InvalidLength {
            expected: 64,
            got: signature.len(),
        });
    }
    if pubkey.len() != 64 {
        return Err(CryptoError::InvalidLength {
            expected: 64,
            got: pubkey.len(),
        });
    }
    // Safety: all three pointers reference buffers validated above.
    let valid = unsafe { c_verify_ecdsa(msg_hash.as_ptr(), signature.as_ptr(), pubkey.as_ptr()) };
    Ok(valid == 1)
}

/// Recover the uncompressed public key that produced `signature`.
///
/// `recovery_id` is the parity bit of `R.y` (Ethereum's `v - 27`, reduced to
/// 0/1). Returns the 64-byte `x || y` public key on success.
pub fn recover_pubkey(
    msg_hash: &[u8; 32],
    signature: &[u8],
    recovery_id: u8,
) -> Result<[u8; 64], CryptoError> {
    if signature.len() != 64 {
        return Err(CryptoError::InvalidLength {
            expected: 64,
            got: signature.len(),
        });
    }
    if recovery_id > 1 {
        return Err(CryptoError::InvalidSignature);
    }
    let mut pubkey = [0u8; 64];
    // Safety: inputs validated above; `pubkey` is a correctly sized output.
    let ok = unsafe {
        c_recover_pubkey(
            msg_hash.as_ptr(),
            signature.as_ptr(),
            recovery_id,
            pubkey.as_mut_ptr(),
        )
    };
    if ok == 1 {
        Ok(pubkey)
    } else {
        Err(CryptoError::InvalidSignature)
    }
}

/// Recover the sender address from an Ethereum-style signature.
///
/// `recovery_id` accepts Ethereum's `v` form (27/28) as well as a normalised
/// 0/1 parity bit.
pub fn recover_sender(
    msg_hash: &[u8; 32],
    signature: &[u8],
    recovery_id: u8,
) -> Result<[u8; 20], CryptoError> {
    let id = match recovery_id {
        27 => 0,
        28 => 1,
        0 | 1 => recovery_id,
        _ => return Err(CryptoError::InvalidSignature),
    };
    let pubkey = recover_pubkey(msg_hash, signature, id)?;
    pubkey_to_address(&pubkey)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hex(s: &str) -> Vec<u8> {
        let clean: String = s.chars().filter(|c| c.is_ascii_hexdigit()).collect();
        (0..clean.len() / 2)
            .map(|i| u8::from_str_radix(&clean[i * 2..i * 2 + 2], 16).unwrap())
            .collect()
    }

    fn h32(s: &str) -> [u8; 32] {
        hex(s).try_into().unwrap()
    }

    #[test]
    fn keccak256_matches_published_vectors() {
        assert_eq!(
            keccak256(b""),
            h32("c5d2460186f7233c927e7db2dcc703c0e500b653ca82273b7bfad8045d85a470")
        );
        assert_eq!(
            keccak256(b"abc"),
            h32("4e03657aea45a94fc7d47ba826c8d667c0d1e6e33a64a036ec44f58fa12d6c45")
        );
    }

    #[test]
    fn sha3_256_differs_from_keccak256() {
        assert_eq!(
            sha3_256(b"abc"),
            h32("3a985da74fe225b2045c172d6bd390bd855f086e3e9d525b46bfe24511431532")
        );
        assert_ne!(sha3_256(b"abc"), keccak256(b"abc"));
    }

    #[test]
    fn generator_point_address_is_canonical() {
        let g = hex(
            "79be667ef9dcbbac55a06295ce870b07029bfcdb2dce28d959f2815b16f81798\
             483ada7726a3c4655da4fbfc0e1108a8fd17b448a68554199c47d08ffb10d4b8",
        );
        let addr = pubkey_to_address(&g).unwrap();
        assert_eq!(
            addr.to_vec(),
            hex("7e5f4552091a69125d5dfcb7b8c2659029395bdf")
        );
    }

    #[test]
    fn verifies_and_recovers_real_signature() {
        let z = h32("af2bdbe1aa9b6ec1e2ade1d694f41fc71a831d0268e9891562113d8a62add1bf");
        let pubkey = hex(
            "f028892bad7ed57d2fb57bf33081d5cfcf6f9ed3d3d7f159c2e2fff579dc341a\
             07cf33da18bd734c600b96a72bbc4749d5141c90ec8ac328ae52ddfe2e505bdb",
        );
        let sig = hex(
            "749acf843fbbd20be57bd2aca04e20536e3c728fd98c26cd42201d4b2df0057e\
             8059a38dac5c0a67409f7e8e214e4fbbfaaee6cbe084b62d607ba08d2039a06b",
        );

        assert!(verify_ecdsa(&z, &sig, &pubkey).unwrap());
        assert_eq!(recover_pubkey(&z, &sig, 0).unwrap().to_vec(), pubkey);

        // Tampering with the digest must be rejected.
        let mut bad = z;
        bad[0] ^= 1;
        assert!(!verify_ecdsa(&bad, &sig, &pubkey).unwrap());
    }

    #[test]
    fn rejects_malformed_inputs() {
        let z = [0u8; 32];
        assert_eq!(
            verify_ecdsa(&z, &[0u8; 63], &[0u8; 64]),
            Err(CryptoError::InvalidLength {
                expected: 64,
                got: 63
            })
        );
        assert_eq!(
            recover_pubkey(&z, &[0u8; 64], 7),
            Err(CryptoError::InvalidSignature)
        );
        assert!(matches!(
            pubkey_to_address(&[0u8; 10]),
            Err(CryptoError::InvalidLength { .. })
        ));
    }
}
