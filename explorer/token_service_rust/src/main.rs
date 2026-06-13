//! TigerScan Token Service - High Performance Token Price Feed & Holder Tracking
//! Production-grade Rust service for token prices, holder distribution, and DEX tracking

use std::sync::Arc;
use std::time::Duration;

use anyhow::Result;
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
pub enum TokenServiceError {
    #[error("Database error: {0}")]
    Database(#[from] sqlx::Error),
    
    #[error("HTTP request error: {0}")]
    Http(#[from] reqwest::Error),
    
    #[error("Token not found: {0}")]
    NotFound(String),
    
    #[error("Invalid data: {0}")]
    InvalidData(String),
    
    #[error("Price fetch failed: {0}")]
    PriceFetchFailed(String),
}

// ============================================================================
// Data Models
// ============================================================================

/// Token information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Token {
    pub address: String,
    pub name: String,
    pub symbol: String,
    pub decimals: u8,
    pub total_supply: String,
    pub circulating_supply: Option<String>,
    pub holders: u64,
    pub transfers_24h: u64,
    pub price_usd: f64,
    pub price_change_24h: f64,
    pub market_cap: f64,
    pub volume_24h: f64,
    pub liquidity: Option<f64>,
    pub last_updated: DateTime<Utc>,
}

/// Token holder information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenHolder {
    pub address: String,
    pub balance: String,
    pub balance_usd: f64,
    pub percentage: f64,
    pub rank: u64,
    pub first_seen: DateTime<Utc>,
    pub last_updated: DateTime<Utc>,
}

/// Token transfer
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenTransfer {
    pub hash: String,
    pub block_number: u64,
    pub timestamp: DateTime<Utc>,
    pub from: String,
    pub to: String,
    pub value: String,
    pub value_usd: f64,
    pub token_id: Option<String>,
}

/// DEX pair information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DEXPair {
    pub pair_address: String,
    pub dex_name: String,
    pub token0: String,
    pub token1: String,
    pub reserve0: String,
    pub reserve1: String,
    pub total_supply: String,
    pub volume_24h: f64,
    pub volume_7d: f64,
    pub fees_24h: f64,
    pub liquidity: f64,
    pub price: f64,
    pub price_usd: f64,
}

/// Price candle for charts
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PriceCandle {
    pub timestamp: DateTime<Utc>,
    pub open: f64,
    pub high: f64,
    pub low: f64,
    pub close: f64,
    pub volume: f64,
}

/// Price history entry
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PriceHistory {
    pub token_address: String,
    pub timestamp: DateTime<Utc>,
    pub price_usd: f64,
    pub volume_24h: f64,
    pub liquidity: f64,
}

// ============================================================================
// Configuration
// ============================================================================

#[derive(Debug, Clone, Deserialize)]
pub struct Config {
    /// Database settings
    pub database: DatabaseConfig,
    
    /// Price feed settings
    pub price_feed: PriceFeedConfig,
    
    /// Server settings
    pub server: ServerConfig,
    
