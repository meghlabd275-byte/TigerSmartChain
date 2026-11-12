//! ENS name hashing.
//!
//! ENS uses `namehash` (EIP-137): the node for a name is built by hashing each
//! label from right to left, starting from the empty node. The label hash is
//! `keccak256(label)`, not SHA-256, so that a name hashes to the same value as
//! the registry contract computes on chain.

use ethers::core::utils::keccak256;

/// The node of the empty name, all zero bytes.
pub const EMPTY_NODE: [u8; 32] = [0u8; 32];

/// `keccak256(label)`.
pub fn label_hash(label: &str) -> [u8; 32] {
    keccak256(label.as_bytes())
}

/// The `namehash` of `name` (EIP-137).
///
/// Empty labels are skipped, so `foo..eth` and `foo.eth` produce the same node.
pub fn namehash(name: &str) -> [u8; 32] {
    if name.is_empty() {
        return EMPTY_NODE;
    }

    let mut node = EMPTY_NODE;

    for label in name.split('.').rev() {
        if label.is_empty() {
            continue;
        }

        let mut buffer = Vec::with_capacity(64);
        buffer.extend_from_slice(&node);
        buffer.extend_from_slice(&label_hash(label));
        node = keccak256(buffer);
    }

    node
}

/// The reverse node for `address`: `<lowercase-hex>.addr.reverse`.
pub fn reverse_name(address: &str) -> String {
    format!(
        "{}.addr.reverse",
        address.trim_start_matches("0x").to_ascii_lowercase()
    )
}

/// Validate an ENS name.
///
/// Rejects empty names, names longer than 255 bytes, labels longer than 63
/// bytes, and characters outside the LDH set plus underscore.
pub fn validate_name(name: &str) -> bool {
    if name.is_empty() || name.len() > 255 {
        return false;
    }

    for label in name.split('.') {
        if label.is_empty() || label.len() > 63 {
            return false;
        }

        if !label
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
        {
            return false;
        }
    }

    true
}

/// Validate a 20-byte hex address.
pub fn validate_address(address: &str) -> bool {
    address.len() == 42
        && address.starts_with("0x")
        && address[2..].chars().all(|c| c.is_ascii_hexdigit())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hex(bytes: &[u8]) -> String {
        bytes.iter().map(|byte| format!("{byte:02x}")).collect()
    }

    #[test]
    fn the_empty_name_hashes_to_the_zero_node() {
        assert_eq!(hex(&namehash("")), hex(&EMPTY_NODE));
    }

    #[test]
    fn namehash_matches_the_published_vectors() {
        // EIP-137 reference values.
        assert_eq!(
            hex(&namehash("eth")),
            "93cdeb708b7545dc668eb9280176169d1c33cfd8ed6f04690a0bcc88a93fc4ae"
        );
        assert_eq!(
            hex(&namehash("foo.eth")),
            "de9b09fd7c5f901e23a3f19fecc54828e9c848539801e86591bd9801b019f84f"
        );
    }

    #[test]
    fn a_trailing_dot_does_not_change_the_node() {
        assert_eq!(hex(&namehash("foo.eth.")), hex(&namehash("foo.eth")));
    }

    #[test]
    fn label_hash_is_keccak_not_sha256() {
        assert_eq!(
            hex(&label_hash("eth")),
            "4f5b812789fc606be1b3b16908db13fc7a9adf7ca72641f84d75b47069d3d7f0"
        );
    }

    #[test]
    fn name_validation_rejects_malformed_names() {
        assert!(validate_name("foo.eth"));
        assert!(validate_name("eth"));
        assert!(!validate_name(""));
        assert!(!validate_name(".eth"));
        assert!(!validate_name("foo..eth"));
        assert!(!validate_name("foo bar.eth"));
        assert!(!validate_name(&format!("{}.eth", "a".repeat(64))));
    }

    #[test]
    fn address_validation_is_strict() {
        assert!(validate_address("0x0000000000000000000000000000000000000000"));
        assert!(!validate_address("0000000000000000000000000000000000000000"));
        assert!(!validate_address("0x00000000000000000000000000000000000000"));
        assert!(!validate_address("0xzz00000000000000000000000000000000000000"));
    }

    #[test]
    fn the_reverse_name_is_lowercase_without_the_prefix() {
        assert_eq!(
            reverse_name("0xAbC0000000000000000000000000000000000001"),
            "abc0000000000000000000000000000000000001.addr.reverse"
        );
    }
}
