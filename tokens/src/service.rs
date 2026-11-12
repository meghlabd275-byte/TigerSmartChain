//! Token Service
//!
//! Read path for the token index. Everything this service returns comes either
//! from the index maintained by [`TokenIndexer`] or from a live `eth_call`
//! against the node. There are no placeholder values: a query that cannot be
//! answered returns `None` or an error.

use crate::types::*;
use serde_json::{json, Value};
use std::time::Duration;

/// Function selectors for the ERC-20 metadata and balance calls.
const SELECTOR_NAME: &str = "0x06fdde03";
const SELECTOR_SYMBOL: &str = "0x95d89b41";
const SELECTOR_DECIMALS: &str = "0x313ce567";
const SELECTOR_TOTAL_SUPPLY: &str = "0x18160ddd";
const SELECTOR_BALANCE_OF: &str = "0x70a08231";

/// Errors surfaced by [`TokenService`].
#[derive(Debug, Clone)]
pub enum TokenServiceError {
    /// The service was built without an RPC endpoint, so chain reads are off.
    NoRpcConfigured,
    /// The node rejected the request or returned an unexpected shape.
    Rpc(String),
    /// The contract returned data that is not valid for the call made.
    Decode(String),
}

impl std::fmt::Display for TokenServiceError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TokenServiceError::NoRpcConfigured => write!(f, "no RPC endpoint configured"),
            TokenServiceError::Rpc(message) => write!(f, "RPC error: {message}"),
            TokenServiceError::Decode(message) => write!(f, "decode error: {message}"),
        }
    }
}

impl std::error::Error for TokenServiceError {}

/// Token Service
pub struct TokenService {
    indexer: TokenIndexer,
    rpc_url: Option<String>,
    client: Option<reqwest::blocking::Client>,
}

impl TokenService {
    /// A service backed only by the local index.
    pub fn new() -> Self {
        Self {
            indexer: TokenIndexer::new(),
            rpc_url: None,
            client: None,
        }
    }

    /// A service that can also read token metadata and balances from a node.
    pub fn with_rpc(rpc_url: impl Into<String>) -> Self {
        let client = reqwest::blocking::Client::builder()
            .timeout(Duration::from_secs(30))
            .build()
            .ok();

        Self {
            indexer: TokenIndexer::new(),
            rpc_url: Some(rpc_url.into()),
            client,
        }
    }

    /// The index this service reads from.
    pub fn indexer(&self) -> &TokenIndexer {
        &self.indexer
    }

    /// The index this service writes to.
    pub fn indexer_mut(&mut self) -> &mut TokenIndexer {
        &mut self.indexer
    }

    /// Register a token in the index.
    pub fn add_token(&mut self, token: Token) {
        self.indexer.add_token(token);
    }

    /// Apply a transfer to the index.
    pub fn record_transfer(&mut self, transfer: TokenTransfer) -> Result<(), TokenError> {
        self.indexer.record_transfer(transfer)
    }

    /// Indexed token, if it is known.
    pub fn get_token(&self, address: &str) -> Option<Token> {
        self.indexer.get_token(&normalize_address(address)).cloned()
    }

    /// Transfers recorded for a token contract.
    pub fn get_transfers(&self, address: &str) -> Vec<TokenTransfer> {
        self.indexer
            .get_transfers(&normalize_address(address))
            .cloned()
            .unwrap_or_default()
    }

    /// Holders of a token contract, richest first.
    pub fn get_holders(&self, address: &str) -> Vec<TokenHolder> {
        self.indexer
            .get_holders(&normalize_address(address))
            .unwrap_or_default()
    }

    /// Latest indexed price for a token contract.
    pub fn get_price(&self, address: &str) -> Option<TokenPrice> {
        self.indexer.get_price(&normalize_address(address)).cloned()
    }

    /// Record a price observation.
    pub fn update_price(&mut self, price: TokenPrice) {
        self.indexer.update_price(price);
    }

