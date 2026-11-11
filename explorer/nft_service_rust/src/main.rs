//! TigerScan NFT Service - Metadata, IPFS/Arweave, Rarity Analysis
//! Production-grade Rust service for NFT metadata indexing and rarity analysis

use std::collections::HashMap;
use std::net::{IpAddr, SocketAddr};
use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::Result;
use axum::extract::{ConnectInfo, DefaultBodyLimit, State};
use axum::http::{header, HeaderName, HeaderValue, StatusCode};
use axum::response::IntoResponse;
use axum::routing::{get, post};
use axum::{Json, Router};
use chrono::{DateTime, Utc};
use parking_lot::{Mutex, RwLock};
use serde::{Deserialize, Serialize};
use sqlx::postgres::{PgPool, PgPoolOptions};
use thiserror::Error;
use tokio::sync::mpsc;
use tokio::time::interval;
use tower_http::set_header::SetResponseHeaderLayer;
use tracing::{error, info, warn};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

/// Largest accepted request body.
const MAX_BODY_BYTES: usize = 64 * 1024;

/// Returned when a client exceeds its request budget.
const RATE_LIMITED: i32 = -32029;

// ============================================================================
// Rate limiting
// ============================================================================

struct Bucket {
    tokens: f64,
    last_refill: Instant,
}

/// Token bucket per client address.
///
/// Each client refills at `per_second` tokens per second up to `burst`. The
/// bucket map is bounded: once it passes a fixed ceiling the least recently seen
/// half is dropped, so a client cycling through source addresses cannot make the
/// map grow without limit.
pub struct RateLimiter {
    buckets: Mutex<HashMap<IpAddr, Bucket>>,
    per_second: f64,
    burst: f64,
}

impl RateLimiter {
    pub fn new(per_second: u32, burst: u32) -> Self {
        Self {
            buckets: Mutex::new(HashMap::new()),
            per_second: per_second.max(1) as f64,
            burst: burst.max(1) as f64,
        }
    }

    /// Charge one request against `ip`; `false` means the bucket is empty.
    fn allow(&self, ip: IpAddr) -> bool {
        const MAX_BUCKETS: usize = 100_000;

        let now = Instant::now();
        let mut buckets = self.buckets.lock();

        if buckets.len() > MAX_BUCKETS {
            let mut by_age: Vec<(IpAddr, Instant)> =
                buckets.iter().map(|(ip, bucket)| (*ip, bucket.last_refill)).collect();
            by_age.sort_by_key(|(_, seen)| *seen);
            for (ip, _) in by_age.into_iter().take(MAX_BUCKETS / 2) {
                buckets.remove(&ip);
            }
        }

        let bucket = buckets.entry(ip).or_insert(Bucket { tokens: self.burst, last_refill: now });

        let elapsed = now.duration_since(bucket.last_refill).as_secs_f64();
        bucket.tokens = (bucket.tokens + elapsed * self.per_second).min(self.burst);
        bucket.last_refill = now;

        if bucket.tokens < 1.0 {
            return false;
        }
        bucket.tokens -= 1.0;
        true
    }
}

/// Largest page a caller may request.
const MAX_PAGE: i64 = 1000;

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

    #[error("Configuration error: {0}")]
    Config(String),

    #[error("Invalid request: {0}")]
    InvalidRequest(String),
}

// ============================================================================
// Data Models
// ============================================================================

/// A collection row, as `nft_collections` stores it.
///
/// `floor_price`, `volume_24h` and `market_cap` are `NUMERIC(78,0)` — raw wei
/// amounts. They are carried as `String` and only converted to `f64` at the
/// point of display, so no precision is lost on the way in. `social` is JSONB
/// holding `{twitter, discord, website}`.
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct NFTCollection {
    pub address: String,
    pub name: String,
    pub symbol: Option<String>,
    pub contract_type: String,
    /// BIGINT columns; exposed as `u64` through the accessors below.
    pub total_supply: Option<i64>,
    pub holders_count: Option<i64>,
    pub transfers_count: Option<i64>,
    pub floor_price: Option<String>,
    pub volume_24h: Option<String>,
    pub market_cap: Option<String>,
    pub description: Option<String>,
    pub image_url: Option<String>,
    pub banner_url: Option<String>,
    pub external_url: Option<String>,
    pub social: Option<serde_json::Value>,
    pub is_verified: bool,
    pub is_spam: bool,
    pub spam_score: Option<i32>,
    pub updated_at: DateTime<Utc>,
}

impl NFTCollection {
    /// Supply as a non-negative count.
    pub fn supply(&self) -> Option<u64> {
        self.total_supply.and_then(|value| u64::try_from(value).ok())
    }

    /// Holders as a non-negative count.
    pub fn holders(&self) -> Option<u64> {
        self.holders_count.and_then(|value| u64::try_from(value).ok())
    }

    /// Transfers as a non-negative count.
    pub fn transfers(&self) -> Option<u64> {
        self.transfers_count.and_then(|value| u64::try_from(value).ok())
    }
}

/// An individual token row from `nfts`.
///
/// Column names follow the schema exactly: the contract is `token_address`, the
/// metadata pointer is `uri`, and the raw JSON body is `metadata`.
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct NFT {
    /// `token_id` is `NUMERIC(78,0)`; read as text so a 78-digit id survives.
    pub token_id: String,
    pub token_address: String,
    pub owner: String,
    pub uri: Option<String>,
    pub name: Option<String>,
    pub description: Option<String>,
    pub image_url: Option<String>,
    pub external_url: Option<String>,
    pub attributes: Option<serde_json::Value>,
    pub metadata: Option<serde_json::Value>,
    pub is_burned: bool,
    pub last_transfer_block: Option<i64>,
    pub last_transfer_timestamp: Option<DateTime<Utc>>,
}

/// A holder row, aggregated from `nfts` by owner.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NFTHolder {
    pub address: String,
    pub balance: u64,
    pub percentage: f64,
}

/// A transfer row from `nft_transfers`.
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct NFTTransfer {
    pub transaction_hash: String,
    pub block_number: i64,
    pub token_address: String,
    pub token_id: String,
    pub from_address: String,
    pub to_address: String,
    /// `NUMERIC(78,0)`, read as text.
    pub value: String,
    pub log_index: i32,
    pub created_at: DateTime<Utc>,
}

impl NFTTransfer {
    /// A mint is a transfer out of the zero address.
    pub fn is_mint(&self) -> bool {
        self.from_address.eq_ignore_ascii_case(ZERO_ADDRESS)
    }

    /// A burn is a transfer to the zero address.
    pub fn is_burn(&self) -> bool {
        self.to_address.eq_ignore_ascii_case(ZERO_ADDRESS)
    }
}

/// The zero address, which marks mints and burns.
const ZERO_ADDRESS: &str = "0x0000000000000000000000000000000000000000";

