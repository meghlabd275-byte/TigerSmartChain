//! ECDSA cryptographic operations using secp256k1.

use k256::ecdsa::{SigningKey, VerifyingKey};
use k256::SecretKey;
use zeroize::Zeroize;

/// Private key for ECDSA operations.
#[derive(Clone, Zeroize)]
#[zeroize(drop)]
pub struct PrivateKey(SecretKey);

/// Public key for ECDSA operations.
#[derive(Clone, Debug)]
pub struct PublicKey(VerifyingKey);

/// ECDSA signature.
#[derive(Clone, Debug)]
pub struct Signature {
    sig: k256::ecdsa::Signature,
}

/// Generate a new ECDSA key pair.
pub fn generate_key_pair() -> (PrivateKey, PublicKey) {
    let secret_key = SecretKey::random(rng::OsRng);
    let public_key = VerifyingKey::from(&secret_key);
    
    (PrivateKey(secret_key), PublicKey(public_key))
}

/// Sign a message with ECDSA.
pub fn sign(private_key: &PrivateKey, message: &[u8]) -> Result<Signature, super::Error> {
    let signing_key = SigningKey::from(&private_key.0);
    
    let signature = signing_key.sign(message);
    
    Ok(Signature { sig: signature })
}

/// Verify an ECDSA signature.
pub fn verify(public_key: &PublicKey, message: &[u8], signature: &Signature) -> Result<bool, super::Error> {
    Ok(public_key.0.verify(message, &signature.sig).is_ok())
}

/// Recover public key from signature.
pub fn recover(message: &[u8], signature: &Signature) -> Result<PublicKey, super::Error> {
    let pk = signature.sig.verify_msg_prehash(message)
        .map_err(|e| super::Error::VerificationFailed(e.to_string()))?;
    
    Ok(PublicKey(VerifyingKey::from(&pk)))
}

impl PrivateKey {
    /// Get the public key for this private key.
    pub fn public_key(&self) -> PublicKey {
        PublicKey(VerifyingKey::from(&self.0))
    }
    
    /// Export the private key as bytes.
    pub fn to_bytes(&self) -> [u8; 32] {
        self.0.to_bytes().into()
    }
    
    /// Import a private key from bytes.
    pub fn from_bytes(bytes: [u8; 32]) -> Result<PrivateKey, super::Error> {
        SecretKey::from_bytes(bytes.into())
            .map(PrivateKey)
            .map_err(|e| super::Error::InvalidKey(e.to_string()))
    }
}

impl PublicKey {
    /// Export the public key as compressed bytes.
    pub fn to_compressed_bytes(&self) -> [u33; 33] {
        let bytes = self.0.to_encoded_point(true);
        let mut result = [0u8; 33];
        result.copy_from_slice(&bytes);
        result
    }
    
    /// Export the public key as uncompressed bytes.
    pub fn to_uncompressed_bytes(&self) -> [u65; 65] {
        let bytes = self.0.to_encoded_point(false);
        let mut result = [0u8; 65];
        result.copy_from_slice(&bytes);
        result
    }
}

impl Signature {
    /// Export the signature as bytes.
    pub fn to_bytes(&self) -> Vec<u8> {
        self.sig.to_bytes().to_vec()
    }
    
    /// Import a signature from bytes.
    pub fn from_bytes(bytes: &[u8]) -> Result<Signature, super::Error> {
        let sig = k256::ecdsa::Signature::from_slice(bytes)
            .map_err(|e| super::Error::InvalidSignature(e.to_string()))?;
        
        Ok(Signature { sig })
    }
}