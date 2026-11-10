//! Recursive Length Prefix (RLP) encoding and decoding.
//!
//! Implements the Ethereum Yellow Paper's Appendix B specification. RLP is the
//! serialisation used for trie nodes, transactions, receipts and block
//! headers, so the trie's root hash is only correct if this is exact.

use std::fmt;

/// An RLP item: either a byte string or a list of items.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RlpItem {
    /// A byte string (the "string" of the specification).
    Bytes(Vec<u8>),
    /// A list of nested items.
    List(Vec<RlpItem>),
}

impl RlpItem {
    /// Convenience constructor for a byte string.
    pub fn bytes(data: impl Into<Vec<u8>>) -> Self {
        RlpItem::Bytes(data.into())
    }

    /// Convenience constructor for a list.
    pub fn list(items: Vec<RlpItem>) -> Self {
        RlpItem::List(items)
    }

    /// RLP-encode this item, appending to `out`.
    pub fn encode_to(&self, out: &mut Vec<u8>) {
        match self {
            RlpItem::Bytes(data) => encode_bytes(data, out),
            RlpItem::List(items) => {
                let mut payload = Vec::new();
                for item in items {
                    item.encode_to(&mut payload);
                }
                encode_length(payload.len(), 0xc0, out);
                out.extend_from_slice(&payload);
            }
        }
    }

    /// RLP-encode this item into a fresh buffer.
    pub fn encode(&self) -> Vec<u8> {
        let mut out = Vec::new();
        self.encode_to(&mut out);
        out
    }

    /// The canonical big-endian minimal encoding of a `u64` scalar.
    ///
    /// RLP encodes scalars without leading zero bytes, and zero is the empty
    /// string. Callers relying on this must not pass pre-encoded big integers.
    pub fn from_u64(value: u64) -> Self {
        if value == 0 {
            return RlpItem::Bytes(Vec::new());
        }
        let bytes = value.to_be_bytes();
        let first = bytes.iter().position(|&b| b != 0).unwrap_or(8);
        RlpItem::Bytes(bytes[first..].to_vec())
    }
}

/// Errors produced while decoding RLP.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RlpError {
    /// The input ended before a complete item could be read.
    Truncated,
    /// A length prefix used the long form for a payload that fits the short
    /// form, or otherwise violated the canonical encoding rules.
    NonCanonical,
    /// Bytes remained after decoding a single item.
    TrailingBytes,
}

impl fmt::Display for RlpError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            RlpError::Truncated => write!(f, "truncated RLP input"),
            RlpError::NonCanonical => write!(f, "non-canonical RLP encoding"),
            RlpError::TrailingBytes => write!(f, "trailing bytes after RLP item"),
        }
    }
}

impl std::error::Error for RlpError {}

fn encode_bytes(data: &[u8], out: &mut Vec<u8>) {
    if data.len() == 1 && data[0] < 0x80 {
        // A single byte below 0x80 is its own encoding.
        out.push(data[0]);
        return;
    }
    encode_length(data.len(), 0x80, out);
    out.extend_from_slice(data);
}

fn encode_length(len: usize, offset: u8, out: &mut Vec<u8>) {
    if len < 56 {
        out.push(offset + len as u8);
        return;
    }
    let len_bytes = len.to_be_bytes();
    let first = len_bytes.iter().position(|&b| b != 0).unwrap_or(8);
    let significant = &len_bytes[first..];
    out.push(offset + 55 + significant.len() as u8);
    out.extend_from_slice(significant);
}

/// Decode a single RLP item, returning it and the number of bytes consumed.
pub fn decode_prefix(input: &[u8]) -> Result<(RlpItem, usize), RlpError> {
    if input.is_empty() {
        return Err(RlpError::Truncated);
    }
    let prefix = input[0];

    if prefix < 0x80 {
        return Ok((RlpItem::Bytes(vec![prefix]), 1));
    }

    if prefix < 0xb8 {
        let len = (prefix - 0x80) as usize;
        let end = 1 + len;
        if input.len() < end {
            return Err(RlpError::Truncated);
        }
        return Ok((RlpItem::Bytes(input[1..end].to_vec()), end));
    }

    if prefix < 0xc0 {
        let len_of_len = (prefix - 0xb7) as usize;
        let end = 1 + len_of_len;
        if input.len() < end {
            return Err(RlpError::Truncated);
        }
        let len = read_length(&input[1..end])?;
        if len < 56 {
            return Err(RlpError::NonCanonical);
        }
        if input.len() < end + len {
            return Err(RlpError::Truncated);
        }
        return Ok((RlpItem::Bytes(input[end..end + len].to_vec()), end + len));
    }

    if prefix < 0xf8 {
        let len = (prefix - 0xc0) as usize;
        let end = 1 + len;
        if input.len() < end {
            return Err(RlpError::Truncated);
        }
        let items = decode_list_payload(&input[1..end])?;
        return Ok((RlpItem::List(items), end));
    }

    let len_of_len = (prefix - 0xf7) as usize;
    let end = 1 + len_of_len;
    if input.len() < end {
        return Err(RlpError::Truncated);
    }
    let len = read_length(&input[1..end])?;
    if len < 56 {
        return Err(RlpError::NonCanonical);
    }
    if input.len() < end + len {
        return Err(RlpError::Truncated);
    }
    let items = decode_list_payload(&input[end..end + len])?;
    Ok((RlpItem::List(items), end + len))
}

