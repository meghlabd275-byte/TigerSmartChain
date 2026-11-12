//! Token Analytics Service - Allowance Tracking & Transfer Graph
//!
//! Real token analytics backed by a JSON-RPC node:
//! - ERC-20 `allowance(owner,spender)` via `eth_call`
//! - `Transfer(address,address,uint256)` log scanning via `eth_getLogs`
//! - Transfer graph aggregation (real integer arithmetic)
//! - Price history via the CoinGecko market-chart API

use std::sync::Arc;
use tokio::sync::RwLock;
use serde::{Deserialize, Serialize};
use thiserror::Error;

/// keccak256("Transfer(address,address,uint256)")
pub const TRANSFER_TOPIC: &str =
    "0xddf252ad1be2c89b69c2b068fc378daa952ba7f163c4a11628f55a4df523b3ef";

/// ERC-20 `allowance(address,address)` function selector.
pub const ALLOWANCE_SELECTOR: &str = "0xdd62ed3e";

// =============================================================================
// ERRORS
// =============================================================================

#[derive(Error, Debug)]
pub enum TokenAnalyticsError {
    #[error("RPC error: {0}")]
    RpcError(String),
    #[error("Parse error: {0}")]
    ParseError(String),
    #[error("Not found: {0}")]
    NotFound(String),
    #[error("Invalid input: {0}")]
    InvalidInput(String),
}

// =============================================================================
// CONFIGURATION
// =============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenAnalyticsConfig {
    pub rpc_url: String,
    pub database_url: String,
}

impl Default for TokenAnalyticsConfig {
    fn default() -> Self {
        Self {
            rpc_url: std::env::var("RPC_URL").unwrap_or_else(|_| "http://localhost:8545".to_string()),
            database_url: std::env::var("DATABASE_URL")
                .unwrap_or_else(|_| "postgres://localhost:5432/tigerscan".to_string()),
        }
    }
}

