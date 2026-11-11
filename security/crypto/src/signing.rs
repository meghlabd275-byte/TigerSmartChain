//! secp256k1 signing, the curve Ethereum uses.
//!
//! Signatures are ECDSA over a 32-byte digest, in the compact 64-byte
//! `r || s` form, with a recovery id available so a signer can be identified
//! from a signature alone.
//!
//! Two properties this module is careful about:
//!
//!   * Nonces are deterministic (RFC 6979), so signing the same message twice
//!     yields the same signature and no nonce is ever reused. Reusing a
//!     nonce across two messages reveals the private key.
//!   * Signatures are low-`s`. `(r, s)` and `(r, n - s)` both verify, so
//!     without normalisation a transaction has two valid encodings and two
//!     hashes, which breaks any system that treats the hash as an id.

use k256::ecdsa::{
    RecoveryId, Signature as K256Signature, SigningKey, VerifyingKey,
};
use k256::ecdsa::signature::hazmat::PrehashVerifier;
use rand_core::OsRng;
use zeroize::{Zeroize, ZeroizeOnDrop};

use crate::{Error, Result};

/// A secp256k1 private key.
///
/// Zeroed on drop and omitted from `Debug` output.
#[derive(Clone, Zeroize, ZeroizeOnDrop)]
pub struct PrivateKey {
    bytes: [u8; 32],
}

impl std::fmt::Debug for PrivateKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("PrivateKey(<redacted>)")
    }
}

/// A secp256k1 public key, stored uncompressed (65 bytes, `0x04 || x || y`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PublicKey {
    bytes: [u8; 65],
}

/// An ECDSA signature: 64 bytes of `r || s`, plus the recovery id.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Signature {
    bytes: [u8; 64],
    recovery_id: u8,
}

/// Generate a key pair from the operating system CSPRNG.
pub fn generate_key_pair() -> (PrivateKey, PublicKey) {
    let signing_key = SigningKey::random(&mut OsRng);
    let private = PrivateKey {
        bytes: signing_key.to_bytes().into(),
    };
    let public = public_from_signing_key(&signing_key);
    (private, public)
}

/// Sign the 32-byte `digest` with `private_key`.
pub fn sign(private_key: &PrivateKey, digest: &[u8]) -> Result<Signature> {
    if digest.len() != 32 {
        return Err(Error::InvalidData(format!(
            "digest must be 32 bytes, got {}",
            digest.len()
        )));
    }

    let signing_key = SigningKey::from_slice(&private_key.bytes)
        .map_err(|e| Error::InvalidKey(e.to_string()))?;

    // Deterministic per RFC 6979. `sign_digest_recoverable` also returns the
    // recovery id, which is what identifies the signer.
    let (signature, recovery_id) = signing_key
        .sign_prehash_recoverable(digest)
        .map_err(|e| Error::SigningFailed(e.to_string()))?;

    // Reject a high-`s` signature rather than letting two encodings exist.
    let signature = signature.normalize_s().unwrap_or(signature);

    Ok(Signature {
        bytes: signature.to_bytes().into(),
        recovery_id: recovery_id.to_byte(),
    })
}

/// Verify a signature against a digest.
pub fn verify(public_key: &PublicKey, digest: &[u8], signature: &Signature) -> Result<()> {
    if digest.len() != 32 {
        return Err(Error::InvalidData(format!(
            "digest must be 32 bytes, got {}",
            digest.len()
        )));
    }

    let verifying_key = VerifyingKey::from_sec1_bytes(&public_key.bytes)
        .map_err(|e| Error::InvalidKey(e.to_string()))?;

    let signature = K256Signature::from_slice(&signature.bytes)
        .map_err(|e| Error::InvalidSignature(e.to_string()))?;

    verifying_key
        .verify_prehash(digest, &signature)
        .map_err(|_| Error::VerificationFailed("signature mismatch".to_string()))
}

/// Recover the public key that produced `signature` over `digest`.
///
/// This is how an Ethereum sender is derived: the transaction carries the
/// recovery id, and the sender address comes from the recovered key.
pub fn recover(digest: &[u8], signature: &Signature) -> Result<PublicKey> {
    if digest.len() != 32 {
        return Err(Error::InvalidData(format!(
            "digest must be 32 bytes, got {}",
            digest.len()
        )));
    }

    // Read the recovery id before shadowing `signature` with the parsed
    // k256 form, which does not carry one.
    let recovery_id = RecoveryId::from_byte(signature.recovery_id)
        .ok_or_else(|| Error::InvalidSignature("invalid recovery id".to_string()))?;

    let parsed = K256Signature::from_slice(&signature.bytes)
        .map_err(|e| Error::InvalidSignature(e.to_string()))?;

    let verifying_key = VerifyingKey::recover_from_prehash(digest, &parsed, recovery_id)
        .map_err(|e| Error::VerificationFailed(e.to_string()))?;

    Ok(public_from_verifying_key(&verifying_key))
}

