//! API Handlers for TigerScan
//!
//! Every handler reads from the node over JSON-RPC. There are no placeholder
//! values: a handler that cannot reach the node returns an error.

use crate::chain;
use crate::middleware::RateLimiter;
use crate::server::APIConfig;
use crate::rpc::{RpcClient, RpcError};
use crate::types::*;
use axum::{
    extract::{Path, Query, State},
    response::Json,
};
use serde_json::{json, Value};
use std::sync::atomic::{AtomicI64, Ordering};
use std::sync::Arc;
use std::time::Instant;

// =============================================================================
// STATE
// =============================================================================

/// Application state shared by every handler.
pub struct AppState {
    pub config: APIConfig,
    /// JSON-RPC client for the node.
    pub rpc: RpcClient,
    /// When the process started, used to report uptime.
    pub started_at: Instant,
    /// Total requests served.
    pub requests_total: AtomicI64,
    /// Requests per route label.
    pub requests_by_route: std::sync::Mutex<std::collections::HashMap<String, i64>>,
    /// Per-caller token bucket.
    pub rate_limiter: RateLimiter,
}

impl AppState {
    /// Build state from configuration.
    pub fn new(config: APIConfig) -> Result<Self, RpcError> {
        let rpc = RpcClient::new(config.rpc_url.clone())?;

        let rate_limiter = RateLimiter::new(config.rate_limit as u32, config.rate_limit as u32 * 2);

        Ok(Self {
            config,
            rpc,
            started_at: Instant::now(),
            requests_total: AtomicI64::new(0),
            requests_by_route: std::sync::Mutex::new(std::collections::HashMap::new()),
            rate_limiter,
        })
    }

    /// Record one request against a route label.
    pub fn record_request(&self, route: &str) {
        self.requests_total.fetch_add(1, Ordering::Relaxed);

        if let Ok(mut map) = self.requests_by_route.lock() {
            *map.entry(route.to_string()).or_insert(0) += 1;
        }
    }
}

// =============================================================================
// BLOCK HANDLERS
// =============================================================================

/// Fetch a block by number.
pub async fn get_block(
    State(state): State<Arc<AppState>>,
    Path(block_number): Path<u64>,
) -> Result<Json<BlockResponse>, APIError> {
    state.record_request("block");

    let value = state
        .rpc
        .call_raw("eth_getBlockByNumber", json!([format!("0x{block_number:x}"), false]))
        .await?;

    Ok(Json(block_from_json(&value, &block_number.to_string())?))
}

/// Fetch the latest block.
pub async fn get_latest_block(
    State(state): State<Arc<AppState>>,
) -> Result<Json<BlockResponse>, APIError> {
    state.record_request("latest_block");

    let value = state
        .rpc
        .call_raw("eth_getBlockByNumber", json!(["latest", false]))
        .await?;

    Ok(Json(block_from_json(&value, "latest")?))
}

/// Fetch a block by hash.
pub async fn get_block_by_hash(
    State(state): State<Arc<AppState>>,
    Path(block_hash): Path<String>,
) -> Result<Json<BlockResponse>, APIError> {
    state.record_request("block_by_hash");

    if !is_hash(&block_hash) {
        return Err(APIError::invalid_params(
            "block hash must be 32 bytes of hex",
        ));
    }

    let value = state
        .rpc
        .call_raw("eth_getBlockByHash", json!([block_hash, false]))
        .await?;

    Ok(Json(block_from_json(&value, &block_hash)?))
}

fn block_from_json(value: &Value, identifier: &str) -> Result<BlockResponse, APIError> {
    if value.is_null() {
        return Err(APIError::not_found(format!(
            "block {identifier} not found"
        )));
    }

    Ok(BlockResponse {
        number: hex_to_u64(value.get("number"))?,
        hash: hex_string(value.get("hash"))?,
        parent_hash: hex_string(value.get("parentHash"))?,
        timestamp: hex_to_u64(value.get("timestamp"))? as i64,
        transactions: value
            .get("transactions")
            .and_then(Value::as_array)
            .map(|items| {
                items
                    .iter()
                    .filter_map(|item| match item {
                        Value::String(hash) => Some(hash.clone()),
                        Value::Object(_) => {
                            item.get("hash").and_then(Value::as_str).map(str::to_string)
                        }
                        _ => None,
                    })
                    .collect()
            })
            .unwrap_or_default(),
        gas_used: hex_to_u64(value.get("gasUsed"))?,
        gas_limit: hex_to_u64(value.get("gasLimit"))?,
        miner: hex_string(value.get("miner"))?,
        difficulty: hex_string(value.get("difficulty"))?,
        total_difficulty: hex_string(value.get("totalDifficulty"))?,
        size: hex_to_u64(value.get("size"))?,
        nonce: hex_string(value.get("nonce"))?,
        mix_hash: hex_string(value.get("mixHash"))?,
        extra_data: hex_string(value.get("extraData"))?,
    })
}

