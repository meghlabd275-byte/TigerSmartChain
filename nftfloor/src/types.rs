//! NFT Floor Types

use serde::{Deserialize, Serialize};

// =============================================================================
// NFT FLOOR
// =============================================================================

/// NFT Collection
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NftCollection {
    pub address: String,
    pub name: String,
    pub symbol: String,
    pub total_supply: u64,
    pub floor_price: u64,
    pub volume_24h: u64,
}

/// Sale
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Sale {
    pub token_id: u64,
    pub price: u64,
    pub seller: String,
    pub buyer: String,
    pub timestamp: u64,
}

/// Floor Price
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FloorPrice {
    pub collection: String,
    pub price: u64,
    pub updated_at: u64,
}

/// NFT Floor Service
pub struct Service {
    collections: std::collections::HashMap<String, NftCollection>,
}

impl Service {
    pub fn new() -> Self {
        Self {
            collections: std::collections::HashMap::new(),
        }
    }

    /// Add collection
    pub fn add_collection(&mut self, collection: NftCollection) {
        self.collections.insert(collection.address.clone(), collection);
    }

    /// Get collection
    pub fn get_collection(&self, address: &str) -> Option<&NftCollection> {
        self.collections.get(address)
    }
}

impl Default for Service {
    fn default() -> Self {
        Self::new()
    }
}