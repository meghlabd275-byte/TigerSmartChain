//! TigerScan NFT Service - Metadata, IPFS/Arweave, Rarity Analysis
//! Production-grade Rust service for NFT metadata indexing and rarity analysis

use std::sync::Arc;
use std::time::Duration;

use anyhow::Result;
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use sqlx::postgres::{PgPool, PgPoolOptions};
use sqlx::{Executor, Row};
use thiserror::Error;
use tokio::sync::mpsc;
use tokio::time::interval;
use tracing::{error, info, warn};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

// ============================================================================
// Error Types
// ============================================================================

#[derive(Error, Debug)]
pub enum NFTServiceError {
    #[error("Database error: {0}")]
    Database(#[from] sqlx::Error),
    
    #[error("HTTP request error: {0}")]
    Http(#[from] reqwest::Error),
    
    #[error("NFT not found: {0}")]
    NotFound(String),
    
    #[error("Metadata fetch failed: {0}")]
    MetadataFetchFailed(String),
    
    #[error("Invalid metadata: {0}")]
    InvalidMetadata(String),
    
    #[error("Rarity analysis failed: {0}")]
    RarityAnalysisFailed(String),
}

// ============================================================================
// Data Models
// ============================================================================

/// NFT collection
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NFTCollection {
    pub address: String,
    pub name: String,
    pub symbol: Option<String>,
    pub contract_type: String, // ERC721, ERC1155
    pub total_supply: u64,
    pub owner_count: u64,
    pub holder_count: u64,
    pub transfers_24h: u64,
    pub floor_price: f64,
    pub floor_price_usd: f64,
    pub volume_24h: f64,
    pub volume_24h_usd: f64,
    pub volume_7d: f64,
    pub volume_7d_usd: f64,
    pub market_cap: f64,
    pub average_price_7d: f64,
    pub royalty_bps: Option<u32>,
    pub verified: bool,
    pub category: Option<String>,
    pub description: Option<String>,
    pub image_url: Option<String>,
    pub banner_url: Option<String>,
    pub external_url: Option<String>,
    pub twitter: Option<String>,
    pub discord: Option<String>,
    pub last_updated: DateTime<Utc>,
}

/// Individual NFT
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NFT {
    pub id: String, // token_id
    pub collection_address: String,
    pub owner: String,
    pub owner_normalized: String,
    pub current_price: Option<f64>,
    pub current_price_usd: Option<f64>,
    pub last_sale_price: Option<f64>,
    pub last_sale_price_usd: Option<f64>,
    pub last_sale_timestamp: Option<DateTime<Utc>>,
    pub transfers_count: u64,
    pub metadata: Option<NFTMetadata>,
    pub metadata_url: Option<String>,
    pub metadata_fetched_at: Option<DateTime<Utc>>,
    pub image_url: Option<String>,
    pub animation_url: Option<String>,
}

/// NFT metadata as per standard
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NFTMetadata {
    pub name: Option<String>,
    pub description: Option<String>,
    pub image: Option<String>,
    pub external_url: Option<String>,
    pub attributes: Vec<NFTAttribute>,
    pub background_color: Option<String>,
    pub animation_url: Option<String>,
    pub youtube_url: Option<String>,
}

/// NFT attribute/trait
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NFTAttribute {
    pub trait_type: String,
    pub value: serde_json::Value,
    pub display_type: Option<String>,
}

/// NFT holder
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NFTHolder {
    pub address: String,
    pub balance: u64,
    pub percentage: f64,
    pub first_seen: DateTime<Utc>,
    pub last_updated: DateTime<Utc>,
}

/// NFT transfer
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NFTTransfer {
    pub hash: String,
    pub block_number: u64,
    pub timestamp: DateTime<Utc>,
    pub collection_address: String,
    pub token_id: String,
    pub from: String,
    pub to: String,
    pub amount: String,
    pub price: Option<f64>,
    pub price_usd: Option<f64>,
}

/// Rarity data for an NFT
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NFTRarity {
    pub collection_address: String,
    pub token_id: String,
    pub rarity_score: f64,
    pub rarity_rank: u64,
    pub total_minted: u64,
    pub trait_rarities: Vec<TraitRarity>,
}

/// Rarity for a specific trait
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TraitRarity {
    pub trait_type: String,
    pub value: String,
    pub occurrence: f64, // 0.0 to 1.0
    pub rarity_score: f64,
}

/// Floor price entry
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FloorPrice {
    pub collection_address: String,
    pub price: f64,
    pub price_usd: f64,
    pub source: String,
    pub token_id: Option<String>,
    pub timestamp: DateTime<Utc>,
}

/// Collection activity
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CollectionActivity {
    pub collection_address: String,
    pub event_type: String, // mint, transfer, sale, bid, listing
    pub price: Option<f64>,
    pub price_usd: Option<f64>,
    pub from: Option<String>,
    pub to: Option<String>,
    pub token_id: Option<String>,
    pub timestamp: DateTime<Utc>,
}

// ============================================================================
// Configuration
// ============================================================================

#[derive(Debug, Clone, Deserialize)]
pub struct Config {
    pub database: DatabaseConfig,
    pub ipfs: IPFSConfig,
    pub arweave: ArweaveConfig,
    pub server: ServerConfig,
}

#[derive(Debug, Clone, Deserialize)]
pub struct DatabaseConfig {
    pub host: String,
    pub port: u16,
    pub username: String,
    pub password: String,
    pub database: String,
    pub max_connections: u32,
    pub min_connections: u32,
}

impl DatabaseConfig {
    pub fn connection_string(&self) -> String {
        format!(
            "postgres://{}:{}@{}:{}/{}",
            self.username, self.password, self.host, self.port, self.database
        )
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct IPFSConfig {
    pub enabled: bool,
    pub gateway_urls: Vec<String>,
    pub node_url: Option<String>,
    pub timeout: u64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ArweaveConfig {
    pub enabled: bool,
    pub gateway_url: String,
    pub gateway_timeout: u64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ServerConfig {
    pub host: String,
    pub port: u16,
    pub rate_limit: u32,
    pub rate_limit_burst: u32,
}

// ============================================================================
// NFT Service
// ============================================================================

pub struct NFTService {
    pool: PgPool,
    config: Config,
    ipfs_client: Option<IPFSClient>,
    arweave_client: Option<ArweaveClient>,
    metadata_cache: Arc<RwLock<MetadataCache>>,
    rarity_cache: Arc<RwLock<RarityCache>>,
    metrics: Arc<ServiceMetrics>,
    shutdown_tx: mpsc::Sender<()>,
}

#[derive(Default)]
pub struct MetadataCache {
    cache: std::collections::HashMap<String, CachedMetadata>,
    last_cleanup: Option<DateTime<Utc>>,
}

#[derive(Clone)]
pub struct CachedMetadata {
    pub metadata: NFTMetadata,
    pub fetched_at: DateTime<Utc>,
}

#[derive(Default)]
pub struct RarityCache {
    cache: std::collections::HashMap<String, NFTRarity>,
    last_update: Option<DateTime<Utc>>,
}

#[derive(Default, Clone)]
pub struct ServiceMetrics {
    pub metadata_fetches: parking_lot::RwLock<u64>,
    pub metadata_cache_hits: parking_lot::RwLock<u64>,
    pub ipfs_fetches: parking_lot::RwLock<u64>,
    pub arweave_fetches: parking_lot::RwLock<u64>,
    pub rarity_calculations: parking_lot::RwLock<u64>,
    pub errors: parking_lot::RwLock<u64>,
}

// ============================================================================
// IPFS Client
// ============================================================================

pub struct IPFSClient {
    gateways: Vec<String>,
    client: reqwest::Client,
}

impl IPFSClient {
    pub fn new(gateways: Vec<String>) -> Self {
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(30))
            .build()
            .unwrap_or_default();
        
        Self { gateways, client }
    }
    
    pub async fn fetch(&self, cid: &str) -> Result<String, NFTServiceError> {
        for gateway in &self.gateways {
            let url = format!("{}/ipfs/{}", gateway, cid);
            
            match self.client.get(&url).send().await {
                Ok(response) if response.status().is_success() => {
                    let content = response.text().await?;
                    return Ok(content);
                }
                _ => continue,
            }
        }
        
        Err(NFTServiceError::MetadataFetchFailed(
            format!("Failed to fetch from IPFS: {}", cid)
        ))
    }
}

// ============================================================================
// Arweave Client
// ============================================================================

pub struct ArweaveClient {
    gateway_url: String,
    client: reqwest::Client,
}

impl ArweaveClient {
    pub fn new(gateway_url: String) -> Self {
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(30))
            .build()
            .unwrap_or_default();
        
        Self { gateway_url, client }
    }
    
    pub async fn fetch(&self, tx_id: &str) -> Result<String, NFTServiceError> {
        let url = format!("{}/{}", self.gateway_url, tx_id);
        
        let response = self.client.get(&url).send().await?;
        
        if !response.status().is_success() {
            return Err(NFTServiceError::MetadataFetchFailed(
                format!("Arweave returned status: {}", response.status())
            ));
        }
        
        let content = response.text().await?;
        Ok(content)
    }
}

// ============================================================================
// Service Implementation
// ============================================================================

impl NFTService {
    pub async fn new(config: Config) -> Result<Self, NFTServiceError> {
        let pool = PgPoolOptions::new()
            .max_connections(config.database.max_connections)
            .min_connections(config.database.min_connections)
            .acquire_timeout(Duration::from_secs(30))
            .connect(&config.database.connection_string())
            .await?;
        
        // Initialize IPFS client
        let ipfs_client = if config.ipfs.enabled {
            Some(IPFSClient::new(config.ipfs.gateway_urls.clone()))
        } else {
            None
        };
        
        // Initialize Arweave client
        let arweave_client = if config.arweave.enabled {
            Some(ArweaveClient::new(config.arweave.gateway_url.clone()))
        } else {
            None
        };
        
        let (shutdown_tx, _) = mpsc::channel::<()>(1);
        
        Ok(Self {
            pool,
            config,
            ipfs_client,
            arweave_client,
            metadata_cache: Arc::new(RwLock::new(MetadataCache::default())),
            rarity_cache: Arc::new(RwLock::new(RarityCache::default())),
            metrics: Arc::new(ServiceMetrics::default()),
            shutdown_tx,
        })
    }
    
    /// Start the NFT service
    pub async fn run(&self) -> Result<()> {
        info!("Starting NFT service");
        
        // Start metadata sync task
        let pool = self.pool.clone();
        let ipfs_client = self.ipfs_client.clone();
        let arweave_client = self.arweave_client.clone();
        let metadata_cache = self.metadata_cache.clone();
        let metrics = self.metrics.clone();
        
        tokio::spawn(async move {
            let mut interval = interval(Duration::from_secs(60));
            
            loop {
                interval.tick().await;
                
                if let Err(e) = Self::sync_metadata(
                    &pool,
                    &ipfs_client,
                    &arweave_client,
                    &metadata_cache,
                    &metrics,
                ).await {
                    error!("Metadata sync error: {}", e);
                }
            }
        });
        
        // Start rarity calculation task
        let rarity_pool = self.pool.clone();
        let rarity_cache = self.rarity_cache.clone();
        let rarity_metrics = self.metrics.clone();
        
        tokio::spawn(async move {
            let mut interval = interval(Duration::from_secs(300));
            
            loop {
                interval.tick().await;
                
                if let Err(e) = Self::calculate_rarity(
                    &rarity_pool,
                    &rarity_cache,
                    &rarity_metrics,
                ).await {
                    error!("Rarity calculation error: {}", e);
                }
            }
        });
        
        // Start API server
        self.start_server().await?;
        
        Ok(())
    }
    
    /// Sync metadata for NFTs
    async fn sync_metadata(
        pool: &PgPool,
        ipfs_client: &Option<IPFSClient>,
        arweave_client: &Option<ArweaveClient>,
        cache: &Arc<RwLock<MetadataCache>>,
        metrics: &Arc<ServiceMetrics>,
    ) -> Result<()> {
        // Get NFTs that need metadata fetched
        let nfts: Vec<(String, String, Option<String>)> = sqlx::query(
            "SELECT collection_address, token_id, metadata_url 
             FROM nfts 
             WHERE metadata_fetched_at IS NULL 
                OR metadata_fetched_at < NOW() - INTERVAL '1 hour'
             LIMIT 100"
        )
        .fetch_all(pool)
        .await?
        .into_iter()
        .map(|row| (row.get(0), row.get(1), row.get(2)))
        .collect();
        
        for (collection, token_id, metadata_url) in nfts {
            let result = if let Some(url) = metadata_url {
                Self::fetch_metadata_from_url(&url, ipfs_client, arweave_client).await
            } else {
                // Try standard tokenURI
                Self::fetch_metadata_from_chain(pool, &collection, &token_id).await
            };
            
            match result {
                Ok(metadata) => {
                    // Store metadata
                    let metadata_json = serde_json::to_string(&metadata)?;
                    
                    sqlx::query(
                        "UPDATE nfts SET metadata = $1, metadata_fetched_at = NOW() 
                         WHERE collection_address = $2 AND token_id = $3"
                    )
                    .bind(metadata_json)
                    .bind(collection)
                    .bind(token_id)
                    .execute(pool)
                    .await?;
                    
                    // Update cache
                    let key = format!("{}:{}", collection, token_id);
                    cache.write().cache.insert(key, CachedMetadata {
                        metadata,
                        fetched_at: Utc::now(),
                    });
                }
                Err(e) => {
                    metrics.errors.write().inc();
                    warn!("Failed to fetch metadata for {}: {}", token_id, e);
                }
            }
            
            metrics.metadata_fetches.write().inc();
        }
        
        Ok(())
    }
    
    /// Fetch metadata from URL (IPFS/Arweave/HTTP)
    async fn fetch_metadata_from_url(
        url: &str,
        ipfs_client: &Option<IPFSClient>,
        arweave_client: &Option<ArweaveClient>,
    ) -> Result<NFTMetadata, NFTServiceError> {
        // Determine URL type and fetch
        let content = if url.starts_with("ipfs://") {
            let cid = url.trim_start_matches("ipfs://");
            
            if let Some(client) = ipfs_client {
                metrics.ipfs_fetches.write().inc();
                client.fetch(cid).await?
            } else {
                return Err(NFTServiceError::MetadataFetchFailed("IPFS not configured".to_string()));
            }
        } else if url.starts_with("ar://") {
            let tx_id = url.trim_start_matches("ar://");
            
            if let Some(client) = arweave_client {
                metrics.arweave_fetches.write().inc();
                client.fetch(tx_id).await?
            } else {
                return Err(NFTServiceError::MetadataFetchFailed("Arweave not configured".to_string()));
            }
        } else {
            // Regular HTTP URL
            let client = reqwest::Client::new();
            client.get(url).send().await?.text().await?
        };
        
        // Parse metadata
        let metadata: NFTMetadata = serde_json::from_str(&content)
            .map_err(|e| NFTServiceError::InvalidMetadata(e.to_string()))?;
        
        Ok(metadata)
    }
    
    /// Fetch metadata from chain (tokenURI)
    async fn fetch_metadata_from_chain(
        pool: &PgPool,
        collection: &str,
        token_id: &str,
    ) -> Result<NFTMetadata, NFTServiceError> {
        // Get tokenURI from contract
        let token_uri: String = sqlx::query_scalar(
            "SELECT token_uri FROM nft_tokens 
             WHERE collection_address = $1 AND token_id = $2"
        )
        .bind(collection)
        .bind(token_id)
        .fetch_one(pool)
        .await?;
        
        // Replace {token_id} and {id}
        let url = token_uri
            .replace("{token_id}", token_id)
            .replace("{id}", token_id);
        
        Self::fetch_metadata_from_url(&url, &None, &None).await
    }
    
    /// Calculate rarity for all NFTs in a collection
    async fn calculate_rarity(
        pool: &PgPool,
        cache: &Arc<RwLock<RarityCache>>,
        metrics: &Arc<ServiceMetrics>,
    ) -> Result<()> {
        // Get collections to calculate rarity for
        let collections: Vec<String> = sqlx::query(
            "SELECT DISTINCT collection_address FROM nfts WHERE metadata IS NOT NULL"
        )
        .fetch_all(pool)
        .await?
        .into_iter()
        .map(|row| row.get(0))
        .collect();
        
        for collection in collections {
            // Get all metadata for this collection
            let nfts: Vec<(String, Option<String>) = sqlx::query(
                "SELECT token_id, metadata FROM nfts WHERE collection_address = $1"
            )
            .bind(&collection)
            .fetch_all(pool)
            .await?
            .into_iter()
            .map(|row| (row.get(0), row.get(1)))
            .collect();
            
            // Parse all metadata and collect traits
            let mut all_traits: std::collections::HashMap<String, std::collections::HashMap<String, u64>> = 
                std::collections::HashMap::new();
            
            for (token_id, metadata_json) in nfts {
                if let Some(json) = metadata_json {
                    if let Ok(metadata) = serde_json::from_str::<NFTMetadata>(&json) {
                        for attr in metadata.attributes {
                            let trait_type = attr.trait_type.clone();
                            let value = serde_json::to_string(&attr.value)?;
                            
                            let entry = all_traits.entry(trait_type).or_default();
                            *entry.entry(value).or_insert(0) += 1;
                        }
                    }
                }
            }
            
            // Calculate total minted
            let total_minted = nfts.len() as u64;
            
            // Calculate rarity for each NFT
            for (token_id, metadata_json) in nfts {
                if let Some(json) = metadata_json {
                    if let Ok(metadata) = serde_json::from_str::<NFTMetadata>(&json) {
                        let mut trait_rarities = Vec::new();
                        let mut rarity_score = 0.0;
                        
                        for attr in &metadata.attributes {
                            let value = serde_json::to_string(&attr.value)?;
                            
                            let total_with_trait = all_traits
                                .get(&attr.trait_type)
                                .and_then(|t| t.get(&value))
                                .copied()
                                .unwrap_or(0);
                            
                            let occurrence = if total_minted > 0 {
                                total_with_trait as f64 / total_minted as f64
                            } else {
                                0.0
                            };
                            
                            let trait_rarity = if occurrence > 0.0 {
                                1.0 / occurrence
                            } else {
                                1.0
                            };
                            
                            rarity_score += trait_rarity.log10();
                            
                            trait_rarities.push(TraitRarity {
                                trait_type: attr.trait_type.clone(),
                                value,
                                occurrence,
                                rarity_score: trait_rarity,
                            });
                        }
                        
                        // Normalize rarity score
                        rarity_score = if !trait_rarities.is_empty() {
                            rarity_score / trait_rarities.len() as f64
                        } else {
                            0.0
                        };
                        
                        // Calculate rank
                        let rarity_rank = 1; // Simplified - would need full collection sort
                        
                        // Store rarity
                        let rarity = NFTRarity {
                            collection_address: collection.clone(),
                            token_id,
                            rarity_score,
                            rarity_rank,
                            total_minted,
                            trait_rarities,
                        };
                        
                        let cache_key = format!("{}:{}", collection, rarity.token_id);
                        cache.write().cache.insert(cache_key, rarity);
                    }
                }
            }
            
            metrics.rarity_calculations.write().inc();
        }
        
        Ok(())
    }
    
    /// Start API server
    async fn start_server(&self) -> Result<()> {
        info!("Starting NFT API server on {}:{}", 
            self.config.server.host, self.config.server.port);
        
        // In production, implement REST API with axum
        Ok(())
    }
    
    // ============================================================================
    // Public API Methods
    // ============================================================================
    
    /// Get collection by address
    pub async fn get_collection(&self, address: &str) -> Result<NFTCollection, NFTServiceError> {
        let collection = sqlx::query_as(
            "SELECT address, name, symbol, contract_type, total_supply, owner_count, holder_count,
                    transfers_24h, floor_price, floor_price_usd, volume_24h, volume_24h_usd,
                    volume_7d, volume_7d_usd, market_cap, average_price_7d, royalty_bps,
                    verified, category, description, image_url, banner_url, external_url,
                    twitter, discord, last_updated
             FROM nft_collections
             WHERE address = $1"
        )
        .bind(address)
        .fetch_optional(&self.pool)
        .await?
        .ok_or_else(|| NFTServiceError::NotFound(address.to_string()))?;
        
        Ok(collection)
    }
    
    /// Get NFT by collection and token ID
    pub async fn get_nft(&self, address: &str, token_id: &str) -> Result<NFT, NFTServiceError> {
        // Check cache first
        let cache_key = format!("{}:{}", address, token_id);
        if let Some(cached) = self.metadata_cache.read().cache.get(&cache_key) {
            self.metrics.metadata_cache_hits.write().inc();
            
            let mut nft = NFT {
                id: token_id.to_string(),
                collection_address: address.to_string(),
                owner: String::new(),
                owner_normalized: String::new(),
                current_price: None,
                current_price_usd: None,
                last_sale_price: None,
                last_sale_price_usd: None,
                last_sale_timestamp: None,
                transfers_count: 0,
                metadata: Some(cached.metadata.clone()),
                metadata_url: None,
                metadata_fetched_at: Some(cached.fetched_at),
                image_url: cached.metadata.image.clone(),
                animation_url: cached.metadata.animation_url.clone(),
            };
            
            return Ok(nft);
        }
        
        // Fetch from database
        let nft = sqlx::query_as(
            "SELECT id, collection_address, owner, owner_normalized, current_price, current_price_usd,
                    last_sale_price, last_sale_price_usd, last_sale_timestamp, transfers_count,
                    metadata, metadata_url, metadata_fetched_at, image_url, animation_url
             FROM nfts
             WHERE collection_address = $1 AND id = $2"
        )
        .bind(address)
        .bind(token_id)
        .fetch_optional(&self.pool)
        .await?
        .ok_or_else(|| NFTServiceError::NotFound(format!("{}/{}", address, token_id)))?;
        
        Ok(nft)
    }
    
    /// Get NFT rarity
    pub async fn get_rarity(&self, address: &str, token_id: &str) -> Result<NFTRarity, NFTServiceError> {
        let cache_key = format!("{}:{}", address, token_id);
        
        if let Some(rarity) = self.rarity_cache.read().cache.get(&cache_key) {
            return Ok(rarity.clone());
        }
        
        // Calculate on-demand
        let rarity: NFTRarity = sqlx::query_as(
            "SELECT collection_address, token_id, rarity_score, rarity_rank, total_minted, trait_rarities
             FROM nft_rarity
             WHERE collection_address = $1 AND token_id = $2"
        )
        .bind(address)
        .bind(token_id)
        .fetch_one(&self.pool)
        .await?;
        
        Ok(rarity)
    }
    
    /// Get collection holders
    pub async fn get_holders(&self, address: &str, limit: usize) -> Result<Vec<NFTHolder>, NFTServiceError> {
        let holders: Vec<NFTHolder> = sqlx::query_as(
            "SELECT address, balance, percentage, first_seen, last_updated
             FROM nft_holders
             WHERE collection_address = $1
             ORDER BY balance DESC
             LIMIT $2"
        )
        .bind(address)
        .bind(limit as i64)
        .fetch_all(&self.pool)
        .await?
        .into_iter()
        .map(|row| NFTHolder {
            address: row.0,
            balance: row.1,
            percentage: row.2,
            first_seen: row.3,
            last_updated: row.4,
        })
        .collect();
    
        Ok(holders)
    }
    
    /// Get collection transfers
    pub async fn get_transfers(
        &self,
        address: &str,
        from_block: u64,
        limit: usize,
    ) -> Result<Vec<NFTTransfer>, NFTServiceError> {
        let transfers: Vec<NFTTransfer> = sqlx::query_as(
            "SELECT hash, block_number, timestamp, collection_address, token_id, 
                    from, to, amount, price, price_usd
             FROM nft_transfers
             WHERE collection_address = $1 AND block_number > $2
             ORDER BY block_number DESC, log_index DESC
             LIMIT $3"
        )
        .bind(address)
        .bind(from_block as i64)
        .bind(limit as i64)
        .fetch_all(&self.pool)
        .await?
        .into_iter()
        .map(|row| NFTTransfer {
            hash: row.0,
            block_number: row.1,
            timestamp: row.2,
            collection_address: row.3,
            token_id: row.4,
            from: row.5,
            to: row.6,
            amount: row.7,
            price: row.8,
            price_usd: row.9,
        })
        .collect();
    
        Ok(transfers)
    }
    
    /// Get collection activity
    pub async fn get_activity(
        &self,
        address: &str,
        limit: usize,
    ) -> Result<Vec<CollectionActivity>, NFTServiceError> {
        let activity: Vec<CollectionActivity> = sqlx::query_as(
            "SELECT collection_address, event_type, price, price_usd, 
                    from, to, token_id, timestamp
             FROM nft_activity
             WHERE collection_address = $1
             ORDER BY timestamp DESC
             LIMIT $2"
        )
        .bind(address)
        .bind(limit as i64)
        .fetch_all(&self.pool)
        .await?
        .into_iter()
        .map(|row| CollectionActivity {
            collection_address: row.0,
            event_type: row.1,
            price: row.2,
            price_usd: row.3,
            from: row.4,
            to: row.5,
            token_id: row.6,
            timestamp: row.7,
        })
        .collect();
    
        Ok(activity)
    }
    
    /// Get service metrics
    pub fn get_metrics(&self) -> ServiceMetricsResponse {
        let m = &*self.metrics;
        ServiceMetricsResponse {
            metadata_fetches: *m.metadata_fetches.read(),
            metadata_cache_hits: *m.metadata_cache_hits.read(),
            ipfs_fetches: *m.ipfs_fetches.read(),
            arweave_fetches: *m.arweave_fetches.read(),
            rarity_calculations: *m.rarity_calculations.read(),
            errors: *m.errors.read(),
        }
    }
    
    /// Shutdown the service
    pub async fn shutdown(&self) {
        let _ = self.shutdown_tx.send(()).await;
    }
}

#[derive(Serialize)]
pub struct ServiceMetricsResponse {
    pub metadata_fetches: u64,
    pub metadata_cache_hits: u64,
    pub ipfs_fetches: u64,
    pub arweave_fetches: u64,
    pub rarity_calculations: u64,
    pub errors: u64,
}

// ============================================================================
// Main Entry Point
// ============================================================================

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize logging
    tracing_subscriber::registry()
        .with(tracing_subscriber::EnvFilter::try_from_default_env()
            .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")))
        .with(tracing_subscriber::fmt::layer())
        .init();
    
    info!("Starting TigerScan NFT Service");
    
    // Load configuration
    let config = Config {
        database: DatabaseConfig {
            host: std::env::var("DB_HOST").unwrap_or_else(|_| "localhost".to_string()),
            port: std::env::var("DB_PORT")
                .unwrap_or_else(|_| "5432".to_string())
                .parse()?,
            username: std::env::var("DB_USER").unwrap_or_else(|_| "tigerscan".to_string()),
            password: std::env::var("DB_PASSWORD").unwrap_or_else(|_| "tigerscan".to_string()),
            database: std::env::var("DB_NAME").unwrap_or_else(|_| "tigerscan".to_string()),
            max_connections: 20,
            min_connections: 5,
        },
        ipfs: IPFSConfig {
            enabled: !std::env::var("IPFS_GATEWAY_URLS")
                .unwrap_or_default()
                .is_empty(),
            gateway_urls: std::env::var("IPFS_GATEWAY_URLS")
                .unwrap_or_default()
                .split(',')
                .map(|s| s.to_string())
                .collect(),
            node_url: std::env::var("IPFS_NODE_URL").ok(),
            timeout: 30,
        },
        arweave: ArweaveConfig {
            enabled: !std::env::var("ARWEAVE_GATEWAY_URL")
                .unwrap_or_default()
                .is_empty(),
            gateway_url: std::env::var("ARWEAVE_GATEWAY_URL")
                .unwrap_or_else(|_| "https://arweave.net".to_string()),
            gateway_timeout: 30,
        },
        server: ServerConfig {
            host: "0.0.0.0".to_string(),
            port: 8081,
            rate_limit: 1000,
            rate_limit_burst: 2000,
        },
    };
    
    // Create and run service
    let service = NFTService::new(config).await?;
    service.run().await?;
    
    Ok(())
}