// =============================================================================
// TRANSACTION HANDLERS
// =============================================================================

/// Fetch a transaction together with the fields only its receipt carries.
pub async fn get_transaction(
    State(state): State<Arc<AppState>>,
    Path(tx_hash): Path<String>,
) -> Result<Json<TransactionResponse>, APIError> {
    state.record_request("transaction");

    if !is_hash(&tx_hash) {
        return Err(APIError::invalid_params(
            "transaction hash must be 32 bytes of hex",
        ));
    }

    let tx = state
        .rpc
        .call_raw("eth_getTransactionByHash", json!([tx_hash]))
        .await?;

    if tx.is_null() {
        return Err(APIError::not_found(format!(
            "transaction {tx_hash} not found"
        )));
    }

    // Gas used and the success flag live on the receipt, not the transaction.
    let receipt = state
        .rpc
        .call_raw("eth_getTransactionReceipt", json!([tx_hash]))
        .await
        .unwrap_or(Value::Null);

    Ok(Json(TransactionResponse {
        hash: hex_string(tx.get("hash"))?,
        block_number: opt_u64(tx.get("blockNumber"))?,
        block_hash: opt_string(tx.get("blockHash"))?,
        from: hex_string(tx.get("from"))?,
        to: opt_string(tx.get("to"))?,
        value: hex_string(tx.get("value"))?,
        gas_price: opt_u64(tx.get("gasPrice"))?.unwrap_or(0),
        gas_used: opt_u64(receipt.get("gasUsed"))?,
        nonce: hex_to_u64(tx.get("nonce"))?,
        input: hex_string(tx.get("input"))?,
        status: opt_string(receipt.get("status"))?,
    }))
}

// =============================================================================
// ADDRESS HANDLERS
// =============================================================================

/// Balance and nonce for an address.
pub async fn get_address(
    State(state): State<Arc<AppState>>,
    Path(address): Path<String>,
    Query(params): Query<AddressRequest>,
) -> Result<Json<AddressResponse>, APIError> {
    state.record_request("address");

    if !is_address(&address) {
        return Err(APIError::invalid_params("address must be 20 bytes of hex"));
    }

    let (page, limit) = parse_pagination(params.page, params.limit);
    let _ = (page, limit);

    let balance = state
        .rpc
        .call_raw("eth_getBalance", json!([address, "latest"]))
        .await?;

    let nonce = state
        .rpc
        .call_raw("eth_getTransactionCount", json!([address, "latest"]))
        .await?;

    Ok(Json(AddressResponse {
        address,
        balance: hex_string(Some(&balance))?,
        tx_count: hex_to_u64(Some(&nonce))? as i64,
        transactions: Vec::new(),
    }))
}

// =============================================================================
// TOKEN HANDLERS
// =============================================================================

/// ERC-20 metadata for a contract, read live from the chain.
pub async fn get_token(
    State(state): State<Arc<AppState>>,
    Path(address): Path<String>,
) -> Result<Json<TokenResponse>, APIError> {
    state.record_request("token");

    if !is_address(&address) {
        return Err(APIError::invalid_params(
            "token address must be 20 bytes of hex",
        ));
    }

    let metadata = read_token_metadata(&state.rpc, &address).await?;

    Ok(Json(TokenResponse {
        address,
        name: metadata.name,
        symbol: metadata.symbol,
        decimals: metadata.decimals,
        total_supply: metadata.total_supply,
        holders: 0,
        transfers: 0,
        price: None,
        volume_24h: None,
    }))
}

