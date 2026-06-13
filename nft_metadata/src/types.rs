//! NFT Metadata Types for TigerScan

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

// =============================================================================
// NFT METADATA
// =============================================================================

/// NFT Metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NFTMetadata {
    /// Token ID
    pub token_id: String,
    /// Collection address
    pub collection: String,
    /// Name
    pub name: Option<String>,
    /// Description
    pub description: Option<String>,
    /// Image URL
    pub image: Option<String>,
    /// Animation URL
    pub animation_url: Option<String>,
    /// External URL
    pub external_url: Option<String>,
    /// Attributes
    pub attributes: Vec<NFTAttribute>,
    /// Background color
    pub background_color: Option<String>,
    /// Media properties
    pub properties: HashMap<String, String>,
}

/// NFT Attribute
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NFTAttribute {
    /// Trait type
    pub trait_type: String,
    /// Value
    pub value: String,
    /// Display type (optional)
    pub display_type: Option<String>,
    /// Max value (for number types)
    pub max_value: Option<f64>,
}

impl NFTMetadata {
    pub fn new(token_id: String, collection: String) -> Self {
        Self {
            token_id,
            collection,
            name: None,
            description: None,
            image: None,
            animation_url: None,
            external_url: None,
            attributes: Vec::new(),
            background_color: None,
            properties: HashMap::new(),
        }
    }
}

// =============================================================================
// COLLECTION
// =============================================================================

/// NFT Collection
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NFTCollection {
    /// Collection address
    pub address: String,
    /// Name
    pub name: String,
    /// Symbol
    pub symbol: Option<String>,
    /// Contract type (ERC721, ERC1155)
    pub contract_type: String,
    /// Total supply
    pub total_supply: u64,
    /// Minted count
    pub minted_count: u64,
    /// Owner count
    pub owner_count: u64,
    /// Floor price
    pub floor_price: f64,
    /// Average price
    pub average_price: f64,
    /// Volume (24h)
    pub volume_24h: f64,
    /// Volume (7d)
    pub volume_7d: f64,
    /// Volume (30d)
    pub volume_30d: f64,
    /// Image URL
    pub image_url: Option<String>,
    /// Banner URL
    pub banner_url: Option<String>,
    /// Description
    pub description: Option<String>,
    /// External URL
    pub external_url: Option<String>,
    /// Twitter
    pub twitter: Option<String>,
    /// Discord
    pub discord: Option<String>,
    /// Wiki
    pub wiki: Option<String>,
    /// Is verified
    pub is_verified: bool,
    /// Is spam
    pub is_spam: bool,
    /// Last updated
    pub last_updated: i64,
}

// =============================================================================
// RARITY
// =============================================================================

/// NFT Rarity
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NFTRarity {
    pub token_id: String,
    pub collection: String,
    pub rarity_score: f64,
    pub rank: u32,
    pub trait_rarities: Vec<TraitRarity>,
}

/// Trait Rarity
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TraitRarity {
    pub trait_type: String,
    pub value: String,
    pub rarity: f64,
}

// =============================================================================
// STATS
// =============================================================================

/// Collection Statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CollectionStats {
    pub address: String,
    pub total_supply: u64,
    pub unique_owners: u64,
    pub listed_count: u64,
    pub floor_price: f64,
    pub average_price: f64,
    pub volume_24h: f64,
    pub volume_7d: f64,
    pub volume_30d: f64,
    pub sales_24h: u64,
    pub sales_7d: u64,
    pub sales_30d: u64,
    pub holder_distribution: Vec<HolderTier>,
}

/// Holder Tier
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HolderTier {
    pub range: String,
    pub count: u64,
    pub percentage: f64,
}

// =============================================================================
// CONFIG
// =============================================================================

/// NFT Metadata Indexer Configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NFTMetadataConfig {
    /// IPFS gateway URLs
    pub ipfs_gateways: Vec<String>,
    /// Arweave gateway URL
    pub arweave_gateway: String,
    /// HTTP timeout
    pub timeout_secs: u64,
    /// Enable metadata caching
    pub enable_cache: bool,
    /// Cache duration
    pub cache_duration_secs: u64,
    /// Max concurrent requests
    pub max_concurrent: u32,
    /// Enable rarity calculation
    pub enable_rarity: bool,
    /// Enable floor price tracking
    pub enable_floor_price: bool,
}

impl Default for NFTMetadataConfig {
    fn default() -> Self {
        Self {
            ipfs_gateways: vec![
                "https://ipfs.io/ipfs/".to_string(),
                "https://cloudflare-ipfs.com/ipfs/".to_string(),
                "https://gateway.pinata.cloud/ipfs/".to_string(),
            ],
            arweave_gateway: "https://arweave.net".to_string(),
            timeout_secs: 30,
            enable_cache: true,
            cache_duration_secs: 3600,
            max_concurrent: 10,
            enable_rarity: true,
            enable_floor_price: true,
        }
    }
}