//! Address validation and verification module
//!
//! Addresses on this chain are Ethereum-compatible: 20 bytes, usually written
//! as `0x` followed by 40 hex digits. EIP-55 defines an optional mixed-case
//! encoding that carries a checksum, which this module produces and verifies.

use lazy_static::lazy_static;
use regex::Regex;
use sha3::{Digest, Keccak256};

lazy_static! {
    static ref ADDRESS_REGEX: Regex = Regex::new(r"^0x[a-fA-F0-9]{40}$").unwrap();
}

/// Validates and verifies blockchain addresses
pub struct AddressValidator;

impl AddressValidator {
    /// Validate Ethereum-style address format
    pub fn is_valid(address: &str) -> bool {
        ADDRESS_REGEX.is_match(address)
    }

    /// Verify an address carries a correct EIP-55 checksum.
    ///
    /// A uniformly-cased address (all lower or all upper) is *not* checksummed:
    /// it carries no information to verify. This returns `false` for such an
    /// address, and for an address with no letters at all, rather than
    /// reporting a passing check that was never performed.
    pub fn verify_checksum(address: &str) -> bool {
        if !ADDRESS_REGEX.is_match(address) {
            return false;
        }

        let addr = address.trim_start_matches("0x");
        if !addr.chars().any(|c| c.is_ascii_alphabetic()) {
            // All digits: no casing exists to verify.
            return false;
        }
        if addr == addr.to_lowercase() || addr == addr.to_uppercase() {
            // Uniformly cased: the caller did not apply EIP-55.
            return false;
        }

        // The hash is taken over the lower-case hex, and the i-th nibble of the
        // hash decides the case of the i-th character.
        let hash_bytes = Self::keccak256_bytes(&addr.to_lowercase());

        for (i, c) in addr.chars().enumerate() {
            if !c.is_ascii_alphabetic() {
                continue;
            }
            let nibble = if i % 2 == 0 {
                hash_bytes[i / 2] >> 4
            } else {
                hash_bytes[i / 2] & 0x0F
            };

            let should_be_upper = nibble >= 8;
            if should_be_upper != c.is_ascii_uppercase() {
                return false;
            }
        }

        true
    }

    /// Convert an address to its EIP-55 checksummed form.
    ///
    /// An input that is not a well-formed address is returned unchanged.
    pub fn to_checksum(address: &str) -> String {
        let addr = address.trim_start_matches("0x").to_lowercase();
        if addr.len() != 40 || !addr.chars().all(|c| c.is_ascii_hexdigit()) {
            return address.to_string();
        }

        let hash_bytes = Self::keccak256_bytes(&addr);

        let mut result = String::with_capacity(42);
        result.push_str("0x");

        for (i, c) in addr.chars().enumerate() {
            let nibble = if i % 2 == 0 {
                hash_bytes[i / 2] >> 4
            } else {
                hash_bytes[i / 2] & 0x0F
            };

            if nibble >= 8 {
                result.push(c.to_ascii_uppercase());
            } else {
                result.push(c);
            }
        }

        result
    }

    /// Whether an address holds code, given the result of an `eth_getCode`
    /// call.
    ///
    /// This cannot be answered from local data: it needs a node. Rather than
    /// guess, callers must supply the code. An empty result (or the literal
    /// `"0x"`) means the address has no code and is an externally-owned
    /// account.
    pub fn is_contract_with_code(code: &[u8]) -> bool {
        !code.is_empty() && code != b"0x"
    }

    /// Generate the checksummed address for the 64-byte body of an
    /// uncompressed public key.
    ///
    /// The input is the key without its leading `0x04` byte. Any other length
    /// yields an empty string rather than a bogus address derived from a
    /// truncated key.
    pub fn from_public_key(public_key: &[u8]) -> String {
        if public_key.len() != 64 {
            return String::new();
        }
        let hash = Keccak256::digest(public_key);
        let address_bytes = &hash[12..];
        Self::to_checksum(&format!("0x{}", hex::encode(address_bytes)))
    }

