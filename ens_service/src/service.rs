//! ENS Resolution Service Implementation

use crate::types::*;
use chrono::Utc;
use std::sync::Arc;
use std::collections::HashMap;
use tokio::sync::RwLock;
use thiserror::Error;
use reqwest::Client;
use serde_json::{json, Value};
use std::time::Duration;

// =============================================================================
// ERRORS
// =============================================================================

#[derive(Error, Debug)]
pub enum ENSError {
    #[error("RPC error: {0}")]
    RPCError(String),
    #[error("Parse error: {0}")]
    ParseError(String),
    #[error("Not found: {0}")]
    NotFound(String),
    #[error("Resolution failed: {0}")]
    ResolutionFailed(String),
}

// =============================================================================
// SERVICE
// =============================================================================

/// ENS Resolution Service
pub struct ENSService {
    config: ENSConfig,
    client: Client,
    cache: Arc<RwLock<HashMap<String, ENSRecord>>,
    stats: Arc<RwLock<ENSStats>>,
}

impl ENSService {
    /// Create new ENS service
    pub fn new(rpc_url: &str) -> Self {
        let config = ENSConfig {
            rpc_url: rpc_url.to_string(),
            ..Default::default()
        };
        
        let client = Client::builder()
            .timeout(Duration::from_secs(config.timeout_secs))
            .build()
            .unwrap_or_else(|_| Client::new());
        
        Self {
            config: config.clone(),
            client,
            cache: Arc::new(RwLock::new(HashMap::new())),
            stats: Arc::new(RwLock::new(ENSStats::default())),
        }
    }

    /// Create with custom config
    pub fn with_config(config: ENSConfig) -> Self {
        let client = Client::builder()
            .timeout(Duration::from_secs(config.timeout_secs))
            .build()
            .unwrap_or_else(|_| Client::new());
        
        Self {
            config: config.clone(),
            client,
            cache: Arc::new(RwLock::new(HashMap::new())),
            stats: Arc::new(RwLock::new(ENSStats::default())),
        }
    }

    /// Resolve a .eth domain to an address
    pub async fn resolve(&self, name: &str) -> Result<ENSResolution, ENSError> {
        let name = name.trim_end_matches('.');
        
        // Check cache
        if self.config.enable_cache {
            let cache = self.cache.read().await;
            if let Some(cached) = cache.get(name) {
                let mut stats = self.stats.write().await;
                stats.cache_hits += 1;
                return Ok(ENSResolution {
                    name: name.to_string(),
                    address: cached.address.clone(),
                    content_hash: cached.content_hash.clone(),
                    texts: cached.text_records.clone(),
                    coin_type: cached.coin_type.as_ref().map(|c| CoinTypeAddress {
                        coin_type: c.coin_type.unwrap_or(60),
                        address: c.address.clone(),
                    }),
                    expiration_date: None,
                    is_wrapped: false,
                    parent_owner: None,
                });
            }
        }

        // Update stats
        {
            let mut stats = self.stats.write().await;
            stats.lookups_total += 1;
        }

        // Compute namehash
        let namehash = self.compute_namehash(name);

        // Resolve address (ETH address)
        let address = self.resolve_record(&namehash, "0x5b60f5").await?;

        // Resolve content hash
        let content_hash = self.resolve_record(&namehash, "0xbc1c58d1").await.ok();

        // Resolve text records
        let mut texts = HashMap::new();
        for key in &["url", "avatar", "description", "com.twitter", "com.github", "org.telegram", "email"] {
            if let Ok(text) = self.resolve_text_record(&namehash, key).await {
                if !text.is_empty() {
                    texts.insert(key.to_string(), text);
                }
            }
        }

        // Resolve coin type (for multi-chain)
        let coin_type = self.resolve_coin_type(&namehash).await.ok();

        let result = ENSResolution {
            name: name.to_string(),
            address,
            content_hash,
            texts,
            coin_type,
            expiration_date: None,
            is_wrapped: false,
            parent_owner: None,
        };

        // Cache it
        if self.config.enable_cache {
            let mut cache = self.cache.write().await;
            cache.insert(name.to_string(), ENSRecord {
                name: name.to_string(),
                address: result.address.clone().unwrap_or_default(),
                resolver: String::new(),
                ttl: 0,
                address: result.address.clone(),
                content_hash: result.content_hash.clone(),
                text_records: result.texts.clone(),
                abi: None,
                coin_type: result.coin_type.as_ref().map(|c| c.address.clone()).map(|a| ()),
                chain_id: result.coin_type.as_ref().map(|c| c.coin_type),
                last_updated: Utc::now().timestamp(),
            });
        }

        // Update stats
        {
            let mut stats = self.stats.write().await;
            if result.address.is_some() {
                stats.lookups_success += 1;
            } else {
                stats.lookups_failed += 1;
            }
            stats.last_update = Utc::now().timestamp();
        }

        Ok(result)
    }

    /// Resolve an address to a .eth domain (reverse resolution)
    pub async fn reverse_resolve(&self, address: &str) -> Result<ENSReverseResolution, ENSError> {
        // Compute reverse namehash
        let reverse_name = format!("{:}.addr.reverse", address.trim_start_matches("0x").to_lowercase());
        let namehash = self.compute_namehash(&reverse_name);

        // Resolve name
        let name = self.resolve_record(&namehash, "0x691f3431").await.ok();

        Ok(ENSReverseResolution {
            address: address.to_string(),
            name,
            resolver: None,
        })
    }