/// Decode exactly one RLP item, rejecting trailing bytes.
pub fn decode(input: &[u8]) -> Result<RlpItem, RlpError> {
    let (item, consumed) = decode_prefix(input)?;
    if consumed != input.len() {
        return Err(RlpError::TrailingBytes);
    }
    Ok(item)
}

fn read_length(bytes: &[u8]) -> Result<usize, RlpError> {
    if bytes.is_empty() || bytes[0] == 0 {
        return Err(RlpError::NonCanonical);
    }
    let mut len: usize = 0;
    for &b in bytes {
        len = len.checked_mul(256).ok_or(RlpError::Truncated)?;
        len = len.checked_add(b as usize).ok_or(RlpError::Truncated)?;
    }
    Ok(len)
}

fn decode_list_payload(payload: &[u8]) -> Result<Vec<RlpItem>, RlpError> {
    let mut items = Vec::new();
    let mut cursor = 0usize;
    while cursor < payload.len() {
        let (item, consumed) = decode_prefix(&payload[cursor..])?;
        items.push(item);
        cursor += consumed;
    }
    Ok(items)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hex(s: &str) -> Vec<u8> {
        (0..s.len() / 2)
            .map(|i| u8::from_str_radix(&s[i * 2..i * 2 + 2], 16).unwrap())
            .collect()
    }

    // Vectors from the Ethereum Wiki "RLP" reference examples.
    #[test]
    fn matches_ethereum_wiki_vectors() {
        assert_eq!(RlpItem::bytes(b"dog".to_vec()).encode(), hex("83646f67"));
        assert_eq!(
            RlpItem::list(vec![
                RlpItem::bytes(b"cat".to_vec()),
                RlpItem::bytes(b"dog".to_vec())
            ])
            .encode(),
            hex("c88363617483646f67")
        );
        assert_eq!(RlpItem::bytes(Vec::new()).encode(), hex("80"));
        assert_eq!(RlpItem::list(vec![]).encode(), hex("c0"));
        assert_eq!(RlpItem::from_u64(0).encode(), hex("80"));
        assert_eq!(RlpItem::from_u64(15).encode(), hex("0f"));
        assert_eq!(RlpItem::from_u64(1024).encode(), hex("820400"));
        // The set theoretical representation of three.
        assert_eq!(
            RlpItem::list(vec![
                RlpItem::list(vec![]),
                RlpItem::list(vec![RlpItem::list(vec![])]),
                RlpItem::list(vec![
                    RlpItem::list(vec![]),
                    RlpItem::list(vec![RlpItem::list(vec![])])
                ]),
            ])
            .encode(),
            hex("c7c0c1c0c3c0c1c0")
        );
        // The long string "Lorem ipsum dolor sit amet, consectetur adipisicing
        // elit" (56 bytes) must use the long form.
        let lorem = b"Lorem ipsum dolor sit amet, consectetur adipisicing elit";
        assert_eq!(RlpItem::bytes(lorem.to_vec()).encode()[0], 0xb8);
    }

    #[test]
    fn round_trips_nested_structures() {
        let item = RlpItem::list(vec![
            RlpItem::bytes(vec![0x01, 0x02, 0x03]),
            RlpItem::list(vec![RlpItem::bytes(vec![0xff; 40])]),
            RlpItem::from_u64(1_000_000),
        ]);
        let encoded = item.encode();
        assert_eq!(decode(&encoded).unwrap(), item);
    }

    #[test]
    fn rejects_non_canonical_and_truncated_input() {
        // 0xb8 0x01 0x41 encodes a 1-byte string in the long form.
        assert_eq!(decode(&hex("b80141")), Err(RlpError::NonCanonical));
        // Missing payload byte.
        assert_eq!(decode(&hex("83646f")), Err(RlpError::Truncated));
        // Extra byte after a complete item.
        assert_eq!(decode(&hex("83646f6700")), Err(RlpError::TrailingBytes));
        // Leading zero in a length prefix.
        assert_eq!(decode(&hex("b90041")), Err(RlpError::NonCanonical));
    }
}
