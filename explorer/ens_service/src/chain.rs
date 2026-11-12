//! Shared contract-call helper for the ENS service.
//!
//! Every ENS lookup is a read-only `eth_call` against a registry or resolver
//! contract. The helper builds the calldata, issues the call over JSON-RPC, and
//! decodes the ABI word that comes back.

use ethers::abi::{Detokenize, Tokenize};
use ethers::core::types::{Address, TransactionRequest, U256};
use ethers::providers::{Http, Middleware, Provider};
use ethers::utils::keccak256;

use crate::errors::{Error, Result};

/// A JSON-RPC provider bound to one endpoint.
pub struct Chain {
    provider: Provider<Http>,
}

impl Chain {
    /// Build a client for `rpc_url`.
    pub fn new(rpc_url: &str) -> Result<Self> {
        let url = rpc_url
            .parse::<url::Url>()
            .map_err(|error| Error::registry(format!("invalid RPC url {rpc_url}: {error}")))?;

        Ok(Self {
            provider: Provider::new(Http::new(url)),
        })
    }

    /// The 4-byte selector for a function signature.
    pub fn selector(signature: &str) -> [u8; 4] {
        let hash = keccak256(signature.as_bytes());

        [hash[0], hash[1], hash[2], hash[3]]
    }

    /// Call `to` with `signature` and `args`, returning the raw output bytes.
    ///
    /// `eth_call` is issued directly rather than through the generated bindings
    /// because the registry and resolver are configured at run time.
    pub async fn call_raw<T: Tokenize>(
        &self,
        to: Address,
        signature: &str,
        args: T,
    ) -> Result<Vec<u8>> {
        let mut calldata = Self::selector(signature).to_vec();
        calldata.extend_from_slice(&ethers::abi::encode(&args.into_tokens()));

        let request = TransactionRequest::new().to(to).data(calldata);

        let output = self
            .provider
            .call(&request.into(), None)
            .await
            .map_err(|error| Error::registry(format!("{signature} call failed: {error}")))?;

        Ok(output.to_vec())
    }

    /// Call `to` with `signature` and `args`, decoding a single `address`.
    pub async fn call_address<T: Tokenize>(
        &self,
        to: Address,
        signature: &str,
        args: T,
    ) -> Result<Option<Address>> {
        let raw = self.call_raw(to, signature, args).await?;
        Ok(word_to_address(&raw).and_then(|text| text.parse().ok()))
    }

    /// Call `to` and decode the output through `D`.
    pub async fn call<T: Tokenize, D: Detokenize>(
        &self,
        to: Address,
        signature: &str,
        args: T,
    ) -> Result<D> {
        let raw = self.call_raw(to, signature, args).await?;

        ethers::abi::decode(&[ethers::abi::ParamType::Address], &raw)
            .map_err(|error| {
                Error::registry(format!("{signature} returned malformed data: {error}"))
            })
            .and_then(|tokens| {
                D::from_tokens(tokens).map_err(|error| {
                    Error::registry(format!("{signature} output mismatch: {error}"))
                })
            })
    }

    /// True when the endpoint answers `eth_chainId`, which is how the service
    /// tells a reachable node from a dead URL at startup.
    pub async fn is_reachable(&self) -> bool {
        self.provider.get_chainid().await.is_ok()
    }
}

/// Decode a `bytes32` return value into a hex string.
pub fn word_to_hex(word: &[u8]) -> String {
    format!("0x{}", hex::encode(word))
}

/// Decode a `bytes32` return value into an address, when it holds one.
///
/// The top 12 bytes must be zero and the address must not be the zero address,
/// which is how ENS signals "no record".
pub fn word_to_address(word: &[u8]) -> Option<String> {
    if word.len() != 32 {
        return None;
    }

    if word[..12].iter().any(|byte| *byte != 0) {
        return None;
    }

    let address = Address::from_slice(&word[12..]);
    if address == Address::zero() {
        return None;
    }

    Some(format!("{address:?}"))
}

/// Decode an ABI-encoded `string`, returning `None` for the empty string.
pub fn decode_string(raw: &[u8]) -> Option<String> {
    if raw.len() < 64 {
        return None;
    }

    let offset = U256::from_big_endian(&raw[..32]).as_usize();
    if raw.len() < offset + 32 {
        return None;
    }

    let length = U256::from_big_endian(&raw[offset..offset + 32]).as_usize();
    if raw.len() < offset + 32 + length {
        return None;
    }

    let value = String::from_utf8(raw[offset + 32..offset + 32 + length].to_vec()).ok()?;
    if value.is_empty() {
        None
    } else {
        Some(value)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ethers::abi::Token;

    #[test]
    fn selectors_match_the_known_ens_abis() {
        assert_eq!(Chain::selector("addr(bytes32)"), [0x3b, 0x3b, 0x57, 0xde]);
        assert_eq!(Chain::selector("name(bytes32)"), [0x69, 0x1f, 0x34, 0x31]);
        assert_eq!(
            Chain::selector("resolver(bytes32)"),
            [0x01, 0x78, 0xb8, 0xbf]
        );
        assert_eq!(Chain::selector("owner(bytes32)"), [0x02, 0x57, 0x1b, 0xe3]);
        assert_eq!(
            Chain::selector("text(bytes32,string)"),
            [0x59, 0xd1, 0xd4, 0x3c]
        );
        assert_eq!(
            Chain::selector("contenthash(bytes32)"),
            [0xbc, 0x1c, 0x58, 0xd1]
        );
    }

    #[test]
    fn a_zero_word_is_not_an_address() {
        assert_eq!(word_to_address(&[0u8; 32]), None);
    }

    #[test]
    fn an_address_word_decodes_with_the_prefix() {
        let mut word = [0u8; 32];
        word[12..].copy_from_slice(&[0x11u8; 20]);
        assert_eq!(
            word_to_address(&word),
            Some("0x1111111111111111111111111111111111111111".to_string())
        );
    }

    #[test]
    fn a_dirty_word_is_not_an_address() {
        let mut word = [0u8; 32];
        word[0] = 1;
        assert_eq!(word_to_address(&word), None);
    }

    #[test]
    fn a_string_encoding_round_trips() {
        let encoded = ethers::abi::encode(&[Token::String("hello".to_string())]);
        assert_eq!(decode_string(&encoded), Some("hello".to_string()));
    }

    #[test]
    fn an_empty_string_decodes_to_none() {
        let encoded = ethers::abi::encode(&[Token::String(String::new())]);
        assert_eq!(decode_string(&encoded), None);
    }

    #[test]
    fn truncated_input_is_rejected() {
        assert_eq!(decode_string(&[0u8; 10]), None);
    }
}
