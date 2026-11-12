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
    /// A sealed block carries no seal. Without one there is nothing tying the
    /// block to the validator its `miner` field names.
    MissingSeal { number: u64 },
    /// The seal is present but cannot be recovered to any address.
    InvalidSeal { number: u64 },
    /// The seal recovers to a key other than the one the header claims.
    SealSignerMismatch {
        number: u64,
        signer: String,
        claimed: String,
    },
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
            ValidationError::MissingSeal { number } => {
                write!(f, "block {number} carries no proposer seal")
            }
            ValidationError::InvalidSeal { number } => {
                write!(f, "block {number} carries a seal that cannot be recovered")
            }
            ValidationError::SealSignerMismatch {
                number,
                signer,
                claimed,
            } => write!(
                f,
                "block {number} is sealed by {signer} but claims to be from {claimed}"
            ),
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

/// RLP-encode the access list of a typed transaction.
///
/// The access list is a list of `[address, [storageKey, ...]]` pairs. It is
/// held in its wire form because both the signing hash and the transaction
/// hash commit to the exact bytes, so re-encoding it from a parsed structure
/// would risk a mismatch.
fn access_list_item(access_list: &Option<String>) -> Result<RlpItem, ValidationError> {
    match access_list {
        None => Ok(RlpItem::List(Vec::new())),
        Some(raw) => {
            let bytes = decode_hex_field(raw, None)?;
            tiger_state::rlp::decode(&bytes)
                .map_err(|_| ValidationError::MalformedField("access_list"))
        }
    }
}

/// The nine signed fields shared by every transaction type.
fn signed_fields(tx: &Transaction) -> Result<Vec<RlpItem>, ValidationError> {
    let to = match &tx.to {
        Some(address) => fixed_item(address, 20)?,
        None => RlpItem::Bytes(Vec::new()),
    };
    Ok(vec![
        RlpItem::from_u64(tx.nonce),
        RlpItem::from_u64(tx.gas_price),
        RlpItem::from_u64(tx.gas),
        to,
        quantity_item(&tx.value)?,
        RlpItem::Bytes(decode_hex_field(&tx.input, None)?),
    ])
}

/// The price per unit of gas a transaction actually paid in a block whose
/// base fee is `base_fee`.
///
/// A legacy or EIP-2930 transaction pays its signed `gasPrice`. An EIP-1559
/// transaction pays `min(maxFeePerGas, baseFee + maxPriorityFeePerGas)`. The
/// transaction carries `maxFeePerGas` in its `gas_price` field, so a receipt
/// that echoed that field would overstate what the sender was charged.
pub fn effective_gas_price(tx: &Transaction, base_fee: u64) -> u64 {
    match tx.max_priority_fee_per_gas {
        None => tx.gas_price,
        Some(priority) => tx.gas_price.min(base_fee.saturating_add(priority)),
    }
}

/// The base fee of the block that follows `parent`.
///
/// EIP-1559 makes the base fee a function of the parent block, so every client
/// derives the same value and a validator cannot choose it to favour itself.
/// The fee rises while blocks are fuller than the target and falls while they
/// are emptier, with the change bounded to an eighth per block.
pub fn next_base_fee(
    parent_base_fee: u64,
    parent_gas_used: u64,
    parent_gas_limit: u64,
) -> u64 {
    const MAX_CHANGE_DENOMINATOR: u64 = 8;
    const ELASTICITY_MULTIPLIER: u64 = 2;

    let target = parent_gas_limit / ELASTICITY_MULTIPLIER;
    if target == 0 {
        // A zero gas limit is not a usable block, and dividing by it would
        // panic; leaving the fee unchanged is the only safe reading.
        return parent_base_fee;
    }

    if parent_gas_used == target {
        return parent_base_fee;
    }

    if parent_gas_used > target {
        let delta = parent_gas_used - target;
        let increase = (parent_base_fee as u128 * delta as u128
            / target as u128
            / MAX_CHANGE_DENOMINATOR as u128)
            .max(1) as u64;
        parent_base_fee.saturating_add(increase)
    } else {
        let delta = target - parent_gas_used;
        let decrease = (parent_base_fee as u128 * delta as u128
            / target as u128
            / MAX_CHANGE_DENOMINATOR as u128) as u64;
        parent_base_fee.saturating_sub(decrease)
    }
}

