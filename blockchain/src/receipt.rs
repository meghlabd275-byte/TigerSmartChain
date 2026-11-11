//! Transaction receipts: RLP encoding, the 2048-bit logs bloom filter, and
//! the receipts trie root.
//!
//! These are consensus-critical. A block's `receipts_root` and `logs_bloom` are
//! part of the header that every other client hashes, so the encoding here has
//! to match the Yellow Paper byte for byte:
//!
//!   Receipt = [status, cumulativeGasUsed, bloom, [Log, ...]]
//!   Log     = [address, [topic, ...], data]
//!
//! The bloom filter is 2048 bits. For every address and topic in a receipt,
//! three bits are set at positions derived from the low 11 bits of three
//! 16-bit words of the value's keccak256 hash.

use crate::block::{hex_encode, ValidationError};
use tiger_state::rlp::RlpItem;
use tiger_state::trie::Trie;

/// A log as it appears in a receipt: no transaction or block context, because
/// that is supplied by the surrounding receipt and block.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReceiptLog {
    /// Contract that emitted the log.
    pub address: [u8; 20],
    /// Indexed topics, in order.
    pub topics: Vec<[u8; 32]>,
    /// Unindexed data payload.
    pub data: Vec<u8>,
}

/// A transaction receipt.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Receipt {
    /// Whether the transaction succeeded.
    pub status: bool,
    /// Gas used by this transaction plus every earlier transaction in the
    /// block, which is what the receipt commits to.
    pub cumulative_gas_used: u64,
    /// Gas used by this transaction alone, reported by `eth_getTransactionReceipt`.
    pub gas_used: u64,
    /// Logs emitted by the transaction.
    pub logs: Vec<ReceiptLog>,
}

impl Receipt {
    /// The 2048-bit bloom filter this receipt contributes to the block's.
    pub fn bloom(&self) -> [u8; 256] {
        let mut bloom = [0u8; 256];
        for log in &self.logs {
            bloom_insert(&mut bloom, &log.address);
            for topic in &log.topics {
                bloom_insert(&mut bloom, topic);
            }
        }
        bloom
    }

    /// RLP-encode the receipt exactly as the Yellow Paper specifies.
    pub fn encode(&self) -> Vec<u8> {
        let bloom = self.bloom();
        let logs: Vec<RlpItem> = self
            .logs
            .iter()
            .map(|log| {
                RlpItem::list(vec![
                    RlpItem::bytes(log.address.to_vec()),
                    RlpItem::list(
                        log.topics.iter().map(|t| RlpItem::bytes(t.to_vec())).collect(),
                    ),
                    RlpItem::bytes(log.data.clone()),
                ])
            })
            .collect();

        RlpItem::list(vec![
            // Post-Byzantium, the first field is a status code, not a state
            // root: 1 for success, 0 for a revert.
            RlpItem::bytes(vec![u8::from(self.status)]),
            RlpItem::from_u64(self.cumulative_gas_used),
            RlpItem::bytes(bloom.to_vec()),
            RlpItem::list(logs),
        ])
        .encode()
    }
}

/// Set the three bloom bits for `value`.
///
/// The positions are the low 11 bits of the first three 16-bit words of the
/// value's keccak256, matching every Ethereum client.
fn bloom_insert(bloom: &mut [u8; 256], value: &[u8]) {
    let hash = tiger_crypto::keccak256(value);
    for i in 0..3 {
        let bit = (((hash[i * 2] as usize) << 8) | hash[i * 2 + 1] as usize) & 2047;
        // Bit numbering is big-endian: byte 0 holds the most significant bits.
        bloom[256 - 1 - bit / 8] |= 1 << (bit % 8);
    }
}

/// The bloom filter of a whole block: the bitwise OR of its receipts' blooms.
pub fn logs_bloom(receipts: &[Receipt]) -> [u8; 256] {
    let mut bloom = [0u8; 256];
    for receipt in receipts {
        let receipt_bloom = receipt.bloom();
        for (byte, value) in bloom.iter_mut().zip(receipt_bloom.iter()) {
            *byte |= *value;
        }
    }
    bloom
}