    /// Security settings
    pub security: SecurityConfig,
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
pub struct PriceFeedConfig {
    /// Primary price API (e.g., CoinGecko)
    pub primary_api_url: String,
    /// Secondary price API
    pub secondary_api_url: Option<String>,
    /// API key for primary
    pub primary_api_key: Option<String>,
    /// Update interval in seconds
    pub update_interval: u64,
    /// Cache TTL in seconds
    pub cache_ttl: u64,
    /// Minimum liquidity to include in price
    pub min_liquidity: f64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ServerConfig {
    pub host: String,
    pub port: u16,
    pub rate_limit: u32,
    pub rate_limit_burst: u32,
}

#[derive(Debug, Clone, Deserialize)]
pub struct SecurityConfig {
    /// Enable price signing
    pub enable_signing: bool,
    /// Signing key path
    pub signing_key: Option<String>,
    /// Allowed API keys
    pub allowed_api_keys: Vec<String>,
}

// ============================================================================
// Token Service
// ============================================================================

pub struct TokenService {
    pool: PgPool,
    config: Config,
    price_cache: Arc<RwLock<PriceCache>>,
    holder_cache: Arc<RwLock<HolderCache>>,
    metrics: Arc<ServiceMetrics>,
    shutdown_tx: mpsc::Sender<()>,
}

#[derive(Default)]
pub struct PriceCache {
    prices: std::collections::HashMap<String, CachedPrice>,
    last_update: Option<DateTime<Utc>>,
}

#[derive(Clone)]
pub struct CachedPrice {
    pub price: f64,
    pub change_24h: f64,
    pub volume: f64,
    pub liquidity: f64,
    pub updated_at: DateTime<Utc>,
}

#[derive(Default)]
pub struct HolderCache {
    holders: std::collections::HashMap<String, Vec<TokenHolder>>,
    last_update: Option<DateTime<Utc>>,
}

#[derive(Default, Clone)]
pub struct ServiceMetrics {
    pub requests_total: parking_lot::RwLock<u64>,
    pub requests_success: parking_lot::RwLock<u64>,
    pub requests_failed: parking_lot::RwLock<u64>,
    pub cache_hits: parking_lot::RwLock<u64>,
    pub cache_misses: parking_lot::RwLock<u64>,
    pub price_updates: parking_lot::RwLock<u64>,
    pub holder_updates: parking_lot::RwLock<u64>,
}

impl TokenService {
    pub async fn new(config: Config) -> Result<Self, TokenServiceError> {
        let pool = PgPoolOptions::new()
            .max_connections(config.database.max_connections)
            .min_connections(config.database.min_connections)
            .acquire_timeout(Duration::from_secs(30))
            .connect(&config.database.connection_string())
            .await?;
        
        let (shutdown_tx, _) = mpsc::channel::<()>(1);
        
        Ok(Self {
            pool,
            config: config.clone(),
            price_cache: Arc::new(RwLock::new(PriceCache::default())),
            holder_cache: Arc::new(RwLock::new(HolderCache::default())),
            metrics: Arc::new(ServiceMetrics::default()),
            shutdown_tx,
        })
    }
    
    /// Start the token service
    pub async fn run(&self) -> Result<()> {
        info!("Starting token service");
        
        let price_feed = self.config.price_feed.clone();
        let pool = self.pool.clone();
        let price_cache = self.price_cache.clone();
        let metrics = self.metrics.clone();
        
        // Start price update task
        tokio::spawn(async move {
            let mut interval = interval(Duration::from_secs(price_feed.update_interval));
            
            loop {
                interval.tick().await;
                
                match Self::update_prices(&pool, &price_feed, &price_cache, &metrics).await {
                    Ok(_) => {
                        info!("Prices updated successfully");
                    }
                    Err(e) => {
                        error!("Failed to update prices: {}", e);
                    }
                }
            }
        });
        
        // Start holder update task
        let holder_pool = self.pool.clone();
        let holder_cache = self.holder_cache.clone();
        let holder_metrics = self.metrics.clone();
        
        tokio::spawn(async move {
            let mut interval = interval(Duration::from_secs(300)); // Every 5 minutes
            
            loop {
                interval.tick().await;
                
                match Self::update_holders(&holder_pool, &holder_cache, &holder_metrics).await {
                    Ok(_) => {
                        info!("Holders updated successfully");
                    }
                    Err(e) => {
                        error!("Failed to update holders: {}", e);
                    }
                }
            }
        });
        
        // Start API server
        self.start_server().await?;
        
        Ok(())
    }
    