    /// Resolve a record from the ENS resolver
    async fn resolve_record(&self, namehash: &str, method: &str) -> Result<String, ENSError> {
        let request_body = json!({
            "jsonrpc": "2.0",
            "method": "eth_call",
            "params": [{
                "to": self.config.resolver_address,
                "data": format!("{}{}", method, namehash)
            }, "latest"],
            "id": 1
        });

        let response = self.client
            .post(&self.config.rpc_url)
            .json(&request_body)
            .send()
            .await
            .map_err(|e| ENSError::RPCError(e.to_string()))?
            .json::<Value>()
            .await
            .map_err(|e| ENSError::ParseError(e.to_string()))?;

        if let Some(error) = response.get("error") {
            return Err(ENSError::ResolutionFailed(
                error.get("message")
                    .and_then(|m| m.as_str())
                    .unwrap_or("Unknown error")
                    .to_string()
            ));
        }

        let result = response.get("result")
            .and_then(|v| v.as_str())
            .map(|s| s.trim_start_matches("0x").to_string())
            .unwrap_or_default();

        // Parse the return value (skip first 32 bytes for offset, next 32 for length)
        if result.len() > 64 {
            let offset = usize::from_str_radix(&result[0..64], 16).unwrap_or(0);
            let length = usize::from_str_radix(&result[64..96], 16).unwrap_or(0);
            
            if length > 0 && offset + 96 <= result.len() {
                let data_start = offset * 2 + 64;
                let data_end = data_start + length * 2;
                if data_end <= result.len() {
                    let data = &result[data_start..data_end];
                    // Convert hex to ASCII
                    let bytes: Vec<u8> = (0..data.len())
                        .step_by(2)
                        .filter_map(|i| u8::from_str_radix(&data[i..i+2], 16).ok())
                        .collect();
                    return Ok(String::from_utf8_lossy(&bytes).to_string());
                }
            }
        }

        // For addresses, the return value is the address directly
        if result.len() >= 40 {
            return Ok(format!("0x{}", &result[result.len()-40..]));
        }

        Err(ENSError::NotFound("Record not found".to_string()))
    }

    /// Resolve a text record
    async fn resolve_text_record(&self, namehash: &str, key: &str) -> Result<String, ENSError> {
        // Compute key hash
        let key_hash = self.keccak256(key);
        
        let request_body = json!({
            "jsonrpc": "2.0",
            "method": "eth_call",
            "params": [{
                "to": self.config.resolver_address,
                "data": format!("0x5f60f5{}00000000000000000000000000000000000000000000000000000000000000200000000000000000000000000000000000000000000000000000000000000000000{}", key_hash)
            }, "latest"],
            "id": 1
        });

        let response = self.client
            .post(&self.config.rpc_url)
            .json(&request_body)
            .send()
            .await
            .map_err(|e| ENSError::RPCError(e.to_string()))?
            .json::<Value>()
            .await
            .map_err(|e| ENSError::ParseError(e.to_string()))?;

        let result = response.get("result")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string())
            .unwrap_or_default();

        // Parse result similarly
        if result.len() > 64 {
            let offset = usize::from_str_radix(&result[0..64], 16).unwrap_or(0);
            let length = usize::from_str_radix(&result[64..96], 16).unwrap_or(0);
            
            if length > 0 && offset + 96 <= result.len() {
                let data_start = offset * 2 + 64;
                let data_end = data_start + length * 2;
                if data_end <= result.len() {
                    let data = &result[data_start..data_end];
                    let bytes: Vec<u8> = (0..data.len())
                        .step_by(2)
                        .filter_map(|i| u8::from_str_radix(&data[i..i+2], 16).ok())
                        .collect();
                    return Ok(String::from_utf8_lossy(&bytes).to_string());
                }
            }
        }

        Ok(String::new())
    }

    /// Resolve coin type address
    async fn resolve_coin_type(&self, namehash: &str) -> Result<CoinTypeAddress, ENSError> {
        // Use addr method with coin type 60 (ETH)
        let address = self.resolve_record(namehash, "0xf1cb7e06").await?;
        
        Ok(CoinTypeAddress {
            coin_type: 60,
            address,
        })
    }

    /// Compute namehash for ENS name
    fn compute_namehash(&self, name: &str) -> String {
        // This is a simplified namehash - in production, use proper DNS packet implementation
        let labels: Vec<&str> = name.split('.').collect();
        
        let mut result = "0x0000000000000000000000000000000000000000000000000000000000000000".to_string();
        
        for label in labels.iter().rev() {
            let label_hash = self.keccak256(label);
            result = self.keccak256(&format!("{}{}", result.trim_start_matches("0x"), label_hash));
        }
        
        result
    }

    /// Simple Keccak256 (placeholder - in production use proper library)
    fn keccak256(&self, input: &str) -> String {
        // This is a placeholder - in production, use a proper Keccak library
        format!("{:0>64}", input.len())
    }

    /// Get service statistics
    pub async fn get_stats(&self) -> ENSStats {
        self.stats.read().await.clone()
    }

    /// Clear cache
    pub async fn clear_cache(&self) {
        self.cache.write().await.clear();
    }
}