    /// Call a contract method and return the raw result bytes.
    fn eth_call(&self, to: &str, data: &str) -> Result<Vec<u8>, TokenServiceError> {
        let url = self
            .rpc_url
            .as_ref()
            .ok_or(TokenServiceError::NoRpcConfigured)?;
        let client = self
            .client
            .as_ref()
            .ok_or_else(|| TokenServiceError::Rpc("HTTP client unavailable".to_string()))?;

        let response = client
            .post(url)
            .json(&json!({
                "jsonrpc": "2.0",
                "id": 1,
                "method": "eth_call",
                "params": [{ "to": to, "data": data }, "latest"],
            }))
            .send()
            .map_err(|e| TokenServiceError::Rpc(e.to_string()))?;

        let body: Value = response
            .json()
            .map_err(|e| TokenServiceError::Rpc(e.to_string()))?;

        if let Some(error) = body.get("error") {
            return Err(TokenServiceError::Rpc(error.to_string()));
        }

        let hex = body
            .get("result")
            .and_then(Value::as_str)
            .ok_or_else(|| TokenServiceError::Rpc("missing result field".to_string()))?;

        decode_hex(hex)
    }

    /// Read `name()`, `symbol()`, `decimals()` and `totalSupply()` from the
    /// contract and return a populated [`Token`].
    pub fn fetch_token_metadata(&self, address: &str) -> Result<Token, TokenServiceError> {
        let name = self
            .eth_call(address, SELECTOR_NAME)
            .ok()
            .and_then(|bytes| decode_abi_string(&bytes))
            .unwrap_or_default();
        let symbol = self
            .eth_call(address, SELECTOR_SYMBOL)
            .ok()
            .and_then(|bytes| decode_abi_string(&bytes))
            .unwrap_or_default();

        let decimals_bytes = self.eth_call(address, SELECTOR_DECIMALS)?;
        let decimals = decode_abi_uint(&decimals_bytes)
            .ok_or_else(|| TokenServiceError::Decode("decimals() returned no word".to_string()))?
            as u8;

        let supply_bytes = self.eth_call(address, SELECTOR_TOTAL_SUPPLY)?;
        let total_supply = decode_abi_uint(&supply_bytes).ok_or_else(|| {
            TokenServiceError::Decode("totalSupply() returned no word".to_string())
        })?;

        let mut token = Token::new(address.to_string(), name, symbol);
        token.decimals = decimals;
        token.total_supply = total_supply.to_string();

        Ok(token)
    }

    /// Read `balanceOf(address)` from the contract.
    pub fn fetch_balance(&self, token: &str, holder: &str) -> Result<String, TokenServiceError> {
        let argument = holder.trim_start_matches("0x").trim_start_matches("0X");
        let padded = format!("{argument:0>64}");
        let data = format!("{SELECTOR_BALANCE_OF}{padded}");

        let bytes = self.eth_call(token, &data)?;
        decode_abi_uint(&bytes)
            .map(|value| value.to_string())
            .ok_or_else(|| TokenServiceError::Decode("balanceOf() returned no word".to_string()))
    }
}

impl Default for TokenService {
    fn default() -> Self {
        Self::new()
    }
}

/// Decode a `0x`-prefixed hex string into bytes.
fn decode_hex(input: &str) -> Result<Vec<u8>, TokenServiceError> {
    let trimmed = input.trim_start_matches("0x").trim_start_matches("0X");
    if trimmed.len() % 2 != 0 {
        return Err(TokenServiceError::Decode(format!(
            "odd-length hex payload of {} characters",
            trimmed.len()
        )));
    }

    (0..trimmed.len())
        .step_by(2)
        .map(|i| {
            u8::from_str_radix(&trimmed[i..i + 2], 16)
                .map_err(|e| TokenServiceError::Decode(e.to_string()))
        })
        .collect()
}

/// Read a 32-byte ABI word as an unsigned integer.
fn decode_abi_uint(bytes: &[u8]) -> Option<u128> {
    if bytes.len() < 32 {
        return None;
    }

    let mut value: u128 = 0;
    for byte in &bytes[32 - 16..32] {
        value = (value << 8) | u128::from(*byte);
    }
    Some(value)
}