    /// Update token prices from price feed
    async fn update_prices(
        pool: &PgPool,
        config: &PriceFeedConfig,
        cache: &Arc<RwLock<PriceCache>>,
        metrics: &Arc<ServiceMetrics>,
    ) -> Result<()> {
        let client = reqwest::Client::new();
        
        // Fetch prices from primary API
        let url = format!("{}/simple/price", config.primary_api_url);
        
        let response = client.get(&url)
            .query(&[
                ("vs_currency", "usd"),
                ("ids", "ethereum,binancecoin,tether,usd-coin"),
            ])
            .send()
            .await?;
        
        if !response.status().is_success() {
            return Err(TokenServiceError::PriceFetchFailed(
                format!("API returned status: {}", response.status())
            ));
        }
        
        let prices: serde_json::Value = response.json().await?;
        
        // Update cache
        {
            let mut cache = cache.write();
            cache.last_update = Some(Utc::now());
            
            if let Some(prices_obj) = prices.as_object() {
                for (id, price_data) in prices_obj {
                    if let (Some(price), Some(change)) = (
                        price_data.get("usd").and_then(|v| v.as_f64()),
                        price_data.get("usd_24h_change").and_then(|v| v.as_f64()),
                    ) {
                        cache.prices.insert(
                            id.clone(),
                            CachedPrice {
                                price,
                                change_24h: change,
                                volume: price_data.get("usd_volume_24h")
                                    .and_then(|v| v.as_f64())
                                    .unwrap_or(0.0),
                                liquidity: price_data.get("usd_liquidity")
                                    .and_then(|v| v.as_f64())
                                    .unwrap_or(0.0),
                                updated_at: Utc::now(),
                            },
                        );
                    }
                }
            }
        }
        
        // Update metrics
        metrics.price_updates.write().inc();
        
        // Save to database
        let cache = cache.read();
        if let Some(last_update) = cache.last_update {
            for (symbol, price) in &cache.prices {
                sqlx::query(
                    "INSERT INTO token_prices (symbol, price_usd, price_change_24h, volume_24h, liquidity, updated_at)
                     VALUES ($1, $2, $3, $4, $5, $6)
                     ON CONFLICT (symbol) DO UPDATE SET
                        price_usd = EXCLUDED.price_usd,
                        price_change_24h = EXCLUDED.price_change_24h,
                        volume_24h = EXCLUDED.volume_24h,
                        liquidity = EXCLUDED.liquidity,
                        updated_at = EXCLUDED.updated_at"
                )
                .bind(symbol)
                .bind(price.price)
                .bind(price.change_24h)
                .bind(price.volume)
                .bind(price.liquidity)
                .bind(price.updated_at)
                .execute(pool)
                .await?;
            }
        }
        
        Ok(())
    }
    
    /// Update token holders
    async fn update_holders(
        pool: &PgPool,
        cache: &Arc<RwLock<HolderCache>>,
        metrics: &Arc<ServiceMetrics>,
    ) -> Result<()> {
        // Get all token addresses
        let tokens: Vec<(String, String)> = sqlx::query(
            "SELECT address, price_usd FROM tokens WHERE verified = true"
        )
        .fetch_all(pool)
        .await?
        .into_iter()
        .map(|row| {
            (
                row.get::<String, "_("0"),
                row.get::<String, "_("0"),
            )
        })
        .collect();
        
        // Update holders for each token
        for (address, price) in tokens {
            let holders = Self::fetch_holders(pool, &address, &price).await?;
            
            let mut cache = cache.write();
            cache.holders.insert(address, holders);
        }
        
        metrics.holder_updates.write().inc();
        
        Ok(())
    }
    
    /// Fetch holders for a token
    async fn fetch_holders(
        pool: &PgPool,
        address: &str,
        price: &str,
    ) -> Result<Vec<TokenHolder>> {
        let holders: Vec<TokenHolder> = sqlx::query_as(
            "SELECT address, balance, balance_usd, percentage, rank, first_seen, last_updated
             FROM token_holders
             WHERE token_address = $1
             ORDER BY balance_usd DESC
             LIMIT 100"
        )
        .bind(address)
        .fetch_all(pool)
        .await?
        .into_iter()
        .map(|row| TokenHolder {
            address: row.0,
            balance: row.1,
            balance_usd: row.2,
            percentage: row.3,
            rank: row.4,
            first_seen: row.5,
            last_updated: row.6,
        })
        .collect();
    
        Ok(holders)
    }
    
    /// Start API server
    async fn start_server(&self) -> Result<()> {
        info!("Starting token API server on {}:{}", 
            self.config.server.host, self.config.server.port);
        
        // In production, implement REST API with actix-web or axum
        Ok(())
    }
    
    // ============================================================================
    // Public API Methods
    // ============================================================================
    
    /// Get token by address
    pub async fn get_token(&self, address: &str) -> Result<Token, TokenServiceError> {
        self.metrics.requests_total.write().inc();
        
        let token = sqlx::query_as(
            "SELECT address, name, symbol, decimals, total_supply, circulating_supply,
                    holders, transfers_24h, price_usd, price_change_24h, market_cap,
                    volume_24h, liquidity, last_updated
             FROM tokens
             WHERE address = $1"
        )
        .bind(address)
        .fetch_optional(&self.pool)
        .await?
        .ok_or_else(|| TokenServiceError::NotFound(address.to_string()))?;
        
        self.metrics.requests_success.write().inc();
        Ok(token)
    }
    
