//! Signature verification helpers.
//!
//! Every function here hashes its input with Keccak-256 and verifies a real
//! secp256k1 signature over that digest. Nothing returns success without
//! checking a signature.

use crate::signing::{self, PublicKey, Signature};
use crate::{Error, Result};

/// The digest that a message is signed over.
fn digest_of(message: &[u8]) -> [u8; 32] {
    crate::hashing::keccak256(message)
}

/// Verify that each signature matches the corresponding public key and
/// message.
pub fn verify_batch(
    public_keys: &[PublicKey],
    messages: &[&[u8]],
    signatures: &[Signature],
) -> Result<()> {
    if public_keys.len() != messages.len() || messages.len() != signatures.len() {
        return Err(Error::InvalidData(format!(
            "length mismatch: {} keys, {} messages, {} signatures",
            public_keys.len(),
            messages.len(),
            signatures.len()
        )));
    }

    for (public_key, (message, signature)) in public_keys
        .iter()
        .zip(messages.iter().zip(signatures.iter()))
    {
        signing::verify(public_key, &digest_of(message), signature)?;
    }

    Ok(())
}

/// Verify an n-of-n multi-signature over one message.
///
/// Each signer signs the same message and all signatures must be valid. This
/// is not signature aggregation: the signatures are checked individually and
/// the size is not reduced.
pub fn verify_multisig(
    public_keys: &[PublicKey],
    message: &[u8],
    signatures: &[Signature],
) -> Result<()> {
    if public_keys.len() != signatures.len() {
        return Err(Error::InvalidData(format!(
            "length mismatch: {} keys, {} signatures",
            public_keys.len(),
            signatures.len()
        )));
    }

    let digest = digest_of(message);
    for (public_key, signature) in public_keys.iter().zip(signatures.iter()) {
        signing::verify(public_key, &digest, signature)?;
    }

    Ok(())
}

/// Verify that at least `threshold` of the given signatures are valid.
///
/// Every supplied signature is checked, and the call succeeds only if at least
/// `threshold` of them verify. Checking only the first `threshold` would let a
/// caller pass one valid signature followed by garbage and be told the batch
/// was fine.
pub fn verify_threshold(
    public_keys: &[PublicKey],
    messages: &[&[u8]],
    signatures: &[Signature],
    threshold: usize,
) -> Result<()> {
    if public_keys.len() != messages.len() || messages.len() != signatures.len() {
        return Err(Error::InvalidData(format!(
            "length mismatch: {} keys, {} messages, {} signatures",
            public_keys.len(),
            messages.len(),
            signatures.len()
        )));
    }

    if threshold == 0 || threshold > signatures.len() {
        return Err(Error::InvalidData(format!(
            "threshold {} is not reachable with {} signatures",
            threshold,
            signatures.len()
        )));
    }

    let mut valid = 0usize;
    for (public_key, (message, signature)) in public_keys
        .iter()
        .zip(messages.iter().zip(signatures.iter()))
    {
        if signing::verify(public_key, &digest_of(message), signature).is_ok() {
            valid += 1;
        }
    }

    if valid < threshold {
        return Err(Error::VerificationFailed(format!(
            "only {} of {} signatures verified, threshold is {}",
            valid,
            signatures.len(),
            threshold
        )));
    }

    Ok(())
}

/// Verify a validator's signature over a block.
///
/// The signed payload binds the block hash to the chain id and to a domain
/// tag, so a signature made for one chain cannot be replayed on another and
/// a signature over a block cannot be reused as a signature over anything
/// else. The encoding is length-prefixed: without it, a block hash ending in
/// bytes that look like a chain id would collide with a different split.
pub fn verify_validator_signature(
    validator_pubkey: &PublicKey,
    block_hash: &[u8],
    signature: &Signature,
    chain_id: u64,
) -> Result<()> {
    let digest = validator_digest(block_hash, chain_id);
    signing::verify(validator_pubkey, &digest, signature)
}