/// The receipts trie root: a trie keyed by `rlp(index)` whose values are the
/// encoded receipts.
pub fn receipts_root(receipts: &[Receipt]) -> [u8; 32] {
    if receipts.is_empty() {
        return crate::block::EMPTY_TRIE_ROOT;
    }
    let mut trie = Trie::new();
    for (index, receipt) in receipts.iter().enumerate() {
        let key = RlpItem::from_u64(index as u64).encode();
        trie.insert(&key, &receipt.encode());
    }
    trie.root_hash()
}

/// Render a bloom filter as a `0x`-prefixed 256-byte hex string.
pub fn bloom_hex(bloom: &[u8; 256]) -> String {
    format!("0x{}", hex_encode(bloom))
}

/// Encode a receipt for the `eth_getTransactionReceipt` response's
/// `logsBloom` field, verifying the bloom is well formed.
pub fn validate_bloom(value: &str) -> Result<(), ValidationError> {
    crate::block::decode_hex_field(value, Some(256)).map(|_| ())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn log(address_byte: u8, topic_byte: u8) -> ReceiptLog {
        ReceiptLog {
            address: [address_byte; 20],
            topics: vec![[topic_byte; 32]],
            data: vec![0xaa, 0xbb],
        }
    }

    #[test]
    fn empty_receipts_root_is_the_empty_trie_root() {
        assert_eq!(receipts_root(&[]), crate::block::EMPTY_TRIE_ROOT);
        assert_eq!(
            hex_encode(&receipts_root(&[])),
            "56e81f171bcc55a6ff8345e692c0f86e5b48e01b996cadc001622fb5e363b421"
        );
    }

    #[test]
    fn bloom_sets_three_bits_per_value() {
        let mut bloom = [0u8; 256];
        bloom_insert(&mut bloom, &[0x11u8; 20]);
        let set_bits: u32 = bloom.iter().map(|b| b.count_ones()).sum();
        assert_eq!(set_bits, 3, "each value must set exactly three bits");
    }

    #[test]
    fn bloom_is_empty_without_logs() {
        let receipt = Receipt {
            status: true,
            cumulative_gas_used: 21_000,
            gas_used: 21_000,
            logs: Vec::new(),
        };
        assert_eq!(receipt.bloom(), [0u8; 256]);
    }

    #[test]
    fn bloom_matches_a_reference_value() {
        // keccak256 of the 20-byte address 0x11..11 is fixed, so the three bit
        // positions are fixed too; this pins the bit-ordering convention.
        let mut bloom = [0u8; 256];
        bloom_insert(&mut bloom, &[0x11u8; 20]);
        let hash = tiger_crypto::keccak256(&[0x11u8; 20]);
        for i in 0..3 {
            let bit = (((hash[i * 2] as usize) << 8) | hash[i * 2 + 1] as usize) & 2047;
            let byte = 256 - 1 - bit / 8;
            assert_eq!(
                bloom[byte] & (1 << (bit % 8)),
                1 << (bit % 8),
                "bit {bit} must be set"
            );
        }
    }

    #[test]
    fn receipt_encodes_status_cumulative_gas_bloom_and_logs() {
        let receipt = Receipt {
            status: true,
            cumulative_gas_used: 21_000,
            gas_used: 21_000,
            logs: vec![log(0x22, 0x33)],
        };
        let encoded = receipt.encode();
        let decoded = tiger_state::rlp::decode(&encoded).unwrap();
        match decoded {
            RlpItem::List(items) => {
                assert_eq!(items.len(), 4, "receipt has four fields");
                assert_eq!(items[0], RlpItem::bytes(vec![1]));
                assert_eq!(items[1], RlpItem::from_u64(21_000));
                assert_eq!(items[2], RlpItem::bytes(receipt.bloom().to_vec()));
                match &items[3] {
                    RlpItem::List(logs) => assert_eq!(logs.len(), 1),
                    _ => panic!("logs must be a list"),
                }
            }
            _ => panic!("receipt must be an RLP list"),
        }
    }

    #[test]
    fn failed_receipt_encodes_status_zero() {
        let receipt = Receipt {
            status: false,
            cumulative_gas_used: 30_000,
            gas_used: 30_000,
            logs: Vec::new(),
        };
        let decoded = tiger_state::rlp::decode(&receipt.encode()).unwrap();
        match decoded {
            RlpItem::List(items) => assert_eq!(items[0], RlpItem::bytes(vec![0])),
            _ => panic!("receipt must be an RLP list"),
        }
    }

    #[test]
    fn block_bloom_is_the_union_of_receipt_blooms() {
        let a = Receipt {
            status: true,
            cumulative_gas_used: 21_000,
            gas_used: 21_000,
            logs: vec![log(0x44, 0x55)],
        };
        let b = Receipt {
            status: true,
            cumulative_gas_used: 42_000,
            gas_used: 21_000,
            logs: vec![log(0x66, 0x77)],
        };
        let combined = logs_bloom(&[a.clone(), b.clone()]);
        let ba = a.bloom();
        let bb = b.bloom();
        for i in 0..256 {
            assert_eq!(combined[i], ba[i] | bb[i]);
        }
    }

    /// These expected values were produced by an independent reference
    /// implementation (`rlp` + `py-trie` + `pycryptodome` keccak256), not by
    /// this code, so a change in the encoding is caught rather than
    /// self-confirmed.
    #[test]
    fn encoding_matches_an_independent_reference_implementation() {
        let plain = Receipt {
            status: true,
            cumulative_gas_used: 21_000,
            gas_used: 21_000,
            logs: Vec::new(),
        };
        assert_eq!(hex_encode(&plain.encode()), "f9010801825208b9010000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000c0");
        assert_eq!(
            hex_encode(&receipts_root(std::slice::from_ref(&plain))),
            "056b23fbba480696b65fe5a59b8f2148a1299103c4f57df839233af2cf4ca2d2"
        );

        let with_log = Receipt {
            status: true,
            cumulative_gas_used: 21_000,
            gas_used: 21_000,
            logs: vec![log(0x22, 0x33)],
        };
        assert_eq!(hex_encode(&with_log.bloom()), "00000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000200000000000000000000000000000000000000000000000000000000000010000000000000000000000000000000000000000000000000000000000000001000000000000000000000000000000000400000000000000000000000000000000000000000000000000000000000100000010000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000");
        assert_eq!(hex_encode(&with_log.encode()), "f9014501825208b9010000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000200000000000000000000000000000000000000000000000000000000000010000000000000000000000000000000000000000000000000000000000000001000000000000000000000000000000000400000000000000000000000000000000000000000000000000000000000100000010000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000f83cf83a942222222222222222222222222222222222222222e1a0333333333333333333333333333333333333333333333333333333333333333382aabb");
        assert_eq!(
            hex_encode(&receipts_root(std::slice::from_ref(&with_log))),
            "1a2b75b2e39ace348652a5e49436c47322731ec9339b1156adba1d70bfa1df9b"
        );

        let second = Receipt {
            status: false,
            cumulative_gas_used: 51_000,
            gas_used: 30_000,
            logs: Vec::new(),
        };
        assert_eq!(
            hex_encode(&receipts_root(&[with_log, second])),
            "eb2de616115ab78d737bca8e317ce7d11dc920a4b3c5d8661ec719b3c60f1ef2"
        );
    }

    #[test]
    fn receipts_root_changes_with_the_receipts() {
        let one = Receipt {
            status: true,
            cumulative_gas_used: 21_000,
            gas_used: 21_000,
            logs: vec![log(0x01, 0x02)],
        };
        let two = Receipt {
            status: false,
            cumulative_gas_used: 51_000,
            gas_used: 30_000,
            logs: Vec::new(),
        };
        let single = receipts_root(std::slice::from_ref(&one));
        let both = receipts_root(&[one, two]);
        assert_ne!(single, both);
        assert_ne!(single, crate::block::EMPTY_TRIE_ROOT);
    }
}
