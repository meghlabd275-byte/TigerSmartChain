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

    // Merkle Patricia Trie. The handle is opaque on the C++ side, so no layout
    // is assumed across the boundary.
    #[link_name = "tiger_trie_new"]
    fn c_trie_new() -> *mut core::ffi::c_void;
    #[link_name = "tiger_trie_free"]
    fn c_trie_free(trie: *mut core::ffi::c_void);
    #[link_name = "tiger_trie_insert"]
    fn c_trie_insert(
        trie: *mut core::ffi::c_void,
        key: *const u8,
        key_len: usize,
        value: *const u8,
        value_len: usize,
    );
    #[link_name = "tiger_trie_root"]
    fn c_trie_root(trie: *mut core::ffi::c_void, out: *mut u8);
    #[link_name = "tiger_trie_root_encoding"]
    fn c_trie_root_encoding(trie: *mut core::ffi::c_void, out: *mut u8, out_cap: usize) -> usize;
    #[link_name = "tiger_trie_len"]
    fn c_trie_len(trie: *mut core::ffi::c_void) -> usize;
    #[link_name = "tiger_trie_get"]
    fn c_trie_get(
        trie: *mut core::ffi::c_void,
        key: *const u8,
        key_len: usize,
        out: *mut u8,
        out_cap: usize,
    ) -> usize;
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

/// A Merkle Patricia Trie, as specified by the Ethereum Yellow Paper
/// Appendix D.
///
/// This wraps the C++ implementation in `crypto_cpp/trie.cpp`. That code is
/// the same one used on the consensus hot path, so a state root computed here
/// is byte-identical to the one the chain commits to.
///
/// The C++ object is heap-allocated and freed when this value is dropped, so
/// there is no way to leak or double-free it from Rust. The type is not `Sync`
/// or `Send`: a trie handle is single-threaded by design, and sharing one
/// across threads would race on the underlying `std::map`.
pub struct Trie {
    handle: *mut core::ffi::c_void,
}

impl Trie {
    /// Create an empty trie.
    pub fn new() -> Self {
        // Safety: the constructor takes no arguments and returns an owned
        // handle (or null, which every other call tolerates).
        let handle = unsafe { c_trie_new() };
        Self { handle }
    }

    /// Insert or overwrite `value` at `key`.
    ///
    /// Keys are used as given. The Ethereum state trie is keyed by
    /// `keccak256(address)`; callers hash the key themselves, exactly as the
    /// reference implementations do.
    pub fn insert(&mut self, key: &[u8], value: &[u8]) {
        // Safety: `handle` is owned by this value and the slices are valid for
        // their stated lengths.
        unsafe {
            c_trie_insert(
                self.handle,
                key.as_ptr(),
                key.len(),
                value.as_ptr(),
                value.len(),
            );
        }
    }

    /// Number of stored entries.
    pub fn len(&self) -> usize {
        // Safety: `handle` is owned by this value.
        unsafe { c_trie_len(self.handle) }
    }

    /// Whether the trie holds no entries.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// The keccak256 hash of the RLP-encoded root node.
    ///
    /// The empty trie hashes to
    /// `56e81f171bcc55a6ff8345e692c0f86e5b48e01b996cadc001622fb5e363b421`.
    pub fn root_hash(&self) -> [u8; 32] {
        let mut out = [0u8; 32];
        // Safety: `out` is a distinct 32-byte buffer.
        unsafe { c_trie_root(self.handle, out.as_mut_ptr()) };
        out
    }

    /// The RLP encoding of the root node. The empty trie encodes to `0x80`.
    pub fn root_encoding(&self) -> Vec<u8> {
        // Safety: a null output pointer with capacity 0 asks the C++ side for
        // the required length without writing anything.
        let len = unsafe { c_trie_root_encoding(self.handle, core::ptr::null_mut(), 0) };
        if len == 0 {
            return Vec::new();
        }
        let mut out = vec![0u8; len];
        let written =
            unsafe { c_trie_root_encoding(self.handle, out.as_mut_ptr(), out.len()) };
        debug_assert_eq!(written, len);
        out
    }

