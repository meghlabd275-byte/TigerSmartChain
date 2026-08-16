//! Historical State API Service
//!
//! Provides historical state queries for:
//! - Historical balance queries at any block
//! - Historical storage slot queries
//! - State proofs generation
//! - Account state at point in time

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::time::{SystemTime, UNIX_EPOCH};

// =============================================================================
// ERRORS
// =============================================================================

/// Historical State Errors
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum HistoricalStateError {
    #[serde(rename = "block_not_found")]
    BlockNotFound(u64),
    #[serde(rename = "state_not_found")]
    StateNotFound(String),
    #[serde(rename = "invalid_block")]
    InvalidBlock(String),
    #[serde(rename = "query_error")]
    QueryError(String),
}

// =============================================================================
// DATA STRUCTURES
// =============================================================================

/// Historical account state
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AccountState {
    pub address: String,
    pub block_number: u64,
    pub nonce: u64,
    pub balance: String,
    pub code_hash: String,
    pub storage_root: String,
    pub code: Option<String>,
    pub timestamp: u64,
}

/// Historical storage slot
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StorageSlot {
    pub address: String,
    pub slot: String,
    pub key: String,
    pub value: String,
    pub block_number: u64,
    pub timestamp: u64,
}

/// Historical balance
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HistoricalBalance {
    pub address: String,
    pub balance: String,
    pub block_number: u64,
    pub block_hash: String,
    pub timestamp: u64,
}

/// State proof
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StateProof {
    pub address: String,
    pub block_number: u64,
    pub account_proof: Vec<String>,
    pub storage_proof: Vec<StorageProof>,
}

/// Storage proof for a single slot
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StorageProof {
    pub key: String,
    pub value: String,
    pub proof: Vec<String>,
}

/// Block state snapshot
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BlockState {
    pub block_number: u64,
    pub block_hash: String,
    pub state_root: String,
    pub transaction_root: String,
    pub receipts_root: String,
    pub total_difficulty: String,
    pub timestamp: u64,
}

// =============================================================================
// STORAGE TYPES
// =============================================================================

/// Stored account state
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoredAccount {
    pub address: String,
    pub block_number: u64,
    pub nonce: String,
    pub balance: String,
    pub code_hash: String,
    pub storage_root: String,
    pub code: Option<String>,
}

/// Stored storage slot
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoredSlot {
    pub address: String,
    pub slot: String,
    pub key: String,
    pub value: String,
    pub block_number: u64,
}

// =============================================================================
// HISTORICAL STATE INDEXER
// =============================================================================

/// Historical State Indexer - maintains historical state snapshots
pub struct HistoricalIndexer {
    /// RPC endpoint for state queries
    rpc_url: String,
    /// Blocking HTTP client for synchronous JSON-RPC calls
    http: reqwest::blocking::Client,
    /// Cache of recent states
    state_cache: HashMap<String, AccountState>,
    /// Balance history cache
    balance_cache: HashMap<String, Vec<HistoricalBalance>>,
    /// Storage cache
    storage_cache: HashMap<String, HashMap<String, String>>,
    /// Maximum cache size per address
    max_cache_size: usize,
    /// Statistics
    stats: IndexerStats,
}

/// Indexer statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IndexerStats {
    pub total_queries: u64,
    pub cache_hits: u64,
    pub cache_misses: u64,
    pub errors: u64,
}

impl Default for IndexerStats {
    fn default() -> Self {
        Self {
            total_queries: 0,
            cache_hits: 0,
            cache_misses: 0,
            errors: 0,
        }
    }
}

impl HistoricalIndexer {
    /// Create new historical indexer
    pub fn new(rpc_url: String) -> Self {
        let http = reqwest::blocking::Client::builder()
            .timeout(std::time::Duration::from_secs(15))
            .build()
            .unwrap_or_else(|_| reqwest::blocking::Client::new());
        Self {
            rpc_url,
            http,
            state_cache: HashMap::new(),
            balance_cache: HashMap::new(),
            storage_cache: HashMap::new(),
            max_cache_size: 1000,
            stats: IndexerStats::default(),
        }
    }

