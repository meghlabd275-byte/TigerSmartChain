//! TigerSmartChain name service (ENS-compatible).
//!
//! Names resolve through EIP-137 namehash, so a name hashes identically here
//! and in any other ENS implementation.

pub mod types;

pub use types::*;

/// Chain id this registry resolves against.
pub const CHAIN_ID: u64 = 6666;

#[cfg(test)]
mod tests {
    use super::*;

    fn hex_namehash(name: &str) -> String {
        hex::encode(namehash(name))
    }

    #[test]
    fn namehash_matches_eip137_vectors() {
        // Reference values, independently computed with keccak256.
        assert_eq!(
            hex_namehash(""),
            "0000000000000000000000000000000000000000000000000000000000000000"
        );
        assert_eq!(
            hex_namehash("eth"),
            "93cdeb708b7545dc668eb9280176169d1c33cfd8ed6f04690a0bcc88a93fc4ae"
        );
        assert_eq!(
            hex_namehash("foo.eth"),
            "de9b09fd7c5f901e23a3f19fecc54828e9c848539801e86591bd9801b019f84f"
        );
        assert_eq!(
            hex_namehash("tiger.eth"),
            "6af23a298cef3160d5a41bdb3294a3d690d1de28b9427b589e1eb17fa6815be6"
        );
        assert_eq!(
            hex_namehash("a.b.c"),
            "257d8d183501fdaabc229b5b3b63dc77d8dc3685dc5623e91f472e7f2e443c5a"
        );
    }

    #[test]
    fn namehash_is_a_keccak_construction_not_siphash() {
        // SipHash is not stable across Rust releases; keccak256("") is fixed.
        assert_eq!(
            hex::encode(keccak256(b"")),
            "c5d2460186f7233c927e7db2dcc703c0e500b653ca82273b7bfad8045d85a470"
        );
        // Subdomains must depend on the parent.
        assert_ne!(namehash("a.b.c"), namehash("b.c"));
    }

    #[test]
    fn the_registry_resolves_against_this_chain() {
        assert_eq!(CHAIN_ID, 6666);
        let resolver = Resolver::new("0x1111111111111111111111111111111111111111".to_string());
        assert_eq!(resolver.chain_id, CHAIN_ID);
        assert!(resolver.supports(&RecordType::Addr));
        assert!(resolver.supports(&RecordType::Abi));
    }

    #[test]
    fn registration_resolution_and_transfer_round_trip() {
        let mut registry = Registry::new();
        assert!(registry.available("tiger.eth"));

        let mut record = Record::new("tiger.eth".to_string(), "0xowner".to_string());
        record.address = Some("0x2222222222222222222222222222222222222222".to_string());
        registry.set_record(record);

        assert!(!registry.available("tiger.eth"));
        assert_eq!(
            registry.resolve("tiger.eth").unwrap(),
            "0x2222222222222222222222222222222222222222"
        );
        assert_eq!(registry.owner("tiger.eth").as_deref(), Some("0xowner"));

        registry.transfer("tiger.eth", "0xnew".to_string()).unwrap();
        assert_eq!(registry.owner("tiger.eth").as_deref(), Some("0xnew"));

        // An unknown name resolves to an error, not to an empty address.
        assert!(registry.resolve("nope.eth").is_err());
        assert!(registry.transfer("nope.eth", "0x1".to_string()).is_err());
    }

    #[test]
    fn a_record_without_an_address_does_not_resolve() {
        let mut registry = Registry::new();
        registry.set_record(Record::new("bare.eth".to_string(), "0xowner".to_string()));
        assert!(registry.resolve("bare.eth").is_err());
    }
}