    /// Get token price
    pub async fn get_price(&self, address: &str) -> Result<f64, TokenServiceError> {
        // Check cache first
        {
            let cache = self.price_cache.read();
            if let Some(price) = cache.prices.get(address) {
                if cache.last_update
                    .map(|t| Utc::now().signed_duration_since(t))
                    .map(|d| d.num_seconds() < self.config.price_feed.cache_ttl as i64)
                    .unwrap_or(false)
                {
                    self.metrics.cache_hits.write().inc();
                    return Ok(price.price);
                }
            }
        }
        
        self.metrics.cache_misses.write().inc();
        
        // Fetch from database
        let price: f64 = sqlx::query_scalar(
            "SELECT price_usd FROM token_prices WHERE symbol = $1"
        )
        .bind(address)
        .fetch_one(&self.pool)
        .await?;
    
        Ok(price)
    }
    
    /// Get token holders
    pub async fn get_holders(&self, address: &str, limit: usize) -> Result<Vec<TokenHolder>, TokenServiceError> {
        self.metrics.requests_total.write().inc();
        
        // Check cache
        {
            let cache = self.holder_cache.read();
            if let Some(holders) = cache.holders.get(address) {
                self.metrics.cache_hits.write().inc();
                return Ok(holders.iter().take(limit).cloned().collect());
            }
        }
        
        self.metrics.cache_misses.write().inc();
        
        // Fetch from database
        let holders: Vec<TokenHolder> = sqlx::query_as(
            "SELECT address, balance, balance_usd, percentage, rank, first_seen, last_updated
             FROM token_holders
             WHERE token_address = $1
             ORDER BY balance_usd DESC
             LIMIT $2"
        )
        .bind(address)
        .bind(limit as i64)
        .fetch_all(&self.pool)
        .await?
        .into_iter()
        .map(|row| TokenHolder {
            address: row.0,
            balance: row.1,
            balance_usd: row.2,
            percentage: row.3,
            rank: row.4,
            first_seen: row.5,
            last_updated: row.6,
        })
        .collect();
    
        self.metrics.requests_success.write().inc();
        Ok(holders)
    }
    
    /// Get token transfers
    pub async fn get_transfers(
        &self,
        address: &str,
        from_block: u64,
        limit: usize,
    ) -> Result<Vec<TokenTransfer>, TokenServiceError> {
        let transfers: Vec<TokenTransfer> = sqlx::query_as(
            "SELECT hash, block_number, timestamp, from, to, value, value_usd, token_id
             FROM token_transfers
             WHERE token_address = $1 AND block_number > $2
             ORDER BY block_number DESC, log_index DESC
             LIMIT $3"
        )
        .bind(address)
        .bind(from_block as i64)
        .bind(limit as i64)
        .fetch_all(&self.pool)
        .await?
        .into_iter()
        .map(|row| TokenTransfer {
            hash: row.0,
            block_number: row.1,
            timestamp: row.2,
            from: row.3,
            to: row.4,
            value: row.5,
            value_usd: row.6,
            token_id: row.7,
        })
        .collect();
    
        Ok(transfers)
    }
    
    /// Get DEX pairs for a token
    pub async fn get_dex_pairs(&self, address: &str) -> Result<Vec<DEXPair>, TokenServiceError> {
        let pairs: Vec<DEXPair> = sqlx::query_as(
            "SELECT pair_address, dex_name, token0, token1, reserve0, reserve1,
                    total_supply, volume_24h, volume_7d, fees_24h, liquidity,
                    price, price_usd
             FROM dex_pairs
             WHERE token0 = $1 OR token1 = $1
             ORDER BY liquidity DESC"
        )
        .bind(address)
        .fetch_all(&self.pool)
        .await?
        .into_iter()
        .map(|row| DEXPair {
            pair_address: row.0,
            dex_name: row.1,
            token0: row.2,
            token1: row.3,
            reserve0: row.4,
            reserve1: row.5,
            total_supply: row.6,
            volume_24h: row.7,
            volume_7d: row.8,
            fees_24h: row.9,
            liquidity: row.10,
            price: row.11,
            price_usd: row.12,
        })
        .collect();
    
        Ok(pairs)
    }
    
