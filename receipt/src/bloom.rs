//! Block bloom filters.
//!
//! A bloom filter is 2048 bits (256 bytes). For each element — a log address or
//! a log topic — three bits are set, taken from pairs of bytes of the
//! element's Keccak-256 hash: `(h[2i] << 8 | h[2i+1]) % 2048` for `i` in
//! `0, 2, 4`.
//!
//! This is what makes `eth_getLogs` over a block range cheap: a client reads
//! the 256-byte bloom stored in each block header and discards every block that
//! cannot contain the topic it wants.

use crate::types::{address_to_word, Log};
use serde::{Deserialize, Serialize};
use sha3::{Digest, Keccak256};

/// Number of bits in a bloom filter.
pub const BLOOM_BITS: usize = 2048;

/// Number of bits set per element.
pub const BITS_PER_ELEMENT: usize = 3;

/// A 2048-bit bloom filter.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Bloom {
    #[serde(with = "hex_bytes")]
    pub data: Vec<u8>,
}

impl Bloom {
    /// An empty filter, with every bit clear.
    pub fn new() -> Self {
        Self {
            data: vec![0u8; BLOOM_BITS / 8],
        }
    }

    /// Build a filter over `elements`.
    pub fn from_elements(elements: &[[u8; 32]]) -> Self {
        let mut bloom = Self::new();
        for element in elements {
            for bit in bloom_bits(element) {
                bloom.set(bit);
            }
        }
        bloom
    }

    /// Build a filter covering every address and topic of `logs`.
    pub fn from_logs(logs: &[Log]) -> Self {
        let mut bloom = Self::new();
        for log in logs {
            bloom.accrue_log(log);
        }
        bloom
    }

    /// Fold one log into this filter.
    pub fn accrue_log(&mut self, log: &Log) {
        for element in log.bloom_elements() {
            for bit in bloom_bits(&element) {
                self.set(bit);
            }
        }
    }

    /// Set one bit.
    pub fn set(&mut self, bit: usize) {
        let byte = (bit % BLOOM_BITS) / 8;
        let offset = bit % 8;
        self.data[byte] |= 1 << (7 - offset);
    }

    /// Test one bit.
    pub fn test(&self, bit: usize) -> bool {
        let byte = (bit % BLOOM_BITS) / 8;
        let offset = bit % 8;
        self.data[byte] & (1 << (7 - offset)) != 0
    }

    /// True when the filter contains `element`.
    ///
    /// A `false` result is definitive: bloom filters never miss an element that
    /// was added. A `true` result means "possibly present".
    pub fn contains(&self, element: &[u8; 32]) -> bool {
        bloom_bits(element).iter().all(|bit| self.test(*bit))
    }

    /// True when this filter contains every element of `other`.
    pub fn contains_bloom(&self, other: &Bloom) -> bool {
        self.data
            .iter()
            .zip(other.data.iter())
            .all(|(mine, theirs)| mine & theirs == *theirs)
    }

    /// Parse a filter from the 512-character hex string used by JSON-RPC.
    pub fn from_hex(input: &str) -> Result<Self, String> {
        let trimmed = input.trim_start_matches("0x").trim_start_matches("0X");
        if trimmed.len() != BLOOM_BITS / 4 {
            return Err(format!(
                "bloom must be {} hex characters, got {}",
                BLOOM_BITS / 4,
                trimmed.len()
            ));
        }

        let mut data = Vec::with_capacity(BLOOM_BITS / 8);
        for pair in trimmed.as_bytes().chunks(2) {
            let high = (pair[0] as char)
                .to_digit(16)
                .ok_or_else(|| format!("invalid hex digit {:?}", pair[0] as char))?;
            let low = (pair[1] as char)
                .to_digit(16)
                .ok_or_else(|| format!("invalid hex digit {:?}", pair[1] as char))?;
            data.push(((high << 4) | low) as u8);
        }

        Ok(Self { data })
    }

    /// Render as the `0x`-prefixed hex string used by JSON-RPC.
    pub fn to_hex(&self) -> String {
        let mut out = String::with_capacity(2 + self.data.len() * 2);
        out.push_str("0x");
        for byte in &self.data {
            out.push_str(&format!("{byte:02x}"));
        }
        out
    }

    /// An address-only filter, matching how a `Log` carries its address.
    pub fn from_address(address: &str) -> Self {
        Self::from_elements(&[address_to_word(address)])
    }
}

impl Default for Bloom {
    fn default() -> Self {
        Self::new()
    }
}

