//! Minimal Ethereum ABI encoder/decoder for the ENS contracts.
//!
//! Only the types ENS needs are supported: `bytes32`, `uint256`, `address`,
//! `string` and `bytes`. Every routine follows the ABI specification in the
//! Solidity documentation (head/tail encoding with 32 byte words).

use sha3::{Digest, Keccak256};

/// keccak256 of the input.
pub fn keccak256(data: &[u8]) -> [u8; 32] {
    let mut hasher = Keccak256::new();
    hasher.update(data);
    hasher.finalize().into()
}

/// keccak256 of the empty string, the code hash of an account with no code.
pub const EMPTY_KECCAK: [u8; 32] = [
    0xc5, 0xd2, 0x46, 0x01, 0x86, 0xf7, 0x23, 0x3c, 0x92, 0x7e, 0x7d, 0xb2, 0xdc, 0xc7, 0x03, 0xc0,
    0xe5, 0x00, 0xb6, 0x53, 0xca, 0x82, 0x27, 0x3b, 0x7b, 0xfa, 0xd8, 0x04, 0x5d, 0x85, 0xa4, 0x70,
];

/// The four byte function selector for a canonical signature.
pub fn function_selector(signature: &str) -> [u8; 4] {
    let hash = keccak256(signature.as_bytes());
    [hash[0], hash[1], hash[2], hash[3]]
}

/// EIP-137 namehash: recursively hash each label from the right.
pub fn namehash(name: &str) -> [u8; 32] {
    let mut node = [0u8; 32];
    let trimmed = name.trim().trim_end_matches('.');

    if trimmed.is_empty() {
        return node;
    }

    for label in trimmed.split('.').rev() {
        let mut buf = Vec::with_capacity(64);
        buf.extend_from_slice(&node);
        buf.extend_from_slice(&keccak256(label.as_bytes()));
        node = keccak256(&buf);
    }

    node
}

fn push_word(out: &mut Vec<u8>, word: &[u8; 32]) {
    out.extend_from_slice(word);
}

/// Left pad a value to a 32 byte word.
fn word_from_bytes(value: &[u8]) -> [u8; 32] {
    let mut word = [0u8; 32];
    let start = 32usize.saturating_sub(value.len());
    let slice = &value[value.len().saturating_sub(32)..];
    word[start..].copy_from_slice(slice);
    word
}

fn word_from_u64(value: u64) -> [u8; 32] {
    let mut word = [0u8; 32];
    word[24..].copy_from_slice(&value.to_be_bytes());
    word
}

/// ABI argument values supported by the encoder.
#[derive(Debug, Clone)]
pub enum AbiValue {
    Bytes32([u8; 32]),
    Uint(u64),
    Address([u8; 20]),
    Str(String),
    Bytes(Vec<u8>),
}

impl AbiValue {
    fn is_dynamic(&self) -> bool {
        matches!(self, AbiValue::Str(_) | AbiValue::Bytes(_))
    }

    fn encode_tail(&self) -> Vec<u8> {
        match self {
            AbiValue::Str(s) => encode_bytes(s.as_bytes()),
            AbiValue::Bytes(b) => encode_bytes(b),
            other => {
                let mut out = Vec::with_capacity(32);
                push_word(&mut out, &other.encode_head_word());
                out
            }
        }
    }

    fn encode_head_word(&self) -> [u8; 32] {
        match self {
            AbiValue::Bytes32(b) => *b,
            AbiValue::Uint(v) => word_from_u64(*v),
            AbiValue::Address(a) => word_from_bytes(a),
            AbiValue::Str(_) | AbiValue::Bytes(_) => [0u8; 32],
        }
    }
}

fn encode_bytes(data: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(32 + data.len().div_ceil(32) * 32);
    out.extend_from_slice(&word_from_u64(data.len() as u64));
    out.extend_from_slice(data);
    let padding = (32 - data.len() % 32) % 32;
    out.extend(std::iter::repeat(0u8).take(padding));
    out
}

/// Encode a call to a function with the given selector and arguments.
pub fn encode_call(selector: [u8; 4], args: &[AbiValue]) -> Vec<u8> {
    let head_len = args.len() * 32;
    let mut heads = Vec::with_capacity(head_len);
    let mut tails = Vec::new();

    for arg in args {
        if arg.is_dynamic() {
            let offset = head_len + tails.len();
            heads.extend_from_slice(&word_from_u64(offset as u64));
            tails.extend_from_slice(&arg.encode_tail());
        } else {
            heads.extend_from_slice(&arg.encode_head_word());
        }
    }

    let mut out = Vec::with_capacity(4 + heads.len() + tails.len());
    out.extend_from_slice(&selector);
    out.extend_from_slice(&heads);
    out.extend_from_slice(&tails);
    out
}

