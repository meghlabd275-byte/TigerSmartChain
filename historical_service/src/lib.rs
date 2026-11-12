//! Historical State API - Archive Node Queries
//!
//! Provides past balance, storage, transaction and block queries by talking to
//! a real archive node over JSON-RPC, with an on-disk block-snapshot fallback.

use std::sync::Arc;
use tokio::sync::RwLock;
use serde::{Deserialize, Serialize};
use thiserror::Error;

// =============================================================================
// ERRORS
// =============================================================================

#[derive(Error, Debug)]
pub enum HistoricalError {
    #[error("RPC error: {0}")]
    RpcError(String),

    #[error("Parse error: {0}")]
    ParseError(String),

    #[error("Not found: {0}")]
    NotFound(String),
}

// =============================================================================
// CONFIGURATION
// =============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HistoricalConfig {
    pub archive_url: Option<String>,
    pub snapshot_dir: String,
    pub max_history_blocks: u64,
}

impl Default for HistoricalConfig {
    fn default() -> Self {
        Self {
            archive_url: std::env::var("ARCHIVE_RPC_URL").ok(),
            snapshot_dir: std::env::var("SNAPSHOT_DIR")
                .unwrap_or_else(|_| "/var/lib/tigerscan/snapshots".to_string()),
            max_history_blocks: 100000,
        }
    }
}