    /// Generate the checksummed address for a secp256k1 private key.
    pub fn from_private_key(private_key: &[u8]) -> Option<String> {
        use secp256k1::{Secp256k1, SecretKey};

        let secp = Secp256k1::new();
        let secret = SecretKey::from_slice(private_key).ok()?;
        let public_key = secp256k1::PublicKey::from_secret_key(&secp, &secret);

        let uncompressed = public_key.serialize_uncompressed();
        // Skip the first byte (0x04) for an uncompressed key.
        let address = Self::from_public_key(&uncompressed[1..]);
        if address.is_empty() {
            None
        } else {
            Some(address)
        }
    }

    fn keccak256_bytes(data: &str) -> [u8; 32] {
        Keccak256::digest(data.as_bytes()).into()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_address_validation() {
        assert!(AddressValidator::is_valid("0x742d35Cc6634C0532925a3b844Bc9e7595f0eB1E"));
        assert!(!AddressValidator::is_valid("0x742d35Cc6634C0532925a3b844Bc9e7595f0eB1"));
        assert!(!AddressValidator::is_valid("not-an-address"));
    }

    /// The EIP-55 specification's own test vectors.
    #[test]
    fn test_eip55_spec_vectors() {
        let vectors = [
            "0x5aAeb6053F3E94C9b9A09f33669435E7Ef1BeAed",
            "0xfB6916095ca1df60bB79Ce92cE3Ea74c37c5d359",
            "0xdbF03B407c01E7cD3CBea99509d93f8DDDC8C6FB",
            "0xD1220A0cf47c7B9Be7A2E6BA89F429762e7b9aDb",
        ];

        for expected in vectors {
            let lowered = expected.to_lowercase();
            assert_eq!(AddressValidator::to_checksum(&lowered), expected);
            assert!(AddressValidator::verify_checksum(expected));
        }
    }

    #[test]
    fn test_checksum_rejects_unchecksummed_address() {
        // Uniformly cased input is not a checksum, so it must not pass.
        assert!(!AddressValidator::verify_checksum(
            "0x5aaeb6053f3e94c9b9a09f33669435e7ef1beaed"
        ));
        // An all-digit address has no casing to check.
        assert!(!AddressValidator::verify_checksum(
            "0x1111111111111111111111111111111111111111"
        ));
    }

    #[test]
    fn test_checksum_rejects_flipped_case() {
        // Take a valid checksummed address and flip one letter's case.
        let valid = "0x5aAeb6053F3E94C9b9A09f33669435E7Ef1BeAed";
        let broken = "0x5aaeb6053F3E94C9b9A09f33669435E7Ef1BeAed";
        assert!(AddressValidator::verify_checksum(valid));
        assert!(!AddressValidator::verify_checksum(broken));
    }

    #[test]
    fn test_checksum_roundtrip() {
        let addr = "0x742d35cc6634c0532925a3b844bc9e7595f0eb1e";
        let checksum = AddressValidator::to_checksum(addr);
        assert!(AddressValidator::verify_checksum(&checksum));
        // Round-tripping the checksummed form is stable.
        assert_eq!(AddressValidator::to_checksum(&checksum), checksum);
    }

    #[test]
    fn test_from_private_key_matches_known_vector() {
        // The well-known key 0x...01 has a fixed, widely published address.
        let mut key = [0u8; 32];
        key[31] = 1;
        let address = AddressValidator::from_private_key(&key).expect("valid key");
        assert_eq!(
            address.to_lowercase(),
            "0x7e5f4552091a69125d5dfcb7b8c2659029395bdf"
        );
        // It is emitted checksummed.
        assert!(AddressValidator::verify_checksum(&address));
    }

    #[test]
    fn test_from_private_key_rejects_invalid_key() {
        // Zero is not a valid secp256k1 scalar.
        assert!(AddressValidator::from_private_key(&[0u8; 32]).is_none());
        assert!(AddressValidator::from_private_key(&[1u8; 8]).is_none());
    }

    #[test]
    fn test_from_public_key_rejects_wrong_length() {
        assert_eq!(AddressValidator::from_public_key(&[0u8; 63]), "");
        assert_eq!(AddressValidator::from_public_key(&[0u8; 65]), "");
    }

    #[test]
    fn test_is_contract_with_code() {
        assert!(!AddressValidator::is_contract_with_code(b""));
        assert!(!AddressValidator::is_contract_with_code(b"0x"));
        assert!(AddressValidator::is_contract_with_code(&[0x60, 0x80, 0x60, 0x40]));
    }
}
