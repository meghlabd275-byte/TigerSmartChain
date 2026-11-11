//! ENS resolution against the on-chain registry, registry resolver lookup and
//! resolver record calls.

use crate::abi::{self, AbiValue};
use crate::types::*;
use chrono::Utc;
use reqwest::Client;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use thiserror::Error;
use tokio::sync::RwLock;

/// Errors returned by the resolver.
#[derive(Error, Debug)]
pub enum ENSError {
    #[error("rpc error: {0}")]
    Rpc(String),
    #[error("malformed response: {0}")]
    Malformed(String),
    #[error("name is not registered: {0}")]
    NotFound(String),
    #[error("invalid name: {0}")]
    InvalidName(String),
    #[error("configuration error: {0}")]
    Config(String),
}

const SELECTOR_ADDR: [u8; 4] = [0x3b, 0x3b, 0x57, 0xde];
const SELECTOR_ADDR_COIN: [u8; 4] = [0xf1, 0xcb, 0x7e, 0x06];
const SELECTOR_NAME: [u8; 4] = [0x69, 0x1f, 0x34, 0x31];
const SELECTOR_TEXT: [u8; 4] = [0x59, 0xd1, 0xd4, 0x3c];
const SELECTOR_CONTENTHASH: [u8; 4] = [0xbc, 0x1c, 0x58, 0xd1];
const SELECTOR_RESOLVER: [u8; 4] = [0x01, 0x78, 0xb8, 0xbf];
const SELECTOR_OWNER: [u8; 4] = [0x02, 0x57, 0x1b, 0xe3];

/// Text record keys queried by [`ENSService::resolve`].
const TEXT_KEYS: [&str; 7] = [
    "url",
    "avatar",
    "description",
    "com.twitter",
    "com.github",
    "org.telegram",
    "email",
];

/// ENS resolution service.
pub struct ENSService {
    config: ENSConfig,
    client: Client,
    cache: Arc<RwLock<HashMap<String, CachedResolution>>>,
    stats: Arc<RwLock<ENSStats>>,
}

#[derive(Debug, Clone)]
struct CachedResolution {
    resolution: ENSResolution,
    cached_at: i64,
}

impl ENSService {
    /// Create a service that talks to `rpc_url` using the default registry.
    pub fn new(rpc_url: &str) -> Self {
        Self::with_config(ENSConfig {
            rpc_url: rpc_url.to_string(),
            ..Default::default()
        })
    }

    /// Create a service with an explicit configuration.
    pub fn with_config(config: ENSConfig) -> Self {
        let client = Client::builder()
            .timeout(Duration::from_secs(config.timeout_secs.max(1)))
            .build()
            .unwrap_or_else(|_| Client::new());

        Self {
            config,
            client,
            cache: Arc::new(RwLock::new(HashMap::new())),
            stats: Arc::new(RwLock::new(ENSStats::default())),
        }
    }

    /// Resolve a name to its address and records.
    pub async fn resolve(&self, name: &str) -> Result<ENSResolution, ENSError> {
        let normalized = normalize_name(name)?;
        let namehash = abi::namehash(&normalized);

        if let Some(cached) = self.cache_get(&normalized).await {
            return Ok(cached);
        }

        {
            let mut stats = self.stats.write().await;
            stats.lookups_total += 1;
        }

        let resolver = self.resolver_of(&namehash).await?;
        let resolver = match resolver {
            Some(addr) => addr,
            None => {
                self.record_failure().await;
                return Err(ENSError::NotFound(normalized));
            }
        };

        let address = match self.call_resolver(&resolver, &namehash, SELECTOR_ADDR, &[]).await {
            Ok(data) => abi::decode_address(&data),
            Err(_) => None,
        };

        let content_hash = match self
            .call_resolver(&resolver, &namehash, SELECTOR_CONTENTHASH, &[])
            .await
        {
            Ok(data) => abi::decode_bytes32(&data).map(|h| abi::encode_hex(&h)),
            Err(_) => None,
        };

        let mut texts = HashMap::new();
        for key in TEXT_KEYS {
            let args = [AbiValue::Str(key.to_string())];
            if let Ok(data) = self
                .call_resolver(&resolver, &namehash, SELECTOR_TEXT, &args)
                .await
            {
                if let Some(value) = abi::decode_string(&data) {
                    if !value.is_empty() {
                        texts.insert(key.to_string(), value);
                    }
                }
            }
        }

        let coin_type = match self
            .call_resolver(
                &resolver,
                &namehash,
                SELECTOR_ADDR_COIN,
                &[AbiValue::Uint(60)],
            )
            .await
        {
            Ok(data) => abi::decode_bytes(&data).and_then(|bytes| {
                if bytes.len() == 20 {
                    let mut addr = [0u8; 20];
                    addr.copy_from_slice(&bytes);
                    Some(CoinTypeAddress {
                        coin_type: 60,
                        address: abi::checksum_address(&addr),
                    })
                } else {
                    None
                }
            }),
            Err(_) => None,
        };

        let resolution = ENSResolution {
            name: normalized.clone(),
            address: address.map(|a| abi::checksum_address(&a)),
            content_hash,
            texts,
            coin_type,
            expiration_date: None,
            is_wrapped: false,
            parent_owner: None,
        };

        if resolution.address.is_some() {
            let mut stats = self.stats.write().await;
            stats.lookups_success += 1;
            stats.last_update = Utc::now().timestamp();
        } else {
            self.record_failure().await;
        }

        self.cache_put(&normalized, resolution.clone()).await;
        Ok(resolution)
    }

