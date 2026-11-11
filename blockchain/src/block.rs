//! Block header serialisation, hashing and structural validation.
//!
//! The block hash is the keccak256 of the RLP encoding of the header. Getting
//! this wrong means the chain cannot agree with any other client, so the field
//! order below follows the Yellow Paper exactly and the tests are checked
//! against an independent reference implementation (py-trie's `rlp` plus
//! pycryptodome's keccak).

use crate::types::{Block, BlockHeader, Transaction};
use tiger_crypto::keccak256;
use tiger_state::rlp::RlpItem;
use tiger_state::trie::Trie;

/// keccak256 of the empty trie root, i.e. the transactions root of a block
/// with no transactions.
pub const EMPTY_TRIE_ROOT: [u8; 32] = [
    0x56, 0xe8, 0x1f, 0x17, 0x1b, 0xcc, 0x55, 0xa6, 0xff, 0x83, 0x45, 0xe6, 0x92, 0xc0, 0xf8, 0x6e,
    0x5b, 0x48, 0xe0, 0x1b, 0x99, 0x6c, 0xad, 0xc0, 0x01, 0x62, 0x2f, 0xb5, 0xe3, 0x63, 0xb4, 0x21,
];

/// Error raised when a block fails structural validation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ValidationError {
    /// A field is not valid hex, or has the wrong length for its type.
    MalformedField(&'static str),
    /// The block's number does not follow its parent.
    NonSequentialNumber { expected: u64, found: u64 },
    /// The parent hash does not match the parent block's hash.
    ParentHashMismatch { expected: String, found: String },
    /// The block claims more gas used than its gas limit.
    GasUsedExceedsLimit { gas_used: u64, gas_limit: u64 },
    /// The timestamp is not greater than the parent's.
    NonMonotonicTimestamp { parent: u64, found: u64 },
    /// The declared transactions root does not match the transactions.
    TransactionsRootMismatch { expected: String, found: String },
    /// A transaction in the block has no hash.
    TransactionMissingHash(usize),
    /// The declared receipts root does not match the block's receipts.
    ReceiptsRootMismatch { expected: String, found: String },
    /// The declared logs bloom does not match the block's receipts.
    LogsBloomMismatch,
}

impl std::fmt::Display for ValidationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ValidationError::MalformedField(name) => write!(f, "malformed field: {name}"),
            ValidationError::NonSequentialNumber { expected, found } => write!(
                f,
                "block number {found} does not follow parent ({expected})"
            ),
            ValidationError::ParentHashMismatch { expected, found } => write!(
                f,
                "parent hash mismatch: expected {expected}, found {found}"
            ),
            ValidationError::GasUsedExceedsLimit {
                gas_used,
                gas_limit,
            } => write!(
                f,
                "gas used {gas_used} exceeds gas limit {gas_limit}"
            ),
            ValidationError::NonMonotonicTimestamp { parent, found } => write!(
                f,
                "timestamp {found} is not greater than parent {parent}"
            ),
            ValidationError::TransactionsRootMismatch { expected, found } => write!(
                f,
                "transactions root mismatch: expected {expected}, found {found}"
            ),
            ValidationError::TransactionMissingHash(index) => {
                write!(f, "transaction at index {index} has no hash")
            }
            ValidationError::ReceiptsRootMismatch { expected, found } => write!(
                f,
                "receipts root mismatch: expected {expected}, found {found}"
            ),
            ValidationError::LogsBloomMismatch => {
                write!(f, "logs bloom does not match the block's receipts")
            }
        }
    }
}

impl std::error::Error for ValidationError {}

/// Decode a hex string (with or without a `0x` prefix) into bytes, requiring
/// the result to be exactly `len` bytes when a length is given.
pub fn decode_hex_field(value: &str, expected_len: Option<usize>) -> Result<Vec<u8>, ValidationError> {
    let trimmed = value.strip_prefix("0x").unwrap_or(value);
    // Odd-length hex is invalid: it cannot represent whole bytes.
    if trimmed.len() % 2 != 0 {
        return Err(ValidationError::MalformedField("hex length"));
    }
    let mut out = Vec::with_capacity(trimmed.len() / 2);
    let bytes = trimmed.as_bytes();
    for pair in bytes.chunks(2) {
        let hi = (pair[0] as char)
            .to_digit(16)
            .ok_or(ValidationError::MalformedField("hex digit"))?;
        let lo = (pair[1] as char)
            .to_digit(16)
            .ok_or(ValidationError::MalformedField("hex digit"))?;
        out.push(((hi << 4) | lo) as u8);
    }
    if let Some(len) = expected_len {
        if out.len() != len {
            return Err(ValidationError::MalformedField("hex width"));
        }
    }
    Ok(out)
}

