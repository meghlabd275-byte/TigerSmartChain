//! Historical State API - Archive Node Queries
//! 
//! Provides past balance and storage queries using archive node or state snapshots.
//! Supports historical queries at specific block numbers.

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
        let archive_client = config.archive_url.as_ref()
            .map(|url| ArchiveClient::new(url));
        
        Self {
            config,
            archive_client,
            snapshot_cache: Arc::new(RwLock::new(SnapshotCache::default())),
        }
    }
    
    /// Get historical balance at block
    pub async fn get_balance_at(&self, address: &str, block_number: u64) -> Result<HistoricalBalance, HistoricalError> {
        if let Some(ref client) = self.archive_client {
            if let Ok(balance) = client.get_balance_at(address, block_number).await {
                return Ok(balance);
            }
        }
        
        self.get_balance_from_snapshot(address, block_number).await
    }
    
    /// Get historical storage at block
    pub async fn get_storage_at(&self, address: &str, key: &str, block_number: u64) -> Result<HistoricalStorage, HistoricalError> {
        if let Some(ref client) = self.archive_client {
            if let Ok(storage) = client.get_storage_at(address, key, block_number).await {
                return Ok(storage);
            }
        }
        
        self.get_storage_from_snapshot(address, key, block_number).await
    }
    
    /// Get historical transaction
    pub async fn get_transaction_at(&self, tx_hash: &str, block_number: u64) -> Result<HistoricalTransaction, HistoricalError> {
        if let Some(ref client) = self.archive_client {
            return client.get_transaction_at(tx_hash, block_number).await;
        }
        
        Err(HistoricalError::NotFound("No archive node configured".to_string()))
    }
    
    /// Get balance from snapshot
    async fn get_balance_from_snapshot(&self, address: &str, block_number: u64) -> Result<HistoricalBalance, HistoricalError> {
        let cache = self.snapshot_cache.read().await;
        
        let snapshot = cache.snapshots.values()
            .filter(|s| s.block_number <= block_number)
            .max_by_key(|s| s.block_number);
        
        if let Some(snap) = snapshot {
            for account in &snap.accounts {
                if account.address.to_lowercase() == address.to_lowercase() {
                    return Ok(HistoricalBalance {
                        address: address.to_string(),
                        balance: account.balance.clone(),
                        block_number: snap.block_number,
                        timestamp: snap.timestamp,
                    });
                }
            }
        }
        
        Err(HistoricalError::NotFound(format!("No snapshot for block {}", block_number)))
    }
    
    async fn get_storage_from_snapshot(&self, _address: &str, _key: &str, _block_number: u64) -> Result<HistoricalStorage, HistoricalError> {
        Err(HistoricalError::NotFound("Storage snapshot not available".to_string()))
    }
    
    /// Load snapshot from disk
    pub async fn load_snapshot(&self, block_number: u64) -> Result<BlockSnapshot, HistoricalError> {
        let path = format!("{}/snapshot_{}.json", self.config.snapshot_dir, block_number);
        
        let data = tokio::fs::read(&path).await
            .map_err(|e| HistoricalError::NotFound(e.to_string()))?;
        
        let snapshot: BlockSnapshot = serde_json::from_slice(&data)
            .map_err(|e| HistoricalError::ParseError(e.to_string()))?;
        
        let mut cache = self.snapshot_cache.write().await;
        cache.snapshots.insert(block_number, snapshot.clone());
        
        Ok(snapshot)
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
    
    pub async fn get_balance_at(&self, address: &str, block_number: u64) -> Result<HistoricalBalance, HistoricalError> {
        let block_hex = format!("0x{:x}", block_number);
        
        let request = serde_json::json!({
            "jsonrpc": "2.0",
            "method": "eth_getBalance",
            "params": [address, block_hex],
            "id": 1
        });
        
        let response = self.client.post(&self.url)
            .json(&request)
            .send()
            .await
            .map_err(|e| HistoricalError::RpcError(e.to_string()))?;
        
        let result: serde_json::Value = response.json().await
            .map_err(|e| HistoricalError::ParseError(e.to_string()))?;
        
        let balance = result["result"].as_str()
            .ok_or_else(|| HistoricalError::NotFound("No result".to_string()))?;
        
        let timestamp = self.get_block_timestamp(block_number).await.unwrap_or(0);
        
        Ok(HistoricalBalance {
            address: address.to_string(),
            balance: balance.to_string(),
            block_number,
            timestamp,
        })
    }
    
    pub async fn get_storage_at(&self, address: &str, key: &str, block_number: u64) -> Result<HistoricalStorage, HistoricalError> {
        let block_hex = format!("0x{:x}", block_number);
        
        let request = serde_json::json!({
            "jsonrpc": "2.0",
            "method": "eth_getStorageAt",
            "params": [address, key, block_hex],
            "id": 1
        });
        
        let response = self.client.post(&self.url)
            .json(&request)
            .send()
            .await
            .map_err(|e| HistoricalError::RpcError(e.to_string()))?;
        
        let result: serde_json::Value = response.json().await
            .map_err(|e| HistoricalError::ParseError(e.to_string()))?;
        
        let value = result["result"].as_str()
            .ok_or_else(|| HistoricalError::NotFound("No result".to_string()))?;
        
        let timestamp = self.get_block_timestamp(block_number).await.unwrap_or(0);
        
        Ok(HistoricalStorage {
            address: address.to_string(),
            key: key.to_string(),
            value: value.to_string(),
            block_number,
            timestamp,
        })
    }
    
    pub async fn get_transaction_at(&self, tx_hash: &str, _block_number: u64) -> Result<HistoricalTransaction, HistoricalError> {
        let request = serde_json::json!({
            "jsonrpc": "2.0",
            "method": "eth_getTransactionReceipt",
            "params": [tx_hash],
            "id": 1
        });
        
        let response = self.client.post(&self.url)
            .json(&request)
            .send()
            .await
            .map_err(|e| HistoricalError::RpcError(e.to_string()))?;
        
        let result: serde_json::Value = response.json().await
            .map_err(|e| HistoricalError::ParseError(e.to_string()))?;
        
        let receipt = result["result"].as_object()
            .ok_or_else(|| HistoricalError::NotFound("Transaction not found".to_string()))?;
        
        let block_number = receipt["blockNumber"]
            .and_then(|b| b.as_str())
            .map(|s| u64::from_str_radix(s.trim_start_matches("0x"), 16).unwrap_or(0))
            .unwrap_or(0);
        
        let timestamp = self.get_block_timestamp(block_number).await.unwrap_or(0);
        
        Ok(HistoricalTransaction {
            hash: tx_hash.to_string(),
            block_number,
            timestamp,
            from: receipt["from"].and_then(|f| f.as_str()).unwrap_or("").to_string(),
            to: receipt["to"].and_then(|t| t.as_str()).unwrap_or("").to_string(),
            value: receipt["value"].and_then(|v| v.as_str()).unwrap_or("0x0").to_string(),
            input: receipt["input"].and_then(|i| i.as_str()).unwrap_or("0x").to_string(),
            status: receipt["status"].and_then(|s| s.as_str()).unwrap_or("0x1").to_string(),
        })
    }
    
    async fn get_block_timestamp(&self, block_number: u64) -> Result<u64, HistoricalError> {
        let block_hex = format!("0x{:x}", block_number);
        
        let request = serde_json::json!({
            "jsonrpc": "2.0",
            "method": "eth_getBlockByNumber",
            "params": [block_hex, false],
            "id": 1
        });
        
        let response = self.client.post(&self.url)
            .json(&request)
            .send()
            .await
            .map_err(|e| HistoricalError::RpcError(e.to_string()))?;
        
        let result: serde_json::Value = response.json().await
            .map_err(|e| HistoricalError::ParseError(e.to_string()))?;
        
        let timestamp = result["result"]
            .and_then(|r| r.get("timestamp"))
            .and_then(|t| t.as_str())
            .map(|s| u64::from_str_radix(s.trim_start_matches("0x"), 16).unwrap_or(0))
            .unwrap_or(0);
        
        Ok(timestamp)
    }
}