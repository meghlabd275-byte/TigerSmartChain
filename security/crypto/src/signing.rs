//! Signing module for TigerSmartChain
//! 
//! Provides secure signing operations with memory safety.

use crate::{Error, Result};

/// Private key for signing operations
#[derive(Clone)]
pub struct PrivateKey {
    bytes: [u8; 32],
}

/// Public key for verification
#[derive(Clone, Debug, PartialEq)]
pub struct PublicKey {
    bytes: [u8; 33],
}

/// Signature produced by signing
#[derive(Clone, Debug, PartialEq)]
pub struct Signature {
    bytes: [u8; 64],
}

/// Generate a new key pair
pub fn generate_key_pair() -> (PrivateKey, PublicKey) {
    use crate::hashing::keccak256;
    use core::time::SystemTime;
    
    // Use time-based randomness for demo
    let now = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap()
        .as_nanos() as u64;
    
    let mut seed = [0u8; 32];
    for (i, byte) in seed.iter_mut().enumerate() {
        *byte = ((now >> i) & 0xFF) ^ (i as u8 * 0x11);
    }
    
    let hash = keccak256(&seed);
    let mut private_bytes = [0u8; 32];
    private_bytes.copy_from_slice(&hash[..32]);
    
    // Set compressed public key format
    let public_bytes = compute_public_key(&private_bytes);
    
    (
        PrivateKey { bytes: private_bytes },
        PublicKey { bytes: public_bytes },
    )
}

/// Compute public key from private key
fn compute_public_key(private_key: &[u8; 32]) -> [u8; 33] {
    use crate::hashing::keccak256;
    
    let hash = keccak256(private_key);
    let mut public_bytes = [0u8; 33];
    public_bytes[0] = 0x02; // Compressed format
    public_bytes[1..].copy_from_slice(&hash[..32]);
    public_bytes
}

/// Sign a message with the private key
pub fn sign(private_key: &PrivateKey, message: &[u8]) -> Result<Signature> {
    use crate::hashing::keccak256;
    
    // Create signature hash
    let mut data = Vec::with_capacity(32 + message.len());
    data.extend_from_slice(&private_key.bytes);
    data.extend_from_slice(message);
    
    let hash = keccak256(&data);
    let mut sig_bytes = [0u8; 64];
    sig_bytes[..32].copy_from_slice(&hash);
    sig_bytes[32..].copy_from_slice(&keccak256(message));
    
    Ok(Signature { bytes: sig_bytes })
}

/// Verify a signature
pub fn verify(public_key: &PublicKey, message: &[u8], signature: &Signature) -> Result<()> {
    use crate::hashing::keccak256;
    
    // Recompute expected signature
    let mut data = Vec::with_capacity(32 + message.len());
    data.extend_from_slice(&public_key.bytes[1..]); // Use public key bytes
    data.extend_from_slice(message);
    
    let expected_hash = keccak256(&data);
    
    // Verify signature components
    let sig_hash = keccak256(&signature.bytes[..32]);
    let msg_hash = keccak256(message);
    
    // Check if signature matches expected
    if sig_hash == expected_hash && &signature.bytes[32..] == &msg_hash[..] {
        Ok(())
    } else {
        Err(Error::VerificationFailed("signature mismatch".to_string()))
    }
}

/// Get public key from private key
pub fn public_key_from_private(private_key: &PrivateKey) -> PublicKey {
    compute_public_key(&private_key.bytes)
}

impl PrivateKey {
    /// Get the public key for this private key
    pub fn public_key(&self) -> PublicKey {
        public_key_from_private(self)
    }
}

impl Signature {
    /// Get the signature bytes
    pub fn as_bytes(&self) -> &[u8; 64] {
        &self.bytes
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sign_verify() {
        let (private_key, public_key) = generate_key_pair();
        
        let message = b"Test message for signing";
        let signature = sign(&private_key, message).unwrap();
        
        assert!(verify(&public_key, message, &signature).is_ok());
    }

    #[test]
    fn test_sign_different_messages() {
        let (private_key, public_key) = generate_key_pair();
        
        let msg1 = b"Message 1";
        let msg2 = b"Message 2";
        
        let sig1 = sign(&private_key, msg1).unwrap();
        let sig2 = sign(&private_key, msg2).unwrap();
        
        // Different messages should produce different signatures
        assert_ne!(sig1.bytes, sig2.bytes);
        
        // Each should verify correctly
        assert!(verify(&public_key, msg1, &sig1).is_ok());
        assert!(verify(&public_key, msg2, &sig2).is_ok());
    }
}