/// RLP-encode a transaction in the envelope its type calls for.
///
/// Legacy transactions (type 0) encode as a bare nine-field list. Typed
/// transactions (EIP-2930 type 1, EIP-1559 type 2) encode as the type byte
/// followed by the RLP list, which is what `eth_sendRawTransaction` receives
/// and what the transactions trie stores.
pub fn encode_transaction(tx: &Transaction) -> Result<Vec<u8>, ValidationError> {
    if tx.tx_type == 0 {
        return encode_legacy_transaction(tx);
    }

    let mut fields = typed_unsigned_fields(tx)?;
    fields.push(quantity_item(&tx.v)?);
    fields.push(quantity_item(&tx.r)?);
    fields.push(quantity_item(&tx.s)?);

    let mut encoded = vec![tx.tx_type as u8];
    encoded.extend_from_slice(&RlpItem::list(fields).encode());
    Ok(encoded)
}

/// The unsigned field list of a typed transaction, in wire order.
///
/// A type 1 (EIP-2930) transaction is `[chainId, nonce, gasPrice, gasLimit,
/// to, value, data, accessList]`; a type 2 (EIP-1559) transaction replaces the
/// single `gasPrice` with `maxPriorityFeePerGas` and `maxFeePerGas`. The
/// signature and the transaction hash are both taken over these fields, so
/// both callers build them here rather than each keeping their own copy of
/// the ordering.
fn typed_unsigned_fields(tx: &Transaction) -> Result<Vec<RlpItem>, ValidationError> {
    let mut fields = vec![
        RlpItem::from_u64(tx.chain_id.unwrap_or(0)),
        RlpItem::from_u64(tx.nonce),
    ];
    match tx.tx_type {
        1 => fields.push(RlpItem::from_u64(tx.gas_price)),
        2 => {
            fields.push(RlpItem::from_u64(tx.max_priority_fee_per_gas.unwrap_or(0)));
            fields.push(RlpItem::from_u64(tx.gas_price));
        }
        other => {
            return Err(ValidationError::MalformedField(match other {
                3 => "blob transaction",
                _ => "transaction type",
            }))
        }
    }
    fields.push(RlpItem::from_u64(tx.gas));
    fields.push(match &tx.to {
        Some(address) => fixed_item(address, 20)?,
        None => RlpItem::Bytes(Vec::new()),
    });
    fields.push(quantity_item(&tx.value)?);
    fields.push(RlpItem::Bytes(decode_hex_field(&tx.input, None)?));
    fields.push(access_list_item(&tx.access_list)?);
    Ok(fields)
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
///
/// This is the hash of the bytes that travel on the wire, so a typed
/// transaction hashes its type byte and its list together.
pub fn transaction_hash(tx: &Transaction) -> Result<String, ValidationError> {
    let encoded = encode_transaction(tx)?;
    Ok(format!("0x{}", hex_encode(&keccak256(&encoded))))
}

/// Decode a signed transaction from the bytes a wallet submits.
///
/// An EIP-2718 payload is either a bare RLP list (a legacy transaction) or a
/// type byte below `0x80` followed by that type's list. Both envelopes are
/// accepted here, and the chain id that a typed transaction carries is
/// returned in the transaction so its hash can be recomputed over the exact
/// bytes that arrived.
pub fn decode_transaction(bytes: &[u8]) -> Result<Transaction, ValidationError> {
    let first = *bytes
        .first()
        .ok_or(ValidationError::MalformedField("empty transaction"))?;
    if first < 0x80 {
        if first == 0 {
            return Err(ValidationError::MalformedField(
                "type 0 must be encoded as a legacy list",
            ));
        }
        return decode_typed_transaction(first as u64, &bytes[1..]);
    }
    decode_legacy_transaction(bytes)
}

/// Decode the payload of an EIP-2718 typed transaction, its type byte already
/// stripped.
fn decode_typed_transaction(tx_type: u64, payload: &[u8]) -> Result<Transaction, ValidationError> {
    if tx_type == 3 {
        return Err(ValidationError::MalformedField("blob transaction"));
    }
    if tx_type > 2 {
        return Err(ValidationError::MalformedField("transaction type"));
    }

    let item = tiger_state::rlp::decode(payload)
        .map_err(|_| ValidationError::MalformedField("transaction rlp"))?;
    let fields = match item {
        RlpItem::List(items) => items,
        RlpItem::Bytes(_) => {
            return Err(ValidationError::MalformedField("transaction must be a list"))
        }
    };

    // `[chainId, nonce, fee..., gasLimit, to, value, data, accessList,
    // yParity, r, s]`, where a type 2 transaction has two fee fields and a
    // type 1 transaction has one.
    let expected = if tx_type == 2 { 12 } else { 11 };
    if fields.len() != expected {
        return Err(ValidationError::MalformedField("transaction field count"));
    }

    let chain_id = rlp_u64(&fields[0]).ok_or(ValidationError::MalformedField("chain id"))?;
    let nonce = rlp_u64(&fields[1]).ok_or(ValidationError::MalformedField("nonce"))?;
    let (max_priority_fee, gas_price, fee_index) = if tx_type == 2 {
        (
            Some(rlp_u64(&fields[2]).ok_or(ValidationError::MalformedField("maxPriorityFeePerGas"))?),
            rlp_u64(&fields[3]).ok_or(ValidationError::MalformedField("maxFeePerGas"))?,
            4usize,
        )
    } else {
        (
            None,
            rlp_u64(&fields[2]).ok_or(ValidationError::MalformedField("gasPrice"))?,
            3usize,
        )
    };

    let gas = rlp_u64(&fields[fee_index]).ok_or(ValidationError::MalformedField("gasLimit"))?;
    let to = decode_to_field(&fields[fee_index + 1])?;
    let value = rlp_bytes(&fields[fee_index + 2])
        .ok_or(ValidationError::MalformedField("value"))?;
    let input = rlp_bytes(&fields[fee_index + 3])
        .ok_or(ValidationError::MalformedField("data"))?;
    let access_list = match &fields[fee_index + 4] {
        RlpItem::List(_) => fields[fee_index + 4].encode(),
        _ => return Err(ValidationError::MalformedField("accessList")),
    };
    let v = rlp_u64(&fields[fee_index + 5]).ok_or(ValidationError::MalformedField("yParity"))?;
    let r = rlp_bytes(&fields[fee_index + 6]).unwrap_or_default();
    let s = rlp_bytes(&fields[fee_index + 7]).unwrap_or_default();

    let mut tx = Transaction {
        hash: String::new(),
        nonce,
        block_hash: None,
        block_number: None,
        transaction_index: None,
        // Recovered from the signature; the wire carries no sender.
        from: format!("0x{}", hex_encode(&[0u8; 20])),
        to,
        value: hex_quantity(&value),
        gas_price,
        gas,
        input: format!("0x{}", hex_encode(&input)),
        v: format!("0x{v:x}"),
        r: format!("0x{}", hex_encode(&r)),
        s: format!("0x{}", hex_encode(&s)),
        tx_type,
        access_list: Some(format!("0x{}", hex_encode(&access_list))),
        max_priority_fee_per_gas: max_priority_fee,
        chain_id: Some(chain_id),
    };
    tx.hash = transaction_hash(&tx)?;
    Ok(tx)
}

/// Decode a legacy (type 0) RLP transaction.
fn decode_legacy_transaction(bytes: &[u8]) -> Result<Transaction, ValidationError> {
    let item = tiger_state::rlp::decode(bytes)
        .map_err(|_| ValidationError::MalformedField("transaction rlp"))?;
    let fields = match item {
        RlpItem::List(items) => items,
        RlpItem::Bytes(_) => {
            return Err(ValidationError::MalformedField("transaction must be a list"))
        }
    };
    if fields.len() != 9 {
        return Err(ValidationError::MalformedField("legacy field count"));
    }

    let nonce = rlp_u64(&fields[0]).ok_or(ValidationError::MalformedField("nonce"))?;
    let gas_price = rlp_u64(&fields[1]).ok_or(ValidationError::MalformedField("gasPrice"))?;
    let gas = rlp_u64(&fields[2]).ok_or(ValidationError::MalformedField("gasLimit"))?;
    let to = decode_to_field(&fields[3])?;
    let value = rlp_bytes(&fields[4]).ok_or(ValidationError::MalformedField("value"))?;
    let input = rlp_bytes(&fields[5]).ok_or(ValidationError::MalformedField("data"))?;
    let v = rlp_u64(&fields[6]).ok_or(ValidationError::MalformedField("v"))?;
    let r = rlp_bytes(&fields[7]).unwrap_or_default();
    let s = rlp_bytes(&fields[8]).unwrap_or_default();

    let mut tx = Transaction {
        hash: String::new(),
        nonce,
        block_hash: None,
        block_number: None,
        transaction_index: None,
        from: format!("0x{}", hex_encode(&[0u8; 20])),
        to,
        value: hex_quantity(&value),
        gas_price,
        gas,
        input: format!("0x{}", hex_encode(&input)),
        v: format!("0x{v:x}"),
        r: format!("0x{}", hex_encode(&r)),
        s: format!("0x{}", hex_encode(&s)),
        tx_type: 0,
        access_list: None,
        max_priority_fee_per_gas: None,
        chain_id: None,
    };
    tx.hash = transaction_hash(&tx)?;
    Ok(tx)
}

/// Format a big-endian integer as a canonical JSON-RPC quantity.
///
/// A quantity has no leading zeros at all: not a zero byte, and not a zero
/// nibble either, so the value 1000 is `0x3e8` and never `0x03e8`. Zero is
/// the single exception and is written `0x0`. Clients compare quantities by
/// value but echo them back, and a non-canonical form fails strict parsers.
fn hex_quantity(bytes: &[u8]) -> String {
    let first = bytes.iter().position(|&b| b != 0).unwrap_or(bytes.len());
    let trimmed = &bytes[first..];
    let digits = hex_encode(trimmed);
    let digits = digits.trim_start_matches('0');
    if digits.is_empty() {
        "0x0".to_string()
    } else {
        format!("0x{digits}")
    }
}

fn decode_to_field(item: &RlpItem) -> Result<Option<String>, ValidationError> {
    match item {
        RlpItem::Bytes(b) if b.is_empty() => Ok(None),
        RlpItem::Bytes(b) if b.len() == 20 => Ok(Some(format!("0x{}", hex_encode(b)))),
        _ => Err(ValidationError::MalformedField("to")),
    }
}

fn rlp_bytes(item: &RlpItem) -> Option<Vec<u8>> {
    match item {
        RlpItem::Bytes(b) => Some(b.clone()),
        RlpItem::List(_) => None,
    }
}

fn rlp_u64(item: &RlpItem) -> Option<u64> {
    let bytes = rlp_bytes(item)?;
    if bytes.len() > 8 {
        return None;
    }
    let mut out = 0u64;
    for &b in &bytes {
        out = (out << 8) | b as u64;
    }
    Some(out)
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
        let value = encode_transaction(tx)?;
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
        // The difficulty is not free for a block to choose. Fork choice weighs
        // a branch by the work its blocks claim, so a validator that could
        // write any difficulty could claim more work than it did and win a
        // fork it should lose. Requiring every block to carry the chain's
        // configured difficulty makes the two branches of a fork compare on
        // height alone, which is the PoSA rule.
        if !difficulty_matches(header, parent, config) {
            return Err(ValidationError::MalformedField("difficulty"));
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

    // EIP-1559: the base fee is mandatory once London is active, and it must
    // be exactly what the parent's usage implies. Accepting any other value
    // would let a validator raise the fee to tax users or lower it to fill
    // blocks cheaply, and would let two clients disagree on a block's hash.
    if config.london_block.map(|b| header.number >= b).unwrap_or(false) {
        let Some(base_fee) = header.base_fee_per_gas else {
            return Err(ValidationError::MalformedField("base_fee_per_gas"));
        };
        if let Some(parent) = parent {
            let expected = next_base_fee(
                parent.header.base_fee_per_gas.unwrap_or(0),
                parent.header.gas_used,
                parent.header.gas_limit,
            );
            if base_fee != expected {
                return Err(ValidationError::MalformedField("base_fee_per_gas"));
            }
        }
    }

    Ok(())
}

/// Whether a block's declared difficulty is the one the chain requires.
///
/// PoSA blocks all carry the same difficulty, taken from the genesis
/// definition, so a valid block's difficulty is simply its parent's. Comparing
/// against the parent rather than against a constant means a chain whose
/// genesis names any value still validates, and it needs no extra config.
fn difficulty_matches(
    header: &BlockHeader,
    parent: &Block,
    _config: &crate::types::ChainConfig,
) -> bool {
    // Compare numerically rather than as strings: `0x1`, `0x01` and `1` are
    // the same difficulty, and a peer is entitled to send any of them.
    match (decode_hex_quantity(&header.difficulty), decode_hex_quantity(&parent.header.difficulty)) {
        (Ok(a), Ok(b)) => a == b,
        _ => false,
    }
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

/// The hash a transaction's signature commits to.
///
/// A legacy transaction signs the nine-field EIP-155 preimage
/// `[nonce, gasPrice, gasLimit, to, value, data, chainId, 0, 0]`; including
/// the chain id is what prevents a transaction signed for one chain from
/// being replayed on another.
///
/// A typed transaction (EIP-2930, EIP-1559) signs the type byte followed by
/// its own field list *without* the signature, and without a chain id inside
/// the list: the chain id is an explicit field, so it is already covered.
pub fn signing_hash(tx: &Transaction, chain_id: u64) -> Result<[u8; 32], ValidationError> {
    if tx.tx_type == 0 {
        let mut fields = signed_fields(tx)?;
        fields.push(RlpItem::from_u64(chain_id));
        fields.push(RlpItem::Bytes(Vec::new()));
        fields.push(RlpItem::Bytes(Vec::new()));
        return Ok(tiger_crypto::keccak256(&RlpItem::list(fields).encode()));
    }

    // A typed transaction signs the same field list it travels with, minus
    // the signature, prefixed by its type byte.
    let fields = typed_unsigned_fields(tx)?;

    let mut encoded = vec![tx.tx_type as u8];
    encoded.extend_from_slice(&RlpItem::list(fields).encode());
    Ok(tiger_crypto::keccak256(&encoded))
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

    tx.v = if tx.tx_type == 0 {
        format!("0x{:x}", signature.v(Some(chain_id)))
    } else {
        // A typed transaction stores the bare y-parity in `v`.
        format!("0x{:x}", signature.v(None))
    };
    tx.r = format!("0x{}", hex_encode(&signature.r()));
    tx.s = format!("0x{}", hex_encode(&signature.s()));
    tx.hash = transaction_hash(tx)?;
    Ok(())
}

/// Recover the sender address of a signed transaction.
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

    let recovery_id = if tx.tx_type == 0 {
        // EIP-155 encodes the chain id in v as v = recovery_id + 35 + 2 *
        // chain_id. Values of 27/28 are the pre-EIP-155 form.
        match v_value {
            27 | 28 => (v_value - 27) as u8,
            other => {
                let expected_parity_base = 35 + 2 * chain_id;
                if other < expected_parity_base {
                    return Err(ValidationError::MalformedField("v"));
                }
                ((other - expected_parity_base) % 2) as u8
            }
        }
    } else {
        // A typed transaction carries the raw y-parity, and the chain id is
        // an explicit field, so there is no recovery information to unpack.
        // Anything above 1 is not a parity bit and cannot be a valid
        // signature; accepting it would let a malleable `v` select a
        // different key.
        match v_value {
            0 | 1 => v_value as u8,
            _ => return Err(ValidationError::MalformedField("v")),
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

    /// Decode a hex string, ignoring an optional `0x` prefix.
    fn hex_bytes(value: &str) -> Vec<u8> {
        let trimmed = value.strip_prefix("0x").unwrap_or(value);
        (0..trimmed.len() / 2)
            .map(|i| u8::from_str_radix(&trimmed[i * 2..i * 2 + 2], 16).unwrap())
            .collect()
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
            tx_type: 0,
            access_list: None,
            max_priority_fee_per_gas: None,
            chain_id: None,
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

    /// A block that builds on `parent`, with the base fee the parent's usage
    /// implies. A hand-written fixture cannot guess that value, and a child
    /// with the wrong fee is now correctly rejected.
    fn block_on(parent: &Block, number: u64, timestamp: u64) -> Block {
        let mut block = build_block(number, block_hash(&parent.header).unwrap(), timestamp);
        block.header.base_fee_per_gas = Some(next_base_fee(
            parent.header.base_fee_per_gas.unwrap_or(0),
            parent.header.gas_used,
            parent.header.gas_limit,
        ));
        block
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

        let child = block_on(&genesis, 1, 1_700_000_012);
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
            tx_type: 0,
            access_list: None,
            max_priority_fee_per_gas: None,
            chain_id: None,
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

    /// A transaction exactly as `eth-account` signs it: an EIP-1559 transfer
    /// of 1 TGR on chain 6666, key `0x..01`. The raw bytes and the resulting
    /// hash come from that independent implementation, so this pins the whole
    /// path — envelope, field order, signing hash, recovery — to the format
    /// real wallets emit.
    #[test]
    fn typed_transactions_match_an_independent_implementation() {
        let raw = "02f874821a0a80843b9aca00847735940082520894000000000000000000000000000000000000dead880de0b6b3a764000080c001a0e971283a8f5fc4b7a9e3d49cd6f7246a5abe77cf9de36899cc9576f3bf3e8ceca03f15699cbd527e9009ce59cc7ac15ee2d6f53faa3fb0cc59fac7898038af5241";
        let tx = decode_transaction(&hex_bytes(raw)).unwrap();

        assert_eq!(tx.tx_type, 2);
        assert_eq!(tx.chain_id, Some(6666));
        assert_eq!(tx.nonce, 0);
        assert_eq!(tx.max_priority_fee_per_gas, Some(1_000_000_000));
        assert_eq!(tx.gas_price, 2_000_000_000);
        assert_eq!(tx.gas, 21_000);
        assert_eq!(tx.v, "0x1");

        assert_eq!(
            transaction_hash(&tx).unwrap(),
            "0x313eb46d853c9aa910ea75ce88f606a80c98b288df35e1977ece45ca3d51e8d1"
        );

        // Re-encoding must reproduce the wire bytes, otherwise the
        // transactions root would differ from every other client's.
        assert_eq!(hex_encode(&encode_transaction(&tx).unwrap()), raw);

        let sender = recover_transaction_sender(&tx, 6666).unwrap();
        assert_eq!(
            hex_encode(&sender),
            "7e5f4552091a69125d5dfcb7b8c2659029395bdf"
        );
    }

    /// The same for an EIP-2930 transaction with a populated access list.
    #[test]
    fn typed_transaction_with_access_list_matches() {
        let raw = "01f8a0821a0a0784b2d05e0082753094000000000000000000000000000000000000dead0180f838f7940000000000000000000000000000000000000002e1a0000000000000000000000000000000000000000000000000000000000000000001a04a85e1d78363f35c3af6bf3984cc39ff5dadc9c6829a0968ce74cb50565be303a049f8c209dcdb0f8dc699c3686aaab7e02ea5424128173cebd5c851ec580bbc97";
        let tx = decode_transaction(&hex_bytes(raw)).unwrap();
        assert_eq!(tx.tx_type, 1);
        assert_eq!(tx.chain_id, Some(6666));
        assert_eq!(tx.nonce, 7);
        assert_eq!(tx.gas_price, 3_000_000_000);
        assert_eq!(tx.max_priority_fee_per_gas, None);
        assert_eq!(
            transaction_hash(&tx).unwrap(),
            "0xdfcb68555c0cb0746bd88907386da0acbaeaea10d96bc8163ac8ffca71962538"
        );
        assert_eq!(hex_encode(&encode_transaction(&tx).unwrap()), raw);
        assert_eq!(
            hex_encode(&recover_transaction_sender(&tx, 6666).unwrap()),
            "7e5f4552091a69125d5dfcb7b8c2659029395bdf"
        );
    }

    /// A typed transaction signed for one chain must not recover on another:
    /// the chain id is inside the signed preimage.
    #[test]
    fn typed_transaction_is_bound_to_its_chain() {
        let raw = "02f874821a0a80843b9aca00847735940082520894000000000000000000000000000000000000dead880de0b6b3a764000080c001a0e971283a8f5fc4b7a9e3d49cd6f7246a5abe77cf9de36899cc9576f3bf3e8ceca03f15699cbd527e9009ce59cc7ac15ee2d6f53faa3fb0cc59fac7898038af5241";
        let tx = decode_transaction(&hex_bytes(raw)).unwrap();
        // The chain id is inside the signed list, so a payload claiming a
        // different chain recovers a different sender. A node that ignored
        // the payload's chain id would accept a replay from another chain.
        let mut replayed = tx.clone();
        replayed.chain_id = Some(1);
        let other = recover_transaction_sender(&replayed, 1).unwrap();
        assert_ne!(
            hex_encode(&other),
            "7e5f4552091a69125d5dfcb7b8c2659029395bdf"
        );
    }

    /// `yParity` is a single bit. A `v` above 1 is not a parity and must be
    /// refused rather than silently reduced modulo 2, which would let a
    /// malleable `v` name a different key.
    #[test]
    fn typed_transaction_rejects_a_non_parity_v() {
        let raw = "02f874821a0a80843b9aca00847735940082520894000000000000000000000000000000000000dead880de0b6b3a764000080c001a0e971283a8f5fc4b7a9e3d49cd6f7246a5abe77cf9de36899cc9576f3bf3e8ceca03f15699cbd527e9009ce59cc7ac15ee2d6f53faa3fb0cc59fac7898038af5241";
        let mut tx = decode_transaction(&hex_bytes(raw)).unwrap();
        tx.v = "0x2".to_string();
        assert!(recover_transaction_sender(&tx, 6666).is_err());
    }

    /// A type byte that is not a supported envelope is refused, so a blob
    /// transaction cannot be mistaken for a transfer.
    #[test]
    fn unsupported_transaction_types_are_refused() {
        let mut tx = reference_tx();
        tx.tx_type = 3;
        assert!(encode_transaction(&tx).is_err());
        assert!(signing_hash(&tx, 1).is_err());
    }


    /// The base fee follows the parent's usage: it rises on a full block,
    /// falls on an empty one, and holds at the target. Every client must
    /// compute the same value, so the arithmetic is pinned here.
    #[test]
    fn base_fee_follows_the_parent_usage() {
        // An empty block: the fee falls by an eighth.
        assert_eq!(next_base_fee(1_000_000_000, 0, 30_000_000), 875_000_000);
        // Exactly the target (half the limit): unchanged.
        assert_eq!(next_base_fee(1_000_000_000, 15_000_000, 30_000_000), 1_000_000_000);
        // A completely full block: the fee rises by an eighth.
        assert_eq!(next_base_fee(1_000_000_000, 30_000_000, 30_000_000), 1_125_000_000);
        // A zero fee stays zero rather than dividing by a zero target.
        assert_eq!(next_base_fee(0, 0, 0), 0);
        // A block above the limit cannot make the fee overflow.
        assert!(next_base_fee(u64::MAX, 30_000_000, 30_000_000) > 0);
    }

    /// A validator cannot publish a block whose base fee is not the one its
    /// parent implies.
    #[test]
    fn a_block_with_the_wrong_base_fee_is_rejected() {
        let parent = build_block(1, h(0x00), 1_700_000_000);
        let mut child = block_on(&parent, 2, 1_700_000_012);
        let expected = child.header.base_fee_per_gas.unwrap();
        assert!(validate_block(&child, Some(&parent), &test_config()).is_ok());

        // One wei away from the derived value is still wrong.
        child.header.base_fee_per_gas = Some(expected + 1);
        assert!(validate_block(&child, Some(&parent), &test_config()).is_err());
    }


    /// A decoded transaction must expose integer fields in canonical
    /// quantity form. A leading zero nibble such as `0x03e8` is not a valid
    /// quantity and strict clients reject it.
    #[test]
    fn decoded_quantities_are_canonical() {
        // 1000 encodes as the two bytes 0x03 0xe8, whose high nibble is zero.
        let tx = reference_tx();
        let encoded = encode_transaction(&tx).unwrap();
        let decoded = decode_transaction(&encoded).unwrap();
        assert_eq!(decoded.value, "0x3e8");
        assert_eq!(decoded.nonce, 1);
        assert_eq!(decoded.gas, 21_000);

        // A zero value stays `0x0` rather than becoming an empty string.
        let mut zero = reference_tx();
        zero.value = "0x0".to_string();
        let encoded = encode_transaction(&zero).unwrap();
        assert_eq!(decode_transaction(&encoded).unwrap().value, "0x0");
    }

    /// A typed transaction round-trips through the encoder and decoder with
    /// every field preserved, so the node can hash and store what a wallet
    /// sent without altering it.
    #[test]
    fn typed_transaction_round_trips() {
        let raw = "02f874821a0a80843b9aca00847735940082520894000000000000000000000000000000000000dead880de0b6b3a764000080c001a0e971283a8f5fc4b7a9e3d49cd6f7246a5abe77cf9de36899cc9576f3bf3e8ceca03f15699cbd527e9009ce59cc7ac15ee2d6f53faa3fb0cc59fac7898038af5241";
        let decoded = decode_transaction(&hex_bytes(raw)).unwrap();
        assert_eq!(hex_encode(&encode_transaction(&decoded).unwrap()), raw);
        // Re-encoding preserves the hash, which is what makes the receipt
        // lookup by hash work after a restart.
        assert_eq!(
            decoded.hash,
            transaction_hash(&decode_transaction(&hex_bytes(raw)).unwrap()).unwrap()
        );
    }

    /// The effective gas price is the one the sender was charged: an EIP-1559
    /// transaction never pays its ceiling when the base fee plus its tip is
    /// lower.
    #[test]
    fn effective_gas_price_respects_the_ceiling_and_the_tip() {
        let mut tx = reference_tx();
        // A legacy transaction pays exactly what it signed.
        assert_eq!(effective_gas_price(&tx, 999), tx.gas_price);

        // An EIP-1559 transaction pays base fee + tip when that is below the
        // ceiling.
        tx.tx_type = 2;
        tx.chain_id = Some(6666);
        tx.access_list = Some("0xc0".to_string());
        tx.gas_price = 1_000;
        tx.max_priority_fee_per_gas = Some(10);
        assert_eq!(effective_gas_price(&tx, 100), 110);
        // A tip so large that base + tip exceeds the ceiling is clamped to
        // the ceiling, because the sender never authorised more.
        assert_eq!(effective_gas_price(&tx, 100), 110);
        tx.max_priority_fee_per_gas = Some(5_000);
        assert_eq!(effective_gas_price(&tx, 100), 1_000);
    }

}