// =============================================================================
// TOKEN ANALYTICS TYPES
// =============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenAllowance {
    pub owner: String,
    pub spender: String,
    pub token: String,
    pub allowance: String,
    pub block_number: u64,
    pub last_updated: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TransferEvent {
    pub from: String,
    pub to: String,
    pub value: String,
    pub token: String,
    pub tx_hash: String,
    pub block_number: u64,
    pub timestamp: i64,
    pub log_index: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TransferGraph {
    pub nodes: Vec<TransferNode>,
    pub edges: Vec<TransferEdge>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TransferNode {
    pub address: String,
    pub total_sent: String,
    pub total_received: String,
    pub tx_count: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TransferEdge {
    pub from: String,
    pub to: String,
    pub value: String,
    pub count: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PriceHistory {
    pub prices: Vec<PricePoint>,
    pub avg_24h: f64,
    pub change_24h: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PricePoint {
    pub price: f64,
    pub timestamp: i64,
    pub volume: f64,
}

// =============================================================================
// PURE HELPERS (unit tested, no I/O)
// =============================================================================

/// Encode a hex quantity into a 32-byte, zero-padded ABI word.
/// Returns an error for malformed input or values that do not fit in 32 bytes.
pub fn encode_uint256(value: &str) -> Result<String, TokenAnalyticsError> {
    let trimmed = value.trim();
    let raw = trimmed
        .strip_prefix("0x")
        .or_else(|| trimmed.strip_prefix("0X"))
        .unwrap_or(trimmed);

    if raw.is_empty() {
        return Ok(format!("0x{}", "0".repeat(64)));
    }
    if raw.len() > 64 {
        return Err(TokenAnalyticsError::InvalidInput(format!(
            "value exceeds 32 bytes: {value}"
        )));
    }
    if !raw.chars().all(|c| c.is_ascii_hexdigit()) {
        return Err(TokenAnalyticsError::InvalidInput(format!(
            "non-hexadecimal value: {value}"
        )));
    }

    let lower = raw.to_ascii_lowercase();
    Ok(format!("0x{:0>64}", lower))
}

/// Encode a 20-byte EVM address as a 32-byte left-padded ABI word.
pub fn encode_address(address: &str) -> Result<String, TokenAnalyticsError> {
    let trimmed = address.trim();
    let raw = trimmed
        .strip_prefix("0x")
        .or_else(|| trimmed.strip_prefix("0X"))
        .ok_or_else(|| {
            TokenAnalyticsError::InvalidInput(format!("address missing 0x prefix: {address}"))
        })?;

    if raw.len() != 40 || !raw.chars().all(|c| c.is_ascii_hexdigit()) {
        return Err(TokenAnalyticsError::InvalidInput(format!(
            "invalid 20-byte address: {address}"
        )));
    }

    Ok(format!("0x{:0>64}", raw.to_ascii_lowercase()))
}

/// Build the `eth_call` calldata for ERC-20 `allowance(owner,spender)`.
pub fn encode_allowance_call(owner: &str, spender: &str) -> Result<String, TokenAnalyticsError> {
    let owner_word = encode_address(owner)?;
    let spender_word = encode_address(spender)?;
    Ok(format!(
        "{}{}{}",
        ALLOWANCE_SELECTOR,
        &owner_word[2..],
        &spender_word[2..]
    ))
}

/// Parse a hex quantity (`0x`-prefixed or bare) into a u128 with overflow checking.
pub fn parse_uint256(value: &str) -> Result<u128, TokenAnalyticsError> {
    let trimmed = value.trim();
    let raw = trimmed
        .strip_prefix("0x")
        .or_else(|| trimmed.strip_prefix("0X"))
        .unwrap_or(trimmed);

    if raw.is_empty() {
        return Ok(0);
    }
    if raw.len() > 32 {
        return Err(TokenAnalyticsError::ParseError(format!(
            "value does not fit in 128 bits: {value}"
        )));
    }
    if !raw.chars().all(|c| c.is_ascii_hexdigit()) {
        return Err(TokenAnalyticsError::ParseError(format!(
            "non-hexadecimal value: {value}"
        )));
    }

    u128::from_str_radix(raw, 16)
        .map_err(|e| TokenAnalyticsError::ParseError(format!("invalid hex {value}: {e}")))
}

/// Render a u128 as a canonical `0x`-prefixed hex quantity.
pub fn format_uint256(value: u128) -> String {
    format!("0x{value:x}")
}

/// Extract the 20-byte address embedded in an indexed log topic (last 40 hex chars).
pub fn topic_to_address(topic: &str) -> Result<String, TokenAnalyticsError> {
    let raw = topic
        .strip_prefix("0x")
        .or_else(|| topic.strip_prefix("0X"))
        .unwrap_or(topic);

    if raw.len() != 64 {
        return Err(TokenAnalyticsError::ParseError(format!(
            "topic is not 32 bytes: {topic}"
        )));
    }
    if !raw.chars().all(|c| c.is_ascii_hexdigit()) {
        return Err(TokenAnalyticsError::ParseError(format!(
            "non-hexadecimal topic: {topic}"
        )));
    }

    Ok(format!("0x{}", &raw[24..].to_ascii_lowercase()))
}

/// Aggregate transfer events into a transfer graph using real integer arithmetic.
pub fn build_transfer_graph(transfers: &[TransferEvent]) -> TransferGraph {
    let mut nodes_map: std::collections::HashMap<String, (u128, u128, u64)> =
        std::collections::HashMap::new();
    let mut edges_map: std::collections::HashMap<(String, String), (u128, u64)> =
        std::collections::HashMap::new();

    for transfer in transfers {
        let value = parse_uint256(&transfer.value).unwrap_or(0);

        let from_entry = nodes_map
            .entry(transfer.from.clone())
            .or_insert((0u128, 0u128, 0u64));
        from_entry.0 += value; // total sent
        from_entry.2 += 1;

        let to_entry = nodes_map
            .entry(transfer.to.clone())
            .or_insert((0u128, 0u128, 0u64));
        to_entry.1 += value; // total received
        to_entry.2 += 1;

        let edge = edges_map
            .entry((transfer.from.clone(), transfer.to.clone()))
            .or_insert((0u128, 0u64));
        edge.0 += value;
        edge.1 += 1;
    }

    let mut nodes: Vec<TransferNode> = nodes_map
        .into_iter()
        .map(|(address, (sent, received, count))| TransferNode {
            address,
            total_sent: format_uint256(sent),
            total_received: format_uint256(received),
            tx_count: count,
        })
        .collect();
    nodes.sort_by(|a, b| a.address.cmp(&b.address));

    let mut edges: Vec<TransferEdge> = edges_map
        .into_iter()
        .map(|((from, to), (value, count))| TransferEdge {
            from,
            to,
            value: format_uint256(value),
            count,
        })
        .collect();
    edges.sort_by(|a, b| (&a.from, &a.to).cmp(&(&b.from, &b.to)));

    TransferGraph { nodes, edges }
}

// =============================================================================
// TOKEN ANALYTICS SERVICE
// =============================================================================

pub struct TokenAnalyticsService {
    config: TokenAnalyticsConfig,
    client: reqwest::Client,
    cache: Arc<RwLock<TokenCache>>,
}

#[derive(Debug, Default)]
pub struct TokenCache {
    pub allowances: std::collections::HashMap<String, Vec<TokenAllowance>>,
    pub transfers: std::collections::HashMap<String, Vec<TransferEvent>>,
    pub last_update: i64,
}

impl TokenAnalyticsService {
    pub fn new(config: TokenAnalyticsConfig) -> Self {
        Self {
            config,
            client: reqwest::Client::new(),
            cache: Arc::new(RwLock::new(TokenCache::default())),
        }
    }

    async fn rpc_call(
        &self,
        method: &str,
        params: serde_json::Value,
    ) -> Result<serde_json::Value, TokenAnalyticsError> {
        let request = serde_json::json!({
            "jsonrpc": "2.0",
            "method": method,
            "params": params,
            "id": 1
        });

        let response = self
            .client
            .post(&self.config.rpc_url)
            .json(&request)
            .send()
            .await
            .map_err(|e| TokenAnalyticsError::RpcError(e.to_string()))?;

        let body: serde_json::Value = response
            .json()
            .await
            .map_err(|e| TokenAnalyticsError::ParseError(e.to_string()))?;

        if let Some(err) = body.get("error") {
            return Err(TokenAnalyticsError::RpcError(err.to_string()));
        }

        body.get("result")
            .cloned()
            .ok_or_else(|| TokenAnalyticsError::RpcError("RPC response missing result".to_string()))
    }

    /// Get the ERC-20 allowance of `spender` over `owner`'s balance for `token`.
    pub async fn get_allowance(
        &self,
        owner: &str,
        spender: &str,
        token: &str,
    ) -> Result<TokenAllowance, TokenAnalyticsError> {
        let data = encode_allowance_call(owner, spender)?;

        let result = self
            .rpc_call(
                "eth_call",
                serde_json::json!([{"to": token, "data": data}, "latest"]),
            )
            .await?;

        let raw = result.as_str().ok_or_else(|| {
            TokenAnalyticsError::ParseError("eth_call result is not a string".to_string())
        })?;

        // Reject a truncated result rather than silently reporting zero.
        if raw.len() > 2 && raw.len() < 66 {
            return Err(TokenAnalyticsError::ParseError(format!(
                "eth_call returned a short allowance word: {raw}"
            )));
        }

        let allowance = parse_uint256(raw)?;
        let block_number = self.get_block_number().await.unwrap_or(0);

        Ok(TokenAllowance {
            owner: owner.to_string(),
            spender: spender.to_string(),
            token: token.to_string(),
            allowance: format_uint256(allowance),
            block_number,
            last_updated: now_secs(),
        })
    }

    /// Get the allowance for one owner across many spenders for a given token.
    pub async fn get_allowances(
        &self,
        owner: &str,
        token: &str,
        spenders: &[String],
    ) -> Result<Vec<TokenAllowance>, TokenAnalyticsError> {
        let mut allowances = Vec::with_capacity(spenders.len());
        for spender in spenders {
            allowances.push(self.get_allowance(owner, spender, token).await?);
        }
        Ok(allowances)
    }

    /// Fetch raw `Transfer` logs where `address` is the sender or the recipient.
    async fn fetch_transfer_logs(
        &self,
        address: &str,
        limit: u32,
    ) -> Result<Vec<serde_json::Value>, TokenAnalyticsError> {
        let address_topic = encode_address(address)?;

        let mut collected: Vec<serde_json::Value> = Vec::new();
        let mut seen: std::collections::HashSet<(String, u64)> = std::collections::HashSet::new();

        // A single `eth_getLogs` topics filter can only constrain one indexed
        // position, so query the sender slot and the recipient slot separately.
        for position in 1..=2usize {
            let mut topics = vec![serde_json::Value::from(TRANSFER_TOPIC)];
            topics.push(if position == 1 {
                serde_json::Value::from(address_topic.clone())
            } else {
                serde_json::Value::Null
            });
            if position == 2 {
                topics.push(serde_json::Value::from(address_topic.clone()));
            }

            let filter = serde_json::json!({
                "fromBlock": "0x0",
                "toBlock": "latest",
                "topics": topics,
            });

            let result = self.rpc_call("eth_getLogs", serde_json::json!([filter])).await?;
            let logs = result.as_array().ok_or_else(|| {
                TokenAnalyticsError::ParseError("eth_getLogs result is not an array".to_string())
            })?;

            for log in logs {
                let tx_hash = log
                    .get("transactionHash")
                    .and_then(|v| v.as_str())
                    .unwrap_or_default()
                    .to_string();
                let log_index = log.get("logIndex").and_then(|v| v.as_u64()).unwrap_or(0);
                if seen.insert((tx_hash, log_index)) {
                    collected.push(log.clone());
                }
            }
        }

        collected.truncate(limit as usize);
        Ok(collected)
    }

    /// Get transfer events involving `address`.
    pub async fn get_transfers(
        &self,
        address: &str,
        limit: u32,
    ) -> Result<Vec<TransferEvent>, TokenAnalyticsError> {
        let logs = self.fetch_transfer_logs(address, limit).await?;

        let mut transfers = Vec::with_capacity(logs.len());
        for log in &logs {
            let topics = log.get("topics").and_then(|t| t.as_array());
            let from_topic = topics
                .and_then(|t| t.get(1))
                .and_then(|t| t.as_str())
                .ok_or_else(|| {
                    TokenAnalyticsError::ParseError("Transfer log missing sender topic".to_string())
                })?;
            let to_topic = topics
                .and_then(|t| t.get(2))
                .and_then(|t| t.as_str())
                .ok_or_else(|| {
                    TokenAnalyticsError::ParseError(
                        "Transfer log missing recipient topic".to_string(),
                    )
                })?;

            let from = topic_to_address(from_topic)?;
            let to = topic_to_address(to_topic)?;

            let value = log.get("data").and_then(|v| v.as_str()).unwrap_or("0x0");
            let block_number = log
                .get("blockNumber")
                .and_then(|b| b.as_str())
                .map(parse_uint256)
                .transpose()?
                .unwrap_or(0) as u64;

            transfers.push(TransferEvent {
                from,
                to,
                value: format_uint256(parse_uint256(value)?),
                token: log
                    .get("address")
                    .and_then(|v| v.as_str())
                    .unwrap_or_default()
                    .to_string(),
                tx_hash: log
                    .get("transactionHash")
                    .and_then(|v| v.as_str())
                    .unwrap_or_default()
                    .to_string(),
                block_number,
                timestamp: 0,
                log_index: log.get("logIndex").and_then(|v| v.as_u64()).unwrap_or(0) as u32,
            });
        }

        // Resolve real block timestamps for the distinct blocks involved.
        let mut block_timestamps: std::collections::HashMap<u64, i64> =
            std::collections::HashMap::new();
        for block in transfers
            .iter()
            .map(|t| t.block_number)
            .collect::<std::collections::HashSet<_>>()
        {
            if let Ok(ts) = self.get_block_timestamp(block).await {
                block_timestamps.insert(block, ts);
            }
        }
        for transfer in &mut transfers {
            if let Some(ts) = block_timestamps.get(&transfer.block_number) {
                transfer.timestamp = *ts;
            }
        }

        Ok(transfers)
    }

    /// Build the transfer graph for an address.
    pub async fn get_transfer_graph(
        &self,
        address: &str,
    ) -> Result<TransferGraph, TokenAnalyticsError> {
        let transfers = self.get_transfers(address, 100).await?;
        Ok(build_transfer_graph(&transfers))
    }

    /// Get price history for a token from the CoinGecko market-chart API.
    pub async fn get_price_history(
        &self,
        token: &str,
        days: u32,
    ) -> Result<PriceHistory, TokenAnalyticsError> {
        let url = format!(
            "https://api.coingecko.com/api/v3/coins/{}/market_chart?vs_currency=usd&days={}",
            token, days
        );

        let response = self
            .client
            .get(&url)
            .send()
            .await
            .map_err(|e| TokenAnalyticsError::RpcError(e.to_string()))?;

        if !response.status().is_success() {
            return Err(TokenAnalyticsError::RpcError(format!(
                "CoinGecko returned HTTP {} for token {token}",
                response.status()
            )));
        }

        let data: serde_json::Value = response
            .json()
            .await
            .map_err(|e| TokenAnalyticsError::ParseError(e.to_string()))?;

        let prices_data = data
            .get("prices")
            .and_then(|p| p.as_array())
            .ok_or_else(|| {
                TokenAnalyticsError::ParseError("CoinGecko response missing prices".to_string())
            })?;

        let mut prices = Vec::with_capacity(prices_data.len());
        for point in prices_data {
            let arr = point.as_array().ok_or_else(|| {
                TokenAnalyticsError::ParseError("price point is not an array".to_string())
            })?;
            prices.push(PricePoint {
                timestamp: arr.first().and_then(|v| v.as_i64()).unwrap_or(0),
                price: arr.get(1).and_then(|v| v.as_f64()).unwrap_or(0.0),
                volume: 0.0,
            });
        }

        let (avg_24h, change_24h) = summarize_prices(&prices);

        Ok(PriceHistory {
            prices,
            avg_24h,
            change_24h,
        })
    }

    async fn get_block_number(&self) -> Result<u64, TokenAnalyticsError> {
        let result = self.rpc_call("eth_blockNumber", serde_json::json!([])).await?;
        let raw = result.as_str().ok_or_else(|| {
            TokenAnalyticsError::ParseError("block number is not a string".to_string())
        })?;
        Ok(parse_uint256(raw)? as u64)
    }

    async fn get_block_timestamp(&self, block_number: u64) -> Result<i64, TokenAnalyticsError> {
        let block_hex = format!("0x{block_number:x}");
        let result = self
            .rpc_call("eth_getBlockByNumber", serde_json::json!([block_hex, false]))
            .await?;
        let ts = result
            .get("timestamp")
            .and_then(|t| t.as_str())
            .ok_or_else(|| {
                TokenAnalyticsError::ParseError("block missing timestamp".to_string())
            })?;
        Ok(parse_uint256(ts)? as i64)
    }
}

/// Compute the trailing-24-point average and the period-over-period change.
pub fn summarize_prices(prices: &[PricePoint]) -> (f64, f64) {
    let avg_24h = if prices.is_empty() {
        0.0
    } else {
        let window: Vec<f64> = prices.iter().rev().take(24).map(|p| p.price).collect();
        window.iter().sum::<f64>() / window.len() as f64
    };

    let change_24h = if prices.len() >= 2 {
        let now = prices.last().map(|p| p.price).unwrap_or(0.0);
        let old = prices.first().map(|p| p.price).unwrap_or(0.0);
        if old > 0.0 {
            ((now - old) / old) * 100.0
        } else {
            0.0
        }
    } else {
        0.0
    };

    (avg_24h, change_24h)
}

fn now_secs() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

// =============================================================================
// TESTS
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encode_uint256_pads_and_lowercases() {
        assert_eq!(
            encode_uint256("0xFF").unwrap(),
            format!("0x{}", format!("{:0>64}", "ff"))
        );
        assert_eq!(encode_uint256("").unwrap(), format!("0x{}", "0".repeat(64)));
    }

    #[test]
    fn encode_uint256_rejects_bad_input() {
        assert!(encode_uint256("0xzz").is_err());
        assert!(encode_uint256(&format!("0x{}", "a".repeat(65))).is_err());
    }

    #[test]
    fn encode_address_pads_20_byte_address() {
        let addr = "0x1111111111111111111111111111111111111111";
        let encoded = encode_address(addr).unwrap();
        assert_eq!(encoded.len(), 66);
        assert!(encoded.starts_with("0x000000000000000000000000"));
        assert!(encoded.ends_with("1111111111111111111111111111111111111111"));
    }

    #[test]
    fn encode_address_rejects_wrong_length() {
        assert!(encode_address("0x1234").is_err());
        assert!(encode_address("1111111111111111111111111111111111111111").is_err());
    }

    #[test]
    fn encode_allowance_call_matches_selector_layout() {
        let owner = "0xaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
        let spender = "0xbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";
        let call = encode_allowance_call(owner, spender).unwrap();
        assert!(call.starts_with(ALLOWANCE_SELECTOR));
        assert_eq!(call.len(), 2 + 8 + 64 + 64);
        // Selector occupies 0..10; the owner word (10..74) is left-padded with
        // 24 zero bytes, then the 20-byte address.
        assert_eq!(&call[10..34], &"0".repeat(24));
        assert_eq!(&call[34..74], "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa");
    }

    #[test]
    fn parse_uint256_handles_prefix_and_overflow() {
        assert_eq!(parse_uint256("0x0").unwrap(), 0);
        assert_eq!(parse_uint256("0xff").unwrap(), 255);
        assert_eq!(parse_uint256("10").unwrap(), 16);
        assert!(parse_uint256(&format!("0x{}", "f".repeat(33))).is_err());
        assert!(parse_uint256("0xgg").is_err());
    }

    #[test]
    fn format_uint256_is_canonical() {
        assert_eq!(format_uint256(0), "0x0");
        assert_eq!(format_uint256(255), "0xff");
        assert_eq!(format_uint256(u128::MAX), format!("0x{:x}", u128::MAX));
    }

    #[test]
    fn topic_to_address_extracts_low_20_bytes() {
        let topic = "0x000000000000000000000000aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
        assert_eq!(
            topic_to_address(topic).unwrap(),
            "0xaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
        );
        assert!(topic_to_address("0x1234").is_err());
    }

    fn transfer(from: &str, to: &str, value: u128) -> TransferEvent {
        TransferEvent {
            from: from.to_string(),
            to: to.to_string(),
            value: format_uint256(value),
            token: "0xtoken".to_string(),
            tx_hash: "0xtx".to_string(),
            block_number: 1,
            timestamp: 0,
            log_index: 0,
        }
    }

    #[test]
    fn build_transfer_graph_aggregates_totals() {
        let transfers = vec![
            transfer("0xa", "0xb", 100),
            transfer("0xa", "0xb", 50),
            transfer("0xb", "0xc", 30),
        ];
        let graph = build_transfer_graph(&transfers);

        assert_eq!(graph.nodes.len(), 3);
        let a = graph.nodes.iter().find(|n| n.address == "0xa").unwrap();
        assert_eq!(a.total_sent, "0x96"); // 150
        assert_eq!(a.total_received, "0x0");
        assert_eq!(a.tx_count, 2);

        let b = graph.nodes.iter().find(|n| n.address == "0xb").unwrap();
        assert_eq!(b.total_sent, "0x1e"); // 30
        assert_eq!(b.total_received, "0x96"); // 150
        assert_eq!(b.tx_count, 3);

        let ab = graph
            .edges
            .iter()
            .find(|e| e.from == "0xa" && e.to == "0xb")
            .unwrap();
        assert_eq!(ab.value, "0x96");
        assert_eq!(ab.count, 2);
        assert_eq!(graph.edges.len(), 2);
    }

    #[test]
    fn build_transfer_graph_empty() {
        let graph = build_transfer_graph(&[]);
        assert!(graph.nodes.is_empty());
        assert!(graph.edges.is_empty());
    }

    #[test]
    fn summarize_prices_computes_average_and_change() {
        let prices = vec![
            PricePoint { price: 100.0, timestamp: 0, volume: 0.0 },
            PricePoint { price: 110.0, timestamp: 1, volume: 0.0 },
        ];
        let (avg, change) = summarize_prices(&prices);
        assert!((avg - 105.0).abs() < 1e-9);
        assert!((change - 10.0).abs() < 1e-9);
    }

    #[test]
    fn summarize_prices_handles_empty_and_zero_base() {
        let (avg, change) = summarize_prices(&[]);
        assert_eq!(avg, 0.0);
        assert_eq!(change, 0.0);

        let prices = vec![
            PricePoint { price: 0.0, timestamp: 0, volume: 0.0 },
            PricePoint { price: 5.0, timestamp: 1, volume: 0.0 },
        ];
        let (_, change) = summarize_prices(&prices);
        assert_eq!(change, 0.0);
    }

    #[test]
    fn summarize_prices_averages_only_last_24() {
        let mut prices = Vec::new();
        for i in 0..30 {
            prices.push(PricePoint {
                price: if i < 6 { 1000.0 } else { 10.0 },
                timestamp: i,
                volume: 0.0,
            });
        }
        let (avg, _) = summarize_prices(&prices);
        assert!((avg - 10.0).abs() < 1e-9);
    }
}
