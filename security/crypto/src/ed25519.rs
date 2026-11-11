//! Ed25519 signing.
//!
//! Ed25519 is used where a validator or relayer identity needs a small,
//! fast, deterministic signature that does not depend on the random number
//! generator at signing time. It is not the curve Ethereum uses; for
//! transaction signatures and address recovery see [`crate::signing`].

use ed25519_dalek::{Signer, SigningKey, Verifier, VerifyingKey};
use rand_core::OsRng;
use zeroize::{Zeroize, ZeroizeOnDrop};

use crate::{Error, Result};

/// An Ed25519 private key.
#[derive(Clone, Zeroize, ZeroizeOnDrop)]
pub struct PrivateKey {
    bytes: [u8; 32],
}

impl std::fmt::Debug for PrivateKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("PrivateKey(<redacted>)")
    }
}

/// An Ed25519 public key.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PublicKey(VerifyingKey);

/// An Ed25519 signature.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Signature(ed25519_dalek::Signature);

/// Generate a key pair from the operating system CSPRNG.
pub fn generate_key_pair() -> (PrivateKey, PublicKey) {
    let signing_key = SigningKey::generate(&mut OsRng);
    let public = PublicKey(signing_key.verifying_key());
    (
        PrivateKey {
            bytes: signing_key.to_bytes(),
        },
        public,
    )
}

/// Sign a message.
///
/// The message is signed as given. For domain separation, pass a prefixed
/// message rather than relying on the caller to remember a context string.
pub fn sign(private_key: &PrivateKey, message: &[u8]) -> Result<Signature> {
    let signing_key = SigningKey::from_bytes(&private_key.bytes);
    Ok(Signature(signing_key.sign(message)))
}

/// Verify a signature.
pub fn verify(public_key: &PublicKey, message: &[u8], signature: &Signature) -> Result<()> {
    public_key
        .0
        .verify(message, &signature.0)
        .map_err(|_| Error::VerificationFailed("signature mismatch".to_string()))
}

impl PrivateKey {
    /// The public key for this private key.
    pub fn public_key(&self) -> PublicKey {
        PublicKey(SigningKey::from_bytes(&self.bytes).verifying_key())
    }

    /// Export the private key bytes.
    pub fn to_bytes(&self) -> [u8; 32] {
        self.bytes
    }

    /// Import a private key from bytes.
    pub fn from_bytes(bytes: [u8; 32]) -> Result<Self> {
        // Any 32 bytes is a valid Ed25519 seed, so this cannot fail, but keep
        // the fallible signature for symmetry with the other key types.
        Ok(PrivateKey { bytes })
    }
}

impl PublicKey {
    /// Export the 32-byte public key.
    pub fn to_bytes(&self) -> [u8; 32] {
        self.0.to_bytes()
    }

    /// Import from 32 bytes.
    pub fn from_bytes(bytes: [u8; 32]) -> Result<Self> {
        VerifyingKey::from_bytes(&bytes)
            .map(PublicKey)
            .map_err(|e| Error::InvalidKey(e.to_string()))
    }
}

impl Signature {
    /// Export the 64-byte signature.
    pub fn to_bytes(&self) -> [u8; 64] {
        self.0.to_bytes()
    }

    /// Import from 64 bytes.
    pub fn from_bytes(bytes: [u8; 64]) -> Result<Self> {
        // Reject a non-canonical encoding rather than accepting bytes that
        // could be interpreted two ways.
        let signature = ed25519_dalek::Signature::from_slice(&bytes)
            .map_err(|e| Error::InvalidSignature(e.to_string()))?;
        Ok(Signature(signature))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sign_and_verify() {
        let (private_key, public_key) = generate_key_pair();
        let message = b"Hello, TigerSmartChain!";

        let signature = sign(&private_key, message).unwrap();
        assert!(verify(&public_key, message, &signature).is_ok());
    }

    #[test]
    fn verify_rejects_a_different_message() {
        let (private_key, public_key) = generate_key_pair();
        let signature = sign(&private_key, b"original").unwrap();

        assert!(verify(&public_key, b"tampered", &signature).is_err());
    }

    #[test]
    fn verify_rejects_a_different_key() {
        let (private_key, _) = generate_key_pair();
        let (_, other_public) = generate_key_pair();

        let signature = sign(&private_key, b"message").unwrap();
        assert!(verify(&other_public, b"message", &signature).is_err());
    }

    #[test]
    fn verify_rejects_a_tampered_signature() {
        let (private_key, public_key) = generate_key_pair();
        let mut signature = sign(&private_key, b"message").unwrap();
        signature.0 = ed25519_dalek::Signature::from_bytes(&{
            let mut bytes = signature.to_bytes();
            bytes[0] ^= 0x01;
            bytes
        });

        assert!(verify(&public_key, b"message", &signature).is_err());
    }

    #[test]
    fn public_key_round_trips_through_bytes() {
        let (private_key, public_key) = generate_key_pair();

        assert_eq!(
            PublicKey::from_bytes(public_key.to_bytes()).unwrap(),
            public_key
        );
        assert_eq!(private_key.public_key(), public_key);
    }

    #[test]
    fn signature_round_trips_through_bytes() {
        let (private_key, public_key) = generate_key_pair();
        let signature = sign(&private_key, b"message").unwrap();

        let restored = Signature::from_bytes(signature.to_bytes()).unwrap();
        assert_eq!(restored, signature);
        assert!(verify(&public_key, b"message", &restored).is_ok());
    }

    #[test]
    fn signing_is_deterministic() {
        // Ed25519 has no signing-time randomness, so the same key and message
        // must always produce the same signature.
        let (private_key, _) = generate_key_pair();

        let first = sign(&private_key, b"message").unwrap();
        let second = sign(&private_key, b"message").unwrap();

        assert_eq!(first, second);
    }

    #[test]
    fn private_key_does_not_appear_in_debug_output() {
        let (private_key, _) = generate_key_pair();
        let rendered = format!("{:?}", private_key);
        assert!(rendered.contains("redacted"));
        assert!(!rendered.contains(&hex::encode(private_key.to_bytes())));
    }

    #[test]
    fn rejects_a_signature_that_does_not_verify() {
        // ed25519-dalek accepts any 64 bytes at parse time, so the guarantee
        // is that a wrong signature fails verification, not that parsing
        // rejects it.
        let (private_key, public_key) = generate_key_pair();
        let message = b"message";

        let signature = Signature::from_bytes([0xFFu8; 64]).unwrap();
        assert!(verify(&public_key, message, &signature).is_err());

        // And a real signature for a different message must not verify.
        let real = sign(&private_key, b"other message").unwrap();
        assert!(verify(&public_key, message, &real).is_err());
    }
}
