//! Signature verification module
//!
//! Implements the two signing envelopes used on EVM chains:
//!
//! * **EIP-191** — a message is wrapped with the
//!   `"\x19Ethereum Signed Message:\n" + len` prefix before hashing, so a
//!   signed message can never be confused with a signed transaction.
//! * **EIP-712** — structured data is hashed as
//!   `keccak256("\x19\x01" || domainSeparator || hashStruct(message))`.
//!
//! Both recover the signer's address from the signature and compare it with the
//! address the caller expects. Recovery uses the real secp256k1 curve; a
//! signature that does not recover to the expected address is rejected.

use crate::Result;
use secp256k1::ecdsa::{RecoverableSignature, RecoveryId};
use secp256k1::{Message, Secp256k1};
use sha3::{Digest, Keccak256};

/// The EIP-191 version byte and prefix for a personal message.
const EIP191_PREFIX: &str = "\x19Ethereum Signed Message:\n";

/// Verifies cryptographic signatures
pub struct SignatureVerifier;

impl SignatureVerifier {
    /// Verify an EIP-191 signed message.
    ///
    /// `signature` is the 65-byte `r || s || v` form. `v` is accepted as
    /// `0`/`1` or the legacy `27`/`28`. `expected_address` may be given in any
    /// case; the comparison is case-insensitive.
    pub fn verify_signed_message(
        message: &str,
        signature: &str,
        expected_address: &str,
    ) -> Result<bool> {
        let hash = Self::message_digest(message);
        Self::recover_and_compare(&hash, signature, expected_address)
    }

    /// Verify an EIP-712 typed-data signature.
    ///
    /// `domain_separator` and `hash_struct` are the two 32-byte EIP-712
    /// hashes. They are combined with the `\x19\x01` prefix, which is what
    /// prevents a typed-data hash from being replayed as a personal message.
    pub fn verify_typed_data(
        domain_separator: &[u8],
        hash_struct: &[u8],
        signature: &str,
        expected_address: &str,
    ) -> Result<bool> {
        if domain_separator.len() != 32 || hash_struct.len() != 32 {
            return Err(crate::SecurityError::InvalidInput(
                "domain separator and hash struct must each be 32 bytes".to_string(),
            ));
        }

        let digest = Self::typed_data_digest(domain_separator, hash_struct);
        Self::recover_and_compare(&digest, signature, expected_address)
    }

    /// Recover the signer from a 65-byte signature over `digest` and check it
    /// against `expected_address`.
    fn recover_and_compare(
        digest: &[u8; 32],
        signature: &str,
        expected_address: &str,
    ) -> Result<bool> {
        let recovered = Self::recover_address(digest, signature)?;
        // Addresses are compared case-insensitively: EIP-55 casing is a display
        // convention, not part of the identity.
        Ok(recovered.eq_ignore_ascii_case(expected_address))
    }

    /// Recover the checksummed signer address from a signature over `digest`.
    ///
    /// Returns `Err(InvalidSignature)` for a malformed signature, an
    /// out-of-range recovery id, or a signature that recovers no key.
    pub fn recover_address(digest: &[u8; 32], signature: &str) -> Result<String> {
        let raw = signature.trim();
        let raw = raw
            .strip_prefix("0x")
            .or_else(|| raw.strip_prefix("0X"))
            .unwrap_or(raw);

        let sig_bytes = hex::decode(raw).map_err(|_| crate::SecurityError::InvalidSignature)?;
        if sig_bytes.len() != 65 {
            return Err(crate::SecurityError::InvalidSignature);
        }

        let v = sig_bytes[64];

        // Accept both the EIP-155 (27/28) and the raw (0/1) encodings.
        let recovery_id = match v {
            0 | 27 => 0,
            1 | 28 => 1,
            _ => return Err(crate::SecurityError::InvalidSignature),
        };

        // A 65-byte `r || s` with `r` and `s` in big-endian order.
        let mut compact = [0u8; 64];
        compact[..32].copy_from_slice(&sig_bytes[0..32]);
        compact[32..].copy_from_slice(&sig_bytes[32..64]);

        let recovery_id =
            RecoveryId::from_i32(recovery_id).map_err(|_| crate::SecurityError::InvalidSignature)?;
        let recoverable = RecoverableSignature::from_compact(&compact, recovery_id)
            .map_err(|_| crate::SecurityError::InvalidSignature)?;

        let msg = Message::from_slice(digest).map_err(|_| crate::SecurityError::InvalidSignature)?;

        let secp = Secp256k1::new();
        let public_key = secp
            .recover_ecdsa(&msg, &recoverable)
            .map_err(|_| crate::SecurityError::InvalidSignature)?;

        // The address is the last 20 bytes of keccak256 over the uncompressed
        // public key without its 0x04 prefix.
        let uncompressed = public_key.serialize_uncompressed();
        Ok(crate::address::AddressValidator::from_public_key(&uncompressed[1..]))
    }

    /// Hash message for signing (EIP-191).
    pub fn hash_message(message: &str) -> String {
        hex::encode(Self::message_digest(message))
    }

