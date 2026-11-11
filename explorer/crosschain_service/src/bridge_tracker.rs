//! Cross-chain bridge transfer tracking.
//!
//! Transfers are read directly from the chain being indexed, using
//! `eth_getLogs` against the bridge contracts an operator configures. Bridge
//! contracts announce every deposit with an event, so the chain itself is the
//! source of truth; no third-party bridge API is consulted and none is assumed
//! to exist.
//!
//! Because each bridge defines its own deposit event, the decoder is driven by
//! configuration: a [`BridgeDeployment`] names the contract, the canonical
//! event signature, and which of that event's parameters carry the sender,
//! receiver, amount and token. The signature is hashed with keccak256 to give
//! the `topics[0]` filter, and the parameters are ABI-decoded with the usual
//! indexed / non-indexed split.

use std::sync::Arc;

use serde::{Deserialize, Serialize};
use sha3::{Digest, Keccak256};
use thiserror::Error;
use tokio::sync::RwLock;

// =============================================================================
// ERRORS
// =============================================================================

#[derive(Error, Debug)]
pub enum BridgeError {
    #[error("RPC error: {0}")]
    ApiError(String),

    #[error("Parse error: {0}")]
    ParseError(String),

    #[error("Not found: {0}")]
    NotFound(String),

    #[error("Configuration error: {0}")]
    Config(String),
}

// =============================================================================
// CONFIGURATION
// =============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BridgeConfig {
    pub rpc_url: String,
    pub database_url: String,
    pub update_interval_secs: u64,
    /// Chain the RPC endpoint serves. Reported on every transfer.
    pub chain_id: u64,
    /// Number of blocks behind the head that still count as unconfirmed.
    pub confirmation_blocks: u64,
    /// How far back `eth_getLogs` may scan in one query. Public nodes commonly
    /// reject unbounded ranges.
    pub lookback_blocks: u64,
    /// Bridge contracts to watch.
    pub deployments: Vec<BridgeDeployment>,
}

impl Default for BridgeConfig {
    fn default() -> Self {
        Self {
            rpc_url: std::env::var("RPC_URL")
                .unwrap_or_else(|_| "http://localhost:8545".to_string()),
            database_url: std::env::var("DATABASE_URL").unwrap_or_default(),
            update_interval_secs: std::env::var("BRIDGE_UPDATE_INTERVAL")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(60),
            chain_id: std::env::var("CHAIN_ID")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(6666),
            confirmation_blocks: std::env::var("BRIDGE_CONFIRMATIONS")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(12),
            lookback_blocks: std::env::var("BRIDGE_LOOKBACK_BLOCKS")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(5_000),
            deployments: Vec::new(),
        }
    }
}

impl BridgeConfig {
    /// Build a configuration from `BRIDGE_DEPLOYMENTS`, a JSON array of
    /// [`BridgeDeployment`] objects. An unset or empty value yields no
    /// deployments, in which case no bridge is watched and every query returns
    /// nothing rather than inventing data.
    pub fn from_env() -> Result<Self, BridgeError> {
        let mut config = Self::default();
        if let Ok(raw) = std::env::var("BRIDGE_DEPLOYMENTS") {
            let trimmed = raw.trim();
            if !trimmed.is_empty() {
                config.deployments = serde_json::from_str(trimmed)
                    .map_err(|e| BridgeError::Config(format!("BRIDGE_DEPLOYMENTS: {e}")))?;
            }
        }
        config.validate()?;
        Ok(config)
    }

    fn validate(&self) -> Result<(), BridgeError> {
        if self.rpc_url.trim().is_empty() {
            return Err(BridgeError::Config("rpc_url must not be empty".into()));
        }
        for d in &self.deployments {
            if !is_address(&d.address) {
                return Err(BridgeError::Config(format!(
                    "bridge '{}' has a malformed address '{}'",
                    d.name, d.address
                )));
            }
            if d.signature.trim().is_empty() {
                return Err(BridgeError::Config(format!(
                    "bridge '{}' has no event signature",
                    d.name
                )));
            }
        }
        Ok(())
    }
}

// =============================================================================
// EVENT DECODING CONFIGURATION
// =============================================================================

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FieldKind {
    Address,
    Uint,
    Int,
    Bool,
    Bytes32,
    /// Dynamic `bytes`.
    Bytes,
    /// Dynamic `string`.
    String,
}