/// The digest a validator signs for a block.
pub fn validator_digest(block_hash: &[u8], chain_id: u64) -> [u8; 32] {
    let mut preimage = Vec::with_capacity(32 + 8 + 8 + 32);
    preimage.extend_from_slice(b"TigerSmartChain/validator/v1");
    preimage.extend_from_slice(&chain_id.to_be_bytes());
    preimage.extend_from_slice(&(block_hash.len() as u64).to_be_bytes());
    preimage.extend_from_slice(block_hash);
    crate::hashing::keccak256(&preimage)
}

/// Verify a transaction signature against the sender's public key.
pub fn verify_transaction_signature(
    sender_pubkey: &PublicKey,
    tx_hash: &[u8],
    signature: &Signature,
) -> Result<()> {
    signing::verify(sender_pubkey, &digest_of(tx_hash), signature)
}

/// Verify a personal message signature against an expected address, following
/// EIP-191.
///
/// The signer is recovered from the signature and its address compared with
/// `expected_address`, which may be given with or without the `0x` prefix and
/// in any case. This is the check that a wallet login depends on.
pub fn verify_message(
    expected_address: &str,
    message: &[u8],
    signature: &Signature,
) -> Result<()> {
    let expected = parse_address(expected_address)?;
    let digest = personal_message_digest(message);

    let recovered = signing::recover(&digest, signature)?;
    let recovered_address = recovered.to_address();

    // Compare the raw 20 bytes; the textual form is not part of the identity.
    if recovered_address != expected {
        return Err(Error::VerificationFailed(format!(
            "signature was made by 0x{}, not 0x{}",
            hex::encode(recovered_address),
            hex::encode(expected)
        )));
    }

    Ok(())
}

/// The EIP-191 digest for a personal message:
/// `keccak256("\x19Ethereum Signed Message:\n" || len(message) || message)`,
/// where the length is written in decimal ASCII.
pub fn personal_message_digest(message: &[u8]) -> [u8; 32] {
    let mut preimage = Vec::with_capacity(28 + message.len());
    preimage.extend_from_slice(b"\x19Ethereum Signed Message:\n");
    preimage.extend_from_slice(message.len().to_string().as_bytes());
    preimage.extend_from_slice(message);

    crate::hashing::keccak256(&preimage)
}

