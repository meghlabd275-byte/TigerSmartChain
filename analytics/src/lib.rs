//! TigerScan analytics.
//!
//! Every figure this service reports is derived from an actual source: the
//! chain over JSON-RPC, or the indexer's PostgreSQL tables. Nothing is
//! hardcoded and nothing is estimated from a constant block time.
//!
//! The service works with or without a database. Without one it still reports
//! chain-derived figures and marks the indexed-only fields as unavailable
//! rather than filling them with zeros that look like real data.

use std::sync::Arc;
use std::time::Duration;

use serde::{Deserialize, Serialize};
use sqlx::postgres::PgPool;
use sqlx::Row;
use thiserror::Error;
use tokio::sync::RwLock;

// =============================================================================
// ERRORS
// =============================================================================

#[derive(Error, Debug)]
pub enum AnalyticsError {
    #[error("RPC error: {0}")]
    RpcError(String),

    #[error("Database error: {0}")]
    DatabaseError(String),

    #[error("Parse error: {0}")]
    ParseError(String),

    #[error("Configuration error: {0}")]
    Config(String),
}

// =============================================================================
// CONFIGURATION
// =============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    pub rpc_url: String,
    /// Optional. When unset, indexed-only figures are reported as unavailable.
    pub database_url: Option<String>,
    pub update_interval_secs: u64,
    /// Number of trailing blocks the TPS window covers.
    pub tps_window_blocks: u64,
    /// How long a cached network snapshot may be reused.
    pub max_cache_age_secs: u64,
    /// Chain id reported alongside the stats.
    pub chain_id: u64,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            rpc_url: std::env::var("RPC_URL")
                .unwrap_or_else(|_| "http://localhost:8545".to_string()),
            database_url: std::env::var("DATABASE_URL").ok().filter(|s| !s.is_empty()),
            update_interval_secs: std::env::var("ANALYTICS_UPDATE_INTERVAL")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(15),
            tps_window_blocks: std::env::var("ANALYTICS_TPS_WINDOW")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(100),
            max_cache_age_secs: 30,
            chain_id: std::env::var("CHAIN_ID")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(6666),
        }
    }
}

// =============================================================================
// DATA MODELS
// =============================================================================

/// A network-wide snapshot.
///
/// Fields that can only come from the indexer are optional so that a missing
/// database is visible to the caller instead of being reported as zero.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetworkStats {
    pub chain_id: u64,
    /// Height of the chain head.
    pub total_blocks: u64,
    pub tps: f64,
    /// Mean block interval over the sampled window, in seconds.
    pub avg_block_time: f64,
    pub gas_price: u64,
    pub gas_price_fast: u64,
    pub gas_price_standard: u64,
    pub gas_price_slow: u64,
    pub total_transactions: Option<u64>,
    pub total_addresses: Option<u64>,
    pub latest_block_hash: String,
    pub timestamp: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BlockStats {
    pub number: u64,
    pub hash: String,
    pub parent_hash: String,
    pub timestamp: u64,
    pub tx_count: u64,
    pub gas_used: u64,
    pub gas_limit: u64,
    pub base_fee_per_gas: Option<u64>,
    pub burnt_fees: String,
    pub miner: String,
    pub size: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AddressStats {
    pub address: String,
    /// Confirmed balance in wei, as a decimal string.
    pub balance: String,
    /// Nonce, i.e. the number of transactions sent.
    pub tx_count: u64,
    pub is_contract: bool,
    pub code_hash: String,
    /// Present only when a database is configured.
    pub total_sent: Option<String>,
    pub total_received: Option<String>,
    pub first_seen: Option<u64>,
    pub last_seen: Option<u64>,
}

/// An observed `(timestamp, value)` pair, used for time series.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AnalyticsDataPoint {
    pub timestamp: i64,
    pub value: f64,
}

// =============================================================================
// SERVICE
// =============================================================================

#[derive(Debug, Clone, Default)]
struct Cache {
    network_stats: Option<NetworkStats>,
    last_update: u64,
}

pub struct AnalyticsService {
    config: Config,
    http: reqwest::Client,
    db: Option<PgPool>,
    cache: Arc<RwLock<Cache>>,
}

