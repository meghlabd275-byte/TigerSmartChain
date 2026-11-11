//! TigerScan Quantum-Resistant Cryptography Module
//!
//! Post-quantum cryptographic primitives backed by reference implementations of
//! the NIST PQC standards:
//!
//! - **Signatures** — ML-DSA (FIPS 204) via `pqc_dilithium`. The compiled-in
//!   parameter set is ML-DSA-65 (Dilithium-3).
//! - **Key encapsulation** — ML-KEM (FIPS 203) via `pqc_kyber`, Kyber-768.
//! - **Message authentication** — HMAC-SHA-512 (RFC 2104), verified in constant
//!   time.
//!
//! # Security properties
//!
//! The algorithms have deliberately different trust models:
//!
//! * ML-DSA and ML-KEM are **asymmetric**. A signature or ciphertext can be
//!   checked against a public key, so a verifier never needs the secret.
//! * [`HashSignature`] is **symmetric**. The same secret both signs and
//!   verifies, so it is a message-authentication code, not a public-key
//!   signature. It authenticates messages between parties that already share a
//!   key.
//!
//! Every verifier in this module rejects a forged, tampered, or wrong-key
//! input.
//!
//! # Legacy lattice types
//!
//! [`Polynomial`], [`PolynomialMatrix`], and [`DilithiumParams`] remain as a
//! lattice arithmetic utility. They are **not** part of any signing path; the
//! parameters are those of ML-DSA-65.

pub mod types;

pub use types::*;

use std::collections::HashMap;

use hmac::{Hmac, Mac};
use pqc_dilithium::{
    verify as dilithium_verify, Keypair as DilithiumKeypair, PUBLICKEYBYTES as DILITHIUM_PK_BYTES,
    SECRETKEYBYTES as DILITHIUM_SK_BYTES, SIGNBYTES as DILITHIUM_SIG_BYTES,
};
use pqc_kyber::{
    decapsulate as kyber_decapsulate, encapsulate as kyber_encapsulate, keypair as kyber_keypair,
    KYBER_CIPHERTEXTBYTES, KYBER_PUBLICKEYBYTES, KYBER_SECRETKEYBYTES,
};
use rand::rngs::OsRng;
use rayon::prelude::*;
use serde::{Deserialize, Serialize};
use sha2::Sha512;
use sha3::{Digest, Keccak256, Sha3_256};
use subtle::ConstantTimeEq;
use thiserror::Error;

/// HMAC-SHA-512, keyed with the caller's secret.
type HmacSha512 = Hmac<Sha512>;

#[derive(Error, Debug)]
pub enum QuantumError {
    #[error("Key generation failed")]
    KeyGenerationFailed,
    #[error("Signing failed")]
    SigningFailed,
    #[error("Verification failed")]
    VerificationFailed,
    #[error("Invalid key format")]
    InvalidKey,
    #[error("Invalid signature format")]
    InvalidSignature,
    #[error("Random number generation failed")]
    RandomGenerationFailed,
}

/// ML-DSA-65 (Dilithium-3) parameter set.
///
/// These are the FIPS 204 parameters for the default `pqc_dilithium` mode, kept
/// for callers that inspect the lattice dimensions. Signing goes through
/// `pqc_dilithium`, not through this struct.
pub struct DilithiumParams {
    pub k: usize,  // Number of polynomials in A
    pub l: usize,  // Number of polynomials in s
    pub eta: i32,  // Bound on coefficients of s
    pub beta: i32, // Bound on challenge
    pub gamma1: i32,
    pub gamma2: i32,
    pub omega: usize,
}

impl Default for DilithiumParams {
    fn default() -> Self {
        // ML-DSA-65 / Dilithium-3.
        Self {
            k: 6,
            l: 5,
            eta: 4,
            beta: 196,
            gamma1: 1 << 19,
            gamma2: (q() - 1) / 32,
            omega: 55,
        }
    }
}