    /// Get price history
    pub async fn get_price_history(
        &self,
        address: &str,
        from: DateTime<Utc>,
        to: DateTime<Utc>,
    ) -> Result<Vec<PriceHistory>, TokenServiceError> {
        let history: Vec<PriceHistory> = sqlx::query_as(
            "SELECT token_address, timestamp, price_usd, volume_24h, liquidity
             FROM price_history
             WHERE token_address = $1 AND timestamp >= $2 AND timestamp <= $3
             ORDER BY timestamp ASC"
        )
        .bind(address)
        .bind(from)
        .bind(to)
        .fetch_all(&self.pool)
        .await?
        .into_iter()
        .map(|row| PriceHistory {
            token_address: row.0,
            timestamp: row.1,
            price_usd: row.2,
            volume_24h: row.3,
            liquidity: row.4,
        })
        .collect();
    
        Ok(history)
    }
    
    /// Get price candles
    pub async fn get_price_candles(
        &self,
        address: &str,
        interval: &str,
        from: DateTime<Utc>,
        to: DateTime<Utc>,
    ) -> Result<Vec<PriceCandle>, TokenServiceError> {
        let interval_sql = match interval {
            "1h" => "1 hour",
            "4h" => "4 hours",
            "1d" => "1 day",
            _ => "1 hour",
        };
        
        let query = format!(
            "SELECT date_trunc('{}', timestamp) as ts,
                    first(price_usd, timestamp) as open,
                    max(price_usd) as high,
                    min(price_usd) as low,
                    last(price_usd, timestamp) as close,
                    sum(volume_24h) as volume
             FROM price_history
             WHERE token_address = $1 AND timestamp >= $2 AND timestamp <= $3
             GROUP BY ts
             ORDER BY ts ASC",
            interval_sql
        );
        
        let candles: Vec<PriceCandle> = sqlx::query_as(&query)
            .bind(address)
            .bind(from)
            .bind(to)
            .fetch_all(&self.pool)
            .await?
            .into_iter()
            .map(|row| PriceCandle {
                timestamp: row.0,
                open: row.1,
                high: row.2,
                low: row.3,
                close: row.4,
                volume: row.5,
            })
            .collect();
    
        Ok(candles)
    }
    
    /// Get service metrics
    pub fn get_metrics(&self) -> ServiceMetricsResponse {
        let m = &*self.metrics;
        ServiceMetricsResponse {
            requests_total: *m.requests_total.read(),
            requests_success: *m.requests_success.read(),
            requests_failed: *m.requests_failed.read(),
            cache_hits: *m.cache_hits.read(),
            cache_misses: *m.cache_misses.read(),
            price_updates: *m.price_updates.read(),
            holder_updates: *m.holder_updates.read(),
        }
    }
    
    /// Shutdown the service
    pub async fn shutdown(&self) {
        let _ = self.shutdown_tx.send(()).await;
    }
}

#[derive(Serialize)]
pub struct ServiceMetricsResponse {
    pub requests_total: u64,
    pub requests_success: u64,
    pub requests_failed: u64,
    pub cache_hits: u64,
    pub cache_misses: u64,
    pub price_updates: u64,
    pub holder_updates: u64,
}

// ============================================================================
// Main Entry Point
// ============================================================================

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize logging
    let filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info"));
    
    tracing_subscriber::registry()
        .with(filter)
        .with(tracing_subscriber::fmt::layer())
        .init();
    
    info!("Starting TigerScan Token Service");
    
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
        price_feed: PriceFeedConfig {
            primary_api_url: std::env::var("PRICE_API_URL")
                .unwrap_or_else(|_| "https://api.coingecko.com/api/v3".to_string()),
            secondary_api_url: None,
            primary_api_key: None,
            update_interval: 60,
            cache_ttl: 30,
            min_liquidity: 10000.0,
        },
        server: ServerConfig {
            host: "0.0.0.0".to_string(),
            port: 8080,
            rate_limit: 1000,
            rate_limit_burst: 2000,
        },
        security: SecurityConfig {
            enable_signing: false,
            signing_key: None,
            allowed_api_keys: vec![],
        },
    };
    
    // Create and run service
    let service = TokenService::new(config).await?;
    service.run().await?;
    
    Ok(())
}