/// Derive the Ethereum address (last 20 bytes of Keccak-256 of the
/// uncompressed public key, without the `0x04` prefix).
pub fn public_key_to_address(public_key: &PublicKey) -> [u8; 20] {
    let hash = crate::hashing::keccak256(&public_key.bytes[1..]);
    let mut address = [0u8; 20];
    address.copy_from_slice(&hash[12..]);
    address
}

fn public_from_signing_key(signing_key: &SigningKey) -> PublicKey {
    public_from_verifying_key(signing_key.verifying_key())
}

fn public_from_verifying_key(verifying_key: &VerifyingKey) -> PublicKey {
    let point = verifying_key.to_encoded_point(false);
    let mut bytes = [0u8; 65];
    bytes.copy_from_slice(point.as_bytes());
    PublicKey { bytes }
}

impl PrivateKey {
    /// The public key for this private key.
    pub fn public_key(&self) -> PublicKey {
        let signing_key = SigningKey::from_slice(&self.bytes)
            .expect("private key bytes are validated on construction");
        public_from_signing_key(&signing_key)
    }

    /// Export the private key bytes.
    pub fn to_bytes(&self) -> [u8; 32] {
        self.bytes
    }

    /// Import a private key from bytes.
    ///
    /// Rejects a zero or out-of-range scalar rather than accepting a key that
    /// cannot sign.
    pub fn from_bytes(bytes: [u8; 32]) -> Result<Self> {
        SigningKey::from_slice(&bytes).map_err(|e| Error::InvalidKey(e.to_string()))?;
        Ok(PrivateKey { bytes })
    }
}

impl PublicKey {
    /// The uncompressed encoding, 65 bytes.
    pub fn to_uncompressed_bytes(&self) -> [u8; 65] {
        self.bytes
    }

    /// The compressed encoding, 33 bytes.
    pub fn to_compressed_bytes(&self) -> [u8; 33] {
        let verifying_key = VerifyingKey::from_sec1_bytes(&self.bytes)
            .expect("stored public key is a valid point");
        let point = verifying_key.to_encoded_point(true);
        let mut bytes = [0u8; 33];
        bytes.copy_from_slice(point.as_bytes());
        bytes
    }

    /// Import from either compressed (33-byte) or uncompressed (65-byte) form.
    pub fn from_bytes(bytes: &[u8]) -> Result<Self> {
        let verifying_key = VerifyingKey::from_sec1_bytes(bytes)
            .map_err(|e| Error::InvalidKey(e.to_string()))?;
        Ok(public_from_verifying_key(&verifying_key))
    }

    /// The Ethereum address for this public key.
    pub fn to_address(&self) -> [u8; 20] {
        public_key_to_address(self)
    }
}

impl Signature {
    /// The 64-byte `r || s` encoding.
    pub fn as_bytes(&self) -> &[u8; 64] {
        &self.bytes
    }

    /// The recovery id, as stored.
    pub fn recovery_id(&self) -> u8 {
        self.recovery_id
    }

