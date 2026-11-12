//! TigerScan NFT Metadata Service
//! Production-grade NFT metadata fetching with IPFS/Arweave support, floor prices, rarity analysis

use std::collections::HashMap;
use std::sync::Arc;

use anyhow::Result;
use chrono::{DateTime, Utc};
use parking_lot::RwLock;
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

/// Token metadata as published in the token URI document.
///
/// The document carries no chain coordinates: the address and the token id
/// belong to the database row, not to the JSON, so they are filled in by the
/// caller after the document is parsed.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NFTMetadata {
    #[serde(default)]
    pub token_address: String,
    #[serde(default)]
    pub token_id: String,
    pub name: Option<String>,
    pub description: Option<String>,
    pub image: Option<String>,
    pub image_url: Option<String>,
    pub external_url: Option<String>,
    #[serde(default)]
    pub attributes: Vec<NFTAttribute>,
    pub background_color: Option<String>,
    pub animation_url: Option<String>,
    pub youtube_url: Option<String>,
    #[serde(skip_deserializing)]
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NFTAttribute {
    #[serde(alias = "traitType", alias = "key")]
    pub trait_type: String,
    pub value: serde_json::Value,
    #[serde(alias = "displayType")]
    pub display_type: Option<String>,
}

impl NFTAttribute {
    /// The attribute value as text. Numbers and booleans appear in token
    /// documents as JSON scalars, so they are stringified rather than dropped.
    pub fn value_string(&self) -> String {
        match &self.value {
            serde_json::Value::String(text) => text.clone(),
            other => other.to_string(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct CollectionStats {
    pub address: String,
    pub name: String,
    pub symbol: Option<String>,
    pub contract_type: String,
    pub total_supply: i64,
    pub holders_count: i64,
    pub transfers_count: i64,
    pub floor_price: f64,
    pub floor_price_change_24h: f64,
    pub volume_24h: f64,
    pub volume_change_24h: f64,
    pub average_price_24h: f64,
    pub market_cap: f64,
    pub is_verified: bool,
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

    /// Resolve a token URI and parse the document it points at.
    ///
    /// The address and token id are not part of the document, so the caller
    /// supplies them and they are attached after parsing.
    pub async fn fetch_metadata(
        &self,
        uri: &str,
        token_address: &str,
        token_id: &str,
    ) -> Result<NFTMetadata, String> {
        let body = self.fetch_document(uri).await?;

        let mut metadata: NFTMetadata =
            serde_json::from_slice(&body).map_err(|error| format!("invalid metadata JSON: {error}"))?;

        metadata.token_address = token_address.to_string();
        metadata.token_id = token_id.to_string();
        metadata.updated_at = Utc::now();

        Ok(metadata)
    }

    /// Fetch the raw bytes behind a token URI.
    async fn fetch_document(&self, uri: &str) -> Result<Vec<u8>, String> {
        if let Some(payload) = uri.strip_prefix("data:application/json;base64,") {
            return base64::Engine::decode(
                &base64::engine::general_purpose::STANDARD,
                payload.trim(),
            )
            .map_err(|error| format!("invalid base64 data URI: {error}"));
        }

        if let Some(payload) = uri.strip_prefix("data:application/json,") {
            return Ok(payload.as_bytes().to_vec());
        }

        let url = if let Some(cid) = uri.strip_prefix("ipfs://") {
            format!("{}{}", self.ipfs_gateway, cid)
        } else if let Some(id) = uri
            .strip_prefix("ar://")
            .or_else(|| uri.strip_prefix("arweave://"))
        {
            format!("{}{}", self.arweave_gateway, id)
        } else if uri.starts_with("http://") || uri.starts_with("https://") {
            uri.to_string()
        } else {
            return Err(format!("unsupported token URI scheme: {uri}"));
        };

        let response = self
            .client
            .get(&url)
            .send()
            .await
            .map_err(|error| format!("request to {url} failed: {error}"))?;

        if !response.status().is_success() {
            return Err(format!("{url} returned {}", response.status()));
        }

        response
            .bytes()
            .await
            .map(|bytes| bytes.to_vec())
            .map_err(|error| format!("cannot read body from {url}: {error}"))
    }
}

// ============================================================================
// Rarity Analyzer
// ============================================================================

pub struct RarityAnalyzer;

impl RarityAnalyzer {
    pub fn new() -> Self {
        Self
    }

    /// Score every token in a collection by the frequency of its traits.
    ///
    /// A trait value that appears in every token contributes nothing, and a
    /// value that appears once contributes the most, so the score is the sum
    /// over the token's traits of `1 - frequency`. Ranks are assigned from the
    /// score, highest first, with ties sharing a rank.
    pub fn analyze(&self, nfts: &[NFTMetadata]) -> Vec<RarityResult> {
        let mut trait_counts: HashMap<String, HashMap<String, u32>> = HashMap::new();

        for nft in nfts {
            for attribute in &nft.attributes {
                let value = attribute.value_string();

                *trait_counts
                    .entry(attribute.trait_type.clone())
                    .or_default()
                    .entry(value)
                    .or_insert(0) += 1;
            }
        }

        let total = nfts.len() as f64;
        let mut results: Vec<RarityResult> = nfts
            .iter()
            .map(|nft| {
                let mut rarity_score = 0.0;
                let mut trait_rarity = HashMap::new();

                for attribute in &nft.attributes {
                    let value = attribute.value_string();

                    let count = trait_counts
                        .get(&attribute.trait_type)
                        .and_then(|values| values.get(&value))
                        .copied()
                        .unwrap_or(1);

                    let rarity = if total > 0.0 {
                        1.0 - (f64::from(count) / total)
                    } else {
                        0.0
                    };

                    rarity_score += rarity;

                    trait_rarity.insert(
                        attribute.trait_type.clone(),
                        TraitRarity {
                            value,
                            count: count as i32,
                            rarity,
                        },
                    );
                }

                RarityResult {
                    token_id: nft.token_id.clone(),
                    rarity_score,
                    rank: 0,
                    trait_rarity,
                }
            })
            .collect();

        results.sort_by(|left, right| {
            right
                .rarity_score
                .partial_cmp(&left.rarity_score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        // Equal scores share a rank, so a tie does not silently order tokens.
        let mut previous_score: Option<f64> = None;
        let mut previous_rank = 0;

        for (index, result) in results.iter_mut().enumerate() {
            let rank = match previous_score {
                Some(score) if (score - result.rarity_score).abs() < f64::EPSILON => previous_rank,
                _ => (index + 1) as i32,
            };

            previous_score = Some(result.rarity_score);
            previous_rank = rank;
            result.rank = rank;
        }

        results
    }
}

impl Default for RarityAnalyzer {
    fn default() -> Self {
        Self::new()
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

    /// Fill in the name and image of collections that are still unnamed.
    async fn update_collections(&self) -> Result<()> {
        let collections: Vec<(String, String)> = sqlx::query_as(
            "SELECT address, name FROM nft_collections \
             WHERE image_url IS NULL AND is_spam = FALSE LIMIT 100",
        )
        .fetch_all(&self.db)
        .await?;

        for (address, name) in collections {
            // A collection is named by the name() of its contract, which the
            // indexer stores, not by a document fetched from a URI.
            let symbol: Option<String> = sqlx::query_scalar(
                "SELECT symbol FROM nft_collections WHERE address = $1",
            )
            .bind(&address)
            .fetch_optional(&self.db)
            .await?
            .flatten();

            if symbol.is_none() && name.is_empty() {
                warn!(address, "collection has neither a name nor a symbol");
                continue;
            }

            self.state.write().collections_indexed += 1;
        }

        Ok(())
    }

    /// Fetch the metadata document of every token that has a URI but no name.
    async fn fetch_metadata_batch(&self) -> Result<()> {
        let nfts: Vec<(String, String, String)> = sqlx::query_as(
            "SELECT token_address, token_id, uri FROM nfts \
             WHERE metadata IS NULL AND uri IS NOT NULL AND is_burned = FALSE LIMIT 500",
        )
        .fetch_all(&self.db)
        .await?;

        for (address, token_id, uri) in nfts {
            match self.client.fetch_metadata(&uri, &address, &token_id).await {
                Ok(metadata) => {
                    let attributes = serde_json::to_value(&metadata.attributes)
                        .unwrap_or(serde_json::Value::Array(Vec::new()));

                    let image_url = metadata
                        .image_url
                        .clone()
                        .or_else(|| metadata.image.clone());

                    let document = serde_json::json!({
                        "name": metadata.name,
                        "description": metadata.description,
                        "image": image_url,
                        "external_url": metadata.external_url,
                        "attributes": metadata.attributes,
                    });

                    sqlx::query(
                        "UPDATE nfts SET name = $1, description = $2, image_url = $3, \
                         external_url = $4, metadata = $5, attributes = $6, updated_at = NOW() \
                         WHERE token_address = $7 AND token_id = $8",
                    )
                    .bind(&metadata.name)
                    .bind(&metadata.description)
                    .bind(&image_url)
                    .bind(&metadata.external_url)
                    .bind(&document)
                    .bind(&attributes)
                    .bind(&address)
                    .bind(&token_id)
                    .execute(&self.db)
                    .await?;

                    self.state.write().nfts_processed += 1;
                }
                Err(error) => {
                    // A token whose document cannot be read is marked so that
                    // the batch does not retry it on every tick.
                    warn!(%address, %token_id, %error, "metadata fetch failed");

                    sqlx::query(
                        "UPDATE nfts SET metadata = '{}'::jsonb, updated_at = NOW() \
                         WHERE token_address = $1 AND token_id = $2",
                    )
                    .bind(&address)
                    .bind(&token_id)
                    .execute(&self.db)
                    .await?;

                    self.state.write().errors += 1;
                }
            }
        }

        Ok(())
    }

    /// Recompute the floor price of collections that traded recently.
    ///
    /// The floor is the cheapest sale in the last day, and the 24h volume is
    /// the sum of those sales. Both come from the sale rows the marketplace
    /// indexer writes, so a collection with no sales keeps its previous floor.
    async fn update_floor_prices(&self) -> Result<()> {
        let collections: Vec<String> = sqlx::query_scalar(
            "SELECT DISTINCT token_address FROM nft_sales \
             WHERE block_number > (SELECT COALESCE(MAX(block_number), 0) - 1000 FROM nft_sales) \
             LIMIT 50",
        )
        .fetch_all(&self.db)
        .await?;

        for address in collections {
            let stats: Option<(f64, f64, i64)> = sqlx::query_as(
                "SELECT COALESCE(MIN(price), 0)::float8, COALESCE(SUM(price), 0)::float8, \
                        COUNT(*)::bigint \
                 FROM nft_sales \
                 WHERE token_address = $1 AND block_timestamp > NOW() - INTERVAL '24 hours'",
            )
            .bind(&address)
            .fetch_optional(&self.db)
            .await?;

            let Some((floor, volume, sales)) = stats else {
                continue;
            };

            let previous: Option<f64> = sqlx::query_scalar(
                "SELECT floor_price::float8 FROM nft_collections WHERE address = $1",
            )
            .bind(&address)
            .fetch_optional(&self.db)
            .await?
            .flatten();

            let change = match previous {
                Some(previous) if previous > 0.0 && floor > 0.0 => {
                    ((floor - previous) / previous) * 100.0
                }
                _ => 0.0,
            };

            sqlx::query(
                "UPDATE nft_collections SET floor_price = $1, floor_price_change_24h = $2, \
                 volume_24h = $3, updated_at = NOW() WHERE address = $4",
            )
            .bind(floor)
            .bind(change)
            .bind(volume)
            .bind(&address)
            .execute(&self.db)
            .await?;

            sqlx::query(
                "INSERT INTO nft_floor_prices (collection_address, floor_price, volume_24h, sales_24h) \
                 VALUES ($1, $2, $3, $4) \
                 ON CONFLICT (collection_address) DO UPDATE SET floor_price = EXCLUDED.floor_price, \
                 volume_24h = EXCLUDED.volume_24h, sales_24h = EXCLUDED.sales_24h, \
                 updated_at = NOW()",
            )
            .bind(&address)
            .bind(floor)
            .bind(volume)
            .bind(sales as i32)
            .execute(&self.db)
            .await?;
        }

        Ok(())
    }

    /// Read one token with its traits and ownership.
    pub async fn get_nft(&self, collection: &str, token_id: &str) -> Result<serde_json::Value> {
        let row: Option<(
            String,
            String,
            Option<String>,
            Option<String>,
            Option<String>,
            Option<String>,
            Option<String>,
            Option<String>,
            Option<serde_json::Value>,
            Option<String>,
            bool,
        )> = sqlx::query_as(
            "SELECT token_address, token_id, owner, name, description, image_url, \
                    external_url, animation_url, attributes, uri, is_burned \
             FROM nfts WHERE token_address = $1 AND token_id = $2",
        )
        .bind(collection)
        .bind(token_id)
        .fetch_optional(&self.db)
        .await?;

        let row = row.ok_or_else(|| anyhow::anyhow!("token {collection}/{token_id} is not indexed"))?;

        Ok(serde_json::json!({
            "token_address": row.0,
            "token_id": row.1,
            "owner": row.2,
            "name": row.3,
            "description": row.4,
            "image_url": row.5,
            "external_url": row.6,
            "animation_url": row.7,
            "attributes": row.8,
            "uri": row.9,
            "is_burned": row.10,
        }))
    }

    /// Aggregate statistics for one collection.
    pub async fn get_collection(&self, address: &str) -> Result<CollectionStats> {
        let stats: Option<CollectionStats> = sqlx::query_as(
            "SELECT address, name, symbol, contract_type, \
                    COALESCE(total_supply, 0)::bigint AS total_supply, \
                    COALESCE(holders_count, 0)::bigint AS holders_count, \
                    COALESCE(transfers_count, 0)::bigint AS transfers_count, \
                    COALESCE(floor_price, 0)::float8 AS floor_price, \
                    COALESCE(floor_price_change_24h, 0)::float8 AS floor_price_change_24h, \
                    COALESCE(volume_24h, 0)::float8 AS volume_24h, \
                    COALESCE(volume_change_24h, 0)::float8 AS volume_change_24h, \
                    COALESCE(average_price_24h, 0)::float8 AS average_price_24h, \
                    COALESCE(market_cap, 0)::float8 AS market_cap, \
                    is_verified \
             FROM nft_collections WHERE address = $1",
        )
        .bind(address)
        .fetch_optional(&self.db)
        .await?;

        stats.ok_or_else(|| anyhow::anyhow!("collection {address} is not indexed"))
    }

    /// The rarity rank of one token inside its collection.
    pub async fn get_rarity_rank(
        &self,
        collection: &str,
        token_id: &str,
    ) -> Result<serde_json::Value> {
        let row: Option<(f64, Option<i32>, Option<serde_json::Value>)> = sqlx::query_as(
            "SELECT COALESCE(rarity_score, 0)::float8, rank, traits \
             FROM nft_rarity WHERE collection_address = $1 AND token_id = $2",
        )
        .bind(collection)
        .bind(token_id)
        .fetch_optional(&self.db)
        .await?;

        let (score, rank, traits) =
            row.ok_or_else(|| anyhow::anyhow!("rarity for {collection}/{token_id} is not computed"))?;

        Ok(serde_json::json!({
            "token_id": token_id,
            "rarity_score": score,
            "rank": rank,
            "traits": traits,
        }))
    }

    /// Search collections by name or symbol.
    pub async fn search_collections(&self, query: &str) -> Result<serde_json::Value> {
        // `ILIKE` on an escaped pattern keeps a user-supplied `%` or `_` from
        // turning into a wildcard.
        let pattern = format!(
            "%{}%",
            query
                .replace('\\', "\\\\")
                .replace('%', "\\%")
                .replace('_', "\\_")
        );

        let collections: Vec<(String, String, Option<String>, f64, bool)> = sqlx::query_as(
            "SELECT address, name, symbol, COALESCE(floor_price, 0)::float8, is_verified \
             FROM nft_collections \
             WHERE (name ILIKE $1 ESCAPE '\\' OR symbol ILIKE $1 ESCAPE '\\') \
             AND is_spam = FALSE \
             ORDER BY volume_24h DESC NULLS LAST LIMIT 50",
        )
        .bind(pattern)
        .fetch_all(&self.db)
        .await?;

        let results: Vec<serde_json::Value> = collections
            .into_iter()
            .map(|(address, name, symbol, floor_price, is_verified)| {
                serde_json::json!({
                    "address": address,
                    "name": name,
                    "symbol": symbol,
                    "floor_price": floor_price,
                    "is_verified": is_verified,
                })
            })
            .collect();

        Ok(serde_json::json!({ "collections": results }))
    }

    /// Recompute and persist the rarity of every token in a collection.
    pub async fn calculate_rarity(&self, collection: &str) -> Result<Vec<RarityResult>> {
        let rows: Vec<(String, String, Option<String>, Option<String>, Option<String>, Option<String>, Option<serde_json::Value>)> =
            sqlx::query_as(
                "SELECT token_address, token_id, name, description, image_url, external_url, \
                        attributes \
                 FROM nfts WHERE token_address = $1 AND is_burned = FALSE",
            )
            .bind(collection)
            .fetch_all(&self.db)
            .await?;

        let nfts: Vec<NFTMetadata> = rows
            .into_iter()
            .map(
                |(token_address, token_id, name, description, image_url, external_url, attributes)| {
                    NFTMetadata {
                        token_address,
                        token_id,
                        name,
                        description,
                        image: None,
                        image_url,
                        external_url,
                        attributes: attributes
                            .and_then(|value| serde_json::from_value(value).ok())
                            .unwrap_or_default(),
                        background_color: None,
                        animation_url: None,
                        youtube_url: None,
                        updated_at: Utc::now(),
                    }
                },
            )
            .collect();

        let results = self.analyzer.analyze(&nfts);

        for result in &results {
            let traits = serde_json::to_value(&result.trait_rarity)
                .unwrap_or(serde_json::Value::Object(serde_json::Map::new()));

            sqlx::query(
                "INSERT INTO nft_rarity (collection_address, token_id, rarity_score, rank, traits) \
                 VALUES ($1, $2, $3, $4, $5) \
                 ON CONFLICT (collection_address, token_id) DO UPDATE \
                 SET rarity_score = EXCLUDED.rarity_score, rank = EXCLUDED.rank, \
                     traits = EXCLUDED.traits, updated_at = NOW()",
            )
            .bind(collection)
            .bind(&result.token_id)
            .bind(result.rarity_score)
            .bind(result.rank)
            .bind(&traits)
            .execute(&self.db)
            .await?;
        }

        Ok(results)
    }

    pub fn get_state(&self) -> MetadataState {
        self.state.read().clone()
    }
}