/// Rarity for one token, computed from `nfts.attributes`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NFTRarity {
    pub collection_address: String,
    pub token_id: String,
    pub rarity_score: f64,
    pub rarity_rank: u64,
    pub total_minted: u64,
    pub trait_rarities: Vec<TraitRarity>,
    /// When this score was computed, used for cache expiry.
    pub updated_at: DateTime<Utc>,
}

/// Rarity for a single trait.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TraitRarity {
    pub trait_type: String,
    pub value: String,
    /// Share of the collection carrying this value, in `[0, 1]`.
    pub occurrence: f64,
    /// `1 / occurrence`.
    pub rarity_score: f64,
}

/// A point on a collection's floor-price history.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FloorPrice {
    pub collection_address: String,
    pub price: f64,
    pub price_usd: f64,
    pub sales_24h: u32,
    pub holders: u32,
    pub timestamp: DateTime<Utc>,
}

/// Metadata for one token, in the shape the NFT metadata standard uses.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct NFTMetadata {
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub image: Option<String>,
    #[serde(default)]
    pub external_url: Option<String>,
    #[serde(default)]
    pub attributes: Vec<NFTAttribute>,
    #[serde(default)]
    pub background_color: Option<String>,
    #[serde(default)]
    pub animation_url: Option<String>,
    #[serde(default)]
    pub youtube_url: Option<String>,
}

/// One metadata attribute.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NFTAttribute {
    pub trait_type: String,
    pub value: serde_json::Value,
    #[serde(default)]
    pub display_type: Option<String>,
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

impl ServerConfig {
    /// The bind address, or an error naming the invalid part.
    pub fn socket_addr(&self) -> Result<SocketAddr, NFTServiceError> {
        format!("{}:{}", self.host, self.port)
            .parse()
            .map_err(|error| NFTServiceError::Config(format!("invalid bind address: {error}")))
    }
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

/// The parts of the service a request handler needs.
///
/// Handlers hold this rather than the whole `NFTService` so the shutdown
/// channel stays owned by the service that created it.
#[derive(Clone)]
pub struct ServiceHandle {
    pool: PgPool,
    metadata_cache: Arc<RwLock<MetadataCache>>,
    rarity_cache: Arc<RwLock<RarityCache>>,
    metrics: Arc<ServiceMetrics>,
}

/// State shared by the HTTP routes.
struct RpcState {
    service: ServiceHandle,
    limiter: RateLimiter,
}

/// Metadata cache with a time-to-live.
///
/// Entries older than `ttl` are dropped when the map is next touched, so the
/// cache cannot grow without bound on a long-running process.
pub struct MetadataCache {
    cache: std::collections::HashMap<String, CachedMetadata>,
    ttl: chrono::Duration,
}

impl Default for MetadataCache {
    fn default() -> Self {
        Self::new(Duration::from_secs(3600))
    }
}

impl MetadataCache {
    pub fn new(ttl: Duration) -> Self {
        Self {
            cache: std::collections::HashMap::new(),
            ttl: chrono::Duration::from_std(ttl).unwrap_or_else(|_| chrono::Duration::hours(1)),
        }
    }

    /// Store an entry, first dropping anything past its TTL.
    pub fn insert(&mut self, key: String, metadata: NFTMetadata, now: DateTime<Utc>) {
        self.evict_expired(now);
        self.cache.insert(key, CachedMetadata { metadata, fetched_at: now });
    }

    /// Read an entry, treating an expired one as absent.
    pub fn get(&mut self, key: &str, now: DateTime<Utc>) -> Option<&CachedMetadata> {
        let expired = self
            .cache
            .get(key)
            .is_some_and(|entry| now - entry.fetched_at > self.ttl);
        if expired {
            self.cache.remove(key);
            return None;
        }
        self.cache.get(key)
    }

    fn evict_expired(&mut self, now: DateTime<Utc>) {
        let ttl = self.ttl;
        self.cache.retain(|_, entry| now - entry.fetched_at <= ttl);
    }
}

#[derive(Clone)]
pub struct CachedMetadata {
    pub metadata: NFTMetadata,
    pub fetched_at: DateTime<Utc>,
}

/// Rarity cache with a time-to-live. Rarity changes only when the collection
/// changes, so the TTL is longer than the metadata one.
pub struct RarityCache {
    cache: std::collections::HashMap<String, NFTRarity>,
    ttl: chrono::Duration,
}

impl Default for RarityCache {
    fn default() -> Self {
        Self::new(Duration::from_secs(86_400))
    }
}

impl RarityCache {
    pub fn new(ttl: Duration) -> Self {
        Self {
            cache: std::collections::HashMap::new(),
            ttl: chrono::Duration::from_std(ttl).unwrap_or_else(|_| chrono::Duration::hours(24)),
        }
    }

    pub fn insert(&mut self, key: String, rarity: NFTRarity, now: DateTime<Utc>) {
        self.evict_expired(now);
        self.cache.insert(key, rarity);
    }

    pub fn get(&mut self, key: &str, now: DateTime<Utc>) -> Option<&NFTRarity> {
        let expired = self
            .cache
            .get(key)
            .is_some_and(|entry| now - entry.updated_at > self.ttl);
        if expired {
            self.cache.remove(key);
            return None;
        }
        self.cache.get(key)
    }

    fn evict_expired(&mut self, now: DateTime<Utc>) {
        let ttl = self.ttl;
        self.cache.retain(|_, entry| now - entry.updated_at <= ttl);
    }
}

#[derive(Default)]
pub struct ServiceMetrics {
    pub metadata_fetches: parking_lot::RwLock<u64>,
    pub metadata_cache_hits: parking_lot::RwLock<u64>,
    pub ipfs_fetches: parking_lot::RwLock<u64>,
    pub arweave_fetches: parking_lot::RwLock<u64>,
    pub rarity_calculations: parking_lot::RwLock<u64>,
    pub errors: parking_lot::RwLock<u64>,
}

impl ServiceMetrics {
    /// A consistent read of every counter.
    ///
    /// Each counter is read under its own lock, so the snapshot is not a single
    /// atomic instant; that is acceptable for counters that only ever increase.
    pub fn snapshot(&self) -> ServiceMetricsResponse {
        ServiceMetricsResponse {
            metadata_fetches: *self.metadata_fetches.read(),
            metadata_cache_hits: *self.metadata_cache_hits.read(),
            ipfs_fetches: *self.ipfs_fetches.read(),
            arweave_fetches: *self.arweave_fetches.read(),
            rarity_calculations: *self.rarity_calculations.read(),
            errors: *self.errors.read(),
        }
    }
}

// ============================================================================
// IPFS Client
// ============================================================================

#[derive(Clone)]
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

#[derive(Clone)]
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
                    Duration::from_secs(3600),
                    100,
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
    