fn q() -> i32 { 8380417 }

/// Polynomial ring R_q = Z_q[X]/(X^n + 1) with n = 256
const N: usize = 256;

/// Polynomial in Z_q[X]/(X^n + 1)
#[derive(Clone, Debug)]
pub struct Polynomial {
    coeffs: [i32; N],
}

impl Polynomial {
    /// Create new zero polynomial
    pub fn zero() -> Self {
        Self { coeffs: [0; N] }
    }

    /// Create polynomial from coefficients
    pub fn from_coeffs(coeffs: [i32; N]) -> Self {
        Self { coeffs }
    }

    /// Get coefficient at index
    pub fn get(&self, i: usize) -> i32 {
        self.coeffs[i]
    }

    /// Set coefficient at index
    pub fn set(&mut self, i: usize, v: i32) {
        self.coeffs[i] = v;
    }

    /// Add two polynomials
    pub fn add(&self, other: &Polynomial) -> Polynomial {
        let mut result = Self::zero();
        for i in 0..N {
            result.coeffs[i] = (self.coeffs[i] + other.coeffs[i]).rem_euclid(q());
        }
        result
    }

    /// Multiply by scalar
    pub fn scalar_mul(&self, scalar: i32) -> Polynomial {
        let mut result = Self::zero();
        for i in 0..N {
            result.coeffs[i] = (self.coeffs[i] * scalar).rem_euclid(q());
        }
        result
    }

    /// Negate polynomial
    pub fn negate(&self) -> Polynomial {
        let mut result = Self::zero();
        for i in 0..N {
            result.coeffs[i] = (-self.coeffs[i]).rem_euclid(q());
        }
        result
    }

    /// Centered mod (reduce to [-q/2, q/2])
    pub fn center(&self) -> Polynomial {
        let mut result = Self::zero();
        for i in 0..N {
            let mut v = self.coeffs[i].rem_euclid(q());
            if v > q() / 2 {
                v -= q();
            }
            result.coeffs[i] = v;
        }
        result
    }

    /// Sample a polynomial from `seed` and a domain-separation `nonce`.
    ///
    /// Coefficients are drawn uniformly from `[-eta, eta]` by rejection
    /// sampling over SHA3-256 output, mirroring the secret sampling of the
    /// reference lattice schemes.
    pub fn sample(&mut self, seed: &[u8], nonce: u8) {
        let eta = 4i32;
        let modulus = (2 * eta + 1) as u8;

        let mut hasher = Sha3_256::new();
        hasher.update(seed);
        hasher.update([nonce]);
        let mut block = hasher.finalize();

        // Largest multiple of `modulus` that fits in a byte; values at or above
        // it are rejected so the accepted range divides evenly. Computed in u16
        // because 256 does not fit in a u8.
        let limit = (256u16 - (256u16 % modulus as u16)) as u8;

        let mut i = 0;
        let mut j = 0;
        let mut counter: u8 = 0;
        while i < N {
            if j >= block.len() {
                // Re-squeeze with an incrementing counter so no byte stream is
                // ever reused.
                counter = counter.wrapping_add(1);
                let mut h = Sha3_256::new();
                h.update(block);
                h.update([counter]);
                block = h.finalize();
                j = 0;
            }

            let b = block[j];
            j += 1;
            if b >= limit {
                continue;
            }

            self.coeffs[i] = (b % modulus) as i32 - eta;
            i += 1;
        }
    }

    /// Serialise the low byte of each coefficient.
    pub fn to_bytes(&self) -> [u8; 32] {
        let mut result = [0u8; 32];
        for i in 0..32 {
            result[i] = self.coeffs[i].rem_euclid(256) as u8;
        }
        result
    }
}

/// Matrix of polynomials (for the A matrix in lattice schemes)
#[derive(Clone, Debug)]
pub struct PolynomialMatrix {
    rows: usize,
    cols: usize,
    data: Vec<Polynomial>,
}