/// Decode a hex quantity.
///
/// Ethereum JSON-RPC encodes quantities without leading zeros, so an odd
/// number of hex digits (for example `0x100`) is valid and is left-padded with
/// a single zero nibble. Fixed-width fields must use [`decode_hex_field`],
/// which rejects odd lengths because a truncated byte string is ambiguous.
pub fn decode_hex_quantity(value: &str) -> Result<Vec<u8>, ValidationError> {
    let trimmed = value.strip_prefix("0x").unwrap_or(value);
    let owned;
    let padded = if trimmed.len() % 2 != 0 {
        owned = format!("0{trimmed}");
        owned.as_str()
    } else {
        trimmed
    };
    decode_hex_field(padded, None)
}

/// RLP-encode a hex quantity as a minimal big-endian scalar.
///
/// Leading zero bytes are stripped, and zero encodes as the empty string, as
/// the Yellow Paper requires.
pub fn quantity_item(value: &str) -> Result<RlpItem, ValidationError> {
    let bytes = decode_hex_quantity(value)?;
    let first = bytes.iter().position(|&b| b != 0).unwrap_or(bytes.len());
    Ok(RlpItem::Bytes(bytes[first..].to_vec()))
}

/// RLP-encode a fixed-width byte string field (hashes, addresses).
pub fn fixed_item(value: &str, len: usize) -> Result<RlpItem, ValidationError> {
    Ok(RlpItem::Bytes(decode_hex_field(value, Some(len))?))
}

/// The RLP encoding of a block header, in Yellow Paper field order.
///
/// `withdrawals_root` is appended only when present, matching the post-Shanghai
/// header layout.
pub fn encode_header(header: &BlockHeader) -> Result<Vec<u8>, ValidationError> {
    let mut fields = vec![
        fixed_item(&header.parent_hash, 32)?,
        fixed_item(&header.sha3_uncles, 32)?,
        fixed_item(&header.miner, 20)?,
        fixed_item(&header.state_root, 32)?,
        fixed_item(&header.transactions_root, 32)?,
        fixed_item(&header.receipts_root, 32)?,
        fixed_item(&header.logs_bloom, 256)?,
        quantity_item(&header.difficulty)?,
        RlpItem::from_u64(header.number),
        RlpItem::from_u64(header.gas_limit),
        RlpItem::from_u64(header.gas_used),
        RlpItem::from_u64(header.timestamp),
        RlpItem::Bytes(decode_hex_field(&header.extra_data, None)?),
        fixed_item(&header.mix_hash, 32)?,
        fixed_item(&header.nonce, 8)?,
    ];
    if let Some(base_fee) = header.base_fee_per_gas {
        fields.push(RlpItem::from_u64(base_fee));
    }
    if let Some(withdrawals_root) = &header.withdrawals_root {
        fields.push(fixed_item(withdrawals_root, 32)?);
    }
    Ok(RlpItem::list(fields).encode())
}

/// The keccak256 hash of a block header, returned as a `0x`-prefixed string.
pub fn block_hash(header: &BlockHeader) -> Result<String, ValidationError> {
    let encoded = encode_header(header)?;
    Ok(format!("0x{}", hex_encode(&keccak256(&encoded))))
}

/// RLP-encode a legacy transaction in Yellow Paper field order:
/// `[nonce, gasPrice, gasLimit, to, value, data, v, r, s]`.
///
/// `to` is the empty string for a contract creation.
pub fn encode_legacy_transaction(tx: &Transaction) -> Result<Vec<u8>, ValidationError> {
    let to = match &tx.to {
        Some(address) => fixed_item(address, 20)?,
        None => RlpItem::Bytes(Vec::new()),
    };
    let fields = vec![
        RlpItem::from_u64(tx.nonce),
        RlpItem::from_u64(tx.gas_price),
        RlpItem::from_u64(tx.gas),
        to,
        quantity_item(&tx.value)?,
        RlpItem::Bytes(decode_hex_field(&tx.input, None)?),
        quantity_item(&tx.v)?,
        quantity_item(&tx.r)?,
        quantity_item(&tx.s)?,
    ];
    Ok(RlpItem::list(fields).encode())
}