    /// Look up `key`, returning `None` when it is absent.
    pub fn get(&self, key: &[u8]) -> Option<Vec<u8>> {
        // Safety: a null output pointer with capacity 0 asks the C++ side for
        // the required length without writing anything.
        let len = unsafe { c_trie_get(self.handle, key.as_ptr(), key.len(), core::ptr::null_mut(), 0) };
        if len == usize::MAX {
            return None;
        }
        let mut out = vec![0u8; len];
        let written = unsafe {
            c_trie_get(
                self.handle,
                key.as_ptr(),
                key.len(),
                out.as_mut_ptr(),
                out.len(),
            )
        };
        debug_assert_eq!(written, len);
        Some(out)
    }
}

impl Default for Trie {
    fn default() -> Self {
        Self::new()
    }
}

impl Drop for Trie {
    fn drop(&mut self) {
        // Safety: `handle` came from `c_trie_new` and is freed exactly once.
        unsafe { c_trie_free(self.handle) };
    }
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

    // ---- Merkle Patricia Trie ----

    fn root_hex(trie: &Trie) -> String {
        trie.root_hash().iter().map(|b| format!("{b:02x}")).collect()
    }

    #[test]
    fn empty_trie_has_the_canonical_root() {
        let trie = Trie::new();
        assert!(trie.is_empty());
        assert_eq!(trie.len(), 0);
        assert_eq!(
            root_hex(&trie),
            "56e81f171bcc55a6ff8345e692c0f86e5b48e01b996cadc001622fb5e363b421"
        );
        assert_eq!(trie.root_encoding(), vec![0x80]);
    }

    /// The canonical example from the Ethereum wiki's trie page, cross-checked
    /// against ethereum/py-trie.
    #[test]
    fn wiki_example_matches_the_reference_root() {
        let mut trie = Trie::new();
        trie.insert(b"do", b"verb");
        trie.insert(b"dog", b"puppy");
        trie.insert(b"doge", b"coin");
        trie.insert(b"horse", b"stallion");
        assert_eq!(
            root_hex(&trie),
            "5991bb8c6514148a29db676a14ac506cd2cd5775ace63c30a4fe457715e9ac84"
        );
    }

    /// 256 keys exercising branch and extension splits at depth, cross-checked
    /// against ethereum/py-trie.
    #[test]
    fn large_keyset_matches_the_reference_root() {
        let mut trie = Trie::new();
        for i in 0..256u32 {
            let mut key = [0u8; 32];
            let mut value = [0u8; 32];
            key[28..].copy_from_slice(&i.to_be_bytes());
            value[28..].copy_from_slice(&(i * 7 + 1).to_be_bytes());
            trie.insert(&key, &value);
        }
        assert_eq!(
            root_hex(&trie),
            "5643a50bb43d87e2552644da07ccf5f8f63b657eb244f7a94f67cfde62771fb3"
        );
    }

    #[test]
    fn insert_and_get_round_trip() {
        let mut trie = Trie::new();
        trie.insert(b"do", b"verb");
        trie.insert(b"dog", b"puppy");
        trie.insert(b"doge", b"coin");
        trie.insert(b"horse", b"stallion");

        assert_eq!(trie.len(), 4);
        assert_eq!(trie.get(b"do").as_deref(), Some(&b"verb"[..]));
        assert_eq!(trie.get(b"dog").as_deref(), Some(&b"puppy"[..]));
        assert_eq!(trie.get(b"doge").as_deref(), Some(&b"coin"[..]));
        assert_eq!(trie.get(b"horse").as_deref(), Some(&b"stallion"[..]));
        assert_eq!(trie.get(b"d"), None);
        assert_eq!(trie.get(b"zebra"), None);
    }

    #[test]
    fn overwrite_replaces_the_value_and_the_root() {
        let mut trie = Trie::new();
        trie.insert(b"dog", b"puppy");
        let before = root_hex(&trie);
        trie.insert(b"dog", b"cat");
        assert_eq!(trie.get(b"dog").as_deref(), Some(&b"cat"[..]));
        assert_eq!(trie.len(), 1);
        assert_ne!(root_hex(&trie), before);
    }