    /// Build from `r || s` and a recovery id.
    pub fn from_bytes(bytes: [u8; 64], recovery_id: u8) -> Result<Self> {
        // Reject a malformed `r` or `s` up front rather than at verification.
        K256Signature::from_slice(&bytes)
            .map_err(|e| Error::InvalidSignature(e.to_string()))?;
        RecoveryId::from_byte(recovery_id)
            .ok_or_else(|| Error::InvalidSignature("invalid recovery id".to_string()))?;

        Ok(Signature { bytes, recovery_id })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hashing::keccak256;

    fn digest(message: &[u8]) -> [u8; 32] {
        keccak256(message)
    }

    #[test]
    fn sign_and_verify() {
        let (private_key, public_key) = generate_key_pair();
        let d = digest(b"Hello, TigerSmartChain!");

        let signature = sign(&private_key, &d).unwrap();
        assert!(verify(&public_key, &d, &signature).is_ok());
    }

    #[test]
    fn verify_rejects_a_different_message() {
        let (private_key, public_key) = generate_key_pair();
        let signature = sign(&private_key, &digest(b"message one")).unwrap();

        assert!(verify(&public_key, &digest(b"message two"), &signature).is_err());
    }

    #[test]
    fn verify_rejects_a_different_key() {
        let (private_key, _) = generate_key_pair();
        let (_, other_public) = generate_key_pair();
        let d = digest(b"message");

        let signature = sign(&private_key, &d).unwrap();
        assert!(verify(&other_public, &d, &signature).is_err());
    }

    #[test]
    fn verify_rejects_a_tampered_signature() {
        let (private_key, public_key) = generate_key_pair();
        let d = digest(b"message");

        let mut signature = sign(&private_key, &d).unwrap();
        signature.bytes[0] ^= 0x01;

        assert!(verify(&public_key, &d, &signature).is_err());
    }

    #[test]
    fn signing_is_deterministic() {
        // RFC 6979: the same key and digest must give the same signature, or
        // a repeated nonce could leak the key.
        let (private_key, _) = generate_key_pair();
        let d = digest(b"message");

        let first = sign(&private_key, &d).unwrap();
        let second = sign(&private_key, &d).unwrap();

        assert_eq!(first.as_bytes(), second.as_bytes());
    }

    #[test]
    fn signatures_are_low_s() {
        // A high-`s` signature would give the same message two valid
        // encodings. Check `s` is in the lower half of the curve order.
        let (private_key, _) = generate_key_pair();
        let half_order: [u8; 32] = [
            0x7f, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff,
            0xff, 0xff, 0x5d, 0x57, 0x6e, 0x73, 0x57, 0xa4, 0x50, 0x1d, 0xdf, 0xe9, 0x2f, 0x46,
            0x68, 0x1b, 0x20, 0xa0,
        ];

        for i in 0..64u8 {
            let signature = sign(&private_key, &digest(&[i])).unwrap();
            let s = &signature.as_bytes()[32..];
            assert!(s <= &half_order[..], "signature {i} had a high s");
        }
    }

    #[test]
    fn recovery_returns_the_signer() {
        let (private_key, public_key) = generate_key_pair();
        let d = digest(b"recover me");

        let signature = sign(&private_key, &d).unwrap();
        let recovered = recover(&d, &signature).unwrap();

        assert_eq!(recovered, public_key);
    }

    #[test]
    fn recovery_rejects_a_tampered_signature() {
        // Recovery over a tampered signature does not fail: it simply yields
        // some other public key. What must never happen is recovering the
        // original signer, so that is what is asserted.
        let (private_key, public_key) = generate_key_pair();
        let d = digest(b"message");

        let mut signature = sign(&private_key, &d).unwrap();
        signature.bytes[10] ^= 0xFF;

        match recover(&d, &signature) {
            Ok(recovered) => assert_ne!(recovered, public_key),
            Err(_) => {} // also acceptable: the point was not recoverable
        }
    }

    #[test]
    fn address_matches_the_known_vector() {
        // Private key 0x01 must give address
        // 0x7e5f4552091a69125d5dfcb7b8c2659029395bdf.
        let mut bytes = [0u8; 32];
        bytes[31] = 1;
        let private_key = PrivateKey::from_bytes(bytes).unwrap();

        let address = private_key.public_key().to_address();
        assert_eq!(
            hex::encode(address),
            "7e5f4552091a69125d5dfcb7b8c2659029395bdf"
        );
    }

    #[test]
    fn compressed_and_uncompressed_agree() {
        let (_, public_key) = generate_key_pair();

        let compressed = public_key.to_compressed_bytes();
        let restored = PublicKey::from_bytes(&compressed).unwrap();

        assert_eq!(restored, public_key);
    }

    #[test]
    fn rejects_a_wrong_length_digest() {
        let (private_key, public_key) = generate_key_pair();
        assert!(sign(&private_key, b"too short").is_err());

        let signature = sign(&private_key, &digest(b"ok")).unwrap();
        assert!(verify(&public_key, b"too short", &signature).is_err());
    }

    #[test]
    fn rejects_an_invalid_private_key() {
        assert!(PrivateKey::from_bytes([0u8; 32]).is_err());
    }

    #[test]
    fn private_key_does_not_appear_in_debug_output() {
        let (private_key, _) = generate_key_pair();
        let rendered = format!("{:?}", private_key);
        assert!(rendered.contains("redacted"));
        assert!(!rendered.contains(&hex::encode(private_key.to_bytes())));
    }
}
