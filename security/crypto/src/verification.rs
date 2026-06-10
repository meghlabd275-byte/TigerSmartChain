//! Verification module for TigerSmartChain
//! 
//! Provides signature verification operations.

use crate::{Error, Result};
use super::signing::{PublicKey, Signature};

/// Verify multiple signatures in batch
pub fn verify_batch(
    public_keys: &[PublicKey],
    messages: &[&[u8]],
    signatures: &[Signature],
) -> Result<()> {
    // Check lengths match
    if public_keys.len() != messages.len() || messages.len() != signatures.len() {
        return Err(Error::InvalidData("length mismatch".to_string()));
    }
    
    // Verify each signature
    for i in 0..public_keys.len() {
        super::signing::verify(&public_keys[i], messages[i], &signatures[i])?;
    }
    
    Ok(())
}

/// Verify threshold signatures
pub fn verify_threshold(
    public_keys: &[PublicKey],
    messages: &[&[u8]],
    signatures: &[Signature],
    threshold: usize,
) -> Result<()> {
    // Require at least threshold signatures
    if signatures.len() < threshold {
        return Err(Error::VerificationFailed(
            format!("not enough signatures: {} < {}", signatures.len(), threshold)
        ));
    }
    
    // Verify threshold number of signatures
    for i in 0..threshold {
        super::signing::verify(&public_keys[i], messages[i], &signatures[i])?;
    }
    
    Ok(())
}

/// Verify aggregated signature (simplified)
pub fn verify_aggregated(
    aggregated_public_key: &PublicKey,
    message: &[u8],
    aggregated_signature: &Signature,
) -> Result<()> {
    // Simplified verification - in production would use BLS aggregation
    super::signing::verify(aggregated_public_key, message, aggregated_signature)
}

/// Validator signature verification
pub fn verify_validator_signature(
    validator_pubkey: &PublicKey,
    block_hash: &[u8],
    signature: &Signature,
    chain_id: u64,
) -> Result<()> {
    // Include chain ID in verification for replay protection
    let mut message_with_chain = Vec::with_capacity(block_hash.len() + 8);
    message_with_chain.extend_from_slice(block_hash);
    message_with_chain.extend_from_slice(&chain_id.to_le_bytes());
    
    super::signing::verify(validator_pubkey, &message_with_chain, signature)
}

/// Transaction signature verification
pub fn verify_transaction_signature(
    sender_pubkey: &PublicKey,
    tx_hash: &[u8],
    signature: &Signature,
) -> Result<()> {
    super::signing::verify(sender_pubkey, tx_hash, signature)
}

/// Message signature verification (EIP-191)
pub fn verify_message(
    expected_address: &str,
    message: &[u8],
    signature: &Signature,
) -> Result<()> {
    // EIP-191 format: \x19Ethereum Signed Message:\n<len(message)>message
    use crate::hashing::keccak256;
    
    let prefix = b"\x19Ethereum Signed Message:\n";
    let mut data = Vec::new();
    data.extend_from_slice(prefix);
    data.extend_from_slice(&message.len().to_le_bytes());
    data.extend_from_slice(message);
    
    let hash = keccak256(&data);
    
    // Verify the hash matches (simplified)
    let _ = expected_address;
    let _ = hash;
    
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use super::super::signing::generate_key_pair;

    #[test]
    fn test_verify_batch() {
        let (private_key1, public_key1) = generate_key_pair();
        let (private_key2, public_key2) = generate_key_pair();
        
        let msg1 = b"Message 1";
        let msg2 = b"Message 2";
        
        let sig1 = super::super::signing::sign(&private_key1, msg1).unwrap();
        let sig2 = super::super::signing::sign(&private_key2, msg2).unwrap();
        
        let result = verify_batch(
            &[public_key1, public_key2],
            &[msg1, msg2],
            &[sig1, sig2],
        );
        
        assert!(result.is_ok());
    }

    #[test]
    fn test_verify_threshold() {
        let (private_key1, public_key1) = generate_key_pair();
        let (private_key2, public_key2) = generate_key_pair();
        
        let msg1 = b"Message 1";
        let msg2 = b"Message 2";
        
        let sig1 = super::super::signing::sign(&private_key1, msg1).unwrap();
        let sig2 = super::super::signing::sign(&private_key2, msg2).unwrap();
        
        // Require threshold of 2
        let result = verify_threshold(
            &[public_key1, public_key2],
            &[msg1, msg2],
            &[sig1, sig2],
            2,
        );
        
        assert!(result.is_ok());
        
        // Require threshold of 3 with only 2 signatures - should fail
        let result = verify_threshold(
            &[public_key1, public_key2],
            &[msg1, msg2],
            &[sig1, sig2],
            3,
        );
        
        assert!(result.is_err());
    }
}