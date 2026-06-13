//! Token Holder Tracker - Real-time holder tracking and transfer events
//! 
//! Tracks token holders and transfer events in real-time using:
//! - RPC queries for balance updates
//! - Event logs for transfers
//! - Database storage for historical holder data

use std::sync::Arc;
use tokio::sync::RwLock;
use serde::{Deserialize, Serialize};
use thiserror::Error;

// =============================================================================
// ERRORS
// =============================================================================

#[derive(Error, Debug)]
pub enum HolderError {
    #[error("RPC error: {0}")]
    RpcError(String),
    
    #[error("Database error: {0}")]
    DatabaseError(String),
    
    #[error("Parse error: {0}")]
    ParseError(String),
}

// =============================================================================
// CONFIGURATION
// =============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HolderConfig {
    pub rpc_url: String,
    pub database_url: String,
    pub redis_url: String,
    pub batch_size: usize,
    pub update_interval_secs: u64,
}

impl Default for HolderConfig {
    fn default() -> Self {
        Self {
            rpc_url: std::env::var("RPC_URL")
                .unwrap_or_else(|_| "http://localhost:8545".to_string()),
            database_url: std::env::var("DATABASE_URL")
                .unwrap_or_else(|_| "postgres://localhost:5432/tigerscan".to_string()),
            redis_url: std::env::var("REDIS_URL")
                .unwrap_or_else(|_| "redis://localhost:6379".to_string()),
            batch_size: 100,
            update_interval_secs: 60,
        }
    }
}