    /// Fetch and store metadata for tokens that need it.
    ///
    /// A token needs a fetch when it has a URI but no stored metadata, or when
    /// its metadata is older than `refresh_after`. The work is capped at
    /// `batch` tokens per pass so a large backlog cannot hold one pass open
    /// indefinitely.
    async fn sync_metadata(
        pool: &PgPool,
        ipfs_client: &Option<IPFSClient>,
        arweave_client: &Option<ArweaveClient>,
        cache: &Arc<RwLock<MetadataCache>>,
        metrics: &Arc<ServiceMetrics>,
        refresh_after: Duration,
        batch: i64,
    ) -> Result<()> {
        let stale = Utc::now() - chrono::Duration::from_std(refresh_after).unwrap_or_default();

        let nfts: Vec<(String, String, Option<String>)> = sqlx::query_as(
            "SELECT token_address, token_id::text, uri
             FROM nfts
             WHERE uri IS NOT NULL
               AND (metadata IS NULL OR updated_at < $1)
             ORDER BY updated_at ASC NULLS FIRST
             LIMIT $2",
        )
        .bind(stale)
        .bind(batch)
        .fetch_all(pool)
        .await?;

        for (collection, token_id, token_uri) in nfts {
            let Some(url) = token_uri else { continue };

            // Standard templates: `{id}` and `{token_id}` are replaced with the
            // decimal token id, which is what ERC-721 and ERC-1155 contracts
            // expect. The id is validated as digits first so a malformed row
            // cannot inject path segments into the URL.
            if !token_id.chars().all(|c| c.is_ascii_digit()) {
                warn!("skipping token with non-numeric id: {token_id}");
                *metrics.errors.write() += 1;
                continue;
            }
            let url = url.replace("{id}", &token_id).replace("{token_id}", &token_id);

            let result =
                Self::fetch_metadata_from_url(&url, ipfs_client, arweave_client, metrics).await;

            match result {
                Ok(metadata) => {
                    let attributes = serde_json::to_value(&metadata.attributes)?;
                    // The full response is kept in `metadata` so a re-render can
                    // show fields the flattened columns do not cover.
                    let body = serde_json::to_value(&metadata)?;

                    sqlx::query(
                        "UPDATE nfts
                         SET name = COALESCE($1, name),
                             description = COALESCE($2, description),
                             image_url = COALESCE($3, image_url),
                             external_url = COALESCE($4, external_url),
                             attributes = $5,
                             metadata = $6,
                             updated_at = NOW()
                         WHERE token_address = $7 AND token_id = $8::numeric",
                    )
                    .bind(&metadata.name)
                    .bind(&metadata.description)
                    .bind(&metadata.image)
                    .bind(&metadata.external_url)
                    .bind(&attributes)
                    .bind(&body)
                    .bind(&collection)
                    .bind(&token_id)
                    .execute(pool)
                    .await?;

                    let key = format!("{collection}:{token_id}");
                    cache.write().insert(key, metadata, Utc::now());
                }
                Err(error) => {
                    *metrics.errors.write() += 1;
                    warn!("failed to fetch metadata for {collection}/{token_id}: {error}");
                }
            }

            *metrics.metadata_fetches.write() += 1;
        }

        Ok(())
    }