/// The keccak256 hash of a transaction, as a `0x`-prefixed string.
pub fn transaction_hash(tx: &Transaction) -> Result<String, ValidationError> {
    let encoded = encode_legacy_transaction(tx)?;
    Ok(format!("0x{}", hex_encode(&keccak256(&encoded))))
}

/// Compute the transactions root for a block.
///
/// The trie is keyed by the RLP encoding of the transaction index and valued
/// by the RLP encoding of the transaction, exactly as the Yellow Paper
/// specifies. An empty block has the empty trie root.
pub fn transactions_root(transactions: &[Transaction]) -> Result<[u8; 32], ValidationError> {
    if transactions.is_empty() {
        return Ok(EMPTY_TRIE_ROOT);
    }
    let mut trie = Trie::new();
    for (index, tx) in transactions.iter().enumerate() {
        let key = RlpItem::from_u64(index as u64).encode();
        let value = encode_legacy_transaction(tx)?;
        trie.insert(&key, &value);
    }
    Ok(trie.root_hash())
}

/// Validate a block against its parent.
///
/// `parent` is `None` for a genesis block. Every check here is a real
/// consensus invariant: without them a node would accept blocks that skip
/// numbers, build on the wrong parent, or claim a transactions root that does
/// not match the transactions they carry.
pub fn validate_block(
    block: &Block,
    parent: Option<&Block>,
    config: &crate::types::ChainConfig,
) -> Result<(), ValidationError> {
    let header = &block.header;

    // Field well-formedness. A malformed field would produce a nonsense hash.
    decode_hex_field(&header.parent_hash, Some(32))?;
    decode_hex_field(&header.sha3_uncles, Some(32))?;
    decode_hex_field(&header.miner, Some(20))?;
    decode_hex_field(&header.state_root, Some(32))?;
    decode_hex_field(&header.transactions_root, Some(32))?;
    decode_hex_field(&header.receipts_root, Some(32))?;
    decode_hex_field(&header.logs_bloom, Some(256))?;
    decode_hex_field(&header.mix_hash, Some(32))?;
    decode_hex_field(&header.nonce, Some(8))?;
    decode_hex_quantity(&header.difficulty)?;
    decode_hex_field(&header.extra_data, None)?;
    if let Some(root) = &header.withdrawals_root {
        decode_hex_field(root, Some(32))?;
    }

    // A block may never claim more gas than its limit allows.
    if header.gas_used > header.gas_limit {
        return Err(ValidationError::GasUsedExceedsLimit {
            gas_used: header.gas_used,
            gas_limit: header.gas_limit,
        });
    }

    // The declared transactions root must match the transactions carried.
    let expected_root = format!(
        "0x{}",
        hex_encode(&transactions_root(&block.transactions)?)
    );
    if !header.transactions_root.eq_ignore_ascii_case(&expected_root) {
        return Err(ValidationError::TransactionsRootMismatch {
            expected: expected_root,
            found: header.transactions_root.clone(),
        });
    }

    for (index, tx) in block.transactions.iter().enumerate() {
        if tx.hash.trim().is_empty() {
            return Err(ValidationError::TransactionMissingHash(index));
        }
        // Each transaction's declared hash must match its contents.
        let computed = transaction_hash(tx)?;
        if !tx.hash.eq_ignore_ascii_case(&computed) {
            return Err(ValidationError::TransactionMissingHash(index));
        }
    }

    if let Some(parent) = parent {
        let expected_number = parent.header.number + 1;
        if header.number != expected_number {
            return Err(ValidationError::NonSequentialNumber {
                expected: expected_number,
                found: header.number,
            });
        }
        let parent_hash = block_hash(&parent.header)?;
        if !header.parent_hash.eq_ignore_ascii_case(&parent_hash) {
            return Err(ValidationError::ParentHashMismatch {
                expected: parent_hash,
                found: header.parent_hash.clone(),
            });
        }
        if header.timestamp <= parent.header.timestamp {
            return Err(ValidationError::NonMonotonicTimestamp {
                parent: parent.header.timestamp,
                found: header.timestamp,
            });
        }
        // The gas limit may only drift by the allowed per-block delta.
        let limit_delta = header.gas_limit.abs_diff(parent.header.gas_limit);
        let allowed = parent.header.gas_limit / 1024;
        if limit_delta > allowed.max(1) {
            return Err(ValidationError::MalformedField("gas limit delta"));
        }
    } else if header.number != 0 {
        // Without a parent the only valid block is the genesis block.
        return Err(ValidationError::NonSequentialNumber {
            expected: 0,
            found: header.number,
        });
    }

    // EIP-1559: the base fee is mandatory once London is active.
    if config.london_block.map(|b| header.number >= b).unwrap_or(false)
        && header.base_fee_per_gas.is_none()
    {
        return Err(ValidationError::MalformedField("base_fee_per_gas"));
    }

    Ok(())
}