// =============================================================================
// HOLDER TYPES
// =============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenHolder {
    pub address: String,
    pub balance: String,
    pub percentage: f64,
    pub rank: u32,
    pub is_contract: bool,
    pub first_seen: u64,
    pub last_update: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HolderChange {
    pub token: String,
    pub holder: String,
    pub balance_before: String,
    pub balance_after: String,
    pub block_number: u64,
    pub transaction_hash: String,
    pub timestamp: u64,
    pub type_: TransferType,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TransferType {
    Transfer,
    Mint,
    Burn,
    Swap,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenHolderStats {
    pub token: String,
    pub total_holders: u64,
    pub holder_distribution: Vec<HolderPercentile>,
    pub top_10_percent: f64,
    pub top_1_percent: f64,
    pub zero_balance_count: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HolderPercentile {
    pub percentile: String,
    pub holder_count: u64,
    pub total_balance: f64,
}

// =============================================================================
// HOLDER TRACKER
// =============================================================================

pub struct HolderTracker {
    config: HolderConfig,
    cache: Arc<RwLock<HolderCache>>,
    is_running: Arc<RwLock<bool>>,
}

#[derive(Debug, Default)]
pub struct HolderCache {
    pub holders: std::collections::HashMap<String, Vec<TokenHolder>>,
    pub last_update: std::collections::HashMap<String, u64>,
}

impl HolderTracker {
    /// Create new holder tracker
    pub fn new(config: HolderConfig) -> Self {
        Self {
            config,
            cache: Arc::new(RwLock::new(HolderCache::default())),
            is_running: Arc::new(RwLock::new(false)),
        }
    }
    
    /// Start tracking
    pub async fn start(&self) {
        *self.is_running.write().await = true;
    }
    
    /// Stop tracking
    pub async fn stop(&self) {
        *self.is_running.write().await = false;
    }
    
    /// Get holders for token (real data from RPC + events)
    pub async fn get_holders(&self, token: &str) -> Result<Vec<TokenHolder>, HolderError> {
        // Check cache
        {
            let cache = self.cache.read().await;
            if let Some(holders) = cache.holders.get(token) {
                let age = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_secs()
                    .saturating_sub(*cache.last_update.get(token).unwrap_or(&0));
                
                if age < 60 {
                    return Ok(holders.clone());
                }
            }
        }
        
        // Fetch holders from multiple sources
        let mut all_holders = Vec::new();
        
        // Method 1: Query transfer events
        let event_holders = self.get_holders_from_events(token).await?;
        all_holders.extend(event_holders);
        
        // Method 2: Try indexer API if available
        if let Ok(indexer_holders) = self.get_holders_from_indexer(token).await {
            for h in indexer_holders {
                if !all_holders.iter().any(|e: &TokenHolder| e.address == h.address) {
                    all_holders.push(h);
                }
            }
        }
        
        // Sort by balance
        all_holders.sort_by(|a, b| {
            let a_balance = u128::from_str_radix(a.balance.trim_start_matches("0x"), 16).unwrap_or(0);
            let b_balance = u128::from_str_radix(b.balance.trim_start_matches("0x"), 16).unwrap_or(0);
            b_balance.cmp(&a_balance)
        });
        
        // Calculate percentages and ranks
        let total_supply = self.get_total_supply(token).await.unwrap_or(0);
        let mut holders = Vec::new();
        
        for (i, h) in all_holders.iter().enumerate() {
            let balance = u128::from_str_radix(h.balance.trim_start_matches("0x"), 16).unwrap_or(0);
            let percentage = if total_supply > 0 {
                (balance as f64 / total_supply as f64) * 100.0
            } else {
                0.0
            };
            
            holders.push(TokenHolder {
                address: h.address.clone(),
                balance: h.balance.clone(),
                percentage,
                rank: (i + 1) as u32,
                is_contract: h.is_contract,
                first_seen: h.first_seen,
                last_update: h.last_update,
            });
        }
        
        // Update cache
        {
            let mut cache = self.cache.write().await;
            cache.holders.insert(token.to_string(), holders.clone());
            cache.last_update.insert(token.to_string(), std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs());
        }
        
        Ok(holders)
    }
    
    /// Get holders from transfer events (RPC)
    async fn get_holders_from_events(&self, token: &str) -> Result<Vec<TokenHolder>, HolderError> {
        let client = reqwest::Client::new();
        
        // Get recent Transfer logs
        let topics = vec![
            format!("0xddf252ad98be945b9c2ecde21f12c0e2b2ea667aab2d6555d9f6f4c8e3a9c8d9d"),
        ];
        
        let request = serde_json::json!({
            "jsonrpc": "2.0",
            "method": "eth_getLogs",
            "params": [{
                "address": token,
                "fromBlock": "0x0",
                "toBlock": "latest",
                "topics": [topics[0]],
            }],
            "id": 1
        });
        
        let response = client.post(&self.config.rpc_url)
            .json(&request)
            .send()
            .await
            .map_err(|e| HolderError::RpcError(e.to_string()))?;
        
        let result: serde_json::Value = response.json().await
            .map_err(|e| HolderError::ParseError(e.to_string()))?;
        
        let logs = result["result"].as_array()
            .map_err(|e| HolderError::ParseError(e.to_string()))?;
        
        let mut holders: std::collections::HashMap<String, String> = std::collections::HashMap::new();
        let mut first_seen: std::collections::HashMap<String, u64> = std::collections::HashMap::new();
        
        for log in logs.iter().take(1000) {
            let from = log["topics"].get(1)
                .and_then(|t| t.as_str())
                .map(|s| format!("0x{}", &s[26..64]))
                .unwrap_or_default();
            
            let to = log["topics"].get(2)
                .and_then(|t| t.as_str())
                .map(|s| format!("0x{}", &s[26..64]))
                .unwrap_or_default();
            
            let block = log["blockNumber"]
                .and_then(|b| b.as_str())
                .map(|s| u64::from_str_radix(s.trim_start_matches("0x"), 16).unwrap_or(0))
                .unwrap_or(0);
            
            if !from.is_empty() && from.len() == 42 {
                *holders.entry(from.clone()).or_insert("0x0").clone();
                first_seen.entry(from.clone()).or_insert(block);
            }
            
            if !to.is_empty() && to.len() == 42 {
                *holders.entry(to.clone()).or_insert("0x0").clone();
                first_seen.entry(to.clone()).or_insert(block);
            }
        }
        
        let mut result_holders = Vec::new();
        for (addr, _) in holders {
            if let Ok(balance) = self.get_token_balance(token, &addr).await {
                let is_contract = self.is_contract(&addr).await.unwrap_or(false);
                
                result_holders.push(TokenHolder {
                    address: addr,
                    balance,
                    percentage: 0.0,
                    rank: 0,
                    is_contract,
                    first_seen: 0,
                    last_update: std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .unwrap()
                        .as_secs() as u64,
                });
            }
        }
        
        Ok(result_holders)
    }
    
    /// Get holders from indexer API
    async fn get_holders_from_indexer(&self, token: &str) -> Result<Vec<TokenHolder>, HolderError> {
        let url = format!("https://api.dexscreener.com/addresses/{}", token);
        
        let client = reqwest::Client::new();
        
        let response = client.get(&url).send().await
            .map_err(|e| HolderError::RpcError(e.to_string()))?;
        
        if !response.status().is_success() {
            return Err(HolderError::RpcError("Indexer not available".to_string()));
        }
        
        let data: serde_json::Value = response.json().await
            .map_err(|e| HolderError::ParseError(e.to_string()))?;
        
        let mut holders = Vec::new();
        
        if let Some(pairs) = data["pairs"].as_array() {
            for pair in pairs {
                let lp = pair["lp"].as_str().unwrap_or("");
                let liquidity = pair["liquidity"].as_str().unwrap_or("0");
                
                if !lp.is_empty() {
                    holders.push(TokenHolder {
                        address: lp.to_string(),
                        balance: liquidity.to_string(),
                        percentage: 0.0,
                        rank: 0,
                        is_contract: true,
                        first_seen: 0,
                        last_update: std::time::SystemTime::now()
                            .duration_since(std::time::UNIX_EPOCH)
                            .unwrap()
                            .as_secs() as u64,
                    });
                }
            }
        }
        
        Ok(holders)
    }
    
    /// Get token balance for address
    async fn get_token_balance(&self, token: &str, address: &str) -> Result<String, HolderError> {
        let client = reqwest::Client::new();
        
        let data = format!(
            "0x70a0823100000000000000000000000000000000000000000000000000000000000000000{}",
            &address[2..42]
        );
        
        let request = serde_json::json!({
            "jsonrpc": "2.0",
            "method": "eth_call",
            "params": [{"to": token, "data": data}, "latest"],
            "id": 1
        });
        
        let response = client.post(&self.config.rpc_url)
            .json(&request)
            .send()
            .await
            .map_err(|e| HolderError::RpcError(e.to_string()))?;
        
        let result: serde_json::Value = response.json().await
            .map_err(|e| HolderError::ParseError(e.to_string()))?;
        
        Ok(result["result"].as_str().unwrap_or("0x0").to_string())
    }
    
    /// Check if address is a contract
    async fn is_contract(&self, address: &str) -> Result<bool, HolderError> {
        let client = reqwest::Client::new();
        
        let request = serde_json::json!({
            "jsonrpc": "2.0",
            "method": "eth_getCode",
            "params": [address, "latest"],
            "id": 1
        });
        
        let response = client.post(&self.config.rpc_url)
            .json(&request)
            .send()
            .await
            .map_err(|e| HolderError::RpcError(e.to_string()))?;
        
        let result: serde_json::Value = response.json().await
            .map_err(|e| HolderError::ParseError(e.to_string()))?;
        
        let code = result["result"].as_str().unwrap_or("0x");
        Ok(code != "0x")
    }
    
    /// Get total supply of token
    async fn get_total_supply(&self, token: &str) -> Result<u128, HolderError> {
        let client = reqwest::Client::new();
        
        let data = "0x18160ddd";
        
        let request = serde_json::json!({
            "jsonrpc": "2.0",
            "method": "eth_call",
            "params": [{"to": token, "data": data}, "latest"],
            "id": 1
        });
        
        let response = client.post(&self.config.rpc_url)
            .json(&request)
            .send()
            .await
            .map_err(|e| HolderError::RpcError(e.to_string()))?;
        
        let result: serde_json::Value = response.json().await
            .map_err(|e| HolderError::ParseError(e.to_string()))?;
        
        let supply = result["result"].as_str()
            .map(|s| u128::from_str_radix(s.trim_start_matches("0x"), 16).unwrap_or(0))
            .unwrap_or(0);
        
        Ok(supply)
    }
    
    /// Get holder statistics
    pub async fn get_holder_stats(&self, token: &str) -> Result<TokenHolderStats, HolderError> {
        let holders = self.get_holders(token).await?;
        
        let total_holders = holders.len() as u64;
        
        let top_10 = holders.iter()
            .take((total_holders as f64 * 0.1) as usize)
            .map(|h| u128::from_str_radix(h.balance.trim_start_matches("0x"), 16).unwrap_or(0) as f64)
            .sum::<f64>();
        
        let top_1 = holders.iter()
            .take((total_holders as f64 * 0.01) as usize)
            .map(|h| u128::from_str_radix(h.balance.trim_start_matches("0x"), 16).unwrap_or(0) as f64)
            .sum::<f64>();
        
        let total_balance: f64 = holders.iter()
            .map(|h| u128::from_str_radix(h.balance.trim_start_matches("0x"), 16).unwrap_or(0) as f64)
            .sum();
        
        let zero_balance = holders.iter()
            .filter(|h| h.balance == "0x0" || h.balance == "0")
            .count() as u64;
        
        Ok(TokenHolderStats {
            token: token.to_string(),
            total_holders,
            holder_distribution: vec![],
            top_10_percent: if total_balance > 0.0 { top_10 / total_balance * 100.0 } else { 0.0 },
            top_1_percent: if total_balance > 0.0 { top_1 / total_balance * 100.0 } else { 0.0 },
            zero_balance_count: zero_balance,
        })
    }
}