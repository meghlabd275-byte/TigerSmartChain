//! NFT Marketplace Integration - Real Floor Prices
//! 
//! Fetches real floor prices from major NFT marketplaces:
//! - OpenSea API
//! - LooksRare API  
//! - BlueMove API (BNB Chain)
//! - NFTGo API

use std::sync::Arc;
use tokio::sync::RwLock;
use serde::{Deserialize, Serialize};
use thiserror::Error;

// =============================================================================
// ERRORS
// =============================================================================

#[derive(Error, Debug)]
pub enum NFTMarketplaceError {
    #[error("API error: {0}")]
    ApiError(String),
    
    #[error("Parse error: {0}")]
    ParseError(String),
    
    #[error("Rate limit: {0}")]
    RateLimit(String),
    
    #[error("Not found: {0}")]
    NotFound(String),
}

// =============================================================================
// MARKETPLACE TYPES
// =============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MarketplaceConfig {
    pub opensea_api_key: Option<String>,
    pub looksrare_api_key: Option<String>,
    pub bluemove_api_key: Option<String>,
    pub nftgo_api_key: Option<String>,
    pub cache_ttl_secs: u64,
}

impl Default for MarketplaceConfig {
    fn default() -> Self {
        Self {
            opensea_api_key: std::env::var("OPENSEA_API_KEY").ok(),
            looksrare_api_key: std::env::var("LOOKSRARE_API_KEY").ok(),
            bluemove_api_key: std::env::var("BLUEMOVE_API_KEY").ok(),
            nftgo_api_key: std::env::var("NFTGO_API_KEY").ok(),
            cache_ttl_secs: 300, // 5 minutes
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CollectionFloorPrice {
    pub collection: String,
    pub floor_price: f64,
    pub floor_price_native: String,
    pub currency: String,
    pub source: String,
    pub timestamp: i64,
    pub volume_24h: f64,
    pub sales_24h: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NFTSale {
    pub token_id: String,
    pub seller: String,
    pub buyer: String,
    pub price: f64,
    pub currency: String,
    pub timestamp: i64,
    pub tx_hash: String,
    pub marketplace: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CollectionStats {
    pub address: String,
    pub name: String,
    pub floor_price: f64,
    pub volume_24h: f64,
    pub volume_7d: f64,
    pub volume_total: f64,
    pub sales_24h: u32,
    pub sales_7d: u32,
    pub avg_price_24h: f64,
    pub median_price_24h: f64,
    pub total_supply: u64,
    pub num_owners: u64,
    pub owner_distribution: Vec<OwnerStats>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OwnerStats {
    pub address: String,
    pub count: u64,
    pub percentage: f64,
}

// =============================================================================
// MARKETPLACE CLIENT
// =============================================================================

pub struct MarketplaceClient {
    config: MarketplaceConfig,
    cache: Arc<RwLock<PriceCache>>,
}

#[derive(Debug, Default)]
pub struct PriceCache {
    pub prices: std::collections::HashMap<String, CollectionFloorPrice>,
    pub last_update: std::collections::HashMap<String, i64>,
}

impl MarketplaceClient {
    /// Create new marketplace client
    pub fn new(config: MarketplaceConfig) -> Self {
        Self {
            config,
            cache: Arc::new(RwLock::new(PriceCache::default())),
        }
    }
    
    /// Get floor price from all marketplaces
    pub async fn get_floor_price(&self, collection: &str) -> Result<CollectionFloorPrice, NFTMarketplaceError> {
        // Check cache first
        {
            let cache = self.cache.read().await;
            if let Some(cached) = cache.prices.get(collection) {
                let age = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_secs() as i64
                    .saturating_sub(cached.timestamp);
                
                if age < self.config.cache_ttl_secs as i64 {
                    return Ok(cached.clone());
                }
            }
        }
        
        // Try each marketplace in order
        let mut errors = Vec::new();
        
        // Try BlueMove (BNB Chain native)
        if let Ok(price) = self.get_bluemove_floor_price(collection).await {
            self.update_cache(collection, &price).await;
            return Ok(price);
        }
        
        // Try OpenSea
        if let Ok(price) = self.get_opensea_floor_price(collection).await {
            self.update_cache(collection, &price).await;
            return Ok(price);
        }
        
        // Try LooksRare
        if let Ok(price) = self.get_looksrare_floor_price(collection).await {
            self.update_cache(collection, &price).await;
            return Ok(price);
        }
        
        // Try NFTGo
        if let Ok(price) = self.get_nftgo_floor_price(collection).await {
            self.update_cache(collection, &price).await;
            return Ok(price);
        }
        
        Err(NFTMarketplaceError::NotFound(format!(
            "No floor price found for {} from any marketplace", collection
        )))
    }
    
    /// Get floor price from BlueMove (BNB Chain)
    async fn get_bluemove_floor_price(&self, collection: &str) -> Result<CollectionFloorPrice, NFTMarketplaceError> {
        let url = format!(
            "https://apis.bluemove.net/v1/collection/{}/stats",
            collection
        );
        
        let client = reqwest::Client::new();
        
        let mut request = client.get(&url);
        
        if let Some(ref key) = self.config.bluemove_api_key {
            request = request.header("x-api-key", key);
        }
        
        let response = request.send().await
            .map_err(|e| NFTMarketplaceError::ApiError(e.to_string()))?;
        
        if response.status() == 404 {
            return Err(NFTMarketplaceError::NotFound(collection.to_string()));
        }
        
        if response.status() == 429 {
            return Err(NFTMarketplaceError::RateLimit("BlueMove rate limited".to_string()));
        }
        
        if !response.status().is_success() {
            return Err(NFTMarketplaceError::ApiError(format!(
                "BlueMove API returned {}", response.status()
            )));
        }
        
        let stats: BlueMoveStats = response.json().await
            .map_err(|e| NFTMarketplaceError::ParseError(e.to_string()))?;
        
        Ok(CollectionFloorPrice {
            collection: collection.to_string(),
            floor_price: stats.floor_price,
            floor_price_native: format!("{} BNB", stats.floor_price),
            currency: "BNB".to_string(),
            source: "BlueMove".to_string(),
            timestamp: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs() as i64,
            volume_24h: stats.volume_24h,
            sales_24h: stats.sales_24h,
        })
    }
    
    /// Get floor price from OpenSea
    async fn get_opensea_floor_price(&self, collection: &str) -> Result<CollectionFloorPrice, NFTMarketplaceError> {
        let url = format!(
            "https://api.opensea.io/api/v2/collection/{}/stats",
            collection
        );
        
        let client = reqwest::Client::new();
        
        let mut request = client.get(&url);
        
        if let Some(ref key) = self.config.opensea_api_key {
            request = request.header("x-api-key", key);
        }
        
        let response = request.send().await
            .map_err(|e| NFTMarketplaceError::ApiError(e.to_string()))?;
        
        if response.status() == 404 {
            return Err(NFTMarketplaceError::NotFound(collection.to_string()));
        }
        
        if response.status() == 429 {
            return Err(NFTMarketplaceError::RateLimit("OpenSea rate limited".to_string()));
        }
        
        if !response.status().is_success() {
            return Err(NFTMarketplaceError::ApiError(format!(
                "OpenSea API returned {}", response.status()
            )));
        }
        
        let stats: OpenSeaStats = response.json().await
            .map_err(|e| NFTMarketplaceError::ParseError(e.to_string()))?;
        
        let floor_price = stats.stats.floor_price.unwrap_or(0.0);
        
        Ok(CollectionFloorPrice {
            collection: collection.to_string(),
            floor_price,
            floor_price_native: format!("{} ETH", floor_price),
            currency: "ETH".to_string(),
            source: "OpenSea".to_string(),
            timestamp: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs() as i64,
            volume_24h: stats.stats.one_day_volume,
            sales_24h: stats.stats.one_day_sales.unwrap_or(0) as u32,
        })
    }
    
    /// Get floor price from LooksRare
    async fn get_looksrare_floor_price(&self, collection: &str) -> Result<CollectionFloorPrice, NFTMarketplaceError> {
        let url = format!(
            "https://api.looksrare.org/api/v1/collection/{}",
            collection
        );
        
        let client = reqwest::Client::new();
        
        let mut request = client.get(&url);
        
        if let Some(ref key) = self.config.looksrare_api_key {
            request = request.header("Authorization", format!("Bearer {}", key));
        }
        
        let response = request.send().await
            .map_err(|e| NFTMarketplaceError::ApiError(e.to_string()))?;
        
        if response.status() == 404 {
            return Err(NFTMarketplaceError::NotFound(collection.to_string()));
        }
        
        if response.status() == 429 {
            return Err(NFTMarketplaceError::RateLimit("LooksRare rate limited".to_string()));
        }
        
        if !response.status().is_success() {
            return Err(NFTMarketplaceError::ApiError(format!(
                "LooksRare API returned {}", response.status()
            )));
        }
        
        let data: LooksRareCollection = response.json().await
            .map_err(|e| NFTMarketplaceError::ParseError(e.to_string()))?;
        
        let floor_price = data.data.floorPrice.parse::<f64>().unwrap_or(0.0)
            / 1_000_000_000_000_000_000.0; // Convert from wei
        
        Ok(CollectionFloorPrice {
            collection: collection.to_string(),
            floor_price,
            floor_price_native: format!("{} ETH", floor_price),
            currency: "ETH".to_string(),
            source: "LooksRare".to_string(),
            timestamp: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs() as i64,
            volume_24h: data.data.oneDayVolume.parse::<f64>().unwrap_or(0.0)
                / 1_000_000_000_000_000_000.0,
            sales_24h: data.data.oneDaySales.parse::<u32>().unwrap_or(0),
        })
    }
    
    /// Get floor price from NFTGo
    async fn get_nftgo_floor_price(&self, collection: &str) -> Result<CollectionFloorPrice, NFTMarketplaceError> {
        let url = format!(
            "https://api.nftgo.eth/v1/collection/{}/floor-price",
            collection
        );
        
        let client = reqwest::Client::new();
        
        let mut request = client.get(&url);
        
        if let Some(ref key) = self.config.nftgo_api_key {
            request = request.header("x-api-key", key);
        }
        
        let response = request.send().await
            .map_err(|e| NFTMarketplaceError::ApiError(e.to_string()))?;
        
        if response.status() == 404 {
            return Err(NFTMarketplaceError::NotFound(collection.to_string()));
        }
        
        if response.status() == 429 {
            return Err(NFTMarketplaceError::RateLimit("NFTGo rate limited".to_string()));
        }
        
        if !response.status().is_success() {
            return Err(NFTMarketplaceError::ApiError(format!(
                "NFTGo API returned {}", response.status()
            )));
        }
        
        let floor: NFTGoFloor = response.json().await
            .map_err(|e| NFTMarketplaceError::ParseError(e.to_string()))?;
        
        Ok(CollectionFloorPrice {
            collection: collection.to_string(),
            floor_price: floor.floorPrice,
            floor_price_native: format!("{} ETH", floor.floorPrice),
            currency: "ETH".to_string(),
            source: "NFTGo".to_string(),
            timestamp: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs() as i64,
            volume_24h: floor.oneDayVolume,
            sales_24h: floor.oneDaySales as u32,
        })
    }
    
    /// Update cache
    async fn update_cache(&self, collection: &str, price: &CollectionFloorPrice) {
        let mut cache = self.cache.write().await;
        cache.prices.insert(collection.to_string(), price.clone());
        cache.last_update.insert(collection.to_string(), price.timestamp);
    }
    
    /// Get collection stats
    pub async fn get_collection_stats(&self, collection: &str) -> Result<CollectionStats, NFTMarketplaceError> {
        // Try to get stats from primary marketplace
        if let Ok(stats) = self.get_bluemove_stats(collection).await {
            return Ok(stats);
        }
        
        if let Ok(stats) = self.get_opensea_stats(collection).await {
            return Ok(stats);
        }
        
        Err(NFTMarketplaceError::NotFound(format!(
            "No stats found for {}", collection
        )))
    }
    
    /// Get BlueMove stats
    async fn get_bluemove_stats(&self, collection: &str) -> Result<CollectionStats, NFTMarketplaceError> {
        let url = format!(
            "https://apis.bluemove.net/v1/collection/{}/stats",
            collection
        );
        
        let client = reqwest::Client::new();
        
        let response = client.get(&url).send().await
            .map_err(|e| NFTMarketplaceError::ApiError(e.to_string()))?;
        
        if !response.status().is_success() {
            return Err(NFTMarketplaceError::ApiError("API error".to_string()));
        }
        
        let stats: BlueMoveStats = response.json().await
            .map_err(|e| NFTMarketplaceError::ParseError(e.to_string()))?;
        
        Ok(CollectionStats {
            address: collection.to_string(),
            name: collection.to_string(),
            floor_price: stats.floor_price,
            volume_24h: stats.volume_24h,
            volume_7d: stats.volume_7d,
            volume_total: stats.volume_total,
            sales_24h: stats.sales_24h,
            sales_7d: stats.sales_7d,
            avg_price_24h: stats.avg_price_24h,
            median_price_24h: stats.median_price_24h,
            total_supply: stats.total_supply,
            num_owners: stats.num_owners,
            owner_distribution: vec![],
        })
    }
    
    /// Get OpenSea stats
    async fn get_opensea_stats(&self, collection: &str) -> Result<CollectionStats, NFTMarketplaceError> {
        let url = format!(
            "https://api.opensea.io/api/v2/collection/{}/stats",
            collection
        );
        
        let client = reqwest::Client::new();
        
        let response = client.get(&url).send().await
            .map_err(|e| NFTMarketplaceError::ApiError(e.to_string()))?;
        
        if !response.status().is_success() {
            return Err(NFTMarketplaceError::ApiError("API error".to_string()));
        }
        
        let stats: OpenSeaStats = response.json().await
            .map_err(|e| NFTMarketplaceError::ParseError(e.to_string()))?;
        
        Ok(CollectionStats {
            address: collection.to_string(),
            name: collection.to_string(),
            floor_price: stats.stats.floor_price.unwrap_or(0.0),
            volume_24h: stats.stats.one_day_volume,
            volume_7d: stats.stats.seven_day_volume,
            volume_total: stats.stats.total_volume,
            sales_24h: stats.stats.one_day_sales.unwrap_or(0) as u32,
            sales_7d: stats.stats.seven_day_sales.unwrap_or(0) as u32,
            avg_price_24h: stats.stats.one_day_average_price.unwrap_or(0.0),
            median_price_24h: stats.stats.one_day_median_price.unwrap_or(0.0),
            total_supply: stats.stats.total_supply.parse().unwrap_or(0),
            num_owners: stats.stats.num_owners.parse().unwrap_or(0),
            owner_distribution: vec![],
        })
    }
}

// =============================================================================
// API RESPONSE TYPES
// =============================================================================

#[derive(Debug, Deserialize)]
struct BlueMoveStats {
    #[serde(rename = "floorPrice")]
    floor_price: f64,
    #[serde(rename = "volume24h")]
    volume_24h: f64,
    #[serde(rename = "volume7d")]
    volume_7d: f64,
    #[serde(rename = "volumeTotal")]
    volume_total: f64,
    #[serde(rename = "sales24h")]
    sales_24h: u32,
    #[serde(rename = "sales7d")]
    sales_7d: u32,
    #[serde(rename = "avgPrice24h")]
    avg_price_24h: f64,
    #[serde(rename = "medianPrice24h")]
    median_price_24h: f64,
    #[serde(rename = "totalSupply")]
    total_supply: u64,
    #[serde(rename = "numOwners")]
    num_owners: u64,
}

#[derive(Debug, Deserialize)]
struct OpenSeaStats {
    stats: OpenSeaCollectionStats,
}

#[derive(Debug, Deserialize)]
struct OpenSeaCollectionStats {
    #[serde(rename = "floor_price")]
    floor_price: Option<f64>,
    #[serde(rename = "one_day_volume")]
    one_day_volume: f64,
    #[serde(rename = "seven_day_volume")]
    seven_day_volume: f64,
    #[serde(rename = "total_volume")]
    total_volume: f64,
    #[serde(rename = "one_day_sales")]
    one_day_sales: Option<u32>,
    #[serde(rename = "seven_day_sales")]
    seven_day_sales: Option<u32>,
    #[serde(rename = "one_day_average_price")]
    one_day_average_price: Option<f64>,
    #[serde(rename = "one_day_median_price")]
    one_day_median_price: Option<f64>,
    #[serde(rename = "total_supply")]
    total_supply: String,
    #[serde(rename = "num_owners")]
    num_owners: String,
}

#[derive(Debug, Deserialize)]
struct LooksRareCollection {
    data: LooksRareData,
}

#[derive(Debug, Deserialize)]
struct LooksRareData {
    #[serde(rename = "floorPrice")]
    floorPrice: String,
    #[serde(rename = "oneDayVolume")]
    oneDayVolume: String,
    #[serde(rename = "oneDaySales")]
    oneDaySales: String,
}

#[derive(Debug, Deserialize)]
struct NFTGoFloor {
    #[serde(rename = "floorPrice")]
    floorPrice: f64,
    #[serde(rename = "oneDayVolume")]
    oneDayVolume: f64,
    #[serde(rename = "oneDaySales")]
    oneDaySales: u32,
}