// =============================================================================
// HISTORICAL TYPES
// =============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HistoricalBalance {
    pub address: String,
    pub balance: String,
    pub block_number: u64,
    pub timestamp: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HistoricalStorage {
    pub address: String,
    pub key: String,
    pub value: String,
    pub block_number: u64,
    pub timestamp: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HistoricalTransaction {
    pub hash: String,
    pub block_number: u64,
    pub timestamp: u64,
    pub from: String,
    pub to: String,
    pub value: String,
    pub input: String,
    pub status: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BlockSnapshot {
    pub block_number: u64,
    pub block_hash: String,
    pub state_root: String,
    pub timestamp: u64,
    pub accounts: Vec<AccountState>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AccountState {
    pub address: String,
    pub balance: String,
    pub nonce: u64,
    pub code_hash: String,
    pub storage_root: String,
}

// =============================================================================
// PURE HELPERS (unit tested, no I/O)
// =============================================================================

/// Parse a `0x`-prefixed hexadecimal quantity into a `u64`.
pub fn parse_hex_u64(value: &str) -> Result<u64, HistoricalError> {
    let raw = value
        .trim()
        .strip_prefix("0x")
        .or_else(|| value.trim().strip_prefix("0X"))
        .unwrap_or(value.trim());

    if raw.is_empty() {
        return Ok(0);
    }
    if !raw.chars().all(|c| c.is_ascii_hexdigit()) {
        return Err(HistoricalError::ParseError(format!(
            "non-hexadecimal quantity: {value}"
        )));
    }

    u64::from_str_radix(raw, 16)
        .map_err(|e| HistoricalError::ParseError(format!("invalid hex {value}: {e}")))
}

/// Render a block number as a `0x`-prefixed JSON-RPC quantity.
pub fn block_tag(block_number: u64) -> String {
    format!("0x{block_number:x}")
}

/// Validate that a string looks like a 20-byte EVM address.
pub fn is_valid_address(address: &str) -> bool {
    let raw = address
        .strip_prefix("0x")
        .or_else(|| address.strip_prefix("0X"))
        .unwrap_or("");
    raw.len() == 40 && raw.chars().all(|c| c.is_ascii_hexdigit())
}

/// Normalise an address to lowercase `0x`-prefixed form (no validation).
pub fn normalize_address(address: &str) -> String {
    let raw = address
        .strip_prefix("0x")
        .or_else(|| address.strip_prefix("0X"))
        .unwrap_or(address);
    format!("0x{}", raw.to_ascii_lowercase())
}

/// Case-insensitive address lookup within a snapshot.
pub fn find_account<'a>(snapshot: &'a BlockSnapshot, address: &str) -> Option<&'a AccountState> {
    let needle = normalize_address(address);
    snapshot
        .accounts
        .iter()
        .find(|a| normalize_address(&a.address) == needle)
}

// =============================================================================
// HISTORICAL SERVICE
// =============================================================================

pub struct HistoricalService {
    config: HistoricalConfig,
    archive_client: Option<ArchiveClient>,
    snapshot_cache: Arc<RwLock<SnapshotCache>>,
}

#[derive(Debug, Default)]
pub struct SnapshotCache {
    pub snapshots: std::collections::HashMap<u64, BlockSnapshot>,
    pub last_load: u64,
}

impl HistoricalService {
    /// Create new historical service
    pub fn new(config: HistoricalConfig) -> Self {
        let archive_client = config
            .archive_url
            .as_ref()
            .map(|url| ArchiveClient::new(url));

        Self {
            config,
            archive_client,
            snapshot_cache: Arc::new(RwLock::new(SnapshotCache::default())),
        }
    }

    /// Get historical balance at block
    pub async fn get_balance_at(
        &self,
        address: &str,
        block_number: u64,
    ) -> Result<HistoricalBalance, HistoricalError> {
        if let Some(ref client) = self.archive_client {
            if let Ok(balance) = client.get_balance_at(address, block_number).await {
                return Ok(balance);
            }
        }

        self.get_balance_from_snapshot(address, block_number).await
    }

    /// Get historical storage at block
    pub async fn get_storage_at(
        &self,
        address: &str,
        key: &str,
        block_number: u64,
    ) -> Result<HistoricalStorage, HistoricalError> {
        if let Some(ref client) = self.archive_client {
            if let Ok(storage) = client.get_storage_at(address, key, block_number).await {
                return Ok(storage);
            }
        }

        self.get_storage_from_snapshot(address, key, block_number)
            .await
    }

    /// Get historical transaction
    pub async fn get_transaction_at(
        &self,
        tx_hash: &str,
        block_number: u64,
    ) -> Result<HistoricalTransaction, HistoricalError> {
        if let Some(ref client) = self.archive_client {
            return client.get_transaction_at(tx_hash, block_number).await;
        }

        Err(HistoricalError::NotFound(
            "No archive node configured".to_string(),
        ))
    }

    /// Get the snapshot in effect at (or before) `block_number`.
    async fn latest_snapshot_at(&self, block_number: u64) -> Option<BlockSnapshot> {
        let cache = self.snapshot_cache.read().await;
        cache
            .snapshots
            .values()
            .filter(|s| s.block_number <= block_number)
            .max_by_key(|s| s.block_number)
            .cloned()
    }

    /// Get balance from snapshot
    async fn get_balance_from_snapshot(
        &self,
        address: &str,
        block_number: u64,
    ) -> Result<HistoricalBalance, HistoricalError> {
        let snapshot = self
            .latest_snapshot_at(block_number)
            .await
            .ok_or_else(|| HistoricalError::NotFound(format!("No snapshot for block {}", block_number)))?;

        let account = find_account(&snapshot, address).ok_or_else(|| {
            HistoricalError::NotFound(format!(
                "Account {} not present in snapshot at block {}",
                address, snapshot.block_number
            ))
        })?;

        Ok(HistoricalBalance {
            address: address.to_string(),
            balance: account.balance.clone(),
            block_number: snapshot.block_number,
            timestamp: snapshot.timestamp,
        })
    }

    /// Get storage value from a snapshot's account state.
    ///
    /// Snapshots store a per-account `storage_root` rather than the full slot
    /// map, so the individual slot cannot be resolved from a snapshot. We
    /// report that explicitly instead of fabricating a value.
    async fn get_storage_from_snapshot(
        &self,
        address: &str,
        key: &str,
        block_number: u64,
    ) -> Result<HistoricalStorage, HistoricalError> {
        match self.latest_snapshot_at(block_number).await {
            Some(snapshot) if find_account(&snapshot, address).is_some() => {
                Err(HistoricalError::NotFound(format!(
                    "Snapshot at block {} only stores storage_root; slot {key} requires an archive node",
                    snapshot.block_number
                )))
            }
            Some(snapshot) => Err(HistoricalError::NotFound(format!(
                "Account {} not present in snapshot at block {}",
                address, snapshot.block_number
            ))),
            None => Err(HistoricalError::NotFound(format!(
                "No snapshot for block {}",
                block_number
            ))),
        }
    }

    /// Load snapshot from disk
    pub async fn load_snapshot(&self, block_number: u64) -> Result<BlockSnapshot, HistoricalError> {
        let path = format!("{}/snapshot_{}.json", self.config.snapshot_dir, block_number);

        let data = tokio::fs::read(&path)
            .await
            .map_err(|e| HistoricalError::NotFound(e.to_string()))?;

        let snapshot: BlockSnapshot = serde_json::from_slice(&data)
            .map_err(|e| HistoricalError::ParseError(e.to_string()))?;

        let mut cache = self.snapshot_cache.write().await;
        cache.snapshots.insert(block_number, snapshot.clone());
        cache.last_load = block_number;

        Ok(snapshot)
    }

    /// Insert a snapshot into the in-memory cache (used by tests and ingestors).
    pub async fn cache_snapshot(&self, snapshot: BlockSnapshot) {
        let block = snapshot.block_number;
        let mut cache = self.snapshot_cache.write().await;
        cache.snapshots.insert(block, snapshot);
    }
}

// =============================================================================
// ARCHIVE CLIENT
// =============================================================================

pub struct ArchiveClient {
    url: String,
    client: reqwest::Client,
}

impl ArchiveClient {
    pub fn new(url: &str) -> Self {
        Self {
            url: url.to_string(),
            client: reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(30))
                .build()
                .unwrap_or_default(),
        }
    }

    async fn rpc(
        &self,
        method: &str,
        params: serde_json::Value,
    ) -> Result<serde_json::Value, HistoricalError> {
        let request = serde_json::json!({
            "jsonrpc": "2.0",
            "method": method,
            "params": params,
            "id": 1
        });

        let response = self
            .client
            .post(&self.url)
            .json(&request)
            .send()
            .await
            .map_err(|e| HistoricalError::RpcError(e.to_string()))?;

        let body: serde_json::Value = response
            .json()
            .await
            .map_err(|e| HistoricalError::ParseError(e.to_string()))?;

        if let Some(err) = body.get("error") {
            return Err(HistoricalError::RpcError(err.to_string()));
        }

        body.get("result")
            .cloned()
            .ok_or_else(|| HistoricalError::RpcError("RPC response missing result".to_string()))
    }

    pub async fn get_balance_at(
        &self,
        address: &str,
        block_number: u64,
    ) -> Result<HistoricalBalance, HistoricalError> {
        let result = self
            .rpc(
                "eth_getBalance",
                serde_json::json!([address, block_tag(block_number)]),
            )
            .await?;

        let balance = result.as_str().ok_or_else(|| {
            HistoricalError::NotFound("eth_getBalance returned no result".to_string())
        })?;

        let timestamp = self.get_block_timestamp(block_number).await.unwrap_or(0);

        Ok(HistoricalBalance {
            address: address.to_string(),
            balance: balance.to_string(),
            block_number,
            timestamp,
        })
    }

    pub async fn get_storage_at(
        &self,
        address: &str,
        key: &str,
        block_number: u64,
    ) -> Result<HistoricalStorage, HistoricalError> {
        let result = self
            .rpc(
                "eth_getStorageAt",
                serde_json::json!([address, key, block_tag(block_number)]),
            )
            .await?;

        let value = result.as_str().ok_or_else(|| {
            HistoricalError::NotFound("eth_getStorageAt returned no result".to_string())
        })?;

        let timestamp = self.get_block_timestamp(block_number).await.unwrap_or(0);

        Ok(HistoricalStorage {
            address: address.to_string(),
            key: key.to_string(),
            value: value.to_string(),
            block_number,
            timestamp,
        })
    }

    pub async fn get_transaction_at(
        &self,
        tx_hash: &str,
        _block_number: u64,
    ) -> Result<HistoricalTransaction, HistoricalError> {
        let result = self
            .rpc("eth_getTransactionReceipt", serde_json::json!([tx_hash]))
            .await?;

        let receipt = result.as_object().ok_or_else(|| {
            HistoricalError::NotFound("Transaction not found".to_string())
        })?;

        let block_number = receipt
            .get("blockNumber")
            .and_then(|b| b.as_str())
            .and_then(|s| parse_hex_u64(s).ok())
            .unwrap_or(0);

        let timestamp = self.get_block_timestamp(block_number).await.unwrap_or(0);

        Ok(HistoricalTransaction {
            hash: tx_hash.to_string(),
            block_number,
            timestamp,
            from: receipt
                .get("from")
                .and_then(|f| f.as_str())
                .unwrap_or("")
                .to_string(),
            to: receipt
                .get("to")
                .and_then(|t| t.as_str())
                .unwrap_or("")
                .to_string(),
            value: receipt
                .get("value")
                .and_then(|v| v.as_str())
                .unwrap_or("0x0")
                .to_string(),
            input: receipt
                .get("input")
                .and_then(|i| i.as_str())
                .unwrap_or("0x")
                .to_string(),
            status: receipt
                .get("status")
                .and_then(|s| s.as_str())
                .unwrap_or("0x1")
                .to_string(),
        })
    }

    async fn get_block_timestamp(&self, block_number: u64) -> Result<u64, HistoricalError> {
        let result = self
            .rpc(
                "eth_getBlockByNumber",
                serde_json::json!([block_tag(block_number), false]),
            )
            .await?;

        let timestamp = result
            .get("timestamp")
            .and_then(|t| t.as_str())
            .ok_or_else(|| HistoricalError::NotFound("block has no timestamp".to_string()))?;

        parse_hex_u64(timestamp)
    }
}

