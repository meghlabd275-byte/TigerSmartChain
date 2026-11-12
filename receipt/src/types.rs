//! Receipt Types
//!
//! A transaction receipt, its logs, and the helpers that keep a receipt
//! internally consistent with the block it belongs to.

use crate::bloom::Bloom;
use serde::{Deserialize, Serialize};

// =============================================================================
// RECEIPT
// =============================================================================

/// Transaction Receipt
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Receipt {
    pub transaction_hash: String,
    pub block_hash: String,
    pub block_number: u64,
    pub contract_address: Option<String>,
    pub cumulative_gas_used: u64,
    pub gas_used: u64,
    pub logs: Vec<Log>,
    pub logs_bloom: String,
    pub status: bool,
}

impl Receipt {
    /// Check that the receipt is internally consistent.
    ///
    /// Every log must belong to this transaction and this block, and each log's
    /// `log_index` must match its position in `logs`. A receipt that fails this
    /// check would let an indexer attribute an event to the wrong transaction.
    pub fn validate(&self) -> Result<(), String> {
        if self.transaction_hash.is_empty() {
            return Err("receipt has no transaction hash".to_string());
        }

        if self.logs.len() > u64::MAX as usize {
            return Err("receipt has more logs than the index type can hold".to_string());
        }

        for (position, log) in self.logs.iter().enumerate() {
            if log.transaction_hash != self.transaction_hash {
                return Err(format!(
                    "log {position} belongs to transaction {} but the receipt is for {}",
                    log.transaction_hash, self.transaction_hash
                ));
            }

            if log.block_hash != self.block_hash {
                return Err(format!(
                    "log {position} belongs to block {} but the receipt is for {}",
                    log.block_hash, self.block_hash
                ));
            }

            if log.block_number != self.block_number {
                return Err(format!(
                    "log {position} is at block {} but the receipt is at {}",
                    log.block_number, self.block_number
                ));
            }

            if log.log_index as usize != position {
                return Err(format!(
                    "log {position} reports log_index {}",
                    log.log_index
                ));
            }
        }

        Ok(())
    }

    /// The bloom filter implied by this receipt's logs.
    pub fn computed_bloom(&self) -> Bloom {
        Bloom::from_logs(&self.logs)
    }

    /// True when the stored `logs_bloom` matches the logs it covers.
    ///
    /// A receipt whose bloom disagrees with its logs makes light clients skip
    /// blocks that do contain the events they are looking for.
    pub fn bloom_matches_logs(&self) -> bool {
        match Bloom::from_hex(&self.logs_bloom) {
            Ok(stored) => stored == self.computed_bloom(),
            Err(_) => false,
        }
    }
}

/// Log
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Log {
    pub address: String,
    pub topics: Vec<String>,
    pub data: String,
    pub log_index: u64,
    pub transaction_index: u64,
    pub transaction_hash: String,
    pub block_number: u64,
    pub block_hash: String,
    pub removed: bool,
}

impl Log {
    /// The 32-byte values this log contributes to a block bloom filter: the
    /// contract address followed by every topic.
    pub fn bloom_elements(&self) -> Vec<[u8; 32]> {
        let mut elements = Vec::with_capacity(self.topics.len() + 1);
        elements.push(address_to_word(&self.address));

        for topic in &self.topics {
            elements.push(address_to_word(topic));
        }

        elements
    }

    /// The bloom filter for this log on its own.
    pub fn bloom(&self) -> Bloom {
        Bloom::from_elements(&self.bloom_elements())
    }

    /// True when `bloom` could contain this log.
    ///
    /// A bloom filter never misses an element that was added, so a `false`
    /// result proves the log is absent and a `true` result is only a candidate.
    pub fn may_match(&self, bloom: &Bloom) -> bool {
        self.bloom_elements()
            .iter()
            .all(|element| bloom.contains(element))
    }
}

/// Read a 32-byte word from a hex string.
///
/// A 20-byte address is left-padded with zeros, matching how the EVM places an
/// address in a 32-byte word. A value longer than 32 bytes keeps its low-order
/// bytes.
pub(crate) fn address_to_word(value: &str) -> [u8; 32] {
    let mut word = [0u8; 32];
    let trimmed = value.trim_start_matches("0x").trim_start_matches("0X");

    let digits: Vec<u8> = trimmed
        .as_bytes()
        .iter()
        .filter_map(|c| (*c as char).to_digit(16))
        .map(|digit| digit as u8)
        .collect();

    let mut decoded = Vec::with_capacity(digits.len() / 2);
    for pair in digits.chunks(2) {
        match pair {
            [high, low] => decoded.push((high << 4) | low),
            [high] => decoded.push(high << 4),
            _ => {}
        }
    }

    if decoded.len() <= 32 {
        word[32 - decoded.len()..].copy_from_slice(&decoded);
    } else {
        word.copy_from_slice(&decoded[decoded.len() - 32..]);
    }

    word
}

#[cfg(test)]
mod tests {
    use super::*;

    const TX: &str = "0xabc";
    const BLOCK: &str = "0xdef";

    fn log(index: u64) -> Log {
        Log {
            address: "0x1111111111111111111111111111111111111111".to_string(),
            topics: vec![
                "0xddf252ad1be2c89b69c2b068fc378daa952ba7f163c4a11628f55a4df523b3ef".to_string(),
            ],
            data: "0x".to_string(),
            log_index: index,
            transaction_index: 0,
            transaction_hash: TX.to_string(),
            block_number: 7,
            block_hash: BLOCK.to_string(),
            removed: false,
        }
    }

    fn receipt(logs: Vec<Log>) -> Receipt {
        let bloom = Bloom::from_logs(&logs);
        Receipt {
            transaction_hash: TX.to_string(),
            block_hash: BLOCK.to_string(),
            block_number: 7,
            contract_address: None,
            cumulative_gas_used: 21_000,
            gas_used: 21_000,
            logs,
            logs_bloom: bloom.to_hex(),
            status: true,
        }
    }

    #[test]
    fn a_consistent_receipt_validates() {
        let r = receipt(vec![log(0), log(1)]);
        assert!(r.validate().is_ok());
        assert!(r.bloom_matches_logs());
    }

    #[test]
    fn a_log_from_another_transaction_is_rejected() {
        let mut r = receipt(vec![log(0)]);
        r.logs[0].transaction_hash = "0xother".to_string();
        assert!(r.validate().is_err());
    }

    #[test]
    fn a_log_with_a_mismatched_index_is_rejected() {
        let mut r = receipt(vec![log(0), log(1)]);
        r.logs[1].log_index = 5;
        assert!(r.validate().is_err());
    }

    #[test]
    fn a_bloom_that_disagrees_with_the_logs_is_detected() {
        let mut r = receipt(vec![log(0)]);

        let mut extra = log(1);
        extra.address = "0x2222222222222222222222222222222222222222".to_string();
        r.logs.push(extra);

        assert!(!r.bloom_matches_logs(), "the bloom was not recomputed");

        r.logs_bloom = r.computed_bloom().to_hex();
        assert!(r.bloom_matches_logs());
    }

    #[test]
    fn an_address_is_left_padded_into_a_word() {
        let word = address_to_word("0x1111111111111111111111111111111111111111");
        assert_eq!(&word[..12], &[0u8; 12]);
        assert!(word[12..].iter().all(|byte| *byte == 0x11));
    }

    #[test]
    fn a_receipt_with_no_logs_has_an_empty_bloom() {
        let r = receipt(vec![]);
        assert!(r.validate().is_ok());
        assert_eq!(r.computed_bloom().data, vec![0u8; 256]);
        assert!(r.bloom_matches_logs());
    }
}
