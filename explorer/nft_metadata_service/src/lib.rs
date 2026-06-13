//! TigerScan NFT Metadata Service
//! Production-grade NFT metadata fetching with IPFS/Arweave support, floor prices, rarity analysis

use std::collections::HashMap;
use std::sync::Arc;

use anyhow::Result;
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use parking_lot::RwLock;
use regex::Regex;
use serde::{Deserialize, Serialize};
use sqlx::postgres::{PgPool, PgPoolOptions};
use tokio::time::interval;
use tracing::{error, info, warn};

// ============================================================================
// Configuration
// ============================================================================

#[derive(Debug, Clone, Deserialize)]
pub struct Config {
    pub database_url: String,
    pub ipfs_gateway: String,
    pub arweave_gateway: String,
    pub update_interval: u64,
    pub max_concurrent: usize,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            database_url: std::env::var("DATABASE_URL")
                .unwrap_or_else(|_| "postgres://localhost:5432/tigerscan".to_string()),
            ipfs_gateway: "https://ipfs.io/ipfs/".to_string(),
            arweave_gateway: "https://arweave.net/".to_string(),
            update_interval: 300,
            max_concurrent: 10,
        }
    }
}

// ============================================================================
// Types
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NFTMetadata {
    pub token_address: String,
    pub token_id: String,
    pub name: Option<String>,
    pub description: Option<String>,
    pub image_url: Option<String>,
    pub external_url: Option<String>,
    pub attributes: Vec<NFTAttribute>,
    pub background_color: Option<String>,
    pub animation_url: Option<String>,
    pub youtube_url: Option<String>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NFTAttribute {
    pub trait_type: String,
    pub value: String,
    pub display_type: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CollectionStats {
    pub address: String,
    pub name: String,
    pub total_supply: i64,
    pub holders_count: i64,
    pub transfers_count: i64,
    pub floor_price: f64,
    pub floor_price_change_24h: f64,
    pub volume_24h: f64,
    pub volume_change_24h: f64,
    pub average_price_24h: f64,
    pub market_cap: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RarityResult {
    pub token_id: String,
    pub rarity_score: f64,
    pub rank: i32,
    pub trait_rarity: HashMap<String, TraitRarity>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TraitRarity {
    pub value: String,
    pub count: i32,
    pub rarity: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FloorPrice {
    pub collection: String,
    pub price: f64,
    pub source: String,
    pub timestamp: i64,
}

// ============================================================================
// IPFS/Arweave Client
// ============================================================================

pub struct MetadataClient {
    client: reqwest::Client,
    ipfs_gateway: String,
    arweave_gateway: String,
}

impl MetadataClient {
    pub fn new(config: &Config) -> Self {
        Self {
            client: reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(30))
                .build()
                .unwrap_or_default(),
            ipfs_gateway: config.ipfs_gateway.clone(),
            arweave_gateway: config.arweave_gateway.clone(),
        }
    }

    pub async fn fetch_metadata(&self, uri: &str) -> Result<NFTMetadata, String> {
        if uri.starts_with("ipfs://") {
            self.fetch_ipfs(uri).await
        } else if uri.starts_with("ar://") || uri.starts_with("arweave://") {
            self.fetch_arweave(uri).await
        } else {
            self.fetch_http(uri).await
        }
    }

    async fn fetch_ipfs(&self, uri: &str) -> Result<NFTMetadata, String> {
        let cid = uri.trim_start_matches("ipfs://");
        let url = format!("{}{}", self.ipfs_gateway, cid);

        let response = self.client
            .get(&url)
            .send()
            .await
            .map_err(|e| e.to_string())?;

        if !response.status().is_success() {
            return Err(format!("IPFS request failed: {}", response.status()));
        }

        let metadata: NFTMetadata = response.json().await.map_err(|e| e.to_string())?;
        Ok(metadata)
    }

    async fn fetch_arweave(&self, uri: &str) -> Result<NFTMetadata, String> {
        let id = uri.trim_start_matches("ar://").trim_start_matches("arweave://");
        let url = format!("{}{}", self.arweave_gateway, id);

        let response = self.client
            .get(&url)
            .send()
            .await
            .map_err(|e| e.to_string())?;

        if !response.status().is_success() {
            return Err(format!("Arweave request failed: {}", response.status()));
        }

        let metadata: NFTMetadata = response.json().await.map_err(|e| e.to_string())?;
        Ok(metadata)
    }

    async fn fetch_http(&self, uri: &str) -> Result<NFTMetadata, String> {
        let response = self.client
            .get(uri)
            .send()
            .await
            .map_err(|e| e.to_string())?;

        if !response.status().is_success() {
            return Err(format!("HTTP request failed: {}", response.status()));
        }

        let metadata: NFTMetadata = response.json().await.map_err(|e| e.to_string())?;
        Ok(metadata)
    }
}

// ============================================================================
// Rarity Analyzer
// ============================================================================

pub struct RarityAnalyzer {
    traits_pattern: Regex,
}

impl RarityAnalyzer {
    pub fn new() -> Self {
        Self {
            traits_pattern: Regex::new(r#""(trait_type|display_type)":\s*"([^"]+)""#).unwrap(),
        }
    }

    pub fn analyze(&self, nfts: &[NFTMetadata]) -> Vec<RarityResult> {
        // Count trait occurrences
        let mut trait_counts: HashMap<String, HashMap<String, i32>> = HashMap::new();
        
        for nft in nfts {
            for attr in &nft.attributes {
                let entry = trait_counts.entry(attr.trait_type.clone()).or_insert_with(HashMap::new);
                *entry.entry(attr.value.clone()).or_insert(0) += 1;
            }
        }

        let total = nfts.len() as f64;
        let mut results = Vec::new();

        for nft in nfts {
            let mut rarity_score = 0.0;
            let mut trait_rarity = HashMap::new();

            for attr in &nft.attributes {
                if let Some(value_counts) = trait_counts.get(&attr.trait_type) {
                    let count = *value_counts.get(&attr.value).unwrap_or(&1) as f64;
                    let rarity = 1.0 - (count / total);
                    rarity_score += rarity;

                    trait_rarity.insert(attr.trait_type.clone(), TraitRarity {
                        value: attr.value.clone(),
                        count: count as i32,
                        rarity,
                    });
                }
            }

            results.push(RarityResult {
                token_id: nft.token_id.clone(),
                rarity_score,
                rank: 0, // Will be sorted later
                trait_rarity,
            });
        }

        // Sort by rarity score descending
        results.sort_by(|a, b| b.rarity_score.partial_cmp(&a.rarity_score).unwrap());

        // Assign ranks
        for (i, result) in results.iter_mut().enumerate() {
            result.rank = (i + 1) as i32;
        }

        results
    }
}

// ============================================================================
// NFT Metadata Service
// ============================================================================

pub struct NFTMetadataService {
    config: Config,
    db: PgPool,
    client: MetadataClient,
    analyzer: RarityAnalyzer,
    state: Arc<RwLock<MetadataState>>,
}

#[derive(Debug, Clone)]
pub struct MetadataState {
    pub collections_indexed: u64,
    pub nfts_processed: u64,
    pub errors: u64,
    pub last_update: Option<DateTime<Utc>>,
}

impl NFTMetadataService {
    pub async fn new(config: Config) -> Result<Self> {
        let db = PgPoolOptions::new()
            .max_connections(10)
            .connect(&config.database_url)
            .await?;

        let client = MetadataClient::new(&config);
        let analyzer = RarityAnalyzer::new();

        Ok(Self {
            config,
            db,
            client,
            analyzer,
            state: Arc::new(RwLock::new(MetadataState {
                collections_indexed: 0,
                nfts_processed: 0,
                errors: 0,
                last_update: None,
            })),
        })
    }

    pub async fn start(&self) {
        info!("Starting NFT metadata service...");

        let mut interval = interval(std::time::Duration::from_secs(self.config.update_interval));

        loop {
            interval.tick().await;

            if let Err(e) = self.update_collections().await {
                error!("Failed to update collections: {}", e);
                self.state.write().errors += 1;
            }

            if let Err(e) = self.fetch_metadata_batch().await {
                error!("Failed to fetch metadata: {}", e);
                self.state.write().errors += 1;
            }

            if let Err(e) = self.update_floor_prices().await {
                error!("Failed to update floor prices: {}", e);
            }

            self.state.write().last_update = Some(Utc::now());
        }
    }

    async fn update_collections(&self) -> Result<()> {
        // Get collections that need indexing
        let collections: Vec<(String, String)> = sqlx::query_as(
            "SELECT address, uri FROM nft_collections WHERE total_supply IS NULL LIMIT 100"
        )
        .fetch_all(&self.db)
        .await?;

        for (address, uri) in collections {
            match self.client.fetch_metadata(&uri).await {
                Ok(metadata) => {
                    // Update collection
                    sqlx::query(
                        "UPDATE nft_collections SET name = $1, description = $2, image_url = $3 WHERE address = $4"
                    )
                    .bind(&metadata.name)
                    .bind(&metadata.description)
                    .bind(&metadata.image_url)
                    .bind(&address)
                    .execute(&self.db)
                    .await?;

                    self.state.write().collections_indexed += 1;
                }
                Err(e) => {
                    warn!("Failed to fetch collection {}: {}", address, e);
                }
            }
        }

        Ok(())
    }

    async fn fetch_metadata_batch(&self) -> Result<()> {
        // Get NFTs that need metadata
        let nfts: Vec<(String, String, String)> = sqlx::query_as(
            "SELECT token_address, token_id, uri FROM nfts WHERE metadata IS NULL AND uri IS NOT NULL LIMIT 500"
        )
        .fetch_all(&self.db)
        .await?;

        for (address, token_id, uri) in nfts {
            match self.client.fetch_metadata(&uri).await {
                Ok(metadata) => {
                    // Save metadata
                    let metadata_json = serde_json::to_string(&metadata).unwrap_or_default();

                    sqlx::query(
                        "UPDATE nfts SET name = $1, description = $2, image_url = $3, metadata = $4, updated_at = NOW() WHERE token_address = $5 AND token_id = $6"
                    )
                    .bind(&metadata.name)
                    .bind(&metadata.description)
                    .bind(&metadata.image_url)
                    .bind(&metadata_json)
                    .bind(&address)
                    .bind(&token_id)
                    .execute(&self.db)
                    .await?;

                    // Save attributes
                    for attr in &metadata.attributes {
                        // Attributes already stored as JSONB
                    }

                    self.state.write().nfts_processed += 1;
                }
                Err(e) => {
                    warn!("Failed to fetch NFT {}/{}: {}", address, token_id, e);
                }
            }
        }

        Ok(())
    }

    async fn update_floor_prices(&self) -> Result<()> {
        // Get collections with recent transfers
        let collections: Vec<String> = sqlx::query_scalar(
            "SELECT DISTINCT token_address FROM nft_transfers WHERE block_number > (SELECT COALESCE(MAX(block_number), 0) - 1000 FROM nft_transfers) LIMIT 50"
        )
        .fetch_all(&self.db)
        .await?;

        for address in collections {
            // Calculate floor price from recent sales
            let floor: Option<f64> = sqlx::query_scalar(
                r#"
                SELECT MIN(CAST(value AS NUMERIC)) 
                FROM nft_transfers 
                WHERE token_address = $1 
                AND block_number > (SELECT MAX(block_number) - 100 FROM nft_transfers)
                AND value > 0
                "#,
            )
            .bind(&address)
            .fetch_optional(&self.db)
            .await?;

            if let Some(price) = floor {
                sqlx::query(
                    "UPDATE nft_collections SET floor_price = $1, updated_at = NOW() WHERE address = $2"
                )
                .bind(price)
                .bind(&address)
                .execute(&self.db)
                .await?;
            }
        }

        Ok(())
    }

    pub async fn calculate_rarity(&self, collection: &str) -> Result<Vec<RarityResult>> {
        let nfts: Vec<NFTMetadata> = sqlx::query_as(
            "SELECT token_address, token_id, name, description, image_url, external_url, attributes, background_color, animation_url, youtube_url, updated_at FROM nfts WHERE token_address = $1"
        )
        .bind(collection)
        .fetch_all(&self.db)
        .await?;

        Ok(self.analyzer.analyze(&nfts))
    }

    pub fn get_state(&self) -> MetadataState {
        self.state.read().clone()
    }
}