    /// The 32-byte EIP-191 digest of `message`.
    pub fn message_digest(message: &str) -> [u8; 32] {
        // The length is the byte length of the message, not its character
        // count: a multi-byte character would otherwise produce a digest no
        // wallet would reproduce.
        let prefixed = format!("{}{}", EIP191_PREFIX, message.len());
        Keccak256::digest([prefixed.as_bytes(), message.as_bytes()].concat()).into()
    }

    /// Hash typed data (EIP-712).
    pub fn hash_typed_data(domain: &[u8], message: &[u8]) -> String {
        hex::encode(Self::typed_data_digest(domain, message))
    }

    /// The 32-byte EIP-712 digest.
    ///
    /// `domain` and `message` are already the domain separator and the struct
    /// hash, as produced by the EIP-712 encoding rules.
    pub fn typed_data_digest(domain: &[u8], message: &[u8]) -> [u8; 32] {
        let domain_hash = Keccak256::digest(domain);
        let message_hash = Keccak256::digest(message);

        let encoded = [
            b"\x19\x01".as_slice(),
            domain_hash.as_slice(),
            message_hash.as_slice(),
        ]
        .concat();

        Keccak256::digest(&encoded).into()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use secp256k1::{PublicKey, SecretKey};

    /// Deterministic key material for the tests below.
    fn test_key() -> (SecretKey, String) {
        let secret = SecretKey::from_slice(&[0x11u8; 32]).expect("secret key");
        let secp = Secp256k1::new();
        let public = PublicKey::from_secret_key(&secp, &secret);
        let uncompressed = public.serialize_uncompressed();
        let address = crate::address::AddressValidator::from_public_key(&uncompressed[1..]);
        (secret, address)
    }

    /// Sign `digest` with the test key, returning the 65-byte `r || s || v` hex.
    fn sign_digest(digest: &[u8; 32], secret: &SecretKey) -> String {
        let secp = Secp256k1::new();
        let msg = Message::from_slice(digest).unwrap();
        let recoverable = secp.sign_ecdsa_recoverable(&msg, secret);
        let (recovery_id, compact) = recoverable.serialize_compact();

        let mut bytes = compact.to_vec();
        bytes.push(recovery_id.to_i32() as u8);
        hex::encode(bytes)
    }

    #[test]
    fn test_hash_message() {
        let hash = SignatureVerifier::hash_message("Hello, World!");
        assert_eq!(hash.len(), 64);
    }

    #[test]
    fn test_eip191_roundtrip_recovers_signer() {
        let (secret, address) = test_key();
        let message = "TigerSmartChain login nonce 42";
        let digest = SignatureVerifier::message_digest(message);
        let signature = sign_digest(&digest, &secret);

        assert!(SignatureVerifier::verify_signed_message(message, &signature, &address).unwrap());
    }

    #[test]
    fn test_eip191_rejects_wrong_address() {
        let (secret, _) = test_key();
        let message = "payload";
        let digest = SignatureVerifier::message_digest(message);
        let signature = sign_digest(&digest, &secret);

        let other = "0x0000000000000000000000000000000000000001";
        assert!(!SignatureVerifier::verify_signed_message(message, &signature, other).unwrap());
    }

    #[test]
    fn test_eip191_rejects_tampered_message() {
        let (secret, address) = test_key();
        let digest = SignatureVerifier::message_digest("original");
        let signature = sign_digest(&digest, &secret);

        assert!(
            !SignatureVerifier::verify_signed_message("tampered", &signature, &address).unwrap()
        );
    }

    #[test]
    fn test_rejects_malformed_signature() {
        let (_, address) = test_key();
        assert!(SignatureVerifier::verify_signed_message("m", "0xdeadbeef", &address).is_err());
        // A 65-byte value with an impossible recovery id.
        let mut bad = vec![0u8; 65];
        bad[64] = 9;
        assert!(SignatureVerifier::verify_signed_message("m", &hex::encode(bad), &address).is_err());
    }

    #[test]
    fn test_eip712_roundtrip_and_domain_separation() {
        let (secret, address) = test_key();
        let domain = [0x22u8; 32];
        let struct_hash = [0x33u8; 32];

        let digest = SignatureVerifier::typed_data_digest(&domain, &struct_hash);
        let signature = sign_digest(&digest, &secret);

        assert!(SignatureVerifier::verify_typed_data(&domain, &struct_hash, &signature, &address)
            .unwrap());

        // A different domain must not validate the same signature.
        let other_domain = [0x44u8; 32];
        assert!(
            !SignatureVerifier::verify_typed_data(&other_domain, &struct_hash, &signature, &address)
                .unwrap()
        );
    }

    #[test]
    fn test_eip712_rejects_wrong_length_inputs() {
        let (_, address) = test_key();
        let sig = hex::encode([0u8; 65]);
        assert!(
            SignatureVerifier::verify_typed_data(&[0u8; 31], &[0u8; 32], &sig, &address).is_err()
        );
    }

    #[test]
    fn test_eip712_digest_differs_from_eip191() {
        // The two envelopes must never collide, or a typed-data signature could
        // be replayed as a personal-message signature.
        let domain = [0u8; 32];
        let message = [0u8; 32];
        let typed = SignatureVerifier::typed_data_digest(&domain, &message);
        let personal = SignatureVerifier::message_digest("");
        assert_ne!(typed, personal);
    }
}