impl FieldKind {
    fn is_dynamic(&self) -> bool {
        matches!(self, FieldKind::Bytes | FieldKind::String)
    }
}

/// What a decoded event parameter means for a transfer record.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FieldRole {
    Sender,
    Receiver,
    Amount,
    Token,
    TransferId,
    DestinationChain,
    /// Decoded, but not mapped onto a transfer field.
    Other,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BridgeField {
    pub name: String,
    pub kind: FieldKind,
    pub role: FieldRole,
}

/// One bridge contract to watch on the indexed chain.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BridgeDeployment {
    /// Human-readable bridge name, e.g. "Wormhole".
    pub name: String,
    /// Contract address that emits the deposit event.
    pub address: String,
    /// Canonical event signature, e.g.
    /// `LogMessagePublished(address,uint64,bytes,uint8)`. Hashed with keccak256
    /// to produce the topic filter.
    pub signature: String,
    /// Parameters declared `indexed`, in declaration order.
    #[serde(default)]
    pub indexed: Vec<BridgeField>,
    /// Parameters not declared `indexed`, in declaration order.
    #[serde(default)]
    pub data: Vec<BridgeField>,
    /// Destination chain id, for contracts that do not emit one.
    #[serde(default)]
    pub remote_chain_id: u64,
    /// Decimals used when normalising `amount` for reporting.
    #[serde(default = "default_decimals")]
    pub token_decimals: u8,
}

fn default_decimals() -> u8 {
    18
}

impl BridgeDeployment {
    /// keccak256 of the canonical signature, as a `0x`-prefixed topic.
    pub fn topic0(&self) -> String {
        format!(
            "0x{}",
            hex::encode(Keccak256::digest(self.signature.as_bytes()))
        )
    }

    /// Topic positions (1-based, position 0 being the signature) whose declared
    /// role is `role`.
    fn topic_indices_for(&self, role: FieldRole) -> Vec<usize> {
        self.indexed
            .iter()
            .enumerate()
            .filter(|(_, f)| f.role == role)
            .map(|(i, _)| i + 1)
            .collect()
    }
}