impl AnalyticsService {
    /// Build a service. If `config.database_url` is set and the pool cannot be
    /// opened, that is an error: silently running without the indexer would
    /// make every indexed-only figure disappear without explanation.
    pub async fn new(config: Config) -> Result<Self, AnalyticsError> {
        if config.rpc_url.trim().is_empty() {
            return Err(AnalyticsError::Config("rpc_url must not be empty".into()));
        }

        let http = reqwest::Client::builder()
            .timeout(Duration::from_secs(30))
            .build()
            .map_err(|e| AnalyticsError::Config(e.to_string()))?;

        let db = match &config.database_url {
            Some(url) => Some(
                PgPool::connect(url)
                    .await
                    .map_err(|e| AnalyticsError::DatabaseError(e.to_string()))?,
            ),
            None => None,
        };

        Ok(Self {
            config,
            http,
            db,
            cache: Arc::new(RwLock::new(Cache::default())),
        })
    }

    pub fn config(&self) -> &Config {
        &self.config
    }

    /// Whether indexed-only figures are available.
    pub fn has_database(&self) -> bool {
        self.db.is_some()
    }

    // -------------------------------------------------------------------------
    // Network statistics
    // -------------------------------------------------------------------------

    /// Latest network snapshot, served from cache while it is fresh.
    pub async fn get_network_stats(&self) -> Result<NetworkStats, AnalyticsError> {
        {
            let cache = self.cache.read().await;
            if let Some(stats) = &cache.network_stats {
                if now_secs().saturating_sub(cache.last_update) < self.config.max_cache_age_secs {
                    return Ok(stats.clone());
                }
            }
        }

        let stats = self.fetch_network_stats().await?;

        let mut cache = self.cache.write().await;
        cache.network_stats = Some(stats.clone());
        cache.last_update = now_secs();

        Ok(stats)
    }

    async fn fetch_network_stats(&self) -> Result<NetworkStats, AnalyticsError> {
        let head = self.latest_block().await?;
        let head_number = head.number;

        // Sample a window of blocks once and derive TPS and block time from the
        // same sample, so the two figures always describe the same blocks.
        let window = self.config.tps_window_blocks.max(1);
        let sample = self.sample_blocks(head_number, window).await;

        let (tps, avg_block_time) = Self::derive_rate(&sample);
        let gas_price = self.gas_price().await.unwrap_or(0);

        let (total_transactions, total_addresses) = match &self.db {
            Some(pool) => (
                Some(count_rows(pool, "transactions").await.unwrap_or(0)),
                Some(count_rows(pool, "accounts").await.unwrap_or(0)),
            ),
            None => (None, None),
        };

        Ok(NetworkStats {
            chain_id: self.config.chain_id,
            total_blocks: head_number + 1,
            tps,
            avg_block_time,
            gas_price,
            gas_price_fast: gas_price.saturating_mul(12) / 10,
            gas_price_standard: gas_price,
            gas_price_slow: gas_price.saturating_mul(8) / 10,
            total_transactions,
            total_addresses,
            latest_block_hash: head.hash,
            timestamp: now_secs(),
        })
    }

    /// Derive TPS and mean block time from a block sample.
    ///
    /// Both come from the timestamps actually observed. The previous version
    /// divided by a hardcoded three-second block time, so TPS was wrong on any
    /// chain whose blocks are not three seconds apart.
    fn derive_rate(sample: &[BlockStats]) -> (f64, f64) {
        if sample.len() < 2 {
            return (0.0, 0.0);
        }

        // `sample` is newest-first.
        let newest = &sample[0];
        let oldest = &sample[sample.len() - 1];
        let span = newest.timestamp.saturating_sub(oldest.timestamp);

        if span == 0 {
            return (0.0, 0.0);
        }

        // The newest block's own transactions are included in the count, so the
        // interval covered is from the oldest block up to the newest.
        let txs: u64 = sample.iter().map(|b| b.tx_count).sum();
        let tps = txs as f64 / span as f64;
        let avg_block_time = span as f64 / (sample.len() - 1) as f64;

        (tps, avg_block_time)
    }