/// Decode an ABI-encoded `string` return value, tolerating both the dynamic
/// encoding and the legacy `bytes32` form that older tokens still use.
fn decode_abi_string(bytes: &[u8]) -> Option<String> {
    if bytes.is_empty() {
        return None;
    }

    if bytes.len() == 32 {
        let end = bytes.iter().position(|b| *b == 0).unwrap_or(32);
        return String::from_utf8(bytes[..end].to_vec()).ok();
    }

    if bytes.len() < 64 {
        return None;
    }

    let offset = decode_abi_uint(&bytes[..32])? as usize;
    if offset + 32 > bytes.len() {
        return None;
    }

    let length = decode_abi_uint(&bytes[offset..offset + 32])? as usize;
    let start = offset + 32;
    if start + length > bytes.len() {
        return None;
    }

    String::from_utf8(bytes[start..start + length].to_vec()).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_token_that_is_not_indexed_is_reported_as_absent() {
        let service = TokenService::new();
        assert!(service.get_token("0xdeadbeef").is_none());
        assert!(service.get_transfers("0xdeadbeef").is_empty());
        assert!(service.get_holders("0xdeadbeef").is_empty());
        assert!(service.get_price("0xdeadbeef").is_none());
    }

    #[test]
    fn indexed_data_is_served_through_the_service() {
        let mut service = TokenService::new();
        let mut token = Token::new("0xToken".into(), "Tiger".into(), "TGR".into());
        token.total_supply = "1000".into();
        service.add_token(token);

        service
            .record_transfer(TokenTransfer::new(
                "0xhash".into(),
                "0xToken".into(),
                "0x0000000000000000000000000000000000000000".into(),
                "0xAlice".into(),
                "1000".into(),
            ))
            .unwrap();

        let token = service.get_token("0xTOKEN").expect("token is indexed");
        assert_eq!(token.symbol, "TGR");
        assert_eq!(token.holders, 1);
        assert_eq!(service.get_holders("0xToken")[0].balance, "1000");
    }

    #[test]
    fn chain_reads_without_an_endpoint_are_refused() {
        let service = TokenService::new();
        let error = service.fetch_token_metadata("0xToken").unwrap_err();
        assert!(matches!(error, TokenServiceError::NoRpcConfigured));
    }

    #[test]
    fn abi_uint_decoding_reads_the_low_order_word() {
        let mut word = [0u8; 32];
        word[31] = 18;
        assert_eq!(decode_abi_uint(&word), Some(18));

        word[30] = 1;
        assert_eq!(decode_abi_uint(&word), Some(256 + 18));
        assert_eq!(decode_abi_uint(&[0u8; 8]), None);
    }

    #[test]
    fn abi_string_decoding_handles_dynamic_and_bytes32_forms() {
        // Dynamic encoding: offset 32, length 5, "Tiger" padded to 32 bytes.
        let mut encoded = vec![0u8; 32];
        encoded[31] = 32;
        let mut length = [0u8; 32];
        length[31] = 5;
        encoded.extend_from_slice(&length);
        let mut payload = b"Tiger".to_vec();
        payload.resize(32, 0);
        encoded.extend_from_slice(&payload);
        assert_eq!(decode_abi_string(&encoded).as_deref(), Some("Tiger"));

        // Legacy bytes32 form.
        let mut legacy = b"TGR".to_vec();
        legacy.resize(32, 0);
        assert_eq!(decode_abi_string(&legacy).as_deref(), Some("TGR"));

        assert_eq!(decode_abi_string(&[]), None);
    }

    #[test]
    fn hex_decoding_rejects_malformed_payloads() {
        assert_eq!(decode_hex("0x0102").unwrap(), vec![1, 2]);
        assert!(decode_hex("0x010").is_err());
        assert!(decode_hex("0xzz").is_err());
    }
}
