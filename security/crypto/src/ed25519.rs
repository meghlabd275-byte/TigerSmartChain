//! Ed25519 cryptographic operations.

use ed25519_dalek::{Signable, SigningKey, Verifier, VerifyingKey};
use ed25519_dalek::ed25519::signature::SignatureInternal;
use zeroize::Zeroize;

/// Ed25519 private key.
#[derive(Clone, Zeroize)]
#[zeroize(drop)]
pub struct PrivateKey(SigningKey);

/// Ed25519 public key.
#[derive(Clone, Debug)]
pub struct PublicKey(VerifyingKey);

/// Ed25519 signature.
#[derive(Clone, Debug)]
pub struct Signature(ed25519_dalek::Signature);

/// Generate a new Ed25519 key pair.
pub fn generate_key_pair() -> PrivateKey {
    PrivateKey(SigningKey::generate(&mut rand::rngs::OsRng))
}

/// Sign a message with Ed25519.
pub fn sign(private_key: &PrivateKey, message: &[u8]) -> Result<Signature, super::Error> {
    let signature = private_key.0.sign(message);
    Ok(Signature(signature))
}

/// Verify an Ed25519 signature.
pub fn verify(public_key: &PublicKey, message: &[u8], signature: &Signature) -> Result<bool, super::Error> {
    Ok(public_key.0.verify(message, &signature.0).is_ok())
}

impl PrivateKey {
    /// Get the public key.
    pub fn public_key(&self) -> PublicKey {
        PublicKey(self.0.verifying_key())
    }
    
    /// Export as bytes.
    pub fn to_bytes(&self) -> [u8; 32] {
        self.0.to_bytes()
    }
    
    /// Import from bytes.
    pub fn from_bytes(bytes: [u8; 32]) -> Result<PrivateKey, super::Error> {
        SigningKey::from_bytes(&bytes)
            .map(PrivateKey)
            .map_err(|e| super::Error::InvalidKey(e.to_string()))
    }
}

impl PublicKey {
    /// Export as bytes.
    pub fn to_bytes(&self) -> [u32; 32] {
        self.0.to_bytes()
    }
}

impl Signature {
    /// Export as bytes.
    pub fn to_bytes(&self) -> [u8; 64] {
        self.0.to_bytes()
    }
    
    /// Import from bytes.
    pub fn from_bytes(bytes: [u8; 64]) -> Result<Signature, super::Error> {
        Ok(Signature(ed25519_dalek::Signature::from_bytes(&bytes)))
    }
}