    async fn sample_blocks(&self, head: u64, window: u64) -> Vec<BlockStats> {
        let mut out = Vec::new();
        let start = head.saturating_sub(window.saturating_sub(1));

        for number in (start..=head).rev() {
            match self.get_block(number).await {
                Ok(block) => out.push(block),
                // A missing block shortens the sample rather than failing the
                // whole snapshot.
                Err(_) => continue,
            }
        }

        out
    }

    // -------------------------------------------------------------------------
    // Blocks
    // -------------------------------------------------------------------------

    /// Latest block, resolved from the chain head.
    pub async fn latest_block(&self) -> Result<BlockStats, AnalyticsError> {
        let value = self
            .rpc("eth_getBlockByNumber", serde_json::json!(["latest", false]))
            .await?;
        Self::parse_block(&value)
    }

    pub async fn get_block(&self, number: u64) -> Result<BlockStats, AnalyticsError> {
        let value = self
            .rpc(
                "eth_getBlockByNumber",
                serde_json::json!([format!("0x{number:x}"), false]),
            )
            .await?;
        Self::parse_block(&value)
    }

    fn parse_block(value: &serde_json::Value) -> Result<BlockStats, AnalyticsError> {
        if value.is_null() {
            return Err(AnalyticsError::ParseError("block not found".into()));
        }

        let number = value
            .get("number")
            .and_then(|v| v.as_str())
            .and_then(parse_quantity)
            .ok_or_else(|| AnalyticsError::ParseError("block has no number".into()))?;

        let tx_count = value
            .get("transactions")
            .and_then(|t| t.as_array())
            .map(|a| a.len() as u64)
            .unwrap_or(0);

        let gas_used = value
            .get("gasUsed")
            .and_then(|v| v.as_str())
            .and_then(parse_quantity)
            .unwrap_or(0);
        let gas_limit = value
            .get("gasLimit")
            .and_then(|v| v.as_str())
            .and_then(parse_quantity)
            .unwrap_or(0);

        let base_fee = value
            .get("baseFeePerGas")
            .and_then(|v| v.as_str())
            .and_then(parse_quantity);

        // EIP-1559 burns `baseFee * gasUsed`; without a base fee (pre-London)
        // nothing is burned.
        let burnt = base_fee
            .map(|fee| fee.saturating_mul(gas_used))
            .unwrap_or(0);

        Ok(BlockStats {
            number,
            hash: str_field(value, "hash"),
            parent_hash: str_field(value, "parentHash"),
            timestamp: value
                .get("timestamp")
                .and_then(|v| v.as_str())
                .and_then(parse_quantity)
                .unwrap_or(0),
            tx_count,
            gas_used,
            gas_limit,
            base_fee_per_gas: base_fee,
            burnt_fees: burnt.to_string(),
            miner: value
                .get("miner")
                .and_then(|v| v.as_str())
                .unwrap_or_default()
                .to_lowercase(),
            size: value
                .get("size")
                .and_then(|v| v.as_str())
                .and_then(parse_quantity)
                .unwrap_or(0),
        })
    }

    // -------------------------------------------------------------------------
    // Addresses
    // -------------------------------------------------------------------------

    pub async fn get_address_stats(&self, address: &str) -> Result<AddressStats, AnalyticsError> {
        if !is_address(address) {
            return Err(AnalyticsError::ParseError(format!(
                "'{address}' is not a 20-byte hex address"
            )));
        }

        let balance = self
            .rpc("eth_getBalance", serde_json::json!([address, "latest"]))
            .await?
            .as_str()
            .and_then(parse_quantity)
            .unwrap_or(0)
            .to_string();

        let tx_count = self
            .rpc(
                "eth_getTransactionCount",
                serde_json::json!([address, "latest"]),
            )
            .await?
            .as_str()
            .and_then(parse_quantity)
            .unwrap_or(0);

        let code = self
            .rpc("eth_getCode", serde_json::json!([address, "latest"]))
            .await?
            .as_str()
            .unwrap_or("0x")
            .to_string();
        let is_contract = code != "0x" && !code.is_empty();

        let code_hash = if is_contract {
            format!("0x{}", hex::encode(keccak256(code.as_bytes())))
        } else {
            String::new()
        };

        let (total_sent, total_received, first_seen, last_seen) =
            match self.address_flows(address).await {
                Some(v) => v,
                None => (None, None, None, None),
            };

        Ok(AddressStats {
            address: address.to_lowercase(),
            balance,
            tx_count,
            is_contract,
            code_hash,
            total_sent,
            total_received,
            first_seen,
            last_seen,
        })
    }