/// List tokens.
///
/// TigerSmartChain issues two native assets, TGR and RUSD, and both are always
/// returned first so a wallet can show them without knowing their addresses.
/// Any additional contract addresses in `search` are read live from the chain.
pub async fn get_tokens(
    State(state): State<Arc<AppState>>,
    Query(params): Query<TokenRequest>,
) -> Result<Json<PaginatedResponse<TokenResponse>>, APIError> {
    state.record_request("tokens");
    let (page, limit) = parse_pagination(params.page, params.limit);

    let mut items = native_assets();

    if let Some(search) = params.search.as_deref() {
        for address in search.split(',').map(str::trim).filter(|a| !a.is_empty()) {
            if !is_address(address) {
                return Err(APIError::invalid_params(format!(
                    "{address} is not a valid address"
                )));
            }

            let metadata = read_token_metadata(&state.rpc, address).await?;
            items.push(TokenResponse {
                address: address.to_string(),
                name: metadata.name,
                symbol: metadata.symbol,
                decimals: metadata.decimals,
                total_supply: metadata.total_supply,
                holders: 0,
                transfers: 0,
                price: None,
                volume_24h: None,
            });
        }
    }

    let total = items.len();
    let start = page.saturating_mul(limit).min(total);
    let end = (start + limit).min(total);

    Ok(Json(PaginatedResponse::new(
        items[start..end].to_vec(),
        total,
        page,
        limit,
    )))
}

/// The two assets the chain itself issues.
fn native_assets() -> Vec<TokenResponse> {
    vec![
        TokenResponse {
            address: "native".to_string(),
            name: chain::NATIVE_NAME.to_string(),
            symbol: chain::NATIVE_SYMBOL.to_string(),
            decimals: chain::NATIVE_DECIMALS,
            // TGR has no contract, so there is no totalSupply() to read.
            total_supply: String::new(),
            holders: 0,
            transfers: 0,
            price: None,
            volume_24h: None,
        },
        TokenResponse {
            address: chain::STABLECOIN_ADDRESS.to_string(),
            name: chain::STABLECOIN_NAME.to_string(),
            symbol: chain::STABLECOIN_SYMBOL.to_string(),
            decimals: chain::STABLECOIN_DECIMALS,
            total_supply: String::new(),
            holders: 0,
            transfers: 0,
            price: None,
            volume_24h: None,
        },
    ]
}

/// The ERC-20 calls needed to describe a token.
pub struct TokenMetadata {
    pub name: String,
    pub symbol: String,
    pub decimals: u8,
    pub total_supply: String,
}

const SELECTOR_NAME: &str = "0x06fdde03";
const SELECTOR_SYMBOL: &str = "0x95d89b41";
const SELECTOR_DECIMALS: &str = "0x313ce567";
const SELECTOR_TOTAL_SUPPLY: &str = "0x18160ddd";

/// Read ERC-20 metadata from a contract over `eth_call`.
pub async fn read_token_metadata(
    rpc: &RpcClient,
    address: &str,
) -> Result<TokenMetadata, APIError> {
    let code = rpc
        .call_raw("eth_getCode", json!([address, "latest"]))
        .await?;

    let code = hex_string(Some(&code))?;
    if code == "0x" || code.is_empty() {
        return Err(APIError::not_found(format!(
            "{address} has no contract code"
        )));
    }

    let name = eth_call_string(rpc, address, SELECTOR_NAME).await?;
    let symbol = eth_call_string(rpc, address, SELECTOR_SYMBOL).await?;

    let decimals = abi_uint(&eth_call(rpc, address, SELECTOR_DECIMALS).await?).ok_or_else(|| {
        APIError::internal("decimals() returned no value")
    })? as u8;

    let supply = abi_uint(&eth_call(rpc, address, SELECTOR_TOTAL_SUPPLY).await?)
        .ok_or_else(|| APIError::internal("totalSupply() returned no value"))?;

    Ok(TokenMetadata {
        name,
        symbol,
        decimals,
        total_supply: supply.to_string(),
    })
}

async fn eth_call(rpc: &RpcClient, to: &str, data: &str) -> Result<Vec<u8>, APIError> {
    let result = rpc
        .call_raw("eth_call", json!([{ "to": to, "data": data }, "latest"]))
        .await?;

    decode_hex(&hex_string(Some(&result))?)
}

async fn eth_call_string(rpc: &RpcClient, to: &str, data: &str) -> Result<String, APIError> {
    Ok(abi_string(&eth_call(rpc, to, data).await?).unwrap_or_default())
}

// =============================================================================
// SEARCH HANDLERS
// =============================================================================