    /// A key that is a strict prefix of another stores its value in the
    /// branch's seventeenth slot. This is the case that separates a correct
    /// branch encoding from one that drops the value.
    #[test]
    fn a_prefix_key_keeps_its_value() {
        let mut trie = Trie::new();
        trie.insert(b"a", b"1");
        trie.insert(b"ab", b"2");
        assert_eq!(trie.get(b"a").as_deref(), Some(&b"1"[..]));
        assert_eq!(trie.get(b"ab").as_deref(), Some(&b"2"[..]));

        let mut longer_only = Trie::new();
        longer_only.insert(b"ab", b"2");
        assert_ne!(root_hex(&trie), root_hex(&longer_only));
    }

    /// An empty value is legal and must still create the key.
    #[test]
    fn an_empty_value_still_contributes_to_the_root() {
        let mut with_empty = Trie::new();
        with_empty.insert(b"b1", b"");
        with_empty.insert(b"b2", &[0x14u8; 48]);

        let mut without = Trie::new();
        without.insert(b"b2", &[0x14u8; 48]);

        assert_ne!(root_hex(&with_empty), root_hex(&without));
        assert_eq!(with_empty.get(b"b1"), Some(Vec::new()));
        assert_eq!(with_empty.len(), 2);
    }

    #[test]
    fn the_root_does_not_depend_on_insertion_order() {
        let mut forward = Trie::new();
        forward.insert(b"do", b"verb");
        forward.insert(b"dog", b"puppy");
        forward.insert(b"doge", b"coin");
        forward.insert(b"horse", b"stallion");

        let mut reverse = Trie::new();
        reverse.insert(b"horse", b"stallion");
        reverse.insert(b"doge", b"coin");
        reverse.insert(b"dog", b"puppy");
        reverse.insert(b"do", b"verb");

        assert_eq!(root_hex(&forward), root_hex(&reverse));
    }

    /// The Ethereum state trie is keyed by the keccak256 of the address, so a
    /// realistic keyset exercises the hashing and the trie together.
    #[test]
    fn a_keccak_keyed_state_trie_is_deterministic() {
        let mut trie = Trie::new();
        for i in 0..500u32 {
            let mut addr = [0u8; 20];
            addr[16..].copy_from_slice(&i.to_be_bytes());
            let key = keccak256(&addr);
            let mut account = Vec::new();
            account.extend_from_slice(&(i as u64).to_be_bytes());
            account.extend_from_slice(&keccak256(&addr));
            trie.insert(&key, &account);
        }

        let first = root_hex(&trie);

        let mut again = Trie::new();
        for i in (0..500u32).rev() {
            let mut addr = [0u8; 20];
            addr[16..].copy_from_slice(&i.to_be_bytes());
            let key = keccak256(&addr);
            let mut account = Vec::new();
            account.extend_from_slice(&(i as u64).to_be_bytes());
            account.extend_from_slice(&keccak256(&addr));
            again.insert(&key, &account);
        }

        assert_eq!(first, root_hex(&again));
        assert_eq!(trie.len(), 500);

        // Every key must read back.
        for i in 0..500u32 {
            let mut addr = [0u8; 20];
            addr[16..].copy_from_slice(&i.to_be_bytes());
            assert!(trie.get(&keccak256(&addr)).is_some());
        }
    }

    #[test]
    fn root_encoding_is_the_hash_preimage() {
        let mut trie = Trie::new();
        trie.insert(b"do", b"verb");
        trie.insert(b"dog", b"puppy");
        assert_eq!(keccak256(&trie.root_encoding()), trie.root_hash());
    }

    /// A large, deeply shared prefix forces an extension chain; the leaves
    /// behind it must still be reachable.
    #[test]
    fn deep_extension_chain_is_traversable() {
        let mut trie = Trie::new();
        let shared = [0xaau8; 40];
        for i in 0..64u8 {
            let mut key = shared.to_vec();
            key.push(i);
            trie.insert(&key, &[i, i, i]);
        }
        for i in 0..64u8 {
            let mut key = shared.to_vec();
            key.push(i);
            assert_eq!(trie.get(&key), Some(vec![i, i, i]));
        }
        // A key that follows the shared prefix but diverges at the last nibble
        // must be absent.
        let mut absent = shared.to_vec();
        absent.push(0xff);
        assert_eq!(trie.get(&absent), None);
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