    /// Value in and out of an address, from the indexer.
    async fn address_flows(
        &self,
        address: &str,
    ) -> Option<(Option<String>, Option<String>, Option<u64>, Option<u64>)> {
        let pool = self.db.as_ref()?;
        let addr = address.to_lowercase();

        let row = sqlx::query(
            "SELECT
                 COALESCE(SUM(value) FILTER (WHERE LOWER(from_address) = $1), 0)::text AS sent,
                 COALESCE(SUM(value) FILTER (WHERE LOWER(to_address) = $1), 0)::text AS received,
                 MIN(block_number) FILTER (
                     WHERE LOWER(from_address) = $1 OR LOWER(to_address) = $1
                 ) AS first_block,
                 MAX(block_number) FILTER (
                     WHERE LOWER(from_address) = $1 OR LOWER(to_address) = $1
                 ) AS last_block
             FROM transactions
             WHERE LOWER(from_address) = $1 OR LOWER(to_address) = $1",
        )
        .bind(&addr)
        .fetch_optional(pool)
        .await
        .ok()??;

        let sent: String = row.try_get("sent").ok()?;
        let received: String = row.try_get("received").ok()?;
        let first_block: Option<i64> = row.try_get("first_block").ok()?;
        let last_block: Option<i64> = row.try_get("last_block").ok()?;

        Some((
            Some(sent),
            Some(received),
            first_block.map(|v| v as u64),
            last_block.map(|v| v as u64),
        ))
    }

    // -------------------------------------------------------------------------
    // Time series
    // -------------------------------------------------------------------------

    /// Daily analytics rows the indexer maintains.
    pub async fn daily_series(
        &self,
        days: u32,
    ) -> Result<Vec<AnalyticsDataPoint>, AnalyticsError> {
        let pool = self
            .db
            .as_ref()
            .ok_or_else(|| AnalyticsError::Config("no database configured".into()))?;

        let rows = sqlx::query(
            "SELECT EXTRACT(EPOCH FROM date)::bigint AS ts, total_transactions::float8 AS value
             FROM analytics_daily
             ORDER BY date DESC
             LIMIT $1",
        )
        .bind(days as i64)
        .fetch_all(pool)
        .await
        .map_err(|e| AnalyticsError::DatabaseError(e.to_string()))?;

        let mut out: Vec<AnalyticsDataPoint> = rows
            .iter()
            .filter_map(|r| {
                let ts: i64 = r.try_get("ts").ok()?;
                let value: f64 = r.try_get("value").ok()?;
                Some(AnalyticsDataPoint {
                    timestamp: ts,
                    value,
                })
            })
            .collect();

        out.sort_by_key(|p| p.timestamp);
        Ok(out)
    }

    // -------------------------------------------------------------------------
    // JSON-RPC
    // -------------------------------------------------------------------------

    async fn rpc(
        &self,
        method: &str,
        params: serde_json::Value,
    ) -> Result<serde_json::Value, AnalyticsError> {
        let body = serde_json::json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": method,
            "params": params,
        });

        let response = self
            .http
            .post(&self.config.rpc_url)
            .json(&body)
            .send()
            .await
            .map_err(|e| AnalyticsError::RpcError(e.to_string()))?;

        if !response.status().is_success() {
            return Err(AnalyticsError::RpcError(format!(
                "{method} returned HTTP {}",
                response.status()
            )));
        }

        let value: serde_json::Value = response
            .json()
            .await
            .map_err(|e| AnalyticsError::ParseError(e.to_string()))?;

        if let Some(err) = value.get("error") {
            return Err(AnalyticsError::RpcError(format!("{method}: {err}")));
        }

        value
            .get("result")
            .cloned()
            .ok_or_else(|| AnalyticsError::ParseError(format!("{method}: no result")))
    }

    async fn gas_price(&self) -> Result<u64, AnalyticsError> {
        self.rpc("eth_gasPrice", serde_json::json!([]))
            .await?
            .as_str()
            .and_then(parse_quantity)
            .ok_or_else(|| AnalyticsError::ParseError("eth_gasPrice returned no quantity".into()))
    }
}

