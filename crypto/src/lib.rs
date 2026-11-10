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
    #[link_name = "sign_ecdsa"]
    fn c_sign_ecdsa(
        msg_hash: *const u8,
        privkey: *const u8,
        sig_out: *mut u8,
        recovery_id_out: *mut u8,
    ) -> i32;
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

/// A recoverable secp256k1 signature.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Signature {
    /// The 64-byte `r || s` pair, low-s normalised per EIP-2.
    pub bytes: [u8; 64],
    /// Recovery id: the parity of `R.y` (0 or 1).
    pub recovery_id: u8,
}

impl Signature {
    /// The `r` component.
    pub fn r(&self) -> [u8; 32] {
        let mut out = [0u8; 32];
        out.copy_from_slice(&self.bytes[..32]);
        out
    }

    /// The `s` component.
    pub fn s(&self) -> [u8; 32] {
        let mut out = [0u8; 32];
        out.copy_from_slice(&self.bytes[32..]);
        out
    }

    /// The EIP-155 `v` value for a given chain id: `recovery_id + 35 + 2 * chain_id`.
    ///
    /// Passing `None` produces the pre-EIP-155 form (`recovery_id + 27`).
    pub fn v(&self, chain_id: Option<u64>) -> u64 {
        match chain_id {
            Some(id) => self.recovery_id as u64 + 35 + 2 * id,
            None => self.recovery_id as u64 + 27,
        }
    }
}

/// Sign a 32-byte digest with a 32-byte private key.
///
/// Nonces are derived deterministically per RFC 6979, so signing is
/// reproducible and never depends on the quality of a random source. The
/// result is low-s normalised, which is required by EIP-2 and prevents
/// signature malleability.
pub fn sign(msg_hash: &[u8; 32], private_key: &[u8; 32]) -> Result<Signature, CryptoError> {
    let mut bytes = [0u8; 64];
    let mut recovery_id = 0u8;
    // Safety: both inputs are fixed-size arrays and both outputs are sized
    // for the writes the C++ side performs.
    let ok = unsafe {
        c_sign_ecdsa(
            msg_hash.as_ptr(),
            private_key.as_ptr(),
            bytes.as_mut_ptr(),
            &mut recovery_id,
        )
    };
    if ok != 1 {
        // The only documented failure is a key outside [1, n-1].
        return Err(CryptoError::InvalidSignature);
    }
    Ok(Signature {
        bytes,
        recovery_id,
    })
}

/// Derive the 20-byte Ethereum address for a private key.
pub fn private_key_to_address(private_key: &[u8; 32]) -> Result<[u8; 20], CryptoError> {
    // A private key determines a public key, which we obtain by signing a
    // fixed digest and recovering the key from the signature. This avoids
    // exposing scalar multiplication separately over the C ABI.
    let probe = keccak256(b"TigerSmartChain public key derivation");
    let sig = sign(&probe, private_key)?;
    let pubkey = recover_pubkey(&probe, &sig.bytes, sig.recovery_id)?;
    pubkey_to_address(&pubkey)
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

    #[test]
    fn signing_matches_the_eip155_reference_vector() {
        // The canonical example from the EIP-155 specification.
        let mut key = [0u8; 32];
        key.fill(0x46);
        let mut hash = [0u8; 32];
        hash.copy_from_slice(
            &hex("daf5a779ae972f972197303d7b574746c7ef83eadac0f2791ad23db92e4c8e53"),
        );

        let sig = sign(&hash, &key).expect("signing must succeed");
        assert_eq!(
            to_hex(&sig.r()),
            "28ef61340bd939bc2195fe537567866003e1a15d3c71ff63e1590620aa636276"
        );
        assert_eq!(
            to_hex(&sig.s()),
            "67cbe9d8997f761aecb703304b3800ccf555c9f3dc64214b297fb1966a3b6d83"
        );
        // v = recovery_id + 35 + 2 * chain_id, with chain id 1.
        assert_eq!(sig.v(Some(1)), 37);

        // The signature must verify and recover to the published sender.
        let pubkey = recover_pubkey(&hash, &sig.bytes, sig.recovery_id).unwrap();
        assert!(verify_ecdsa(&hash, &sig.bytes, &pubkey).unwrap());
        assert_eq!(
            to_hex(&pubkey_to_address(&pubkey).unwrap()),
            "9d8a62f656a8d1615c1294fd71e9cfb3e4855a4f"
        );
        assert_eq!(
            to_hex(&recover_sender(&hash, &sig.bytes, sig.recovery_id as u8).unwrap()),
            "9d8a62f656a8d1615c1294fd71e9cfb3e4855a4f"
        );
    }

    #[test]
    fn signing_is_deterministic() {
        let key = [0x11u8; 32];
        let hash = keccak256(b"determinism");
        let a = sign(&hash, &key).unwrap();
        let b = sign(&hash, &key).unwrap();
        assert_eq!(a, b, "RFC 6979 nonces must make signing reproducible");
    }

    #[test]
    fn signature_is_low_s() {
        // EIP-2 requires s <= n/2, so the top byte of s must stay below 0x80.
        for seed in 0..16u8 {
            let key = [seed.wrapping_add(1); 32];
            let hash = keccak256(&[seed]);
            let sig = sign(&hash, &key).unwrap();
            assert!(
                sig.s()[0] < 0x80,
                "s must be low-s normalised for seed {seed}"
            );
        }
    }

    #[test]
    fn out_of_range_private_keys_are_rejected() {
        let hash = keccak256(b"x");
        assert!(sign(&hash, &[0u8; 32]).is_err(), "zero key must be rejected");
        let mut above_n = [0xffu8; 32];
        above_n[0] = 0xff;
        assert!(
            sign(&hash, &above_n).is_err(),
            "key above the group order must be rejected"
        );
    }

    #[test]
    fn private_key_to_address_is_stable() {
        let key = [0x46u8; 32];
        let addr = private_key_to_address(&key).unwrap();
        assert_eq!(to_hex(&addr), "9d8a62f656a8d1615c1294fd71e9cfb3e4855a4f");
    }

    #[test]
    fn v_encodes_chain_id_correctly() {
        let key = [0x46u8; 32];
        let hash = keccak256(b"v test");
        let sig = sign(&hash, &key).unwrap();
        assert_eq!(sig.v(None), sig.recovery_id as u64 + 27);
        assert_eq!(sig.v(Some(1)), sig.recovery_id as u64 + 37);
        // TigerSmartChain's chain id.
        assert_eq!(sig.v(Some(6666)), sig.recovery_id as u64 + 35 + 13332);
    }

    fn to_hex(bytes: &[u8]) -> String {
        bytes.iter().map(|b| format!("{b:02x}")).collect()
    }

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