// =============================================================================
// TESTS
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_snapshot() -> BlockSnapshot {
        BlockSnapshot {
            block_number: 100,
            block_hash: "0xabc".to_string(),
            state_root: "0xdef".to_string(),
            timestamp: 1_700_000_000,
            accounts: vec![AccountState {
                address: "0xAbCd000000000000000000000000000000000001".to_string(),
                balance: "0xde0b6b3a7640000".to_string(),
                nonce: 7,
                code_hash: "0x00".to_string(),
                storage_root: "0x11".to_string(),
            }],
        }
    }

    #[test]
    fn parse_hex_u64_handles_prefixes() {
        assert_eq!(parse_hex_u64("0x10").unwrap(), 16);
        assert_eq!(parse_hex_u64("0X10").unwrap(), 16);
        assert_eq!(parse_hex_u64("10").unwrap(), 16);
        assert_eq!(parse_hex_u64("0x0").unwrap(), 0);
        assert_eq!(parse_hex_u64("").unwrap(), 0);
        assert!(parse_hex_u64("0xzz").is_err());
        assert!(parse_hex_u64("0x10000000000000000").is_err());
    }

    #[test]
    fn block_tag_is_canonical() {
        assert_eq!(block_tag(0), "0x0");
        assert_eq!(block_tag(255), "0xff");
        assert_eq!(block_tag(1_000_000), "0xf4240");
    }

    #[test]
    fn address_validation_and_normalisation() {
        assert!(is_valid_address("0x1111111111111111111111111111111111111111"));
        assert!(!is_valid_address("0x1111"));
        assert!(!is_valid_address("1111111111111111111111111111111111111111"));
        assert_eq!(
            normalize_address("0xABCDEF0000000000000000000000000000000001"),
            "0xabcdef0000000000000000000000000000000001"
        );
        assert_eq!(
            normalize_address("ABCDEF0000000000000000000000000000000001"),
            "0xabcdef0000000000000000000000000000000001"
        );
    }

    #[test]
    fn find_account_is_case_insensitive() {
        let snapshot = sample_snapshot();
        let found = find_account(&snapshot, "0xabcd000000000000000000000000000000000001");
        assert!(found.is_some());
        assert_eq!(found.unwrap().nonce, 7);
        assert!(find_account(&snapshot, "0x9999999999999999999999999999999999999999").is_none());
    }

    #[tokio::test]
    async fn balance_falls_back_to_snapshot_without_rpc() {
        let config = HistoricalConfig {
            archive_url: None,
            snapshot_dir: "/nonexistent".to_string(),
            max_history_blocks: 10,
        };
        let service = HistoricalService::new(config);
        service.cache_snapshot(sample_snapshot()).await;

        let balance = service
            .get_balance_at("0xabcd000000000000000000000000000000000001", 150)
            .await
            .unwrap();
        assert_eq!(balance.balance, "0xde0b6b3a7640000");
        assert_eq!(balance.block_number, 100);
        assert_eq!(balance.timestamp, 1_700_000_000);
    }

    #[tokio::test]
    async fn balance_before_earliest_snapshot_is_not_found() {
        let config = HistoricalConfig {
            archive_url: None,
            snapshot_dir: "/nonexistent".to_string(),
            max_history_blocks: 10,
        };
        let service = HistoricalService::new(config);
        service.cache_snapshot(sample_snapshot()).await;

        let result = service
            .get_balance_at("0xabcd000000000000000000000000000000000001", 50)
            .await;
        assert!(matches!(result, Err(HistoricalError::NotFound(_))));
    }

    #[tokio::test]
    async fn storage_snapshot_reports_missing_slot_rather_than_faking_it() {
        let config = HistoricalConfig {
            archive_url: None,
            snapshot_dir: "/nonexistent".to_string(),
            max_history_blocks: 10,
        };
        let service = HistoricalService::new(config);
        service.cache_snapshot(sample_snapshot()).await;

        let result = service
            .get_storage_at("0xabcd000000000000000000000000000000000001", "0x0", 100)
            .await;
        assert!(matches!(result, Err(HistoricalError::NotFound(_))));
    }
}