    /// Fetch metadata from URL (IPFS/Arweave/HTTP)
    async fn fetch_metadata_from_url(
        url: &str,
        ipfs_client: &Option<IPFSClient>,
        arweave_client: &Option<ArweaveClient>,
        metrics: &Arc<ServiceMetrics>,
    ) -> Result<NFTMetadata, NFTServiceError> {
        // Determine URL type and fetch
        let content = if url.starts_with("ipfs://") {
            let cid = url.trim_start_matches("ipfs://");
            
            if let Some(client) = ipfs_client {
                *metrics.ipfs_fetches.write() += 1;
                client.fetch(cid).await?
            } else {
                return Err(NFTServiceError::MetadataFetchFailed("IPFS not configured".to_string()));
            }
        } else if url.starts_with("ar://") {
            let tx_id = url.trim_start_matches("ar://");
            
            if let Some(client) = arweave_client {
                *metrics.arweave_fetches.write() += 1;
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
    
    /// Calculate rarity for every NFT in every indexed collection.
    ///
    /// The score is the standard one: for each trait, `1 / frequency`, summed
    /// over the token's traits. A trait shared by every token contributes 1, so
    /// a token's score rises only as its traits get scarcer.
    ///
    /// Two details matter for the number to mean anything:
    ///
    ///   * Traits are de-duplicated per token. A contract that repeats a trait
    ///     would otherwise have that token counted twice, which both inflates
    ///     its score and skews every other token's frequency.
    ///   * Ranks are real. Ties share a rank, and the next distinct score takes
    ///     the rank equal to the number of tokens above it, so rank 1 is the
    ///     rarest token rather than every token.
    async fn calculate_rarity(
        pool: &PgPool,
        cache: &Arc<RwLock<RarityCache>>,
        metrics: &Arc<ServiceMetrics>,
    ) -> Result<()> {
        let collections: Vec<String> = sqlx::query_scalar(
            "SELECT DISTINCT token_address FROM nfts WHERE attributes IS NOT NULL",
        )
        .fetch_all(pool)
        .await?;

        for collection in collections {
            // `attributes` is the JSONB array the indexer writes; `token_id` is
            // NUMERIC, cast to text so a 78-digit id is not truncated.
            let rows: Vec<(String, serde_json::Value)> = sqlx::query_as(
                "SELECT token_id::text, attributes
                 FROM nfts
                 WHERE token_address = $1 AND attributes IS NOT NULL",
            )
            .bind(&collection)
            .fetch_all(pool)
            .await?;

            // Token id -> its unique (trait_type, value) pairs.
            let mut token_traits: Vec<(String, Vec<(String, String)>)> = Vec::new();
            for (token_id, attributes) in &rows {
                let Ok(attributes) =
                    serde_json::from_value::<Vec<NFTAttribute>>(attributes.clone())
                else {
                    continue;
                };

                let mut traits: Vec<(String, String)> = Vec::new();
                for attr in &attributes {
                    let pair = (
                        attr.trait_type.clone(),
                        Self::trait_value_string(&attr.value),
                    );
                    if !traits.contains(&pair) {
                        traits.push(pair);
                    }
                }
                token_traits.push((token_id.clone(), traits));
            }

            // A token with no traits cannot be ranked against the others.
            let ranked_tokens: Vec<&(String, Vec<(String, String)>)> = token_traits
                .iter()
                .filter(|(_, traits)| !traits.is_empty())
                .collect();

            if ranked_tokens.is_empty() {
                continue;
            }

            // trait_type -> value -> how many tokens carry it.
            let mut trait_counts: HashMap<String, HashMap<String, u64>> = HashMap::new();
            for (_, traits) in &ranked_tokens {
                for (trait_type, value) in traits {
                    *trait_counts
                        .entry(trait_type.clone())
                        .or_default()
                        .entry(value.clone())
                        .or_insert(0) += 1;
                }
            }

            let total = ranked_tokens.len() as f64;

            // Score every token.
            let scored: Vec<(String, f64, Vec<TraitRarity>)> = ranked_tokens
                .iter()
                .map(|(token_id, traits)| {
                    let mut score = 0.0;
                    let mut trait_rarities = Vec::with_capacity(traits.len());

                    for (trait_type, value) in traits {
                        let count = trait_counts
                            .get(trait_type)
                            .and_then(|values| values.get(value))
                            .copied()
                            .unwrap_or(0);
                        let occurrence = count as f64 / total;
                        // A count of zero cannot happen for a trait that came
                        // from this set, but guard the division anyway.
                        let trait_rarity = if occurrence > 0.0 {
                            1.0 / occurrence
                        } else {
                            1.0
                        };
                        score += trait_rarity;

                        trait_rarities.push(TraitRarity {
                            trait_type: trait_type.clone(),
                            value: value.clone(),
                            occurrence,
                            rarity_score: trait_rarity,
                        });
                    }

                    (token_id.clone(), score, trait_rarities)
                })
                .collect();

            // Rank by score, descending. Equal scores share a rank, and the
            // next distinct score takes the count of tokens above it.
            let mut order: Vec<usize> = (0..scored.len()).collect();
            order.sort_by(|&a, &b| scored[b].1.total_cmp(&scored[a].1));

            let mut ranks = vec![0u64; scored.len()];
            for (position, &idx) in order.iter().enumerate() {
                ranks[idx] = if position > 0 && scored[order[position - 1]].1 == scored[idx].1 {
                    ranks[order[position - 1]]
                } else {
                    (position + 1) as u64
                };
            }

            for (idx, (token_id, rarity_score, trait_rarities)) in
                scored.into_iter().enumerate()
            {
                let rarity = NFTRarity {
                    collection_address: collection.clone(),
                    token_id: token_id.clone(),
                    rarity_score,
                    rarity_rank: ranks[idx],
                    total_minted: total as u64,
                    trait_rarities,
                    updated_at: Utc::now(),
                };

                // Persist so `get_rarity` can answer without recomputing the
                // whole collection. `rank` and `traits` are the schema's names.
                sqlx::query(
                    "INSERT INTO nft_rarity (collection_address, token_id, rarity_score, rank, traits, updated_at)
                     VALUES ($1, $2, $3, $4, $5, NOW())
                     ON CONFLICT (collection_address, token_id)
                     DO UPDATE SET rarity_score = EXCLUDED.rarity_score,
                                   rank = EXCLUDED.rank,
                                   traits = EXCLUDED.traits,
                                   updated_at = NOW()",
                )
                .bind(&collection)
                .bind(&token_id)
                .bind(rarity_score)
                .bind(rarity.rarity_rank as i32)
                .bind(serde_json::to_value(&rarity.trait_rarities)?)
                .execute(pool)
                .await?;

                let cache_key = format!("{}:{}", collection, token_id);
                cache.write().insert(cache_key, rarity, Utc::now());
            }

            *metrics.rarity_calculations.write() += 1;
        }

        Ok(())
    }

    /// Render a metadata attribute value as a stable string.
    ///
    /// The value is arbitrary JSON. A plain string is used as-is so the stored
    /// trait reads `Red` rather than `"Red"`; anything else keeps its compact
    /// JSON form.
    fn trait_value_string(value: &serde_json::Value) -> String {
        match value {
            serde_json::Value::String(s) => s.clone(),
            other => other.to_string(),
        }
    }

    /// Serve the JSON-RPC API until the process is asked to stop.
    ///
    /// The socket comes from `Config::server`, and every response carries the
    /// same hardening headers as the rest of the platform: no sniffing, no
    /// framing, no caching of chain-derived data.
    async fn start_server(&self) -> Result<()> {
        let addr: SocketAddr = format!("{}:{}", self.config.server.host, self.config.server.port)
            .parse()
            .map_err(|error| NFTServiceError::Config(format!("invalid server address: {error}")))?;

        let state = Arc::new(RpcState {
            service: self.clone_handle(),
            limiter: RateLimiter::new(
                self.config.server.rate_limit,
                self.config.server.rate_limit_burst,
            ),
        });

        let app = Router::new()
            .route("/", post(rpc_handler))
            .route("/health", get(|| async { (StatusCode::OK, "ok") }))
            .layer(DefaultBodyLimit::max(MAX_BODY_BYTES))
            .layer(SetResponseHeaderLayer::overriding(
                header::CONTENT_TYPE,
                HeaderValue::from_static("application/json"),
            ))
            .layer(SetResponseHeaderLayer::overriding(
                header::X_CONTENT_TYPE_OPTIONS,
                HeaderValue::from_static("nosniff"),
            ))
            .layer(SetResponseHeaderLayer::overriding(
                header::CACHE_CONTROL,
                HeaderValue::from_static("no-store"),
            ))
            .layer(SetResponseHeaderLayer::overriding(
                HeaderName::from_static("x-frame-options"),
                HeaderValue::from_static("DENY"),
            ))
            .with_state(state);

        info!("NFT API listening on {addr}");

        let listener = std::net::TcpListener::bind(addr)?;
        listener.set_nonblocking(true)?;
        let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel::<()>();

        tokio::spawn(async move {
            if let Err(error) = tokio::signal::ctrl_c().await {
                warn!("failed to listen for shutdown signal: {error}");
            }
            let _ = shutdown_tx.send(());
        });

        axum::Server::from_tcp(listener)?
            .serve(app.into_make_service_with_connect_info::<SocketAddr>())
            .with_graceful_shutdown(async {
                let _ = shutdown_rx.await;
            })
            .await?;

        Ok(())
    }

    /// A shareable handle for the request handlers.
    ///
    /// Every field is already behind an `Arc`, so this is a cheap clone rather
    /// than a second connection pool.
    fn clone_handle(&self) -> ServiceHandle {
        ServiceHandle {
            pool: self.pool.clone(),
            metadata_cache: self.metadata_cache.clone(),
            rarity_cache: self.rarity_cache.clone(),
            metrics: self.metrics.clone(),
        }
    }

    // ============================================================================
    // Public API Methods
    // ============================================================================

    /// Look up one collection by contract address.
    pub async fn get_collection(&self, address: &str) -> Result<NFTCollection, NFTServiceError> {
        Self::get_collection_with(&self.pool, address).await
    }

    /// Look up one token.
    pub async fn get_nft(&self, address: &str, token_id: &str) -> Result<NFT, NFTServiceError> {
        Self::get_nft_with(&self.pool, &self.metadata_cache, &self.metrics, address, token_id).await
    }

    /// Rarity for one token.
    pub async fn get_rarity(&self, address: &str, token_id: &str) -> Result<NFTRarity, NFTServiceError> {
        Self::get_rarity_with(&self.pool, &self.rarity_cache, &self.metrics, address, token_id).await
    }

    /// Read a rarity row that a previous pass stored.
    async fn load_stored_rarity(
        pool: &PgPool,
        address: &str,
        token_id: &str,
    ) -> Result<Option<NFTRarity>, NFTServiceError> {
        let row: Option<(f64, i32, Option<serde_json::Value>)> = sqlx::query_as(
            "SELECT rarity_score::float8, rank, traits
             FROM nft_rarity
             WHERE collection_address = $1 AND token_id = $2",
        )
        .bind(address)
        .bind(token_id)
        .fetch_optional(pool)
        .await?;

        let Some((rarity_score, rank, traits)) = row else {
            return Ok(None);
        };

        let trait_rarities: Vec<TraitRarity> = traits
            .and_then(|value| serde_json::from_value(value).ok())
            .unwrap_or_default();

        Ok(Some(NFTRarity {
            collection_address: address.to_string(),
            token_id: token_id.to_string(),
            rarity_score,
            rarity_rank: rank.max(0) as u64,
            // The stored row does not carry the minted count; `total_minted` is
            // only meaningful on a freshly computed score.
            total_minted: 0,
            trait_rarities,
            updated_at: Utc::now(),
        }))
    }

    /// Collection lookup, independent of any service instance.
    async fn get_collection_with(
        pool: &PgPool,
        address: &str,
    ) -> Result<NFTCollection, NFTServiceError> {
        let collection = sqlx::query_as::<_, NFTCollection>(
            "SELECT address, name, symbol, contract_type, total_supply, holders_count,
                    transfers_count, floor_price::text AS floor_price,
                    volume_24h::text AS volume_24h, market_cap::text AS market_cap,
                    description, image_url, banner_url, external_url, social,
                    is_verified, is_spam, spam_score, updated_at
             FROM nft_collections
             WHERE address = $1",
        )
        .bind(address)
        .fetch_optional(pool)
        .await?
        .ok_or_else(|| NFTServiceError::NotFound(address.to_string()))?;

        Ok(collection)
    }

    /// Token lookup. `token_id` is compared as a number so "007" and "7" match.
    async fn get_nft_with(
        pool: &PgPool,
        metadata_cache: &Arc<RwLock<MetadataCache>>,
        metrics: &Arc<ServiceMetrics>,
        address: &str,
        token_id: &str,
    ) -> Result<NFT, NFTServiceError> {
        let token_id = validate_token_id(token_id)?;

        if metadata_cache
            .write()
            .get(&format!("{address}:{token_id}"), Utc::now())
            .is_some()
        {
            *metrics.metadata_cache_hits.write() += 1;
        }

        sqlx::query_as::<_, NFT>(
            "SELECT token_id::text, token_address, owner, uri, name, description,
                    image_url, external_url, attributes, metadata, is_burned,
                    last_transfer_block, last_transfer_timestamp
             FROM nfts
             WHERE token_address = $1 AND token_id = $2::numeric",
        )
        .bind(address)
        .bind(&token_id)
        .fetch_optional(pool)
        .await?
        .ok_or_else(|| NFTServiceError::NotFound(format!("{address}/{token_id}")))
    }

    /// Rarity lookup: cache, then stored row, then a fresh computation.
    async fn get_rarity_with(
        pool: &PgPool,
        rarity_cache: &Arc<RwLock<RarityCache>>,
        metrics: &Arc<ServiceMetrics>,
        address: &str,
        token_id: &str,
    ) -> Result<NFTRarity, NFTServiceError> {
        let token_id = validate_token_id(token_id)?;
        let cache_key = format!("{address}:{token_id}");

        if let Some(rarity) = rarity_cache.write().get(&cache_key, Utc::now()).cloned() {
            return Ok(rarity);
        }

        if let Some(rarity) = Self::load_stored_rarity(pool, address, &token_id).await? {
            rarity_cache.write().insert(cache_key, rarity.clone(), Utc::now());
            return Ok(rarity);
        }

        Self::calculate_rarity(pool, rarity_cache, metrics)
            .await
            .map_err(|error| NFTServiceError::RarityAnalysisFailed(error.to_string()))?;

        Self::load_stored_rarity(pool, address, &token_id)
            .await?
            .ok_or_else(|| NFTServiceError::NotFound(format!("{address}/{token_id}")))
    }

    /// Holder distribution, derived from `nfts` (the table that records current
    /// ownership).
    async fn get_holders_with(
        pool: &PgPool,
        address: &str,
        limit: usize,
    ) -> Result<Vec<NFTHolder>, NFTServiceError> {
        let limit = clamp_limit(limit)?;

        let rows: Vec<(String, i64)> = sqlx::query_as(
            "SELECT owner, COUNT(*)::bigint
             FROM nfts
             WHERE token_address = $1
             GROUP BY owner
             ORDER BY COUNT(*) DESC, owner ASC
             LIMIT $2",
        )
        .bind(address)
        .bind(limit)
        .fetch_all(pool)
        .await?;

        let total: i64 =
            sqlx::query_scalar("SELECT COUNT(*)::bigint FROM nfts WHERE token_address = $1")
                .bind(address)
                .fetch_one(pool)
                .await?;

        let total = total.max(1) as f64;
        Ok(rows
            .into_iter()
            .map(|(address, balance)| NFTHolder {
                address,
                balance: balance.max(0) as u64,
                percentage: (balance as f64 / total) * 100.0,
            })
            .collect())
    }

    /// Transfers of a collection above `from_block`, newest first.
    async fn get_transfers_with(
        pool: &PgPool,
        address: &str,
        from_block: u64,
        limit: usize,
    ) -> Result<Vec<NFTTransfer>, NFTServiceError> {
        let limit = clamp_limit(limit)?;

        sqlx::query_as::<_, NFTTransfer>(
            "SELECT transaction_hash, block_number, token_address, token_id::text, from_address,
                    to_address, value::text, log_index, created_at
             FROM nft_transfers
             WHERE token_address = $1 AND block_number > $2
             ORDER BY block_number DESC, log_index DESC
             LIMIT $3",
        )
        .bind(address)
        .bind(from_block as i64)
        .bind(limit)
        .fetch_all(pool)
        .await
        .map_err(Into::into)
    }

    /// Floor price lookup.
    async fn get_floor_price_with(
        pool: &PgPool,
        address: &str,
    ) -> Result<FloorPrice, NFTServiceError> {
        let row: Option<(String, String, i32, i32, DateTime<Utc>)> = sqlx::query_as(
            "SELECT floor_price::text, floor_price_usd::text, sales_24h, holders, updated_at
             FROM nft_floor_prices
             WHERE collection_address = $1",
        )
        .bind(address)
        .fetch_optional(pool)
        .await?;

        let (price, price_usd, sales_24h, holders, timestamp) = row
            .ok_or_else(|| NFTServiceError::NotFound(format!("floor price for {address}")))?;

        Ok(FloorPrice {
            collection_address: address.to_string(),
            price: wei_to_float(&price),
            price_usd: price_usd.parse().unwrap_or(0.0),
            sales_24h: sales_24h.max(0) as u32,
            holders: holders.max(0) as u32,
            timestamp,
        })
    }

    /// Holders of a collection, largest balance first.
    pub async fn get_holders(
        &self,
        address: &str,
        limit: usize,
    ) -> Result<Vec<NFTHolder>, NFTServiceError> {
        Self::get_holders_with(&self.pool, address, limit).await
    }

    /// Transfers of a collection above `from_block`, newest first.
    pub async fn get_transfers(
        &self,
        address: &str,
        from_block: u64,
        limit: usize,
    ) -> Result<Vec<NFTTransfer>, NFTServiceError> {
        Self::get_transfers_with(&self.pool, address, from_block, limit).await
    }

    /// The most recent transfers for a collection, as activity entries.
    pub async fn get_activity(
        &self,
        address: &str,
        limit: usize,
    ) -> Result<Vec<NFTTransfer>, NFTServiceError> {
        Self::get_transfers_with(&self.pool, address, 0, limit).await
    }

    /// Current floor price for a collection, if one has been recorded.
    pub async fn get_floor_price(&self, address: &str) -> Result<FloorPrice, NFTServiceError> {
        Self::get_floor_price_with(&self.pool, address).await
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
            host: std::env::var("SERVER_HOST").unwrap_or_else(|_| "0.0.0.0".to_string()),
            port: std::env::var("SERVER_PORT")
                .ok()
                .and_then(|value| value.parse().ok())
                .unwrap_or(8081),
            rate_limit: std::env::var("RATE_LIMIT_PER_SECOND")
                .ok()
                .and_then(|value| value.parse().ok())
                .unwrap_or(1000),
            rate_limit_burst: std::env::var("RATE_LIMIT_BURST")
                .ok()
                .and_then(|value| value.parse().ok())
                .unwrap_or(2000),
        },
    };
    
    // Create and run service
    let service = NFTService::new(config).await?;
    service.run().await?;
    
    Ok(())
}
// ============================================================================
// JSON-RPC 2.0 API
// ============================================================================

/// JSON-RPC parse error.
const PARSE_ERROR: i32 = -32700;
/// JSON-RPC invalid request.
const INVALID_REQUEST: i32 = -32600;
/// JSON-RPC method not found.
const METHOD_NOT_FOUND: i32 = -32601;
/// JSON-RPC invalid params.
const INVALID_PARAMS: i32 = -32602;
/// JSON-RPC server error.
const SERVER_ERROR: i32 = -32000;

#[derive(Debug, Clone, Deserialize)]
pub struct RpcRequest {
    pub jsonrpc: String,
    pub method: String,
    #[serde(default)]
    pub params: Option<serde_json::Value>,
    #[serde(default)]
    pub id: serde_json::Value,
}

#[derive(Debug, Clone, Serialize)]
pub struct RpcResponse {
    pub jsonrpc: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<RpcError>,
    pub id: serde_json::Value,
}

#[derive(Debug, Clone, Serialize)]
pub struct RpcError {
    pub code: i32,
    pub message: String,
}

impl RpcResponse {
    fn ok(id: serde_json::Value, result: serde_json::Value) -> Self {
        Self { jsonrpc: "2.0", result: Some(result), error: None, id }
    }

    fn err(id: serde_json::Value, code: i32, message: impl Into<String>) -> Self {
        Self {
            jsonrpc: "2.0",
            result: None,
            error: Some(RpcError { code, message: message.into() }),
            id,
        }
    }
}

impl ServiceHandle {
    /// Dispatch one call. Every branch validates its parameters before it
    /// touches the database.
    async fn dispatch(&self, request: &RpcRequest) -> RpcResponse {
        let id = request.id.clone();

        if request.jsonrpc != "2.0" {
            return RpcResponse::err(id, INVALID_REQUEST, "jsonrpc must be the string 2.0");
        }

        let result = match request.method.as_str() {
            "nft_getCollection" => self.rpc_collection(request.params.as_ref()).await,
            "nft_getToken" => self.rpc_token(request.params.as_ref()).await,
            "nft_getRarity" => self.rpc_rarity(request.params.as_ref()).await,
            "nft_getHolders" => self.rpc_holders(request.params.as_ref()).await,
            "nft_getTransfers" => self.rpc_transfers(request.params.as_ref()).await,
            "nft_getFloorPrice" => self.rpc_floor(request.params.as_ref()).await,
            "nft_getMetrics" => Ok(serde_json::to_value(self.metrics.snapshot()).unwrap_or_default()),
            other => {
                return RpcResponse::err(id, METHOD_NOT_FOUND, format!("unknown method: {other}"))
            }
        };

        match result {
            Ok(value) => RpcResponse::ok(id, value),
            Err(error) => RpcResponse::err(id, error.code, error.message),
        }
    }

    async fn rpc_collection(
        &self,
        params: Option<&serde_json::Value>,
    ) -> Result<serde_json::Value, RpcError> {
        let address = required_address(params)?;
        let collection = self.get_collection(&address).await.map_err(map_error)?;
        Ok(serde_json::to_value(collection).unwrap_or_default())
    }

    async fn rpc_token(
        &self,
        params: Option<&serde_json::Value>,
    ) -> Result<serde_json::Value, RpcError> {
        let address = required_address(params)?;
        let token_id = required_token_id(params)?;
        let nft = self.get_nft(&address, &token_id).await.map_err(map_error)?;
        Ok(serde_json::to_value(nft).unwrap_or_default())
    }

    async fn rpc_rarity(
        &self,
        params: Option<&serde_json::Value>,
    ) -> Result<serde_json::Value, RpcError> {
        let address = required_address(params)?;
        let token_id = required_token_id(params)?;
        let rarity = self.get_rarity(&address, &token_id).await.map_err(map_error)?;
        Ok(serde_json::to_value(rarity).unwrap_or_default())
    }

    async fn rpc_holders(
        &self,
        params: Option<&serde_json::Value>,
    ) -> Result<serde_json::Value, RpcError> {
        let address = required_address(params)?;
        let limit = optional_limit(params)?;
        let holders = self.get_holders(&address, limit).await.map_err(map_error)?;
        Ok(serde_json::json!({ "collection": address, "holders": holders }))
    }

    async fn rpc_transfers(
        &self,
        params: Option<&serde_json::Value>,
    ) -> Result<serde_json::Value, RpcError> {
        let address = required_address(params)?;
        let limit = optional_limit(params)?;
        let from_block = params
            .and_then(|p| p.get("from_block"))
            .and_then(|value| value.as_u64())
            .unwrap_or(0);
        let transfers = self
            .get_transfers(&address, from_block, limit)
            .await
            .map_err(map_error)?;
        Ok(serde_json::json!({ "collection": address, "transfers": transfers }))
    }

    async fn rpc_floor(
        &self,
        params: Option<&serde_json::Value>,
    ) -> Result<serde_json::Value, RpcError> {
        let address = required_address(params)?;
        let floor = self.get_floor_price(&address).await.map_err(map_error)?;
        Ok(serde_json::to_value(floor).unwrap_or_default())
    }

    // ---- thin forwards to the service, so each query has one implementation ----

    async fn get_collection(&self, address: &str) -> Result<NFTCollection, NFTServiceError> {
        NFTService::get_collection_with(&self.pool, address).await
    }

    async fn get_nft(&self, address: &str, token_id: &str) -> Result<NFT, NFTServiceError> {
        NFTService::get_nft_with(&self.pool, &self.metadata_cache, &self.metrics, address, token_id)
            .await
    }

    async fn get_rarity(&self, address: &str, token_id: &str) -> Result<NFTRarity, NFTServiceError> {
        NFTService::get_rarity_with(&self.pool, &self.rarity_cache, &self.metrics, address, token_id)
            .await
    }

    async fn get_holders(
        &self,
        address: &str,
        limit: usize,
    ) -> Result<Vec<NFTHolder>, NFTServiceError> {
        NFTService::get_holders_with(&self.pool, address, limit).await
    }

    async fn get_transfers(
        &self,
        address: &str,
        from_block: u64,
        limit: usize,
    ) -> Result<Vec<NFTTransfer>, NFTServiceError> {
        NFTService::get_transfers_with(&self.pool, address, from_block, limit).await
    }

    async fn get_floor_price(&self, address: &str) -> Result<FloorPrice, NFTServiceError> {
        NFTService::get_floor_price_with(&self.pool, address).await
    }
}

/// Translate a service error into a JSON-RPC error.
fn map_error(error: NFTServiceError) -> RpcError {
    let code = match &error {
        NFTServiceError::InvalidRequest(_) => INVALID_PARAMS,
        _ => SERVER_ERROR,
    };
    RpcError { code, message: error.to_string() }
}

/// Read a required contract address.
///
/// The address is validated before it reaches a query: exactly `0x` followed by
/// 40 hex digits, lowercased so two spellings of one address match.
fn required_address(params: Option<&serde_json::Value>) -> Result<String, RpcError> {
    let raw = params
        .and_then(|p| p.get("address"))
        .and_then(|value| value.as_str())
        .ok_or_else(|| RpcError {
            code: INVALID_PARAMS,
            message: "missing parameter: address".to_string(),
        })?;

    validate_address(raw).map_err(|message| RpcError { code: INVALID_PARAMS, message })
}

/// Validate and normalise a contract address.
fn validate_address(raw: &str) -> Result<String, String> {
    let trimmed = raw.trim();
    let body = trimmed
        .strip_prefix("0x")
        .or_else(|| trimmed.strip_prefix("0X"))
        .ok_or_else(|| format!("address must start with 0x: {raw}"))?;

    if body.len() != 40 || !body.chars().all(|c| c.is_ascii_hexdigit()) {
        return Err(format!("address must be 0x followed by 40 hex digits: {raw}"));
    }

    Ok(format!("0x{}", body.to_ascii_lowercase()))
}

/// Read a required token id and normalise it.
fn required_token_id(params: Option<&serde_json::Value>) -> Result<String, RpcError> {
    let raw = params
        .and_then(|p| p.get("token_id"))
        .and_then(|value| value.as_str())
        .ok_or_else(|| RpcError {
            code: INVALID_PARAMS,
            message: "missing parameter: token_id".to_string(),
        })?;

    validate_token_id(raw).map_err(|error| RpcError {
        code: INVALID_PARAMS,
        message: error.to_string(),
    })
}

/// Accept a decimal or `0x`-prefixed token id and return it in decimal.
///
/// Leading zeros are removed so `007` and `7` name the same token. The hex path
/// converts without a bignum, so a 78-digit id is handled exactly.
fn validate_token_id(raw: &str) -> Result<String, NFTServiceError> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Err(NFTServiceError::InvalidRequest("token id is empty".to_string()));
    }