/// Lowercase hex encoding.
pub fn hex_encode(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(bytes.len() * 2);
    for &b in bytes {
        out.push(HEX[(b >> 4) as usize] as char);
        out.push(HEX[(b & 0x0f) as usize] as char);
    }
    out
}


// =============================================================================
// EIP-155 TRANSACTION SIGNING
// =============================================================================

/// The EIP-155 signing hash of a legacy transaction.
///
/// The signed preimage is `[nonce, gasPrice, gasLimit, to, value, data,
/// chainId, 0, 0]`; including the chain id is what prevents a transaction
/// signed for one chain from being replayed on another.
pub fn signing_hash(tx: &Transaction, chain_id: u64) -> Result<[u8; 32], ValidationError> {
    let to = match &tx.to {
        Some(address) => fixed_item(address, 20)?,
        None => RlpItem::Bytes(Vec::new()),
    };
    let fields = vec![
        RlpItem::from_u64(tx.nonce),
        RlpItem::from_u64(tx.gas_price),
        RlpItem::from_u64(tx.gas),
        to,
        quantity_item(&tx.value)?,
        RlpItem::Bytes(decode_hex_field(&tx.input, None)?),
        RlpItem::from_u64(chain_id),
        RlpItem::Bytes(Vec::new()),
        RlpItem::Bytes(Vec::new()),
    ];
    Ok(tiger_crypto::keccak256(&RlpItem::list(fields).encode()))
}

/// Fill in a transaction's signature fields by signing its EIP-155 hash.
///
/// Sets `v`, `r`, `s` and the transaction `hash`.
pub fn sign_transaction(
    tx: &mut Transaction,
    chain_id: u64,
    private_key: &[u8; 32],
) -> Result<(), ValidationError> {
    let hash = signing_hash(tx, chain_id)?;
    let signature = tiger_crypto::sign(&hash, private_key)
        .map_err(|_| ValidationError::MalformedField("private_key"))?;

    tx.v = format!("0x{:x}", signature.v(Some(chain_id)));
    tx.r = format!("0x{}", hex_encode(&signature.r()));
    tx.s = format!("0x{}", hex_encode(&signature.s()));
    tx.hash = transaction_hash(tx)?;
    Ok(())
}

/// Recover the sender address of a signed legacy transaction.
///
/// This is how a node verifies that a transaction was authorised by the
/// account whose nonce and balance it is about to spend.
pub fn recover_transaction_sender(
    tx: &Transaction,
    chain_id: u64,
) -> Result<[u8; 20], ValidationError> {
    let v = decode_hex_quantity(&tx.v)?;
    let v_value = bytes_to_u64(&v);
    let r = decode_hex_quantity(&tx.r)?;
    let s = decode_hex_quantity(&tx.s)?;
    if r.len() > 32 || s.len() > 32 {
        return Err(ValidationError::MalformedField("signature"));
    }

    // EIP-155 encodes the chain id in v as v = recovery_id + 35 + 2 * chain_id.
    // Values of 27/28 are the pre-EIP-155 form.
    let recovery_id = match v_value {
        27 | 28 => (v_value - 27) as u8,
        other => {
            let expected_parity_base = 35 + 2 * chain_id;
            if other < expected_parity_base {
                return Err(ValidationError::MalformedField("v"));
            }
            ((other - expected_parity_base) % 2) as u8
        }
    };

    let mut signature = [0u8; 64];
    signature[32 - r.len()..32].copy_from_slice(&r);
    signature[64 - s.len()..].copy_from_slice(&s);

    let hash = signing_hash(tx, chain_id)?;
    tiger_crypto::recover_sender(&hash, &signature, recovery_id)
        .map_err(|_| ValidationError::MalformedField("signature"))
}

