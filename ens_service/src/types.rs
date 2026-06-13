//! ENS Types for TigerScan

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

// =============================================================================
// ENS RECORD
// =============================================================================

/// ENS Record
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ENSRecord {
    /// Domain name
    pub name: String,
    /// Owner address
    pub owner: String,
    /// Resolver address
    pub resolver: String,
    /// TTL (time to live)
    pub ttl: u64,
    /// Resolution record
    pub address: Option<String>,
    /// Content hash
    pub content_hash: Option<String>,
    /// Text records
    pub text_records: HashMap<String, String>,
    /// ABI record
    pub abi: Option<ABIRecord>,
    /// Cointo address
    pub coin_type: Option<String>,
    /// Chain ID
    pub chain_id: Option<u64>,
    /// Last updated
    pub last_updated: i64,
}

/// ABI Record
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ABIRecord {
    /// Format (JSON, binary)
    pub format: String,
    /// ABI JSON
    pub json: Option<String>,
}

// =============================================================================
// ENS RESOLUTION
// =============================================================================

/// ENS Resolution Result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ENSResolution {
    /// Domain name
    pub name: String,
    /// Resolved address
    pub address: Option<String>,
    /// Content hash
    pub content_hash: Option<String>,
    /// TXT records
    pub texts: HashMap<String, String>,
    /// Coin type address
    pub coin_type: Option<CoinTypeAddress>,
    /// Expiration date
    pub expiration_date: Option<i64>,
    /// Is name wrapped
    pub is_wrapped: bool,
    /// Parent owner
    pub parent_owner: Option<String>,
}

/// Coin Type Address
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CoinTypeAddress {
    /// Coin type (e.g., 60 for ETH)
    pub coin_type: u64,
    /// Address
    pub address: String,
}

// =============================================================================
// REVERSE RESOLUTION
// =============================================================================

/// Reverse Resolution Result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ENSReverseResolution {
    /// Address
    pub address: String,
    /// Domain name
    pub name: Option<String>,
    /// Resolver address
    pub resolver: Option<String>,
}

// =============================================================================
// CONFIG
// =============================================================================

/// ENS Service Configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ENSConfig {
    /// RPC URL
    pub rpc_url: String,
    /// ENS Registry address
    pub registry_address: String,
    /// Resolver address
    pub resolver_address: String,
    /// Reverse Resolver address
    pub reverse_resolver_address: String,
    /// Request timeout
    pub timeout_secs: u64,
    /// Enable caching
    pub enable_cache: bool,
    /// Cache TTL
    pub cache_ttl_secs: u64,
}

impl Default for ENSConfig {
    fn default() -> Self {
        Self {
            rpc_url: "http://localhost:8545".to_string(),
            registry_address: "0x00000000000C2Ee743fDa4B8b2c6eA6F87C72b8C7D".to_string(),
            resolver_address: "0x00000000000C2Ee743fDa4B8b2c6eA6F87C72b8C7D".to_string(),
            reverse_resolver_address: "0x00000000000C2Ee743fDa4B8b2c6eA6F87C72b8C7D".to_string(),
            timeout_secs: 30,
            enable_cache: true,
            cache_ttl_secs: 3600,
        }
    }
}

// =============================================================================
// STATS
// =============================================================================

/// ENS Service Statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ENSStats {
    pub lookups_total: u64,
    pub lookups_success: u64,
    pub lookups_failed: u64,
    pub cache_hits: u64,
    pub last_update: i64,
}

impl Default for ENSStats {
    fn default() -> Self {
        Self {
            lookups_total: 0,
            lookups_success: 0,
            lookups_failed: 0,
            cache_hits: 0,
            last_update: chrono::Utc::now().timestamp(),
        }
    }
}