    let decimal = if let Some(hex) = trimmed.strip_prefix("0x").or_else(|| trimmed.strip_prefix("0X")) {
        if hex.is_empty() || !hex.chars().all(|c| c.is_ascii_hexdigit()) {
            return Err(NFTServiceError::InvalidRequest(format!("invalid token id: {raw}")));
        }

        let digits = hex.trim_start_matches('0');
        if digits.is_empty() {
            return Ok("0".to_string());
        }

        let mut value: Vec<u8> = vec![0];
        for ch in digits.chars() {
            let mut carry = ch.to_digit(16).unwrap_or(0) as u32;
            for byte in value.iter_mut().rev() {
                let next = (*byte as u32) * 16 + carry;
                *byte = (next % 10) as u8;
                carry = next / 10;
            }
            while carry > 0 {
                value.insert(0, (carry % 10) as u8);
                carry /= 10;
            }
        }
        value.iter().map(|digit| char::from(b'0' + digit)).collect::<String>()
    } else {
        if !trimmed.chars().all(|c| c.is_ascii_digit()) {
            return Err(NFTServiceError::InvalidRequest(format!("invalid token id: {raw}")));
        }
        trimmed.to_string()
    };

    let normalised = decimal.trim_start_matches('0');
    Ok(if normalised.is_empty() { "0".to_string() } else { normalised.to_string() })
}