impl PolynomialMatrix {
    /// Create matrix of zeros
    pub fn zeros(rows: usize, cols: usize) -> Self {
        let data = vec![Polynomial::zero(); rows * cols];
        Self { rows, cols, data }
    }

    /// Get element
    pub fn get(&self, i: usize, j: usize) -> &Polynomial {
        &self.data[i * self.cols + j]
    }

    /// Get mutable element
    pub fn get_mut(&mut self, i: usize, j: usize) -> &mut Polynomial {
        &mut self.data[i * self.cols + j]
    }

    /// Sample every entry from one seed, giving each entry a distinct nonce.
    pub fn sample(&mut self, seed: &[u8]) {
        for i in 0..self.rows {
            for j in 0..self.cols {
                let nonce = ((i * self.cols + j) % 256) as u8;
                self.data[i * self.cols + j].sample(seed, nonce);
            }
        }
    }
}

/// Post-quantum signature key pair (ML-DSA-65).
#[derive(Clone)]
pub struct DilithiumKeyPair {
    pub public_key: DilithiumPublicKey,
    pub secret_key: DilithiumSecretKey,
}

/// A signature public key: the only value a verifier needs.
#[derive(Clone, Debug)]
pub struct DilithiumPublicKey {
    pub bytes: Vec<u8>,
}

impl DilithiumPublicKey {
    /// Encoded public key length for the compiled-in parameter set.
    pub const LEN: usize = DILITHIUM_PK_BYTES;

    /// Build from raw bytes, rejecting the wrong length.
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, QuantumError> {
        if bytes.len() != DILITHIUM_PK_BYTES {
            return Err(QuantumError::InvalidKey);
        }
        Ok(Self { bytes: bytes.to_vec() })
    }

    /// Raw public key bytes.
    pub fn as_bytes(&self) -> &[u8] {
        &self.bytes
    }
}

/// A signature secret key. The `Debug` impl elides the key material.
#[derive(Clone)]
pub struct DilithiumSecretKey {
    pub bytes: Vec<u8>,
}

impl DilithiumSecretKey {
    /// Encoded secret key length for the compiled-in parameter set.
    pub const LEN: usize = DILITHIUM_SK_BYTES;

    /// Build from raw bytes, rejecting the wrong length.
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, QuantumError> {
        if bytes.len() != DILITHIUM_SK_BYTES {
            return Err(QuantumError::InvalidKey);
        }
        Ok(Self { bytes: bytes.to_vec() })
    }

    /// Raw secret key bytes. Handle with care.
    pub fn as_bytes(&self) -> &[u8] {
        &self.bytes
    }
}

impl std::fmt::Debug for DilithiumSecretKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "DilithiumSecretKey(<elided>)")
    }
}

/// An ML-DSA signature over a message.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DilithiumSignature {
    /// Encoded signature (SIGNBYTES long).
    pub bytes: Vec<u8>,
}

/// Quantum key pair for key encapsulation (ML-KEM / Kyber-768).
#[derive(Clone)]
pub struct QuantumKeyPair {
    pub public_key: Vec<u8>,
    pub secret_key: Vec<u8>,
}

impl std::fmt::Debug for QuantumKeyPair {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("QuantumKeyPair")
            .field("public_key_len", &self.public_key.len())
            .field("secret_key", &"<elided>")
            .finish()
    }
}

/// Key encapsulation result
#[derive(Clone, Debug)]
pub struct EncapsulationResult {
    pub ciphertext: Vec<u8>,
    pub shared_secret: Vec<u8>,
}

/// Main quantum engine.
///
/// Signing keys are cached by public key so [`QuantumEngine::sign`] can be
/// driven from the key pair it generated.
pub struct QuantumEngine {
    params: DilithiumParams,
    signing_keys: HashMap<Vec<u8>, DilithiumKeypair>,
}

impl QuantumEngine {
    /// Create new quantum engine
    pub fn new() -> Self {
        Self {
            params: DilithiumParams::default(),
            signing_keys: HashMap::new(),
        }
    }