    /// Issue a synchronous JSON-RPC call and return the `result` field.
    fn rpc<T: serde::de::DeserializeOwned>(
        &self,
        method: &str,
        params: serde_json::Value,
    ) -> Result<T, HistoricalStateError> {
        if self.rpc_url.is_empty() {
            return Err(HistoricalStateError::QueryError(
                "no rpc_url configured".to_string(),
            ));
        }
        let body = serde_json::json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": method,
            "params": params,
        });
        let resp: serde_json::Value = self
            .http
            .post(&self.rpc_url)
            .json(&body)
            .send()
            .map_err(|e| HistoricalStateError::QueryError(format!("RPC request failed: {e}")))?
            .json()
            .map_err(|e| HistoricalStateError::QueryError(format!("RPC parse failed: {e}")))?;
        if let Some(err) = resp.get("error") {
            return Err(HistoricalStateError::QueryError(format!("RPC error: {err}")));
        }
        let result = resp
            .get("result")
            .ok_or_else(|| HistoricalStateError::QueryError(format!("missing result: {resp}")))?;
        serde_json::from_value(result.clone())
            .map_err(|e| HistoricalStateError::QueryError(format!("decode failed: {e}")))
    }

    /// Get account state at block via eth_getProof + eth_getCode.
    pub fn get_account_at_block(&mut self, address: &str, block_number: u64) -> Result<AccountState, HistoricalStateError> {
        self.stats.total_queries += 1;

        let cache_key = format!("{}:{}", address, block_number);

        if let Some(state) = self.state_cache.get(&cache_key) {
            self.stats.cache_hits += 1;
            return Ok(state.clone());
        }

        self.stats.cache_misses += 1;

        let block_param = serde_json::Value::String(format!("0x{:x}", block_number));
        let proof: serde_json::Value = self.rpc(
            "eth_getProof",
            serde_json::json!([address, [], block_param]),
        )?;
        let balance = proof.get("balance").and_then(|v| v.as_str()).unwrap_or("0x0").to_string();
        let nonce = proof.get("nonce").and_then(|v| v.as_str())
            .and_then(|s| u64::from_str_radix(s.trim_start_matches("0x"), 16).ok())
            .unwrap_or(0);
        let code_hash = proof.get("codeHash").and_then(|v| v.as_str())
            .unwrap_or("0x0000000000000000000000000000000000000000000000000000000000000000")
            .to_string();
        let storage_root = proof.get("storageHash").and_then(|v| v.as_str())
            .unwrap_or("0x0000000000000000000000000000000000000000000000000000000000000000")
            .to_string();

        let code: Option<String> = self
            .rpc::<String>("eth_getCode", serde_json::json!([address, block_param]))
            .ok()
            .and_then(|c| if c == "0x" || c.is_empty() { None } else { Some(c) });

        let state = AccountState {
            address: address.to_string(),
            block_number,
            nonce,
            balance,
            code_hash,
            storage_root,
            code,
            timestamp: now_unix(),
        };

        if self.state_cache.len() >= self.max_cache_size {
            if let Some(first) = self.state_cache.keys().next().cloned() {
                self.state_cache.remove(&first);
            }
        }
        self.state_cache.insert(cache_key, state.clone());

        Ok(state)
    }

    /// Get historical balance via eth_getBalance.
    pub fn get_balance_at_block(&mut self, address: &str, block_number: u64) -> Result<HistoricalBalance, HistoricalStateError> {
        self.stats.total_queries += 1;

        let cache_key = format!("{}:{}", address, block_number);

        if let Some(balances) = self.balance_cache.get(&cache_key) {
            if let Some(balance) = balances.last() {
                self.stats.cache_hits += 1;
                return Ok(balance.clone());
            }
        }

        self.stats.cache_misses += 1;

        let block_param = serde_json::Value::String(format!("0x{:x}", block_number));
        let balance_hex: String = self.rpc("eth_getBalance", serde_json::json!([address, block_param]))?;
        let block: serde_json::Value = self.rpc("eth_getBlockByNumber", serde_json::json!([block_param, false]))
            .unwrap_or(serde_json::Value::Null);
        let block_hash = block.get("hash").and_then(|v| v.as_str())
            .unwrap_or("0x0000000000000000000000000000000000000000000000000000000000000000")
            .to_string();

        let balance = HistoricalBalance {
            address: address.to_string(),
            balance: balance_hex,
            block_number,
            block_hash,
            timestamp: now_unix(),
        };

        self.balance_cache.entry(cache_key).or_insert_with(Vec::new).push(balance.clone());

        Ok(balance)
    }

    /// Get storage slot at block via eth_getStorageAt.
    pub fn get_storage_at_block(&mut self, address: &str, slot: &str, block_number: u64) -> Result<StorageSlot, HistoricalStateError> {
        self.stats.total_queries += 1;

        let cache_key = format!("{}:{}:{}", address, slot, block_number);

        if let Some(address_cache) = self.storage_cache.get(address) {
            if let Some(value) = address_cache.get(&format!("{}:{}", slot, block_number)) {
                self.stats.cache_hits += 1;
                return Ok(StorageSlot {
                    address: address.to_string(),
                    slot: slot.to_string(),
                    key: slot.to_string(),
                    value: value.clone(),
                    block_number,
                    timestamp: now_unix(),
                });
            }
        }

        self.stats.cache_misses += 1;

        let block_param = serde_json::Value::String(format!("0x{:x}", block_number));
        let slot_key = normalize_slot_key(slot);
        let value: String = self.rpc("eth_getStorageAt", serde_json::json!([address, slot_key, block_param]))?;
        let value = if value.is_empty() {
            "0x0000000000000000000000000000000000000000000000000000000000000000".to_string()
        } else {
            value
        };

        let slot_data = StorageSlot {
            address: address.to_string(),
            slot: slot.to_string(),
            key: slot.to_string(),
            value: value.clone(),
            block_number,
            timestamp: now_unix(),
        };

        self.storage_cache
            .entry(address.to_string())
            .or_insert_with(HashMap::new)
            .insert(format!("{}:{}", slot, block_number), value);

        Ok(slot_data)
    }

    /// Generate state proof via eth_getProof.
    pub fn get_state_proof(&self, address: &str, block_number: u64, storage_keys: &[String]) -> Result<StateProof, HistoricalStateError> {
        let block_param = serde_json::Value::String(format!("0x{:x}", block_number));
        let normalized_keys: Vec<String> = storage_keys.iter().map(|k| normalize_slot_key(k)).collect();
        let proof: serde_json::Value = self.rpc("eth_getProof", serde_json::json!([address, normalized_keys, block_param]))?;

        let account_proof: Vec<String> = proof.get("accountProof").and_then(|v| v.as_array())
            .map(|arr| arr.iter().map(|p| p.as_str().unwrap_or("").to_string()).collect())
            .unwrap_or_default();

        let storage_proof: Vec<StorageProof> = proof.get("storageProof").and_then(|v| v.as_array())
            .map(|arr| arr.iter().map(|p| StorageProof {
                key: p.get("key").and_then(|v| v.as_str()).unwrap_or("").to_string(),
                value: p.get("value").and_then(|v| v.as_str()).unwrap_or("0x0").to_string(),
                proof: p.get("proof").and_then(|v| v.as_array())
                    .map(|a| a.iter().map(|x| x.as_str().unwrap_or("").to_string()).collect())
                    .unwrap_or_default(),
            }).collect())
            .unwrap_or_default();

        Ok(StateProof {
            address: address.to_string(),
            block_number,
            account_proof,
            storage_proof,
        })
    }

    /// Get block state via eth_getBlockByNumber.
    pub fn get_block_state(&self, block_number: u64) -> Result<BlockState, HistoricalStateError> {
        let block_param = serde_json::Value::String(format!("0x{:x}", block_number));
        let block: serde_json::Value = self.rpc("eth_getBlockByNumber", serde_json::json!([block_param, false]))?;
        if block.is_null() {
            return Err(HistoricalStateError::BlockNotFound(block_number));
        }
        Ok(BlockState {
            block_number,
            block_hash: block.get("hash").and_then(|v| v.as_str()).unwrap_or("0x0").to_string(),
            state_root: block.get("stateRoot").and_then(|v| v.as_str()).unwrap_or("0x0").to_string(),
            transaction_root: block.get("transactionsRoot").and_then(|v| v.as_str()).unwrap_or("0x0").to_string(),
            receipts_root: block.get("receiptsRoot").and_then(|v| v.as_str()).unwrap_or("0x0").to_string(),
            total_difficulty: block.get("totalDifficulty").and_then(|v| v.as_str()).unwrap_or("0x0").to_string(),
            timestamp: block.get("timestamp").and_then(|v| v.as_str())
                .and_then(|s| u64::from_str_radix(s.trim_start_matches("0x"), 16).ok()).unwrap_or(0),
        })
    }

    /// Get balance history for address
    pub fn get_balance_history(&self, address: &str, from_block: u64, to_block: u64) -> Vec<HistoricalBalance> {
        let key = format!("{}:history", address);
        self.balance_cache.get(&key)
            .map(|balances| {
                balances.iter()
                    .filter(|b| b.block_number >= from_block && b.block_number <= to_block)
                    .cloned()
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Get statistics
    pub fn stats(&self) -> &IndexerStats {
        &self.stats
    }

    /// Clear cache
    pub fn clear_cache(&mut self) {
        self.state_cache.clear();
        self.balance_cache.clear();
        self.storage_cache.clear();
    }
}

// =============================================================================
// STATE QUERY SERVICE
// =============================================================================

/// State query service for historical lookups
pub struct StateQueryService {
    indexer: HistoricalIndexer,
}

impl StateQueryService {
    /// Create new service
    pub fn new(rpc_url: String) -> Self {
        Self {
            indexer: HistoricalIndexer::new(rpc_url),
        }
    }

    /// Query account at block
    pub fn account_at(&mut self, address: &str, block_number: u64) -> Result<AccountState, HistoricalStateError> {
        self.indexer.get_account_at_block(address, block_number)
    }

    /// Query balance at block
    pub fn balance_at(&mut self, address: &str, block_number: u64) -> Result<HistoricalBalance, HistoricalStateError> {
        self.indexer.get_balance_at_block(address, block_number)
    }

    /// Query storage at block
    pub fn storage_at(&mut self, address: &str, slot: &str, block_number: u64) -> Result<StorageSlot, HistoricalStateError> {
        self.indexer.get_storage_at_block(address, slot, block_number)
    }

    /// Get state proof
    pub fn proof(&self, address: &str, block_number: u64, storage_keys: &[String]) -> Result<StateProof, HistoricalStateError> {
        self.indexer.get_state_proof(address, block_number, storage_keys)
    }

    /// Get block state
    pub fn block_state(&self, block_number: u64) -> Result<BlockState, HistoricalStateError> {
        self.indexer.get_block_state(block_number)
    }

    /// Get balance history
    pub fn balance_history(&self, address: &str, from_block: u64, to_block: u64) -> Vec<HistoricalBalance> {
        self.indexer.get_balance_history(address, from_block, to_block)
    }
}

// =============================================================================
// HELPER FUNCTIONS
// =============================================================================

/// Get current Unix timestamp
fn now_unix() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs()
}

/// Normalize a storage slot key into a 0x-prefixed 32-byte hex string.
/// Accepts decimal or hex input and left-pads to 32 bytes for eth_getStorageAt.
fn normalize_slot_key(slot: &str) -> String {
    let trimmed = slot.trim();
    if let Some(hex) = trimmed.strip_prefix("0x") {
        let clean = hex.trim_start_matches('0');
        if clean.is_empty() {
            return "0x0000000000000000000000000000000000000000000000000000000000000000".to_string();
        }
        let padded = format!("{:0>64}", clean);
        if padded.len() == 64 {
            return format!("0x{padded}");
        }
        // too long; keep as-is (already a full hash)
        return format!("0x{clean}");
    }
    // decimal input
    if let Ok(n) = trimmed.parse::<u128>() {
        return format!("0x{:064x}", n);
    }
    // fall back: pad whatever string we have
    format!("0x{:0>64}", trimmed)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_normalize_slot_key_decimal() {
        assert_eq!(
            normalize_slot_key("0"),
            "0x0000000000000000000000000000000000000000000000000000000000000000"
        );
        assert_eq!(
            normalize_slot_key("1"),
            "0x0000000000000000000000000000000000000000000000000000000000000001"
        );
    }

    #[test]
    fn test_normalize_slot_key_hex() {
        assert_eq!(
            normalize_slot_key("0x1"),
            "0x0000000000000000000000000000000000000000000000000000000000000001"
        );
        // a full 32-byte hash passes through unchanged (0x-prefixed)
        let hash = "0xa9059cbb2ab094b4c47a900cf8077c98832f4a96b05b06d4ddbc8d2a97dbe3b7";
        assert_eq!(normalize_slot_key(hash), hash);
    }

    #[test]
    fn test_indexer_creation() {
        let indexer = HistoricalIndexer::new("https://rpc.example.com".to_string());
        assert_eq!(indexer.stats().total_queries, 0);
    }

    #[test]
    fn test_empty_rpc_url_errors() {
        let mut indexer = HistoricalIndexer::new(String::new());
        let res = indexer.get_block_state(100);
        assert!(res.is_err());
    }
}