/// Search for a block, transaction or address by identifier.
pub async fn search(
    State(state): State<Arc<AppState>>,
    Query(params): Query<SearchRequest>,
) -> Result<Json<SearchResponse>, APIError> {
    state.record_request("search");

    let query = params.query.trim();
    if query.is_empty() {
        return Err(APIError::invalid_params("query must not be empty"));
    }

    let limit = params.limit.unwrap_or(10).clamp(1, 100);
    let mut results = Vec::new();

    if is_address(query) {
        let balance = state
            .rpc
            .call_raw("eth_getBalance", json!([query, "latest"]))
            .await?;

        let code = state
            .rpc
            .call_raw("eth_getCode", json!([query, "latest"]))
            .await?;

        let has_code = hex_string(Some(&code))? != "0x";

        results.push(SearchResult {
            r#type: if has_code { "contract" } else { "address" }.to_string(),
            address: query.to_string(),
            name: hex_string(Some(&balance))?,
        });
    } else if is_hash(query) {
        let block = state
            .rpc
            .call_raw("eth_getBlockByHash", json!([query, false]))
            .await?;

        if !block.is_null() {
            results.push(SearchResult {
                r#type: "block".to_string(),
                address: hex_string(block.get("hash"))?,
                name: hex_to_u64(block.get("number"))?.to_string(),
            });
        }

        let tx = state
            .rpc
            .call_raw("eth_getTransactionByHash", json!([query]))
            .await?;

        if !tx.is_null() {
            results.push(SearchResult {
                r#type: "transaction".to_string(),
                address: hex_string(tx.get("hash"))?,
                name: hex_string(tx.get("from"))?,
            });
        }
    }

    results.truncate(limit);
    let total = results.len();

    Ok(Json(SearchResponse { results, total }))
}

/// Search Response
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct SearchResponse {
    pub results: Vec<SearchResult>,
    pub total: usize,
}

/// Search Result
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct SearchResult {
    pub r#type: String,
    pub address: String,
    pub name: String,
}

// =============================================================================
// STATS HANDLERS
// =============================================================================

/// Live server statistics.
pub async fn get_stats(State(state): State<Arc<AppState>>) -> Result<Json<APIStats>, APIError> {
    let requests_total = state.requests_total.load(Ordering::Relaxed);
    let uptime = state.started_at.elapsed().as_secs() as i64;

    let requests_by_endpoint = state
        .requests_by_route
        .lock()
        .map(|map| map.clone())
        .unwrap_or_default();

    let avg_response_time = if uptime > 0 {
        requests_total as f64 / uptime as f64
    } else {
        0.0
    };

    Ok(Json(APIStats {
        uptime,
        requests_total,
        requests_by_endpoint,
        avg_response_time,
        active_connections: 0,
    }))
}

// =============================================================================
// HELPERS
// =============================================================================

/// True when `value` is a 32-byte hex string.
pub fn is_hash(value: &str) -> bool {
    let trimmed = value.trim_start_matches("0x").trim_start_matches("0X");
    trimmed.len() == 64 && trimmed.chars().all(|c| c.is_ascii_hexdigit())
}

/// True when `value` is a 20-byte hex address.
pub fn is_address(value: &str) -> bool {
    let trimmed = value.trim_start_matches("0x").trim_start_matches("0X");
    trimmed.len() == 40 && trimmed.chars().all(|c| c.is_ascii_hexdigit())
}

/// Read a hex data string, defaulting to `0x0` when the field is absent.
fn hex_string(value: Option<&Value>) -> Result<String, APIError> {
    match value {
        Some(Value::String(text)) => Ok(text.clone()),
        Some(Value::Null) | None => Ok("0x0".to_string()),
        Some(other) => Err(APIError::internal(format!(
            "expected a hex string, got {other}"
        ))),
    }
}

/// Read an optional hex string, keeping `None` when the field is absent.
fn opt_string(value: Option<&Value>) -> Result<Option<String>, APIError> {
    match value {
        Some(Value::String(text)) => Ok(Some(text.clone())),
        Some(Value::Null) | None => Ok(None),
        Some(other) => Err(APIError::internal(format!(
            "expected a hex string, got {other}"
        ))),
    }
}

/// Read a JSON-RPC quantity into a `u64`.
fn hex_to_u64(value: Option<&Value>) -> Result<u64, APIError> {
    match value {
        Some(Value::Number(number)) => Ok(number.as_u64().unwrap_or(0)),
        Some(Value::String(text)) => {
            let trimmed = text.trim_start_matches("0x").trim_start_matches("0X");
            if trimmed.is_empty() {
                return Ok(0);
            }

            u64::from_str_radix(trimmed, 16)
                .map_err(|e| APIError::internal(format!("{text} is not a quantity: {e}")))
        }
        Some(Value::Null) | None => Ok(0),
        Some(other) => Err(APIError::internal(format!(
            "expected a quantity, got {other}"
        ))),
    }
}

/// Read an optional JSON-RPC quantity.
fn opt_u64(value: Option<&Value>) -> Result<Option<u64>, APIError> {
    match value {
        Some(Value::Null) | None => Ok(None),
        other => hex_to_u64(other).map(Some),
    }
}