/// Big-endian byte string to u64, saturating at 8 bytes.
fn bytes_to_u64(bytes: &[u8]) -> u64 {
    let mut out = 0u64;
    for &b in bytes.iter().take(8) {
        out = (out << 8) | b as u64;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{Block, BlockHeader, Transaction};

    fn h(byte: u8) -> String {
        format!("0x{}", hex_encode(&[byte; 32]))
    }

    /// The header used for the reference hash test. Every field is fixed so
    /// the expected hash is reproducible by any implementation.
    fn reference_header() -> BlockHeader {
        BlockHeader {
            parent_hash: h(0xaa),
            sha3_uncles: h(0xbb),
            miner: format!("0x{}", hex_encode(&[0xcc; 20])),
            state_root: h(0xdd),
            transactions_root: h(0xee),
            receipts_root: h(0xff),
            logs_bloom: format!("0x{}", hex_encode(&[0x00; 256])),
            difficulty: "0x100".to_string(),
            number: 1,
            gas_limit: 30_000_000,
            gas_used: 21_000,
            timestamp: 1_700_000_000,
            extra_data: "0x5449474552".to_string(), // "TIGER"
            mix_hash: h(0x12),
            nonce: "0x0000000000000000".to_string(),
            base_fee_per_gas: Some(1_000_000_000),
            withdrawals_root: None,
        }
    }

    #[test]
    fn header_rlp_matches_reference_length() {
        let encoded = encode_header(&reference_header()).unwrap();
        // Cross-checked against Python rlp.encode of the same field list.
        assert_eq!(encoded.len(), 518);
    }

    #[test]
    fn block_hash_matches_reference_implementation() {
        // Expected value produced by an independent Python implementation
        // (rlp + pycryptodome keccak256) over the same header.
        assert_eq!(
            block_hash(&reference_header()).unwrap(),
            "0xb94619d59f53841ab9231c1f0fe7c3d69fa2afccdb670d11bceba840d51025ee"
        );
    }

    #[test]
    fn empty_block_has_the_empty_trie_root() {
        assert_eq!(transactions_root(&[]).unwrap(), EMPTY_TRIE_ROOT);
        assert_eq!(
            hex_encode(&EMPTY_TRIE_ROOT),
            "56e81f171bcc55a6ff8345e692c0f86e5b48e01b996cadc001622fb5e363b421"
        );
    }

    fn reference_tx() -> Transaction {
        Transaction {
            hash: String::new(),
            nonce: 1,
            block_hash: None,
            block_number: None,
            transaction_index: None,
            from: format!("0x{}", hex_encode(&[0x11; 20])),
            to: Some(format!("0x{}", hex_encode(&[0xdd; 20]))),
            value: "0x3e8".to_string(), // 1000
            gas_price: 2_000_000_000,
            gas: 21_000,
            input: "0x".to_string(),
            v: "0x25".to_string(),
            r: "0x01".to_string(),
            s: "0x02".to_string(),
        }
    }

    #[test]
    fn transaction_hash_and_root_match_reference_implementation() {
        let tx = reference_tx();
        assert_eq!(
            transaction_hash(&tx).unwrap(),
            "0xc62b26ea23a81032c361c445c8c76ad8169b1b7faa3fe4f7f73e91d386b02551"
        );
        assert_eq!(
            hex_encode(&transactions_root(&[tx]).unwrap()),
            "17c4c383428af70a00b993bfde4577db206fb686f31836edc701e11a783018d1"
        );
    }

    #[test]
    fn malformed_hex_fields_are_rejected() {
        // Wrong width.
        assert!(matches!(
            decode_hex_field("0x1234", Some(32)),
            Err(ValidationError::MalformedField(_))
        ));
        // Odd number of hex digits.
        assert!(matches!(
            decode_hex_field("0xabc", None),
            Err(ValidationError::MalformedField(_))
        ));
        // Non-hex character.
        assert!(matches!(
            decode_hex_field("0xzz", None),
            Err(ValidationError::MalformedField(_))
        ));
    }

    #[test]
    fn quantity_encoding_strips_leading_zeros_and_encodes_zero_as_empty() {
        assert_eq!(quantity_item("0x0000").unwrap(), RlpItem::Bytes(vec![]));
        // 0x0100 is 256, which needs two bytes even after stripping zeros.
        assert_eq!(quantity_item("0x0100").unwrap(), RlpItem::Bytes(vec![1, 0]));
        assert_eq!(
            quantity_item("0x0000000a").unwrap(),
            RlpItem::Bytes(vec![0x0a])
        );
    }

    #[test]
    fn odd_length_quantities_are_left_padded() {
        // JSON-RPC emits quantities without leading zeros, so 0x100 has an odd
        // number of digits and must decode as the two bytes 0x01 0x00.
        assert_eq!(decode_hex_quantity("0x100").unwrap(), vec![0x01, 0x00]);
        assert_eq!(quantity_item("0x100").unwrap(), RlpItem::Bytes(vec![1, 0]));
        assert_eq!(quantity_item("0x1").unwrap(), RlpItem::Bytes(vec![1]));
        // Fixed-width fields stay strict.
        assert!(decode_hex_field("0x100", Some(2)).is_err());
    }

    fn build_block(number: u64, parent_hash: String, timestamp: u64) -> Block {
        // A transaction's declared hash must match its contents, so it is
        // computed here rather than left blank.
        let mut tx = reference_tx();
        tx.hash = transaction_hash(&tx).unwrap();
        let txs = vec![tx];
        let root = transactions_root(&txs).unwrap();
        Block {
            header: BlockHeader {
                parent_hash,
                transactions_root: format!("0x{}", hex_encode(&root)),
                number,
                timestamp,
                ..reference_header()
            },
            transactions: txs,
            uncles: Vec::new(),
        }
    }

    fn test_config() -> crate::types::ChainConfig {
        crate::types::ChainConfig {
            chain_id: 6666,
            london_block: Some(0),
            ..Default::default()
        }
    }

    #[test]
    fn genesis_must_be_number_zero() {
        let block = build_block(5, h(0x00), 1_700_000_000);
        assert!(matches!(
            validate_block(&block, None, &test_config()),
            Err(ValidationError::NonSequentialNumber { expected: 0, found: 5 })
        ));
    }

    #[test]
    fn valid_block_passes_validation() {
        let genesis = build_block(0, h(0x00), 1_700_000_000);
        assert!(validate_block(&genesis, None, &test_config()).is_ok());

        let genesis_hash = block_hash(&genesis.header).unwrap();
        let child = build_block(1, genesis_hash, 1_700_000_012);
        assert!(validate_block(&child, Some(&genesis), &test_config()).is_ok());
    }

    #[test]
    fn block_with_wrong_parent_hash_is_rejected() {
        let genesis = build_block(0, h(0x00), 1_700_000_000);
        // Deliberately wrong parent hash.
        let child = build_block(1, h(0x99), 1_700_000_012);
        assert!(matches!(
            validate_block(&child, Some(&genesis), &test_config()),
            Err(ValidationError::ParentHashMismatch { .. })
        ));
    }

    #[test]
    fn block_skipping_a_number_is_rejected() {
        let genesis = build_block(0, h(0x00), 1_700_000_000);
        let genesis_hash = block_hash(&genesis.header).unwrap();
        let child = build_block(7, genesis_hash, 1_700_000_012);
        assert!(matches!(
            validate_block(&child, Some(&genesis), &test_config()),
            Err(ValidationError::NonSequentialNumber { expected: 1, found: 7 })
        ));
    }

    #[test]
    fn non_monotonic_timestamp_is_rejected() {
        let genesis = build_block(0, h(0x00), 1_700_000_000);
        let genesis_hash = block_hash(&genesis.header).unwrap();
        // Same timestamp as the parent is not strictly greater.
        let child = build_block(1, genesis_hash, 1_700_000_000);
        assert!(matches!(
            validate_block(&child, Some(&genesis), &test_config()),
            Err(ValidationError::NonMonotonicTimestamp { .. })
        ));
    }

    #[test]
    fn gas_used_above_limit_is_rejected() {
        let mut block = build_block(0, h(0x00), 1_700_000_000);
        block.header.gas_used = block.header.gas_limit + 1;
        assert!(matches!(
            validate_block(&block, None, &test_config()),
            Err(ValidationError::GasUsedExceedsLimit { .. })
        ));
    }

    #[test]
    fn mismatched_transactions_root_is_rejected() {
        let mut block = build_block(0, h(0x00), 1_700_000_000);
        block.header.transactions_root = h(0x77);
        assert!(matches!(
            validate_block(&block, None, &test_config()),
            Err(ValidationError::TransactionsRootMismatch { .. })
        ));
    }

    #[test]
    fn transaction_with_tampered_contents_is_rejected() {
        let mut block = build_block(0, h(0x00), 1_700_000_000);
        // Set a hash that cannot match the contents.
        block.transactions[0].hash = h(0x55);
        block.header.transactions_root =
            format!("0x{}", hex_encode(&transactions_root(&block.transactions).unwrap()));
        assert!(matches!(
            validate_block(&block, None, &test_config()),
            Err(ValidationError::TransactionMissingHash(0))
        ));
    }

    #[test]
    fn eip155_signing_hash_matches_the_specification() {
        // The canonical EIP-155 example: chain id 1, private key 0x4646..46.
        let tx = Transaction {
            hash: String::new(),
            nonce: 9,
            block_hash: None,
            block_number: None,
            transaction_index: None,
            from: format!("0x{}", hex_encode(&[0x00; 20])),
            to: Some(format!("0x{}", hex_encode(&[0x35; 20]))),
            value: format!("0x{}", hex_encode(&1_000_000_000_000_000_000u64.to_be_bytes())),
            gas_price: 20_000_000_000,
            gas: 21_000,
            input: "0x".to_string(),
            v: "0x0".to_string(),
            r: "0x0".to_string(),
            s: "0x0".to_string(),
        };
        assert_eq!(
            hex_encode(&signing_hash(&tx, 1).unwrap()),
            "daf5a779ae972f972197303d7b574746c7ef83eadac0f2791ad23db92e4c8e53"
        );
    }

    #[test]
    fn sign_then_recover_round_trips_to_the_signer() {
        let mut key = [0u8; 32];
        key.fill(0x46);

        let mut tx = reference_tx();
        // reference_tx declares a sender of 0x11..11, but the signer is
        // 0x46..46; recovery must report the real signer, not the declared one.
        sign_transaction(&mut tx, 6666, &key).unwrap();

        // The signature fields must be populated and the hash consistent.
        assert_ne!(tx.v, "0x0");
        assert_eq!(tx.hash, transaction_hash(&tx).unwrap());

        let recovered = recover_transaction_sender(&tx, 6666).unwrap();
        assert_eq!(
            hex_encode(&recovered),
            "9d8a62f656a8d1615c1294fd71e9cfb3e4855a4f",
            "recovered sender must be the address of the signing key"
        );
    }

    #[test]
    fn recovering_on_the_wrong_chain_id_yields_a_different_sender() {
        let mut key = [0u8; 32];
        key.fill(0x46);
        let mut tx = reference_tx();
        sign_transaction(&mut tx, 6666, &key).unwrap();

        let correct = recover_transaction_sender(&tx, 6666).unwrap();
        // Recovering with a different chain id changes the signing hash, so
        // the recovered key cannot be the same. This is exactly the replay
        // protection EIP-155 provides.
        let wrong = recover_transaction_sender(&tx, 1);
        match wrong {
            Ok(addr) => assert_ne!(addr, correct),
            Err(_) => {} // a malformed v for that chain is also acceptable
        }
    }

    #[test]
    fn tampered_transaction_fails_sender_recovery() {
        let mut key = [0u8; 32];
        key.fill(0x46);
        let mut tx = reference_tx();
        sign_transaction(&mut tx, 6666, &key).unwrap();
        let original = recover_transaction_sender(&tx, 6666).unwrap();

        // Change the value after signing: recovery must not return the signer.
        tx.value = "0x7d0".to_string();
        match recover_transaction_sender(&tx, 6666) {
            Ok(addr) => assert_ne!(addr, original, "tampering must change the sender"),
            Err(_) => {}
        }
    }

    #[test]
    fn excessive_gas_limit_drift_is_rejected() {
        let genesis = build_block(0, h(0x00), 1_700_000_000);
        let genesis_hash = block_hash(&genesis.header).unwrap();
        let mut child = build_block(1, genesis_hash, 1_700_000_012);
        // Far more than parent_limit / 1024.
        child.header.gas_limit = genesis.header.gas_limit * 2;
        assert!(matches!(
            validate_block(&child, Some(&genesis), &test_config()),
            Err(ValidationError::MalformedField(_))
        ));
    }
}