/// Read the 32 byte word at `index`.
fn word_at(data: &[u8], index: usize) -> Option<[u8; 32]> {
    let start = index.checked_mul(32)?;
    let end = start.checked_add(32)?;
    let slice = data.get(start..end)?;
    let mut word = [0u8; 32];
    word.copy_from_slice(slice);
    Ok::<_, ()>(word).ok()?;
    Some(word)
}

fn word_to_usize(word: &[u8; 32]) -> Option<usize> {
    // Reject words that cannot fit in a platform usize.
    if word[..24].iter().any(|b| *b != 0) {
        return None;
    }
    let mut buf = [0u8; 8];
    buf.copy_from_slice(&word[24..]);
    usize::try_from(u64::from_be_bytes(buf)).ok()
}

/// Decode an `address` return value, rejecting a zero result.
pub fn decode_address(data: &[u8]) -> Option<[u8; 20]> {
    let word = word_at(data, 0)?;
    if word[..12].iter().any(|b| *b != 0) {
        return None;
    }
    let mut address = [0u8; 20];
    address.copy_from_slice(&word[12..]);
    if address.iter().all(|b| *b == 0) {
        return None;
    }
    Some(address)
}

/// Decode a `bytes32` return value.
pub fn decode_bytes32(data: &[u8]) -> Option<[u8; 32]> {
    word_at(data, 0)
}

/// Decode a `uint256` return value into a u64, rejecting oversized values.
pub fn decode_uint(data: &[u8]) -> Option<u64> {
    let word = word_at(data, 0)?;
    if word[..24].iter().any(|b| *b != 0) {
        return None;
    }
    let mut buf = [0u8; 8];
    buf.copy_from_slice(&word[24..]);
    Some(u64::from_be_bytes(buf))
}

/// Read the dynamic value whose offset is stored in word `word_index`.
///
/// A dynamic value is a length word followed by the payload, padded to a whole
/// number of 32 byte words.
pub fn decode_dynamic_at(data: &[u8], word_index: usize) -> Option<Vec<u8>> {
    let offset = word_to_usize(&word_at(data, word_index)?)?;
    if offset % 32 != 0 {
        return None;
    }

    let length = word_to_usize(&word_at(data, offset / 32)?)?;
    let start = offset.checked_add(32)?;
    let end = start.checked_add(length)?;
    Some(data.get(start..end)?.to_vec())
}

/// Decode a top level dynamic value, as produced by a function that returns
/// `bytes` or `string`.
pub fn decode_bytes(data: &[u8]) -> Option<Vec<u8>> {
    decode_dynamic_at(data, 0)
}

/// Decode a top level `string` return value, requiring valid UTF-8.
pub fn decode_string(data: &[u8]) -> Option<String> {
    String::from_utf8(decode_bytes(data)?).ok()
}

/// Parse a hex string with an optional `0x` prefix into bytes.
pub fn decode_hex(input: &str) -> Result<Vec<u8>, hex::FromHexError> {
    let trimmed = input.trim().trim_start_matches("0x").trim_start_matches("0X");
    hex::decode(trimmed)
}

/// Render bytes as a lowercase `0x` prefixed hex string.
pub fn encode_hex(data: &[u8]) -> String {
    format!("0x{}", hex::encode(data))
}