    /// Create with custom parameters.
    ///
    /// The parameter set is fixed by the compiled-in `pqc_dilithium` mode, so
    /// only the default ML-DSA-65 dimensions are meaningful.
    pub fn with_params(params: DilithiumParams) -> Self {
        Self {
            params,
            signing_keys: HashMap::new(),
        }
    }

    /// The lattice dimensions in use.
    pub fn params(&self) -> &DilithiumParams {
        &self.params
    }

    /// Generate a real ML-DSA-65 key pair from the OS CSPRNG.
    pub fn generate_keypair(&mut self) -> Result<DilithiumKeyPair, QuantumError> {
        let keypair = DilithiumKeypair::generate();
        let public = keypair.public.to_vec();
        let secret = keypair.expose_secret().to_vec();

        let pair = DilithiumKeyPair {
            public_key: DilithiumPublicKey { bytes: public.clone() },
            secret_key: DilithiumSecretKey { bytes: secret },
        };

        self.signing_keys.insert(public, keypair);
        Ok(pair)
    }

    /// Sign a message with ML-DSA-65.
    ///
    /// The keypair must come from this engine's
    /// [`QuantumEngine::generate_keypair`]. An unknown public key is an error,
    /// not a silent no-op.
    pub fn sign(
        &self,
        message: &[u8],
        keypair: &DilithiumKeyPair,
    ) -> Result<DilithiumSignature, QuantumError> {
        let key = self
            .signing_keys
            .get(&keypair.public_key.bytes)
            .ok_or(QuantumError::InvalidKey)?;
        let signature = key.sign(message);
        Ok(DilithiumSignature { bytes: signature.to_vec() })
    }

    /// Verify an ML-DSA-65 signature.
    ///
    /// `Ok(true)` only for a signature valid for this message and public key.
    /// A forged, truncated, or wrong-key signature returns `Ok(false)`; a
    /// malformed length returns `Err`.
    pub fn verify(
        &self,
        message: &[u8],
        signature: &DilithiumSignature,
        public_key: &DilithiumPublicKey,
    ) -> Result<bool, QuantumError> {
        if signature.bytes.len() != DILITHIUM_SIG_BYTES {
            return Err(QuantumError::InvalidSignature);
        }
        if public_key.bytes.len() != DILITHIUM_PK_BYTES {
            return Err(QuantumError::InvalidKey);
        }
        Ok(dilithium_verify(&signature.bytes, message, &public_key.bytes).is_ok())
    }

    /// Generate a real ML-KEM (Kyber-768) key pair.
    pub fn generate_kem_keypair(&self) -> Result<QuantumKeyPair, QuantumError> {
        let mut rng = OsRng;
        let keys = kyber_keypair(&mut rng).map_err(|_| QuantumError::KeyGenerationFailed)?;
        Ok(QuantumKeyPair {
            public_key: keys.public.to_vec(),
            secret_key: keys.secret.to_vec(),
        })
    }

    /// Encapsulate a shared secret against a Kyber-768 public key.
    ///
    /// The ciphertext is sent to the key's owner; the shared secret stays local.
    pub fn encapsulate(&self, public_key: &[u8]) -> Result<EncapsulationResult, QuantumError> {
        if public_key.len() != KYBER_PUBLICKEYBYTES {
            return Err(QuantumError::InvalidKey);
        }
        let mut rng = OsRng;
        let (ciphertext, shared_secret) = kyber_encapsulate(public_key, &mut rng)
            .map_err(|_| QuantumError::KeyGenerationFailed)?;
        Ok(EncapsulationResult {
            ciphertext: ciphertext.to_vec(),
            shared_secret: shared_secret.to_vec(),
        })
    }