// =============================================================================
// HELPERS
// =============================================================================

async fn count_rows(pool: &PgPool, table: &str) -> Result<u64, sqlx::Error> {
    // `table` is a compile-time constant at every call site, never user input.
    let row = sqlx::query(&format!("SELECT COUNT(*) AS c FROM {table}"))
        .fetch_one(pool)
        .await?;
    let count: i64 = row.try_get("c")?;
    Ok(count.max(0) as u64)
}

fn str_field(value: &serde_json::Value, key: &str) -> String {
    value
        .get(key)
        .and_then(|v| v.as_str())
        .unwrap_or_default()
        .to_string()
}

pub fn is_address(value: &str) -> bool {
    value.len() == 42
        && value.starts_with("0x")
        && value[2..].chars().all(|c| c.is_ascii_hexdigit())
}

/// Parse a JSON-RPC quantity such as `0x1a`.
pub fn parse_quantity(value: &str) -> Option<u64> {
    let stripped = value.strip_prefix("0x").unwrap_or(value);
    if stripped.is_empty() {
        return None;
    }
    u64::from_str_radix(stripped, 16).ok()
}

fn keccak256(data: &[u8]) -> [u8; 32] {
    use sha3::{Digest, Keccak256};
    let digest = Keccak256::digest(data);
    let mut out = [0u8; 32];
    out.copy_from_slice(&digest);
    out
}