/// Read an optional `limit`, clamped to a sane page size.
fn optional_limit(params: Option<&serde_json::Value>) -> Result<usize, RpcError> {
    match params.and_then(|p| p.get("limit")).and_then(|value| value.as_i64()) {
        None => Ok(100),
        Some(value) if value <= 0 => Err(RpcError {
            code: INVALID_PARAMS,
            message: "limit must be positive".to_string(),
        }),
        Some(value) => Ok(value.min(MAX_PAGE) as usize),
    }
}

/// Clamp a caller-supplied page size for the direct API surface.
fn clamp_limit(limit: usize) -> Result<i64, NFTServiceError> {
    if limit == 0 {
        return Err(NFTServiceError::InvalidRequest("limit must be positive".to_string()));
    }
    Ok(limit.min(MAX_PAGE as usize) as i64)
}

/// Convert a wei string to a float. Display only.
fn wei_to_float(wei: &str) -> f64 {
    wei.trim().parse::<f64>().unwrap_or(0.0) / 1e18
}

/// HTTP entry point for the JSON-RPC API.
async fn rpc_handler(
    State(state): State<Arc<RpcState>>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    body: String,
) -> impl IntoResponse {
    if !state.limiter.allow(peer.ip()) {
        let response =
            RpcResponse::err(serde_json::Value::Null, RATE_LIMITED, "rate limit exceeded");
        return (StatusCode::TOO_MANY_REQUESTS, Json(response));
    }

    let request: RpcRequest = match serde_json::from_str(&body) {
        Ok(request) => request,
        Err(error) => {
            let response = RpcResponse::err(
                serde_json::Value::Null,
                PARSE_ERROR,
                format!("invalid JSON: {error}"),
            );
            return (StatusCode::BAD_REQUEST, Json(response));
        }
    };

    (StatusCode::OK, Json(state.service.dispatch(&request).await))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn addr(byte: char) -> String {
        format!("0x{}", byte.to_string().repeat(40))
    }

    #[test]
    fn address_is_lowercased_and_checksummed() {
        let mixed = format!("0x{}", "AbCdEf".repeat(6) + "AbCd");
        let normalised = validate_address(&mixed).expect("valid address");
        assert_eq!(normalised, mixed.to_lowercase());
        assert_eq!(normalised.len(), 42);
    }

    #[test]
    fn address_requires_exact_length_and_hex() {
        // Too short, too long, non-hex, and a missing prefix are all rejected.
        for bad in [
            "0x1234",
            &format!("0x{}", "a".repeat(41)),
            &format!("0x{}", "z".repeat(40)),
            &"a".repeat(40),
        ] {
            assert!(validate_address(bad).is_err(), "accepted invalid address {bad}");
        }
    }

    #[test]
    fn address_rejects_sql_metacharacters() {
        // The validator must refuse anything that is not plain hex, so a crafted
        // address can never reach a query as SQL.
        for bad in [
            "0x1; DROP TABLE nfts;--",
            "0x' OR '1'='1",
            "0x00\" UNION SELECT 1",
            "0x0000000000000000000000000000000000000000'",
        ] {
            assert!(validate_address(bad).is_err(), "accepted injection shape {bad}");
        }
    }

    #[test]
    fn token_id_accepts_decimal_and_hex() {
        assert_eq!(validate_token_id("7").unwrap(), "7");
        assert_eq!(validate_token_id("007").unwrap(), "7");
        assert_eq!(validate_token_id("0x1f").unwrap(), "31");
    }

    #[test]
    fn token_id_survives_a_78_digit_value() {
        // uint256 tops out at 78 decimal digits; it must round-trip exactly.
        let huge = "1".to_string() + &"0".repeat(77);
        assert_eq!(validate_token_id(&huge).unwrap(), huge);
    }

    #[test]
    fn token_id_rejects_junk() {
        for bad in ["", "abc", "0xzz", "1;2", "-1"] {
            assert!(validate_token_id(bad).is_err(), "accepted invalid token id {bad}");
        }
    }

    #[test]
    fn trait_value_string_unquotes_plain_strings_only() {
        assert_eq!(
            NFTService::trait_value_string(&serde_json::json!("Red")),
            "Red"
        );
        assert_eq!(NFTService::trait_value_string(&serde_json::json!(42)), "42");
        assert_eq!(
            NFTService::trait_value_string(&serde_json::json!(true)),
            "true"
        );
    }

    #[test]
    fn metadata_cache_expires_entries() {
        let mut cache = MetadataCache::new(Duration::from_secs(60));
        let now = Utc::now();
        let metadata = NFTMetadata {
            name: Some("Ape".to_string()),
            description: None,
            image: None,
            animation_url: None,
            external_url: None,
            background_color: None,
            youtube_url: None,
            attributes: Vec::new(),
        };

        cache.insert("c:1".to_string(), metadata.clone(), now);
        assert!(cache.get("c:1", now).is_some());

        // Still valid one second before expiry, gone one second after.
        let just_before = now + chrono::Duration::seconds(59);
        assert!(cache.get("c:1", just_before).is_some());
        let after = now + chrono::Duration::seconds(61);
        assert!(cache.get("c:1", after).is_none());
    }

    #[test]
    fn metadata_cache_evicts_on_insert() {
        let mut cache = MetadataCache::new(Duration::from_secs(10));
        let now = Utc::now();
        let metadata = NFTMetadata {
            name: None,
            description: None,
            image: None,
            animation_url: None,
            external_url: None,
            background_color: None,
            youtube_url: None,
            attributes: Vec::new(),
        };

        cache.insert("old".to_string(), metadata.clone(), now);
        // An insert well past the TTL drops the stale key as a side effect.
        cache.insert("new".to_string(), metadata, now + chrono::Duration::seconds(30));

        assert!(cache.get("old", now + chrono::Duration::seconds(30)).is_none());
        assert!(cache.get("new", now + chrono::Duration::seconds(30)).is_some());
    }

    #[test]
    fn rate_limiter_allows_burst_then_blocks() {
        let limiter = RateLimiter::new(1, 5);
        let ip: IpAddr = "203.0.113.7".parse().unwrap();

        // The burst is available immediately.
        for i in 0..5 {
            assert!(limiter.allow(ip), "burst request {i} should pass");
        }
        // The next request has no token left.
        assert!(!limiter.allow(ip));
    }

    #[test]
    fn rate_limiter_tracks_clients_independently() {
        let limiter = RateLimiter::new(1, 2);
        let a: IpAddr = "203.0.113.1".parse().unwrap();
        let b: IpAddr = "203.0.113.2".parse().unwrap();

        assert!(limiter.allow(a));
        assert!(limiter.allow(a));
        assert!(!limiter.allow(a), "a is exhausted");

        // b has its own bucket and is unaffected.
        assert!(limiter.allow(b));
        assert!(limiter.allow(b));
        assert!(!limiter.allow(b));
    }

    #[test]
    fn transfer_classifies_mint_and_burn() {
        let base = NFTTransfer {
            transaction_hash: "0x0".to_string(),
            block_number: 1,
            token_address: addr('1'),
            token_id: "1".to_string(),
            from_address: ZERO_ADDRESS.to_string(),
            to_address: addr('2'),
            value: "1".to_string(),
            log_index: 0,
            created_at: Utc::now(),
        };
        assert!(base.is_mint());
        assert!(!base.is_burn());

        let burn = NFTTransfer {
            from_address: addr('2'),
            to_address: ZERO_ADDRESS.to_string(),
            ..base.clone()
        };
        assert!(!burn.is_mint());
        assert!(burn.is_burn());

        let plain = NFTTransfer {
            from_address: addr('2'),
            ..base
        };
        assert!(!plain.is_mint());
        assert!(!plain.is_burn());
    }
}