/// Render a 20 byte address in EIP-55 mixed case form.
pub fn checksum_address(address: &[u8; 20]) -> String {
    let lower = hex::encode(address);
    let hash = keccak256(lower.as_bytes());
    let mut out = String::with_capacity(42);
    out.push_str("0x");
    for (i, ch) in lower.chars().enumerate() {
        if ch.is_ascii_digit() {
            out.push(ch);
            continue;
        }
        let nibble = if i % 2 == 0 {
            hash[i / 2] >> 4
        } else {
            hash[i / 2] & 0x0f
        };
        if nibble >= 8 {
            out.push(ch.to_ascii_uppercase());
        } else {
            out.push(ch);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keccak_of_empty_input_matches_the_known_constant() {
        assert_eq!(keccak256(b""), EMPTY_KECCAK);
    }

    #[test]
    fn namehash_matches_the_eip137_vectors() {
        assert_eq!(namehash(""), [0u8; 32]);

        // keccak256("eth") is a well known constant.
        assert_eq!(
            hex::encode(namehash("eth")),
            "93cdeb708b7545dc668eb9280176169d1c33cfd8ed6f04690a0bcc88a93fc4ae"
        );
        assert_eq!(
            hex::encode(namehash("foo.eth")),
            "de9b09fd7c5f901e23a3f19fecc54828e9c848539801e86591bd9801b019f84f"
        );
        // A trailing dot is not significant.
        assert_eq!(namehash("foo.eth."), namehash("foo.eth"));
    }

    #[test]
    fn selectors_match_the_published_values() {
        assert_eq!(hex::encode(function_selector("addr(bytes32)")), "3b3b57de");
        assert_eq!(hex::encode(function_selector("name(bytes32)")), "691f3431");
        assert_eq!(hex::encode(function_selector("text(bytes32,string)")), "59d1d43c");
        assert_eq!(hex::encode(function_selector("resolver(bytes32)")), "0178b8bf");
        assert_eq!(hex::encode(function_selector("owner(bytes32)")), "02571be3");
        assert_eq!(hex::encode(function_selector("contenthash(bytes32)")), "bc1c58d1");
    }

    #[test]
    fn address_round_trips_through_encode_and_decode() {
        let addr = [0x11u8; 20];
        let encoded = encode_call(function_selector("addr(bytes32)"), &[AbiValue::Address(addr)]);
        assert_eq!(encoded.len(), 4 + 32);
        assert_eq!(decode_address(&encoded[4..]), Some(addr));
    }

    #[test]
    fn a_static_word_after_the_address_does_not_confuse_the_decoder() {
        // decode_address reads only the first word, so a following argument
        // must not change the result.
        let addr = [0x22u8; 20];
        let encoded = encode_call(
            function_selector("addr(bytes32,uint256)"),
            &[AbiValue::Address(addr), AbiValue::Uint(7)],
        );
        assert_eq!(decode_address(&encoded[4..]), Some(addr));
        assert_eq!(decode_uint(&encoded[4 + 32..]), Some(7));
    }

    #[test]
    fn zero_address_decodes_to_none() {
        assert_eq!(decode_address(&[0u8; 32]), None);
    }

    #[test]
    fn dynamic_string_round_trips() {
        // A lone dynamic argument is encoded as [offset][length][data], which is
        // exactly the shape of a string return value.
        let encoded = encode_call(
            function_selector("text(bytes32,string)"),
            &[AbiValue::Str("hello world".to_string())],
        );
        let body = &encoded[4..];
        assert_eq!(decode_uint(body), Some(32));
        assert_eq!(decode_string(body), Some("hello world".to_string()));
    }

    #[test]
    fn dynamic_argument_offsets_account_for_the_head() {
        // With a static argument first, the dynamic tail must start after both
        // head words.
        let encoded = encode_call(
            function_selector("text(bytes32,string)"),
            &[
                AbiValue::Bytes32([0x01; 32]),
                AbiValue::Str("tigerscan.eth".to_string()),
            ],
        );
        let body = &encoded[4..];
        assert_eq!(decode_bytes32(body), Some([0x01; 32]));
        // The second head word holds the offset of the string tail.
        assert_eq!(decode_uint(&body[32..]), Some(64));
        assert_eq!(
            decode_dynamic_at(body, 1),
            Some(b"tigerscan.eth".to_vec())
        );
    }

    #[test]
    fn a_return_blob_matches_the_solidity_reference_encoding() {
        // Solidity encodes `string` returns as [0x20][length][data padded].
        let expected = hex::decode(concat!(
            "0000000000000000000000000000000000000000000000000000000000000020",
            "000000000000000000000000000000000000000000000000000000000000000d",
            "74696765727363616e2e65746800000000000000000000000000000000000000"
        ))
        .unwrap();

        let encoded = encode_call(
            function_selector("name(bytes32)"),
            &[AbiValue::Str("tigerscan.eth".to_string())],
        );
        assert_eq!(&encoded[4..], expected.as_slice());
        assert_eq!(
            decode_string(&encoded[4..]),
            Some("tigerscan.eth".to_string())
        );
    }

    #[test]
    fn oversized_uint_is_rejected_rather_than_truncated() {
        let mut word = [0u8; 32];
        word[0] = 1;
        assert_eq!(decode_uint(&word), None);
    }

    #[test]
    fn checksum_address_follows_eip55() {
        let addr: [u8; 20] = decode_hex("0x5aAeb6053F3E94C9b9A09f33669435E7Ef1BeAed")
            .unwrap()
            .try_into()
            .unwrap();
        assert_eq!(
            checksum_address(&addr),
            "0x5aAeb6053F3E94C9b9A09f33669435E7Ef1BeAed"
        );
    }
}