    /// Decapsulate a shared secret with the Kyber-768 secret key.
    ///
    /// Recovers exactly the secret [`QuantumEngine::encapsulate`] produced.
    pub fn decapsulate(
        &self,
        secret_key: &[u8],
        ciphertext: &[u8],
    ) -> Result<Vec<u8>, QuantumError> {
        if secret_key.len() != KYBER_SECRETKEYBYTES || ciphertext.len() != KYBER_CIPHERTEXTBYTES {
            return Err(QuantumError::InvalidKey);
        }
        let shared = kyber_decapsulate(ciphertext, secret_key)
            .map_err(|_| QuantumError::InvalidKey)?;
        Ok(shared.to_vec())
    }

    /// Derive an HMAC-SHA-512 tag over `message` keyed by `secret_key`.
    ///
    /// A symmetric message-authentication code: the verifier needs the same
    /// secret. See [`HashSignature`] for the object form.
    pub fn hash_sign(&self, message: &[u8], secret_key: &[u8]) -> Result<Vec<u8>, QuantumError> {
        if secret_key.is_empty() {
            return Err(QuantumError::InvalidKey);
        }
        let mut mac =
            HmacSha512::new_from_slice(secret_key).map_err(|_| QuantumError::InvalidKey)?;
        mac.update(message);
        Ok(mac.finalize().into_bytes().to_vec())
    }

    /// Verify an HMAC-SHA-512 tag in constant time.
    pub fn hash_verify(
        &self,
        message: &[u8],
        signature: &[u8],
        secret_key: &[u8],
    ) -> Result<bool, QuantumError> {
        if secret_key.is_empty() {
            return Err(QuantumError::InvalidKey);
        }
        if signature.len() != 64 {
            return Ok(false);
        }
        let mut mac =
            HmacSha512::new_from_slice(secret_key).map_err(|_| QuantumError::InvalidKey)?;
        mac.update(message);
        Ok(mac.verify_slice(signature).is_ok())
    }

    /// Keccak-256 digest, as used by the EVM.
    pub fn keccak256(&self, data: &[u8]) -> [u8; 32] {
        let mut hasher = Keccak256::new();
        hasher.update(data);
        hasher.finalize().into()
    }

    /// SHA3-256 digest (FIPS 202).
    pub fn sha3_256(&self, data: &[u8]) -> [u8; 32] {
        let mut hasher = Sha3_256::new();
        hasher.update(data);
        hasher.finalize().into()
    }

    /// Hash many inputs in parallel, preserving input order.
    pub fn hash_batch(&self, inputs: &[Vec<u8>]) -> Vec<[u8; 32]> {
        inputs.par_iter().map(|input| self.sha3_256(input)).collect()
    }
}

impl Default for QuantumEngine {
    fn default() -> Self {
        Self::new()
    }
}

/// A symmetric hash-based signature (HMAC-SHA-512).
///
/// This is a message-authentication code, not a public-key signature: the same
/// secret signs and verifies, so it authenticates messages only between parties
/// that already share the secret. Use [`QuantumEngine::generate_keypair`] and
/// [`QuantumEngine::sign`] when a verifier must hold only a public key.
pub struct HashSignature {
    pub message_hash: [u8; 32],
    pub signature: Vec<u8>,
}

impl HashSignature {
    /// Sign `message` with a shared `secret`.
    ///
    /// `secret` must be at least 32 bytes; short keys are rejected rather than
    /// silently accepted.
    pub fn new(message: &[u8], secret: &[u8]) -> Result<Self, QuantumError> {
        if secret.len() < 32 {
            return Err(QuantumError::InvalidKey);
        }

        let mut mac =
            HmacSha512::new_from_slice(secret).map_err(|_| QuantumError::InvalidKey)?;
        mac.update(message);
        let signature = mac.finalize().into_bytes().to_vec();

        let mut message_hash = [0u8; 32];
        message_hash.copy_from_slice(&Sha3_256::digest(message));

        Ok(Self { message_hash, signature })
    }