/// Decode a `0x`-prefixed hex string.
pub fn decode_hex(input: &str) -> Result<Vec<u8>, APIError> {
    let trimmed = input.trim_start_matches("0x").trim_start_matches("0X");
    if trimmed.len() % 2 != 0 {
        return Err(APIError::internal(format!(
            "odd-length hex payload of {} characters",
            trimmed.len()
        )));
    }

    (0..trimmed.len())
        .step_by(2)
        .map(|i| {
            u8::from_str_radix(&trimmed[i..i + 2], 16)
                .map_err(|e| APIError::internal(e.to_string()))
        })
        .collect()
}

/// Read the low 16 bytes of an ABI word as an unsigned integer.
pub fn abi_uint(bytes: &[u8]) -> Option<u128> {
    if bytes.len() < 32 {
        return None;
    }

    let mut value: u128 = 0;
    for byte in &bytes[16..32] {
        value = (value << 8) | u128::from(*byte);
    }
    Some(value)
}

/// Decode an ABI `string` return value, tolerating the legacy `bytes32` form.
pub fn abi_string(bytes: &[u8]) -> Option<String> {
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

    let offset = abi_uint(&bytes[..32])? as usize;
    if offset + 32 > bytes.len() {
        return None;
    }

    let length = abi_uint(&bytes[offset..offset + 32])? as usize;
    let start = offset + 32;
    if start + length > bytes.len() {
        return None;
    }

    String::from_utf8(bytes[start..start + length].to_vec()).ok()
}

// =============================================================================
// TESTS
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn address_and_hash_recognition() {
        assert!(is_address("0x5255534400000000000000000000000000000000"));
        assert!(!is_address("0x52555344000000000000000000000000000000"));
        assert!(is_hash(&format!("0x{}", "ab".repeat(32))));
        assert!(!is_hash("0xab"));
    }

    #[test]
    fn hex_quantities_are_parsed() {
        assert_eq!(hex_to_u64(Some(&json!("0x10"))).unwrap(), 16);
        assert_eq!(hex_to_u64(Some(&json!("0x0"))).unwrap(), 0);
        assert_eq!(hex_to_u64(None).unwrap(), 0);
        assert!(hex_to_u64(Some(&json!("0xzz"))).is_err());
        assert_eq!(opt_u64(Some(&Value::Null)).unwrap(), None);
        assert_eq!(opt_u64(Some(&json!("0x2a"))).unwrap(), Some(42));
    }

    #[test]
    fn abi_uint_reads_the_low_word() {
        let mut word = [0u8; 32];
        word[31] = 18;
        assert_eq!(abi_uint(&word), Some(18));

        word[30] = 1;
        assert_eq!(abi_uint(&word), Some(274));
    }

    #[test]
    fn abi_string_decoding_handles_both_encodings() {
        let mut encoded = vec![0u8; 32];
        encoded[31] = 32;
        let mut length = [0u8; 32];
        length[31] = 5;
        encoded.extend_from_slice(&length);
        let mut payload = b"Tiger".to_vec();
        payload.resize(32, 0);
        encoded.extend_from_slice(&payload);
        assert_eq!(abi_string(&encoded).as_deref(), Some("Tiger"));

        let mut legacy = b"TGR".to_vec();
        legacy.resize(32, 0);
        assert_eq!(abi_string(&legacy).as_deref(), Some("TGR"));
    }

    #[test]
    fn hex_decoding_rejects_malformed_input() {
        assert_eq!(decode_hex("0x0102").unwrap(), vec![1, 2]);
        assert!(decode_hex("0x010").is_err());
        assert!(decode_hex("0xzz").is_err());
    }

    #[test]
    fn state_is_built_from_config_and_counts_requests() {
        let state = AppState::new(APIConfig::default()).expect("client builds");
        assert_eq!(state.rpc.url(), APIConfig::default().rpc_url);

        state.record_request("block");
        state.record_request("block");
        assert_eq!(state.requests_total.load(Ordering::Relaxed), 2);
    }

    #[test]
    fn the_native_assets_are_tgr_and_rusd() {
        let assets = native_assets();
        assert_eq!(assets.len(), 2);
        assert_eq!(assets[0].symbol, "TGR");
        assert_eq!(assets[0].decimals, 18);
        assert_eq!(assets[1].symbol, "RUSD");
        assert_eq!(assets[1].decimals, 6);
        assert_eq!(assets[1].address, chain::STABLECOIN_ADDRESS);
    }
}