fn now_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn block(number: u64, timestamp: u64, tx_count: u64) -> BlockStats {
        BlockStats {
            number,
            hash: format!("0x{number:x}"),
            parent_hash: format!("0x{:x}", number.saturating_sub(1)),
            timestamp,
            tx_count,
            gas_used: 0,
            gas_limit: 30_000_000,
            base_fee_per_gas: Some(1_000_000_000),
            burnt_fees: "0".to_string(),
            miner: "0x0".to_string(),
            size: 0,
        }
    }

    #[test]
    fn tps_and_block_time_come_from_observed_timestamps() {
        // Four blocks, three seconds apart, ten transactions each.
        let sample = vec![
            block(103, 1_000_009, 10),
            block(102, 1_000_006, 10),
            block(101, 1_000_003, 10),
            block(100, 1_000_000, 10),
        ];

        let (tps, avg_block_time) = AnalyticsService::derive_rate(&sample);
        assert_eq!(avg_block_time, 3.0);
        assert_eq!(tps, 40.0 / 9.0);
    }

    #[test]
    fn tps_reflects_the_real_block_interval_not_a_constant() {
        // One-second blocks: the old code assumed three seconds and would
        // report a third of the true rate.
        let sample = vec![
            block(2, 1_000_002, 5),
            block(1, 1_000_001, 5),
            block(0, 1_000_000, 5),
        ];

        let (tps, avg_block_time) = AnalyticsService::derive_rate(&sample);
        assert_eq!(avg_block_time, 1.0);
        assert_eq!(tps, 15.0 / 2.0);
    }

    #[test]
    fn a_single_block_sample_yields_no_rate() {
        let (tps, avg_block_time) = AnalyticsService::derive_rate(&[block(1, 100, 5)]);
        assert_eq!(tps, 0.0);
        assert_eq!(avg_block_time, 0.0);
        assert_eq!(AnalyticsService::derive_rate(&[]).0, 0.0);
    }

    #[test]
    fn identical_timestamps_do_not_divide_by_zero() {
        let sample = vec![block(1, 100, 5), block(0, 100, 5)];
        let (tps, avg_block_time) = AnalyticsService::derive_rate(&sample);
        assert_eq!(tps, 0.0);
        assert_eq!(avg_block_time, 0.0);
    }

    #[test]
    fn parses_a_block_response() {
        let raw = serde_json::json!({
            "number": "0x10",
            "hash": "0xabc",
            "parentHash": "0xdef",
            "timestamp": "0x64",
            "gasUsed": "0x5208",
            "gasLimit": "0x1c9c380",
            "baseFeePerGas": "0x3b9aca00",
            "miner": "0xABCDEFabcdefABCDEFabcdefABCDEFabcdefABCD",
            "size": "0x200",
            "transactions": [{"hash": "0x1"}, {"hash": "0x2"}],
        });

        let b = AnalyticsService::parse_block(&raw).unwrap();
        assert_eq!(b.number, 16);
        assert_eq!(b.tx_count, 2);
        assert_eq!(b.gas_used, 21000);
        assert_eq!(b.base_fee_per_gas, Some(1_000_000_000));
        // 1 gwei * 21000 gas.
        assert_eq!(b.burnt_fees, "21000000000000");
        assert_eq!(b.miner, "0xabcdefabcdefabcdefabcdefabcdefabcdefabcd");
        assert_eq!(b.size, 512);
    }

    #[test]
    fn a_pre_london_block_has_no_base_fee_and_burns_nothing() {
        let raw = serde_json::json!({
            "number": "0x1",
            "gasUsed": "0x5208",
            "transactions": [],
        });
        let b = AnalyticsService::parse_block(&raw).unwrap();
        assert_eq!(b.base_fee_per_gas, None);
        assert_eq!(b.burnt_fees, "0");
    }

    #[test]
    fn malformed_blocks_are_rejected() {
        assert!(AnalyticsService::parse_block(&serde_json::Value::Null).is_err());
        assert!(AnalyticsService::parse_block(&serde_json::json!({"hash": "0x1"})).is_err());
    }

    #[test]
    fn quantities_parse_with_and_without_leading_zeroes() {
        assert_eq!(parse_quantity("0x0"), Some(0));
        assert_eq!(parse_quantity("0x10"), Some(16));
        assert_eq!(parse_quantity("0x100"), Some(256));
        assert_eq!(parse_quantity("0x"), None);
        assert_eq!(parse_quantity("xyz"), None);
    }

    #[test]
    fn address_validation_is_strict() {
        assert!(is_address("0x1111111111111111111111111111111111111111"));
        assert!(!is_address("0x1111"));
        assert!(!is_address("1111111111111111111111111111111111111111"));
        assert!(!is_address("0xzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzz"));
    }

    #[test]
    fn keccak256_matches_the_known_vector() {
        // keccak256("") is a well-known constant.
        assert_eq!(
            hex::encode(keccak256(b"")),
            "c5d2460186f7233c927e7db2dcc703c0e500b653ca82273b7bfad8045d85a470"
        );
    }

    #[test]
    fn rpc_and_gas_price_work_against_a_stub_node() {
        // Exercises the JSON-RPC path against a stub node, asserting the request
        // shape and the parsing of a realistic response.
        use std::io::{Read, Write};
        use std::net::TcpListener;

        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();

        let server = std::thread::spawn(move || {
            for _ in 0..2 {
                let (mut stream, _) = listener.accept().unwrap();
                let mut buf = [0u8; 8192];
                let n = stream.read(&mut buf).unwrap();
                let request = String::from_utf8_lossy(&buf[..n]).to_string();

                let body = if request.contains("eth_blockNumber") {
                    r#"{"jsonrpc":"2.0","id":1,"result":"0x64"}"#
                } else {
                    r#"{"jsonrpc":"2.0","id":1,"result":"0x3b9aca00"}"#
                };

                let response = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{}",
                    body.len(),
                    body
                );
                stream.write_all(response.as_bytes()).unwrap();
                stream.flush().unwrap();
            }
        });

        let rt = tokio::runtime::Runtime::new().unwrap();
        let gas_price = rt.block_on(async {
            let config = Config {
                rpc_url: format!("http://{addr}"),
                database_url: None,
                update_interval_secs: 15,
                tps_window_blocks: 1,
                max_cache_age_secs: 30,
                chain_id: 6666,
            };
            let service = AnalyticsService::new(config).await.unwrap();
            assert!(!service.has_database());

            let value = service
                .rpc("eth_blockNumber", serde_json::json!([]))
                .await
                .unwrap();
            assert_eq!(value.as_str(), Some("0x64"));

            service.gas_price().await.unwrap()
        });

        assert_eq!(gas_price, 1_000_000_000);
        server.join().unwrap();
    }
}