    /// Resolve an address to its primary name using reverse resolution.
    pub async fn reverse_resolve(&self, address: &str) -> Result<ENSReverseResolution, ENSError> {
        let bytes = abi::decode_hex(address)
            .map_err(|e| ENSError::InvalidName(format!("invalid address: {e}")))?;
        if bytes.len() != 20 {
            return Err(ENSError::InvalidName(format!(
                "address must be 20 bytes, got {}",
                bytes.len()
            )));
        }

        let mut address_bytes = [0u8; 20];
        address_bytes.copy_from_slice(&bytes);
        let reverse_name = format!("{}.addr.reverse", hex::encode(address_bytes));
        let namehash = abi::namehash(&reverse_name);

        let resolver = self.resolver_of(&namehash).await?;
        let resolver = match resolver {
            Some(addr) => addr,
            None => {
                return Ok(ENSReverseResolution {
                    address: abi::checksum_address(&address_bytes),
                    name: None,
                    resolver: None,
                })
            }
        };

        let name = match self
            .call_resolver(&resolver, &namehash, SELECTOR_NAME, &[])
            .await
        {
            Ok(data) => abi::decode_string(&data).filter(|s| !s.is_empty()),
            Err(_) => None,
        };

        Ok(ENSReverseResolution {
            address: abi::checksum_address(&address_bytes),
            name,
            resolver: Some(abi::checksum_address(&resolver)),
        })
    }

    /// Read the owner of a name from the registry.
    pub async fn owner(&self, name: &str) -> Result<Option<String>, ENSError> {
        let normalized = normalize_name(name)?;
        let namehash = abi::namehash(&normalized);
        let data = self
            .eth_call(
                &self.config.registry_address,
                abi::encode_call(SELECTOR_OWNER, &[AbiValue::Bytes32(namehash)]),
            )
            .await?;

        Ok(abi::decode_address(&data).map(|a| abi::checksum_address(&a)))
    }

    /// Read the resolver of a name from the registry, if one is set.
    async fn resolver_of(&self, namehash: &[u8; 32]) -> Result<Option<[u8; 20]>, ENSError> {
        let data = self
            .eth_call(
                &self.config.registry_address,
                abi::encode_call(SELECTOR_RESOLVER, &[AbiValue::Bytes32(*namehash)]),
            )
            .await?;

        Ok(abi::decode_address(&data))
    }

    /// Call a record getter on a resolver, appending `namehash` then `args`.
    async fn call_resolver(
        &self,
        resolver: &[u8; 20],
        namehash: &[u8; 32],
        selector: [u8; 4],
        args: &[AbiValue],
    ) -> Result<Vec<u8>, ENSError> {
        let mut call_args = Vec::with_capacity(args.len() + 1);
        call_args.push(AbiValue::Bytes32(*namehash));
        call_args.extend_from_slice(args);

        self.eth_call(&abi::checksum_address(resolver), abi::encode_call(selector, &call_args))
            .await
    }