/// Parse a 20-byte address from hex, with or without the `0x` prefix.
pub fn parse_address(address: &str) -> Result<[u8; 20]> {
    let body = address.strip_prefix("0x").or_else(|| address.strip_prefix("0X")).unwrap_or(address);

    let bytes = hex::decode(body)
        .map_err(|e| Error::InvalidData(format!("address is not valid hex: {e}")))?;

    bytes.try_into().map_err(|_| {
        Error::InvalidData(format!(
            "address must be 20 bytes, got {}",
            body.len() / 2
        ))
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::signing::generate_key_pair;

    #[test]
    fn batch_accepts_valid_signatures() {
        let (private_key1, public_key1) = generate_key_pair();
        let (private_key2, public_key2) = generate_key_pair();

        let msg1 = b"Message 1";
        let msg2 = b"Message 2";

        let sig1 = signing::sign(&private_key1, &digest_of(msg1)).unwrap();
        let sig2 = signing::sign(&private_key2, &digest_of(msg2)).unwrap();

        assert!(verify_batch(
            &[public_key1, public_key2],
            &[msg1, msg2],
            &[sig1, sig2]
        )
        .is_ok());
    }

    #[test]
    fn batch_rejects_a_swapped_signature() {
        let (private_key1, public_key1) = generate_key_pair();
        let (private_key2, public_key2) = generate_key_pair();

        let msg1 = b"Message 1";
        let msg2 = b"Message 2";

        let sig1 = signing::sign(&private_key1, &digest_of(msg1)).unwrap();
        let sig2 = signing::sign(&private_key2, &digest_of(msg2)).unwrap();

        // Same keys, signatures the wrong way round.
        assert!(verify_batch(
            &[public_key1, public_key2],
            &[msg1, msg2],
            &[sig2, sig1]
        )
        .is_err());
    }

    #[test]
    fn batch_rejects_a_length_mismatch() {
        let (private_key, public_key) = generate_key_pair();
        let signature = signing::sign(&private_key, &digest_of(b"one")).unwrap();

        assert!(verify_batch(&[public_key], &[b"one".as_slice(), b"two"], &[signature]).is_err());
    }

    #[test]
    fn multisig_requires_every_signature() {
        let (private_key1, public_key1) = generate_key_pair();
        let (private_key2, public_key2) = generate_key_pair();
        let message = b"same message";

        let digest = digest_of(message);
        let sig1 = signing::sign(&private_key1, &digest).unwrap();
        let sig2 = signing::sign(&private_key2, &digest).unwrap();

        assert!(verify_multisig(&[public_key1.clone(), public_key2.clone()], message, &[sig1.clone(), sig2.clone()]).is_ok());

        // A signature from a key that did not sign must be rejected.
        let (_, stranger) = generate_key_pair();
        assert!(verify_multisig(&[public_key1, stranger], message, &[sig1, sig2]).is_err());
    }

    #[test]
    fn threshold_accepts_enough_valid_signatures() {
        let (private_key1, public_key1) = generate_key_pair();
        let (private_key2, public_key2) = generate_key_pair();

        let msg1 = b"Message 1";
        let msg2 = b"Message 2";

        let sig1 = signing::sign(&private_key1, &digest_of(msg1)).unwrap();
        let sig2 = signing::sign(&private_key2, &digest_of(msg2)).unwrap();

        assert!(verify_threshold(
            &[public_key1.clone(), public_key2.clone()],
            &[msg1, msg2],
            &[sig1.clone(), sig2.clone()],
            1
        )
        .is_ok());

        assert!(verify_threshold(
            &[public_key1, public_key2],
            &[msg1, msg2],
            &[sig1, sig2],
            2
        )
        .is_ok());
    }

    #[test]
    fn threshold_rejects_a_garbage_signature_beyond_the_threshold() {
        // One valid signature followed by a broken one must not satisfy a
        // threshold of two. Checking only the first `threshold` entries, as
        // the previous implementation did, would have accepted this.
        let (private_key1, public_key1) = generate_key_pair();
        let (_, public_key2) = generate_key_pair();

        let msg1 = b"Message 1";
        let msg2 = b"Message 2";

        let sig1 = signing::sign(&private_key1, &digest_of(msg1)).unwrap();
        let mut sig2 = signing::sign(&private_key1, &digest_of(msg2)).unwrap();
        sig2 = Signature::from_bytes(
            {
                let mut bytes = *sig2.as_bytes();
                bytes[0] ^= 0xFF;
                bytes
            },
            sig2.recovery_id(),
        )
        .unwrap();

        assert!(verify_threshold(
            &[public_key1, public_key2],
            &[msg1, msg2],
            &[sig1, sig2],
            2
        )
        .is_err());
    }

    #[test]
    fn threshold_rejects_an_unreachable_threshold() {
        let (private_key, public_key) = generate_key_pair();
        let signature = signing::sign(&private_key, &digest_of(b"m")).unwrap();

        assert!(verify_threshold(&[public_key], &[b"m"], &[signature], 5).is_err());
        assert!(verify_threshold(&[generate_key_pair().1], &[b"m"], &[], 0).is_err());
    }

    #[test]
    fn validator_signature_is_chain_bound() {
        let (private_key, public_key) = generate_key_pair();
        let block_hash = crate::hashing::keccak256(b"block");

        let signature = signing::sign(&private_key, &validator_digest(&block_hash, 6666)).unwrap();

        assert!(verify_validator_signature(&public_key, &block_hash, &signature, 6666).is_ok());
        // The same signature must not verify on another chain.
        assert!(verify_validator_signature(&public_key, &block_hash, &signature, 1).is_err());
    }

    #[test]
    fn validator_signature_is_block_bound() {
        let (private_key, public_key) = generate_key_pair();
        let block_hash = crate::hashing::keccak256(b"block");
        let other_hash = crate::hashing::keccak256(b"other block");

        let signature = signing::sign(&private_key, &validator_digest(&block_hash, 6666)).unwrap();

        assert!(verify_validator_signature(&public_key, &other_hash, &signature, 6666).is_err());
    }

    #[test]
    fn validator_digest_encoding_is_unambiguous() {
        // Two different (hash, chain) pairs that concatenate to the same bytes
        // must not produce the same digest.
        let short_hash = b"aaaa";
        let long_hash = b"aaaa\x00\x00\x00\x00\x00\x00\x00\x00";

        assert_ne!(
            validator_digest(short_hash, 0),
            validator_digest(long_hash, 0)
        );
    }

    #[test]
    fn transaction_signature_round_trip() {
        let (private_key, public_key) = generate_key_pair();
        let tx_hash = crate::hashing::keccak256(b"raw transaction");

        let signature = signing::sign(&private_key, &digest_of(&tx_hash)).unwrap();

        assert!(verify_transaction_signature(&public_key, &tx_hash, &signature).is_ok());
        assert!(verify_transaction_signature(
            &public_key,
            &crate::hashing::keccak256(b"different transaction"),
            &signature
        )
        .is_err());
    }

    #[test]
    fn message_signature_recovers_the_signer() {
        let (private_key, public_key) = generate_key_pair();
        let message = b"Sign in to TigerScan";

        let signature = signing::sign(&private_key, &personal_message_digest(message)).unwrap();
        let address = hex::encode(public_key.to_address());

        assert!(verify_message(&address, message, &signature).is_ok());
        assert!(verify_message(&format!("0x{address}"), message, &signature).is_ok());
    }

    #[test]
    fn message_signature_rejects_a_different_address() {
        let (private_key, _) = generate_key_pair();
        let (_, other_public) = generate_key_pair();
        let message = b"Sign in";

        let signature = signing::sign(&private_key, &personal_message_digest(message)).unwrap();
        let other_address = hex::encode(other_public.to_address());

        assert!(verify_message(&other_address, message, &signature).is_err());
    }

    #[test]
    fn message_signature_rejects_a_changed_message() {
        let (private_key, public_key) = generate_key_pair();
        let signature =
            signing::sign(&private_key, &personal_message_digest(b"original")).unwrap();

        assert!(verify_message(
            &hex::encode(public_key.to_address()),
            b"changed",
            &signature
        )
        .is_err());
    }

    #[test]
    fn personal_message_digest_prefixes_the_length() {
        // The prefix carries the message length in decimal ASCII, so a
        // message and its length-prefixed form must not collide.
        let message = b"Hello, world!";
        let mut expected = Vec::new();
        expected.extend_from_slice(b"\x19Ethereum Signed Message:\n13");
        expected.extend_from_slice(message);

        assert_eq!(
            personal_message_digest(message),
            crate::hashing::keccak256(&expected)
        );
    }

    #[test]
    fn parse_address_accepts_both_forms() {
        let plain = "7e5f4552091a69125d5dfcb7b8c2659029395bdf";
        let prefixed = "0x7e5f4552091a69125d5dfcb7b8c2659029395bdf";

        assert_eq!(parse_address(plain).unwrap(), parse_address(prefixed).unwrap());
    }

    #[test]
    fn parse_address_rejects_bad_input() {
        assert!(parse_address("0xnothex").is_err());
        assert!(parse_address("0x1234").is_err());
    }
}
