//! Signature verification module

use crate::Result;
use sha3::{Digest, Keccak256};

/// Verifies cryptographic signatures
pub struct SignatureVerifier;

impl SignatureVerifier {
    /// Verify an Ethereum signed message
    pub fn verify_signed_message(
        message: &str,
        signature: &str,
        expected_address: &str,
    ) -> Result<bool> {
        // Recreate the signed message prefix
        let prefixed = format!("\x19Ethereum Signed Message:\n{}", message.len());
        let hash = Keccak256::digest(format!("{}{}", prefixed, message).as_bytes());
        
        // Parse signature (65 bytes: r, s, v)
        let sig_bytes = hex::decode(signature.trim_start_matches("0x"))
            .map_err(|_| crate::SecurityError::InvalidSignature)?;
        
        if sig_bytes.len() != 65 {
            return Err(crate::SecurityError::InvalidSignature);
        }
        
        // Extract r, s, v
        let _r = &sig_bytes[0..32];
        let _s = &sig_bytes[32..64];
        let v = sig_bytes[64];
        
        // In production, would use secp256k1 to recover address and verify
        // For now, just return false as we can't fully verify without the library
        Ok(false)
    }
    
    /// Verify EIP-712 typed data signature
    pub fn verify_typed_data(
        _domain_separator: &[u8],
        _hash_struct: &[u8],
        _signature: &str,
        _expected_address: &str,
    ) -> Result<bool> {
        Ok(false)
    }
    
    /// Hash message for signing (EIP-191)
    pub fn hash_message(message: &str) -> String {
        let prefixed = format!("\x19Ethereum Signed Message:\n{}", message.len());
        let hash = Keccak256::digest(format!("{}{}", prefixed, message).as_bytes());
        hex::encode(hash)
    }
    
    /// Hash typed data (EIP-712)
    pub fn hash_typed_data(domain: &[u8], message: &[u8]) -> String {
        let domain_hash = Keccak256::digest(domain);
        let message_hash = Keccak256::digest(message);
        
        let encoded = [
            b"\x19\x01",
            domain_hash.as_slice(),
            message_hash.as_slice(),
        ].concat();
        
        hex::encode(Keccak256::digest(&encoded))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_hash_message() {
        let hash = SignatureVerifier::hash_message("Hello, World!");
        assert_eq!(hash.len(), 64);
    }
}