    /// Perform `eth_call` and return the raw return data.
    async fn eth_call(&self, to: &str, data: Vec<u8>) -> Result<Vec<u8>, ENSError> {
        if self.config.rpc_url.is_empty() {
            return Err(ENSError::Config("rpc_url is not set".to_string()));
        }

        let request_body = json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "eth_call",
            "params": [{ "to": to, "data": abi::encode_hex(&data) }, "latest"],
        });

        let response = self
            .client
            .post(&self.config.rpc_url)
            .json(&request_body)
            .send()
            .await
            .map_err(|e| ENSError::Rpc(e.to_string()))?
            .json::<Value>()
            .await
            .map_err(|e| ENSError::Malformed(e.to_string()))?;

        if let Some(error) = response.get("error") {
            let message = error
                .get("message")
                .and_then(|m| m.as_str())
                .unwrap_or("unknown rpc error");
            return Err(ENSError::Rpc(message.to_string()));
        }

        let result = response
            .get("result")
            .and_then(|v| v.as_str())
            .ok_or_else(|| ENSError::Malformed("response has no result".to_string()))?;

        abi::decode_hex(result).map_err(|e| ENSError::Malformed(e.to_string()))
    }

    async fn cache_get(&self, name: &str) -> Option<ENSResolution> {
        if !self.config.enable_cache {
            return None;
        }

        let now = Utc::now().timestamp();
        let cache = self.cache.read().await;
        let entry = cache.get(name)?;

        if now - entry.cached_at > self.config.cache_ttl_secs as i64 {
            return None;
        }

        let resolution = entry.resolution.clone();
        drop(cache);

        let mut stats = self.stats.write().await;
        stats.cache_hits += 1;
        Some(resolution)
    }

    async fn cache_put(&self, name: &str, resolution: ENSResolution) {
        if !self.config.enable_cache {
            return;
        }

        let mut cache = self.cache.write().await;
        cache.insert(
            name.to_string(),
            CachedResolution {
                resolution,
                cached_at: Utc::now().timestamp(),
            },
        );
    }

    async fn record_failure(&self) {
        let mut stats = self.stats.write().await;
        stats.lookups_failed += 1;
        stats.last_update = Utc::now().timestamp();
    }

    /// Snapshot of the service counters.
    pub async fn get_stats(&self) -> ENSStats {
        self.stats.read().await.clone()
    }

    /// Drop every cached resolution.
    pub async fn clear_cache(&self) {
        self.cache.write().await.clear();
    }
}

/// Normalize a name: lowercase, trim, and reject labels that are not valid
/// DNS labels. ENS names are case insensitive because namehash operates on the
/// lowercased labels.
pub fn normalize_name(name: &str) -> Result<String, ENSError> {
    let trimmed = name.trim().trim_end_matches('.').to_ascii_lowercase();
    if trimmed.is_empty() {
        return Err(ENSError::InvalidName("name is empty".to_string()));
    }

    for label in trimmed.split('.') {
        if label.is_empty() {
            return Err(ENSError::InvalidName(format!(
                "name {name} contains an empty label"
            )));
        }
        if label.len() > 63 {
            return Err(ENSError::InvalidName(format!(
                "label {label} exceeds 63 bytes"
            )));
        }
        for byte in label.bytes() {
            let allowed = byte.is_ascii_alphanumeric() || byte == b'-' || byte == b'_';
            if !allowed {
                return Err(ENSError::InvalidName(format!(
                    "label {label} contains an unsupported character"
                )));
            }
        }
    }

    Ok(trimmed)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_lowercases_and_strips_the_root_dot() {
        assert_eq!(normalize_name("Foo.ETH.").unwrap(), "foo.eth");
        assert_eq!(normalize_name("  tigerscan.eth ").unwrap(), "tigerscan.eth");
    }

    #[test]
    fn normalize_rejects_empty_and_malformed_names() {
        assert!(normalize_name("").is_err());
        assert!(normalize_name(".").is_err());
        assert!(normalize_name("foo..eth").is_err());
        assert!(normalize_name("foo bar.eth").is_err());
        assert!(normalize_name(&format!("{}.eth", "a".repeat(64))).is_err());
    }

    #[test]
    fn address_reverse_name_is_lowercase_hex() {
        let name = format!("{}.addr.reverse", hex::encode([0xABu8; 20]));
        assert_eq!(name, format!("{}.addr.reverse", "ab".repeat(20)));
    }
}