/// The three bit positions an element sets in a bloom filter.
pub fn bloom_bits(element: &[u8; 32]) -> [usize; BITS_PER_ELEMENT] {
    let hash = Keccak256::digest(element);
    let mut bits = [0usize; BITS_PER_ELEMENT];

    for (index, slot) in bits.iter_mut().enumerate() {
        let pair = index * 2;
        *slot = (((hash[pair] as usize) << 8) | hash[pair + 1] as usize) % BLOOM_BITS;
    }

    bits
}

/// Serde helper so the byte vector round-trips as a plain JSON array.
mod hex_bytes {
    use serde::{Deserialize, Deserializer, Serializer};

    pub fn serialize<S: Serializer>(bytes: &[u8], serializer: S) -> Result<S::Ok, S::Error> {
        serializer.collect_seq(bytes)
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Vec<u8>, D::Error> {
        Vec::<u8>::deserialize(deserializer)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_empty_bloom_has_no_bits_set() {
        let bloom = Bloom::new();
        assert_eq!(bloom.data.len(), 256);
        assert!(bloom.data.iter().all(|byte| *byte == 0));
        assert_eq!(bloom.to_hex().len(), 514);
    }

    #[test]
    fn bit_positions_follow_the_yellow_paper_rule() {
        // keccak256(0x00 * 32) =
        // 290decd9548b62a8d60345a988386fc84ba6bc95484008f6362f93160ef3e563
        let element = [0u8; 32];
        assert_eq!(bloom_bits(&element), [269, 1241, 1163]);
    }

    #[test]
    fn a_zero_address_sets_three_bits() {
        let bloom = Bloom::from_address("0x0000000000000000000000000000000000000000");

        let set_bits: usize = bloom
            .data
            .iter()
            .map(|byte| byte.count_ones() as usize)
            .sum();
        assert_eq!(set_bits, 3);

        for bit in [269, 1241, 1163] {
            assert!(bloom.test(bit), "bit {bit} should be set");
        }
    }

    #[test]
    fn contains_never_misses_an_added_element() {
        let element = address_to_word("0xdeadbeefdeadbeefdeadbeefdeadbeefdeadbeef");
        let bloom = Bloom::from_elements(&[element]);
        assert!(bloom.contains(&element));
    }

    #[test]
    fn hex_round_trip_is_lossless() {
        let bloom = Bloom::from_address("0x1111111111111111111111111111111111111111");
        let hex = bloom.to_hex();
        assert_eq!(Bloom::from_hex(&hex).unwrap(), bloom);
    }

    #[test]
    fn malformed_hex_is_rejected() {
        assert!(Bloom::from_hex("0x00").is_err());
        assert!(Bloom::from_hex(&"0x".to_string().repeat(1)).is_err());
        assert!(Bloom::from_hex(&format!("0x{}", "zz".repeat(256))).is_err());
    }

    #[test]
    fn a_log_sets_bits_for_its_address_and_every_topic() {
        let log = Log {
            address: "0x1111111111111111111111111111111111111111".to_string(),
            topics: vec![
                "0xddf252ad1be2c89b69c2b068fc378daa952ba7f163c4a11628f55a4df523b3ef".to_string(),
            ],
            data: "0x".to_string(),
            log_index: 0,
            transaction_index: 0,
            transaction_hash: "0x0".to_string(),
            block_number: 1,
            block_hash: "0x0".to_string(),
            removed: false,
        };

        let bloom = log.bloom();
        assert!(log.may_match(&bloom));

        // A different address must not match a bloom built only from this log.
        let other = Log {
            address: "0x2222222222222222222222222222222222222222".to_string(),
            ..log.clone()
        };
        assert!(!other.may_match(&bloom));
    }

    #[test]
    fn merging_logs_keeps_every_element_reachable() {
        let make = |address: &str, topic: &str| Log {
            address: address.to_string(),
            topics: vec![topic.to_string()],
            data: "0x".to_string(),
            log_index: 0,
            transaction_index: 0,
            transaction_hash: "0x0".to_string(),
            block_number: 1,
            block_hash: "0x0".to_string(),
            removed: false,
        };

        let a = make("0x1111111111111111111111111111111111111111", "0xaa");
        let b = make("0x2222222222222222222222222222222222222222", "0xbb");

        let bloom = Bloom::from_logs(&[a.clone(), b.clone()]);
        assert!(a.may_match(&bloom));
        assert!(b.may_match(&bloom));
        assert!(bloom.contains_bloom(&a.bloom()));
        assert!(bloom.contains_bloom(&b.bloom()));
    }
}
