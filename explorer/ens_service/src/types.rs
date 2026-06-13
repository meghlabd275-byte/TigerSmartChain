//! ENS Service Types
//! Core data structures for ENS resolution

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// ENS Record
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ENSRecord {
    /// ENS name
    pub name: String,
    /// Resolved Ethereum address
    pub address: Option<String>,
    /// Resolver contract address
    pub resolver: Option<String>,
    /// Record owner
    pub owner: Option<String>,
    /// Time to live
    pub ttl: Option<u64>,
    /// Content hash (for IPFS, etc.)
    pub content_hash: Option<String>,
    /// Text records
    pub text_records: Option<Vec<TextRecord>>,
    /// Coin addresses
    pub coin_addresses: Option<Vec<CoinAddress>>,
    /// Interface (for ENSIPFS)
    pub interface: Option<String>,
    /// ABI (for contract resolution)
    pub abi: Option<String>,
    /// Avatar
    pub avatar: Option<String>,
    /// Email
    pub email: Option<String>,
    /// Description
    pub description: Option<String>,
    /// Notice
    pub notice: Option<String>,
    /// Keywords
    pub keywords: Option<String>,
    /// URL
    pub url: Option<String>,
    /// Version
    pub version: Option<u32>,
    /// Created at
    pub created_at: Option<DateTime<Utc>>,
    /// Updated at
    pub updated_at: Option<DateTime<Utc>>,
}

impl ENSRecord {
    /// Create new ENS record
    pub fn new(name: String) -> Self {
        Self {
            name,
            address: None,
            resolver: None,
            owner: None,
            ttl: None,
            content_hash: None,
            text_records: None,
            coin_addresses: None,
            interface: None,
            abi: None,
            avatar: None,
            email: None,
            description: None,
            notice: None,
            keywords: None,
            url: None,
            version: None,
            created_at: None,
            updated_at: None,
        }
    }
    
    /// Check if resolved
    pub fn is_resolved(&self) -> bool {
        self.address.is_some()
    }
}

/// Text record for ENS
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TextRecord {
    /// Key
    pub key: String,
    /// Value
    pub value: String,
}

/// Coin address for multi-chain resolution
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CoinAddress {
    /// Coin type (e.g., 60 for ETH, 0 for BTC)
    pub coin_type: u32,
    /// Address
    pub address: String,
}

/// ENS lookup query
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ENSQuery {
    /// Name to resolve
    pub name: String,
    /// Resolver type
    pub resolver_type: Option<ResolverType>,
    /// Include text records
    pub include_text: Option<bool>,
    /// Include coin addresses
    pub include_coins: Option<bool>,
    /// Cache only
    pub cache_only: Option<bool>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ResolverType {
    /// Standard resolver
    Standard,
    /// CCIP resolver
    CCIP,
    /// Custom resolver
    Custom,
}

/// Reverse lookup query
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReverseQuery {
    /// Ethereum address
    pub address: String,
    /// Chain ID
    pub chain_id: Option<u64>,
}

/// ENS domain info
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ENSDomain {
    /// Domain name
    pub name: String,
    /// Label hash
    pub label_hash: Option<String>,
    /// Name hash
    pub name_hash: Option<String>,
    /// Owner
    pub owner: Option<String>,
    /// Resolver
    pub resolver: Option<String>,
    /// TTL
    pub ttl: Option<u64>,
    /// Is  .eth 2LD
    pub is_eth_2ld: bool,
    /// Registration date
    pub registration_date: Option<DateTime<Utc>>,
    /// Expiry date
    pub expiry_date: Option<DateTime<Utc>>,
    /// Is available
    pub is_available: bool,
}