// =============================================================================
// DATA MODELS
// =============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BridgeTransfer {
    pub id: String,
    pub bridge: String,
    pub from_chain: String,
    pub to_chain: String,
    pub sender: String,
    pub receiver: String,
    pub token: String,
    pub amount: String,
    pub status: BridgeTransferStatus,
    pub deposit_tx: String,
    pub receive_tx: Option<String>,
    pub timestamp: i64,
    pub confirmations: u32,
    pub source_chain_id: u64,
    pub destination_chain_id: u64,
    /// Block the deposit landed in, so callers can order and re-check it.
    pub block_number: u64,
    /// Log position within the block.
    pub log_index: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BridgeTransferStatus {
    Pending,
    Sent,
    Confirmed,
    Executed,
    Failed,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BridgeStats {
    pub bridge: String,
    pub total_volume_24h: f64,
    pub total_transfers_24h: u64,
    pub avg_transfer_time: f64,
    pub success_rate: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BridgeTokenInfo {
    pub address: String,
    pub symbol: String,
    pub name: String,
    pub decimals: u8,
    pub bridges: Vec<String>,
    pub chain_id: u64,
}

// =============================================================================
// TRACKER
// =============================================================================

pub struct BridgeTracker {
    config: BridgeConfig,
    http: reqwest::Client,
    cache: Arc<RwLock<BridgeCache>>,
}

#[derive(Debug, Default)]
pub struct BridgeCache {
    pub transfers: std::collections::HashMap<String, Vec<BridgeTransfer>>,
    pub stats: std::collections::HashMap<String, BridgeStats>,
    pub last_update: i64,
}

impl BridgeTracker {
    pub fn new(config: BridgeConfig) -> Result<Self, BridgeError> {
        config.validate()?;
        let http = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(30))
            .build()
            .map_err(|e| BridgeError::ApiError(e.to_string()))?;
        Ok(Self {
            config,
            http,
            cache: Arc::new(RwLock::new(BridgeCache::default())),
        })
    }

    pub fn config(&self) -> &BridgeConfig {
        &self.config
    }

    /// All transfers involving `address`, as sender or receiver.
    pub async fn get_transfers(&self, address: &str) -> Result<Vec<BridgeTransfer>, BridgeError> {
        if !is_address(address) {
            return Err(BridgeError::ParseError(format!(
                "'{address}' is not a 20-byte hex address"
            )));
        }

        {
            let cache = self.cache.read().await;
            if let Some(transfers) = cache.transfers.get(&address.to_lowercase()) {
                return Ok(transfers.clone());
            }
        }

        let mut all = Vec::new();
        for deployment in &self.config.deployments {
            match self.fetch_deployment_transfers(deployment, address).await {
                Ok(mut found) => all.append(&mut found),
                // One unreachable or misconfigured bridge must not hide the
                // transfers recorded by the others.
                Err(e) => tracing::warn!("bridge '{}' query failed: {e}", deployment.name),
            }
        }

        all.sort_by(|a, b| {
            b.block_number
                .cmp(&a.block_number)
                .then(b.log_index.cmp(&a.log_index))
        });
        all.dedup_by(|a, b| a.id == b.id);

        let mut cache = self.cache.write().await;
        cache
            .transfers
            .insert(address.to_lowercase(), all.clone());
        cache.last_update = now_secs();

        Ok(all)
    }

    /// Query every configured bridge for logs mentioning `address`.
    async fn fetch_deployment_transfers(
        &self,
        deployment: &BridgeDeployment,
        address: &str,
    ) -> Result<Vec<BridgeTransfer>, BridgeError> {
        let latest = self.block_number().await?;
        let from = latest.saturating_sub(self.config.lookback_blocks);
        let topic0 = deployment.topic0();
        let padded = padded_address(address);

        // A deposit event may carry the address as sender or as receiver, at
        // different topic positions depending on the bridge. Query each
        // position separately and merge, de-duplicating by log identity.
        let mut positions: Vec<usize> = deployment
            .topic_indices_for(FieldRole::Sender)
            .into_iter()
            .chain(deployment.topic_indices_for(FieldRole::Receiver))
            .collect();
        positions.sort_unstable();
        positions.dedup();

        // With no sender/receiver role declared, issue one unfiltered query and
        // filter on the decoded values instead.
        if positions.is_empty() {
            positions.push(0);
        }

        let mut out: Vec<BridgeTransfer> = Vec::new();
        let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();

        for position in positions {
            let mut topics: Vec<Option<String>> = vec![Some(topic0.clone())];
            for idx in 1..=deployment.indexed.len() {
                if idx == position {
                    topics.push(Some(padded.clone()));
                } else {
                    topics.push(None);
                }
            }

            let logs = self
                .get_logs(&deployment.address, &topics, from, latest)
                .await?;
            for log in logs {
                let key = format!(
                    "{}:{}",
                    log["transactionHash"].as_str().unwrap_or_default(),
                    log["logIndex"].as_str().unwrap_or_default()
                );
                if !seen.insert(key) {
                    continue;
                }
                if let Some(transfer) = self.decode_log(deployment, &log, latest, address) {
                    out.push(transfer);
                }
            }
        }

        Ok(out)
    }

    /// Decode one log into a transfer, or `None` when it does not describe a
    /// well-formed transfer.
    fn decode_log(
        &self,
        deployment: &BridgeDeployment,
        log: &serde_json::Value,
        latest: u64,
        queried_address: &str,
    ) -> Option<BridgeTransfer> {
        let topics: Vec<String> = log["topics"]
            .as_array()?
            .iter()
            .filter_map(|t| t.as_str().map(|s| s.to_ascii_lowercase()))
            .collect();

        let data_hex = log["data"].as_str().unwrap_or("0x");
        let data = hex::decode(data_hex.strip_prefix("0x").unwrap_or(data_hex)).ok()?;

        let mut decoded: Vec<DecodedValue> = Vec::new();
        for (i, field) in deployment.indexed.iter().enumerate() {
            decoded.push(DecodedValue::from_topic(field.kind, topics.get(i + 1)?)?);
        }
        decoded.extend(decode_abi(&deployment.data, &data)?);

        let all_fields: Vec<&BridgeField> = deployment
            .indexed
            .iter()
            .chain(deployment.data.iter())
            .collect();

        let mut sender = String::new();
        let mut receiver = String::new();
        let mut token = String::new();
        let mut amount = String::new();
        let mut transfer_id = String::new();
        let mut destination_chain = deployment.remote_chain_id;

        for (field, value) in all_fields.iter().zip(decoded.iter()) {
            match field.role {
                FieldRole::Sender => sender = value.as_address(),
                FieldRole::Receiver => receiver = value.as_address(),
                FieldRole::Token => token = value.as_address(),
                FieldRole::Amount => amount = value.as_uint_string(),
                FieldRole::TransferId => transfer_id = value.as_transfer_id(),
                FieldRole::DestinationChain => {
                    if let Some(v) = value.as_u64() {
                        destination_chain = v;
                    }
                }
                FieldRole::Other => {}
            }
        }

        // A topic-position query cannot express "either side", so whichever
        // sides the event declares must include the caller. When the event
        // declares neither, there is nothing to cross-check against and the
        // topic filter is the only guarantee.
        let q = queried_address.to_ascii_lowercase();
        let sender_matches = !sender.is_empty() && sender == q;
        let receiver_matches = !receiver.is_empty() && receiver == q;
        if (!sender.is_empty() || !receiver.is_empty()) && !sender_matches && !receiver_matches {
            return None;
        }

        let block_number = parse_quantity(log["blockNumber"].as_str().unwrap_or("0x0")).unwrap_or(0);
        let log_index = parse_quantity(log["logIndex"].as_str().unwrap_or("0x0")).unwrap_or(0);
        let tx_hash = log["transactionHash"]
            .as_str()
            .unwrap_or_default()
            .to_string();

        // A log carries no timestamp; it is left unset rather than guessed.
        let confirmations = latest.saturating_sub(block_number);

        let id = if transfer_id.is_empty() {
            format!("{tx_hash}:{log_index}")
        } else {
            transfer_id
        };

        Some(BridgeTransfer {
            id,
            bridge: deployment.name.clone(),
            from_chain: self.config.chain_id.to_string(),
            to_chain: destination_chain.to_string(),
            sender,
            receiver,
            token,
            amount,
            status: if confirmations >= self.config.confirmation_blocks {
                BridgeTransferStatus::Confirmed
            } else {
                BridgeTransferStatus::Pending
            },
            deposit_tx: tx_hash,
            receive_tx: None,
            timestamp: 0,
            confirmations: confirmations.min(u32::MAX as u64) as u32,
            source_chain_id: self.config.chain_id,
            destination_chain_id: destination_chain,
            block_number,
            log_index,
        })
    }

    /// Aggregate stats for a bridge over the transfers already indexed.
    ///
    /// Volume is reported in whole tokens, normalised with the deployment's
    /// declared decimals.
    pub async fn get_bridge_stats(&self, bridge: &str) -> Result<BridgeStats, BridgeError> {
        {
            let cache = self.cache.read().await;
            if let Some(stats) = cache.stats.get(bridge) {
                return Ok(stats.clone());
            }
        }

        let decimals = self
            .config
            .deployments
            .iter()
            .find(|d| d.name.eq_ignore_ascii_case(bridge))
            .map(|d| d.token_decimals)
            .unwrap_or(18);

        let cutoff = now_secs() - 86_400;
        let mut volume = 0.0f64;
        let mut count = 0u64;
        let mut confirmed = 0u64;

        {
            let cache = self.cache.read().await;
            for transfers in cache.transfers.values() {
                for t in transfers {
                    if !t.bridge.eq_ignore_ascii_case(bridge) {
                        continue;
                    }
                    // Transfers with no timestamp cannot be placed inside the
                    // window, so they are counted but not aged out.
                    if t.timestamp != 0 && t.timestamp < cutoff {
                        continue;
                    }
                    count += 1;
                    if matches!(
                        t.status,
                        BridgeTransferStatus::Confirmed | BridgeTransferStatus::Executed
                    ) {
                        confirmed += 1;
                    }
                    if let Ok(raw) = t.amount.parse::<f64>() {
                        volume += raw / 10f64.powi(decimals as i32);
                    }
                }
            }
        }

        let stats = BridgeStats {
            bridge: bridge.to_string(),
            total_volume_24h: volume,
            total_transfers_24h: count,
            // Latency is not observable from deposit logs alone, so it is
            // reported as zero rather than estimated.
            avg_transfer_time: 0.0,
            success_rate: if count == 0 {
                0.0
            } else {
                confirmed as f64 / count as f64 * 100.0
            },
        };

        let mut cache = self.cache.write().await;
        cache.stats.insert(bridge.to_string(), stats.clone());
        Ok(stats)
    }

    /// Tokens seen in indexed transfers, grouped by token address.
    pub async fn get_bridge_tokens(&self) -> Vec<BridgeTokenInfo> {
        let mut grouped: std::collections::HashMap<String, Vec<String>> =
            std::collections::HashMap::new();

        let cache = self.cache.read().await;
        for transfers in cache.transfers.values() {
            for t in transfers {
                if t.token.is_empty() {
                    continue;
                }
                grouped
                    .entry(t.token.to_lowercase())
                    .or_default()
                    .push(t.bridge.clone());
            }
        }

        grouped
            .into_iter()
            .map(|(address, mut bridges)| {
                bridges.sort();
                bridges.dedup();
                BridgeTokenInfo {
                    address,
                    // Symbol and name are not present in deposit logs; they are
                    // left empty for the caller to resolve from the token
                    // contract rather than fabricated here.
                    symbol: String::new(),
                    name: String::new(),
                    decimals: 18,
                    bridges,
                    chain_id: self.config.chain_id,
                }
            })
            .collect()
    }

    // -------------------------------------------------------------------------
    // JSON-RPC
    // -------------------------------------------------------------------------

    async fn rpc(
        &self,
        method: &str,
        params: serde_json::Value,
    ) -> Result<serde_json::Value, BridgeError> {
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
            .map_err(|e| BridgeError::ApiError(e.to_string()))?;

        if !response.status().is_success() {
            return Err(BridgeError::ApiError(format!(
                "{method} returned HTTP {}",
                response.status()
            )));
        }

        let value: serde_json::Value = response
            .json()
            .await
            .map_err(|e| BridgeError::ParseError(e.to_string()))?;

        if let Some(err) = value.get("error") {
            return Err(BridgeError::ApiError(format!("{method}: {err}")));
        }

        value
            .get("result")
            .cloned()
            .ok_or_else(|| BridgeError::ParseError(format!("{method}: response has no result")))
    }

    async fn block_number(&self) -> Result<u64, BridgeError> {
        let result = self.rpc("eth_blockNumber", serde_json::json!([])).await?;
        result
            .as_str()
            .and_then(parse_quantity)
            .ok_or_else(|| BridgeError::ParseError("eth_blockNumber returned no quantity".into()))
    }

    async fn get_logs(
        &self,
        address: &str,
        topics: &[Option<String>],
        from_block: u64,
        to_block: u64,
    ) -> Result<Vec<serde_json::Value>, BridgeError> {
        let filter = serde_json::json!({
            "address": address,
            "fromBlock": format!("0x{from_block:x}"),
            "toBlock": format!("0x{to_block:x}"),
            "topics": topics,
        });

        let result = self.rpc("eth_getLogs", serde_json::json!([filter])).await?;
        result
            .as_array()
            .cloned()
            .ok_or_else(|| BridgeError::ParseError("eth_getLogs did not return an array".into()))
    }
}

// =============================================================================
// ABI DECODING
// =============================================================================

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DecodedValue {
    Address(String),
    Uint(String),
    Int(String),
    Bool(bool),
    Bytes32(String),
    Bytes(Vec<u8>),
    Text(String),
}

impl DecodedValue {
    /// Interpret a single 32-byte topic word.
    ///
    /// An indexed dynamic value is stored as the keccak256 of its content, so it
    /// is surfaced as a 32-byte hash rather than pretended to be the value.
    fn from_topic(kind: FieldKind, topic: &str) -> Option<Self> {
        let raw = hex::decode(topic.strip_prefix("0x").unwrap_or(topic)).ok()?;
        match kind {
            FieldKind::Address => {
                if raw.len() < 20 {
                    return None;
                }
                Some(DecodedValue::Address(format!(
                    "0x{}",
                    hex::encode(&raw[raw.len() - 20..])
                )))
            }
            FieldKind::Uint => Some(DecodedValue::Uint(format!("0x{}", hex::encode(&raw)))),
            FieldKind::Int => Some(DecodedValue::Int(format!("0x{}", hex::encode(&raw)))),
            FieldKind::Bool => raw.last().map(|b| DecodedValue::Bool(*b != 0)),
            FieldKind::Bytes32 => Some(DecodedValue::Bytes32(format!("0x{}", hex::encode(&raw)))),
            FieldKind::Bytes | FieldKind::String => {
                Some(DecodedValue::Bytes32(format!("0x{}", hex::encode(&raw))))
            }
        }
    }

    pub fn as_address(&self) -> String {
        match self {
            DecodedValue::Address(a) => a.clone(),
            DecodedValue::Bytes32(b) | DecodedValue::Uint(b) | DecodedValue::Int(b) => {
                let raw = decode_hex_bytes(b);
                if raw.len() >= 20 {
                    format!("0x{}", hex::encode(&raw[raw.len() - 20..]))
                } else {
                    String::new()
                }
            }
            _ => String::new(),
        }
    }

    pub fn as_uint_string(&self) -> String {
        match self {
            DecodedValue::Uint(b) | DecodedValue::Int(b) | DecodedValue::Bytes32(b) => {
                decode_u64(b).map(|v| v.to_string()).unwrap_or_default()
            }
            DecodedValue::Bool(v) => (*v as u8).to_string(),
            _ => String::new(),
        }
    }

    pub fn as_u64(&self) -> Option<u64> {
        match self {
            DecodedValue::Uint(b) | DecodedValue::Int(b) | DecodedValue::Bytes32(b) => decode_u64(b),
            DecodedValue::Bool(v) => Some(*v as u64),
            _ => None,
        }
    }

    pub fn as_hex_string(&self) -> String {
        match self {
            DecodedValue::Bytes32(b) => b.clone(),
            DecodedValue::Bytes(b) => format!("0x{}", hex::encode(b)),
            DecodedValue::Uint(b) | DecodedValue::Int(b) => {
                format!("0x{}", hex::encode(decode_hex_bytes(b)))
            }
            _ => String::new(),
        }
    }

    /// A stable identifier for a transfer: the decimal value for numeric ids,
    /// the full hex word for byte ids.
    pub fn as_transfer_id(&self) -> String {
        match self {
            DecodedValue::Uint(_) | DecodedValue::Int(_) => self.as_uint_string(),
            _ => self.as_hex_string(),
        }
    }

    pub fn as_text(&self) -> String {
        match self {
            DecodedValue::Text(s) => s.clone(),
            DecodedValue::Bytes(b) => String::from_utf8_lossy(b).to_string(),
            _ => String::new(),
        }
    }
}

fn decode_hex_bytes(value: &str) -> Vec<u8> {
    hex::decode(value.strip_prefix("0x").unwrap_or(value)).unwrap_or_default()
}

/// Read a big-endian hex word or quantity as an unsigned integer, saturating
/// above `u64`.
fn decode_u64(value: &str) -> Option<u64> {
    let raw = decode_hex_bytes(value);
    let low = raw.len().saturating_sub(8);
    if raw[..low].iter().any(|b| *b != 0) {
        return Some(u64::MAX);
    }
    let mut v = 0u64;
    for b in &raw[low..] {
        v = (v << 8) | *b as u64;
    }
    Some(v)
}

/// Decode the non-indexed parameters of an event.
///
/// Static parameters occupy one 32-byte word each, inline. Dynamic parameters
/// store an offset (relative to the start of the data area) to a length-prefixed
/// payload. Both are handled, in any mixture.
pub fn decode_abi(fields: &[BridgeField], data: &[u8]) -> Option<Vec<DecodedValue>> {
    let mut out = Vec::with_capacity(fields.len());
    let mut head_cursor = 0usize;

    for field in fields {
        if field.kind.is_dynamic() {
            let offset_word = data.get(head_cursor..head_cursor + 32)?;
            let offset = decode_u64(&format!("0x{}", hex::encode(offset_word)))? as usize;
            head_cursor += 32;

            let len_word = data.get(offset..offset + 32)?;
            let len = decode_u64(&format!("0x{}", hex::encode(len_word)))? as usize;
            let start = offset.checked_add(32)?;
            let end = start.checked_add(len)?;
            let payload = data.get(start..end)?.to_vec();

            out.push(match field.kind {
                FieldKind::Bytes => DecodedValue::Bytes(payload),
                _ => DecodedValue::Text(String::from_utf8_lossy(&payload).to_string()),
            });
        } else {
            let word = data.get(head_cursor..head_cursor + 32)?;
            head_cursor += 32;
            let hex_word = format!("0x{}", hex::encode(word));
            out.push(match field.kind {
                FieldKind::Address => {
                    DecodedValue::Address(format!("0x{}", hex::encode(&word[12..32])))
                }
                FieldKind::Uint => DecodedValue::Uint(hex_word),
                FieldKind::Int => DecodedValue::Int(hex_word),
                FieldKind::Bool => DecodedValue::Bool(word[31] != 0),
                FieldKind::Bytes32 => DecodedValue::Bytes32(hex_word),
                FieldKind::Bytes | FieldKind::String => return None,
            });
        }
    }

    Some(out)
}

// =============================================================================
// HELPERS
// =============================================================================

pub fn is_address(value: &str) -> bool {
    value.len() == 42
        && value.starts_with("0x")
        && value[2..].chars().all(|c| c.is_ascii_hexdigit())
}

fn padded_address(address: &str) -> String {
    format!(
        "0x{:0>64}",
        address.trim_start_matches("0x").to_ascii_lowercase()
    )
}

/// Parse a JSON-RPC quantity such as `0x1a`.
pub fn parse_quantity(value: &str) -> Option<u64> {
    let stripped = value.strip_prefix("0x").unwrap_or(value);
    if stripped.is_empty() {
        return None;
    }
    u64::from_str_radix(stripped, 16).ok()
}

fn now_secs() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn deployment() -> BridgeDeployment {
        BridgeDeployment {
            name: "Wormhole".to_string(),
            address: "0x1111111111111111111111111111111111111111".to_string(),
            signature: "LogMessagePublished(address,uint64,bytes,uint8)".to_string(),
            indexed: vec![BridgeField {
                name: "sender".to_string(),
                kind: FieldKind::Address,
                role: FieldRole::Sender,
            }],
            data: vec![
                BridgeField {
                    name: "sequence".to_string(),
                    kind: FieldKind::Uint,
                    role: FieldRole::TransferId,
                },
                BridgeField {
                    name: "amount".to_string(),
                    kind: FieldKind::Uint,
                    role: FieldRole::Amount,
                },
                BridgeField {
                    name: "payload".to_string(),
                    kind: FieldKind::Bytes,
                    role: FieldRole::Other,
                },
                BridgeField {
                    name: "consistencyLevel".to_string(),
                    kind: FieldKind::Uint,
                    role: FieldRole::Other,
                },
            ],
            remote_chain_id: 4,
            token_decimals: 8,
        }
    }

    fn tracker() -> BridgeTracker {
        BridgeTracker::new(BridgeConfig {
            rpc_url: "http://localhost:8545".to_string(),
            database_url: String::new(),
            update_interval_secs: 60,
            chain_id: 6666,
            confirmation_blocks: 12,
            lookback_blocks: 100,
            deployments: vec![deployment()],
        })
        .expect("tracker")
    }

    fn word_u64(v: u64) -> [u8; 32] {
        let mut w = [0u8; 32];
        w[24..].copy_from_slice(&v.to_be_bytes());
        w
    }

    /// ABI-encode `(sequence, amount, payload, consistencyLevel)`, with the
    /// dynamic `payload` reached through a 0x80 offset.
    fn payload_data(sequence: u64, amount: u64, payload: &[u8], consistency: u64) -> Vec<u8> {
        let mut data = Vec::new();
        data.extend_from_slice(&word_u64(sequence));
        data.extend_from_slice(&word_u64(amount));
        data.extend_from_slice(&word_u64(0x80)); // offset of the bytes payload
        data.extend_from_slice(&word_u64(consistency));
        data.extend_from_slice(&word_u64(payload.len() as u64));
        let mut padded = payload.to_vec();
        let rem = padded.len() % 32;
        if rem != 0 {
            padded.resize(padded.len() + (32 - rem), 0);
        }
        data.extend_from_slice(&padded);
        data
    }

    #[test]
    fn topic0_matches_keccak256_of_signature() {
        // Independently computed keccak256 of
        // "LogMessagePublished(address,uint64,bytes,uint8)".
        assert_eq!(
            deployment().topic0(),
            "0x97151772152507ed291e4fa49ecaa9ede7de9f86019637bc1e83ce6b00055211"
        );
    }

    #[test]
    fn quantity_parsing_handles_leading_zeroes() {
        assert_eq!(parse_quantity("0x0"), Some(0));
        assert_eq!(parse_quantity("0x1a"), Some(26));
        assert_eq!(parse_quantity("0x100"), Some(256));
        assert_eq!(parse_quantity("0x"), None);
        assert_eq!(parse_quantity("not-hex"), None);
    }

    #[test]
    fn addresses_are_left_padded_to_32_bytes() {
        assert_eq!(
            padded_address("0xABCDEFabcdefABCDEFabcdefABCDEFabcdefABCD"),
            "0x000000000000000000000000abcdefabcdefabcdefabcdefabcdefabcdefabcd"
        );
    }

    #[test]
    fn rejects_malformed_addresses() {
        assert!(!is_address("0x1234"));
        assert!(!is_address("0xzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzz"));
        assert!(!is_address("1111111111111111111111111111111111111111"));
        assert!(is_address("0x1111111111111111111111111111111111111111"));
    }

    #[test]
    fn decodes_static_then_dynamic_parameters() {
        let values =
            decode_abi(&deployment().data, &payload_data(7, 1500, b"abc", 1)).expect("decode");
        assert_eq!(values.len(), 4);
        assert_eq!(values[0].as_uint_string(), "7");
        assert_eq!(values[1].as_uint_string(), "1500");
        assert_eq!(values[2].as_text(), "abc");
        assert_eq!(values[3].as_uint_string(), "1");
    }

    #[test]
    fn decode_rejects_truncated_data() {
        assert!(decode_abi(&deployment().data, &[0u8; 32]).is_none());
        assert!(decode_abi(&deployment().data, &[]).is_none());

        // A dynamic offset pointing past the end of the data must be rejected
        // rather than panicking.
        let mut bad = Vec::new();
        bad.extend_from_slice(&word_u64(1));
        bad.extend_from_slice(&word_u64(0xffff));
        bad.extend_from_slice(&word_u64(1));
        assert!(decode_abi(&deployment().data, &bad).is_none());
    }

    #[test]
    fn indexed_address_is_recovered_from_topic() {
        let topic = padded_address("0xabcdefabcdefabcdefabcdefabcdefabcdefabcd");
        let value = DecodedValue::from_topic(FieldKind::Address, &topic).unwrap();
        assert_eq!(
            value.as_address(),
            "0xabcdefabcdefabcdefabcdefabcdefabcdefabcd"
        );
    }

    #[test]
    fn decode_log_maps_roles_and_computes_confirmations() {
        let sender = "0xabcdefabcdefabcdefabcdefabcdefabcdefabcd";
        let data = payload_data(7, 1500, &[], 1);

        let log = serde_json::json!({
            "address": "0x1111111111111111111111111111111111111111",
            "topics": [deployment().topic0(), padded_address(sender)],
            "data": format!("0x{}", hex::encode(&data)),
            "blockNumber": "0x64",
            "logIndex": "0x2",
            "transactionHash": "0xdeadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeef",
        });

        let t = tracker()
            .decode_log(&deployment(), &log, 200, sender)
            .expect("decodes");
        assert_eq!(t.sender, sender);
        assert_eq!(t.bridge, "Wormhole");
        assert_eq!(t.amount, "1500");
        assert_eq!(t.id, "7", "a numeric sequence is the transfer id");
        assert_eq!(t.source_chain_id, 6666);
        assert_eq!(t.destination_chain_id, 4);
        assert_eq!(t.block_number, 100);
        assert_eq!(t.log_index, 2);
        assert_eq!(t.confirmations, 100);
        assert_eq!(t.status, BridgeTransferStatus::Confirmed);
        assert!(t.deposit_tx.starts_with("0xdeadbeef"));
        assert_eq!(t.from_chain, "6666");
        assert_eq!(t.to_chain, "4");

        // A query for an unrelated address must not match.
        assert!(tracker()
            .decode_log(
                &deployment(),
                &log,
                200,
                "0x9999999999999999999999999999999999999999"
            )
            .is_none());
    }

    #[test]
    fn a_barely_confirmed_transfer_is_pending() {
        let sender = "0xabcdefabcdefabcdefabcdefabcdefabcdefabcd";
        let log = serde_json::json!({
            "topics": [deployment().topic0(), padded_address(sender)],
            "data": format!("0x{}", hex::encode(payload_data(1, 1, &[], 1))),
            "blockNumber": "0x64",
            "logIndex": "0x0",
            "transactionHash": "0xaa",
        });

        // latest = 105, block = 100, confirmations = 5 < 12.
        let t = tracker().decode_log(&deployment(), &log, 105, sender).unwrap();
        assert_eq!(t.confirmations, 5);
        assert_eq!(t.status, BridgeTransferStatus::Pending);
    }

    #[test]
    fn config_rejects_malformed_deployments() {
        let mut d = deployment();
        d.address = "0x123".to_string();
        let config = BridgeConfig {
            rpc_url: "http://localhost:8545".to_string(),
            database_url: String::new(),
            update_interval_secs: 60,
            chain_id: 6666,
            confirmation_blocks: 12,
            lookback_blocks: 100,
            deployments: vec![d],
        };
        assert!(BridgeTracker::new(config).is_err());
    }
}