    /// Verify against the same shared `secret`.
    pub fn verify(&self, message: &[u8], secret: &[u8]) -> bool {
        if secret.len() < 32 {
            return false;
        }

        // The embedded digest must match the message before the MAC is checked.
        let computed: [u8; 32] = Sha3_256::digest(message).into();
        if computed[..].ct_eq(&self.message_hash[..]).unwrap_u8() != 1 {
            return false;
        }

        let Ok(mut mac) = HmacSha512::new_from_slice(secret) else {
            return false;
        };
        mac.update(message);
        mac.verify_slice(&self.signature).is_ok()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_dilithium_keygen_produces_distinct_keys() {
        let mut engine = QuantumEngine::new();
        let a = engine.generate_keypair().unwrap();
        let b = engine.generate_keypair().unwrap();

        assert_eq!(a.public_key.as_bytes().len(), DilithiumPublicKey::LEN);
        assert_eq!(a.secret_key.as_bytes().len(), DilithiumSecretKey::LEN);
        // Two keygen calls must not collide.
        assert_ne!(a.public_key.as_bytes(), b.public_key.as_bytes());
    }

    #[test]
    fn test_dilithium_sign_verify_roundtrip() {
        let mut engine = QuantumEngine::new();
        let keypair = engine.generate_keypair().unwrap();

        let message = b"Hello, TigerScan!";
        let signature = engine.sign(message, &keypair).unwrap();
        assert_eq!(signature.bytes.len(), DILITHIUM_SIG_BYTES);

        assert!(engine.verify(message, &signature, &keypair.public_key).unwrap());
    }

    #[test]
    fn test_dilithium_rejects_tampered_signature() {
        let mut engine = QuantumEngine::new();
        let keypair = engine.generate_keypair().unwrap();
        let message = b"transfer 100 TGR";

        let mut signature = engine.sign(message, &keypair).unwrap();
        signature.bytes[0] ^= 0x01;

        assert!(!engine.verify(message, &signature, &keypair.public_key).unwrap());
    }

    #[test]
    fn test_dilithium_rejects_wrong_message() {
        let mut engine = QuantumEngine::new();
        let keypair = engine.generate_keypair().unwrap();

        let signature = engine.sign(b"original", &keypair).unwrap();
        assert!(!engine.verify(b"tampered", &signature, &keypair.public_key).unwrap());
    }

    #[test]
    fn test_dilithium_rejects_wrong_public_key() {
        let mut engine = QuantumEngine::new();
        let signer = engine.generate_keypair().unwrap();
        let other = engine.generate_keypair().unwrap();

        let signature = engine.sign(b"msg", &signer).unwrap();
        assert!(!engine.verify(b"msg", &signature, &other.public_key).unwrap());
    }

    #[test]
    fn test_dilithium_rejects_malformed_lengths() {
        let mut engine = QuantumEngine::new();
        let keypair = engine.generate_keypair().unwrap();

        let short = DilithiumSignature { bytes: vec![0u8; 10] };
        assert!(engine.verify(b"msg", &short, &keypair.public_key).is_err());

        let bad_key = DilithiumPublicKey { bytes: vec![0u8; 10] };
        let good_sig = engine.sign(b"msg", &keypair).unwrap();
        assert!(engine.verify(b"msg", &good_sig, &bad_key).is_err());
    }

    #[test]
    fn test_kem_roundtrip_recovers_same_secret() {
        let engine = QuantumEngine::new();
        let keypair = engine.generate_kem_keypair().unwrap();

        assert_eq!(keypair.public_key.len(), KYBER_PUBLICKEYBYTES);
        assert_eq!(keypair.secret_key.len(), KYBER_SECRETKEYBYTES);

        let encapsulated = engine.encapsulate(&keypair.public_key).unwrap();
        assert_eq!(encapsulated.ciphertext.len(), KYBER_CIPHERTEXTBYTES);
        assert_eq!(encapsulated.shared_secret.len(), 32);

        let recovered = engine
            .decapsulate(&keypair.secret_key, &encapsulated.ciphertext)
            .unwrap();
        assert_eq!(recovered, encapsulated.shared_secret);
    }

    #[test]
    fn test_kem_rejects_wrong_key_length() {
        let engine = QuantumEngine::new();
        assert!(engine.encapsulate(&[0u8; 8]).is_err());
        assert!(engine.decapsulate(&[0u8; 8], &[0u8; 8]).is_err());
    }

    #[test]
    fn test_hash_signature_roundtrip() {
        let secret = b"a-shared-secret-that-is-long-enough!";
        let message = b"Test message for signature";

        let signature = HashSignature::new(message, secret).unwrap();
        assert!(signature.verify(message, secret));
    }

    #[test]
    fn test_hash_signature_rejects_wrong_secret_and_message() {
        let secret = b"a-shared-secret-that-is-long-enough!";
        let other = b"a-different-secret-long-enough-too!!";
        let signature = HashSignature::new(b"payload", secret).unwrap();

        assert!(!signature.verify(b"payload", other), "wrong secret must fail");
        assert!(!signature.verify(b"other payload", secret), "wrong message must fail");
    }

    #[test]
    fn test_hash_signature_rejects_short_secret() {
        assert!(HashSignature::new(b"msg", b"too-short").is_err());
        let signature = HashSignature::new(b"msg", b"a-shared-secret-that-is-long-enough!").unwrap();
        assert!(!signature.verify(b"msg", b"too-short"));
    }

    #[test]
    fn test_hmac_helpers_match() {
        let engine = QuantumEngine::new();
        let secret = b"a-shared-secret-that-is-long-enough!";
        let tag = engine.hash_sign(b"body", secret).unwrap();

        assert!(engine.hash_verify(b"body", &tag, secret).unwrap());
        assert!(!engine.hash_verify(b"body2", &tag, secret).unwrap());
        // A truncated tag is rejected rather than compared loosely.
        assert!(!engine.hash_verify(b"body", &tag[..32], secret).unwrap());
    }

    #[test]
    fn test_keccak_matches_known_vector() {
        let engine = QuantumEngine::new();
        // keccak256("") — the canonical empty-string digest.
        let digest = engine.keccak256(b"");
        assert_eq!(
            hex::encode(digest),
            "c5d2460186f7233c927e7db2dcc703c0e500b653ca82273b7bfad8045d85a470"
        );
    }

    #[test]
    fn test_sha3_matches_known_vector() {
        let engine = QuantumEngine::new();
        // SHA3-256("abc")
        let digest = engine.sha3_256(b"abc");
        assert_eq!(
            hex::encode(digest),
            "3a985da74fe225b2045c172d6bd390bd855f086e3e9d525b46bfe24511431532"
        );
    }

    #[test]
    fn test_hash_batch_preserves_order() {
        let engine = QuantumEngine::new();
        let inputs = vec![b"a".to_vec(), b"b".to_vec(), b"c".to_vec()];
        let digests = engine.hash_batch(&inputs);

        assert_eq!(digests[0], engine.sha3_256(b"a"));
        assert_eq!(digests[1], engine.sha3_256(b"b"));
        assert_eq!(digests[2], engine.sha3_256(b"c"));
    }

    #[test]
    fn test_polynomial_ring_arithmetic() {
        let mut a = Polynomial::zero();
        let mut b = Polynomial::zero();
        a.set(0, q() - 1);
        b.set(0, 2);

        // Addition wraps modulo q.
        let sum = a.add(&b);
        assert_eq!(sum.get(0), 1);
    }

    #[test]
    fn test_secret_key_debug_is_elided() {
        let mut engine = QuantumEngine::new();
        let keypair = engine.generate_keypair().unwrap();
        let rendered = format!("{:?}", keypair.secret_key);
        assert!(rendered.contains("elided"));
        assert!(!rendered.contains(&hex::encode(keypair.secret_key.as_bytes())[..16]));
    }
}
