//! TigerScan Price Feed Service
//! Real-time token prices from exchanges

use std::sync::Arc;
use std::time::Duration;

use anyhow::Result;
use async_trait::async_trait;
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
    pub update_interval: u64,
    pub sources: Vec<String>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            database_url: std::env::var("DATABASE_URL")
                .unwrap_or_else(|_| "postgres://localhost:5432/tigerscan".to_string()),
            update_interval: 60,
            sources: vec!["coingecko".to_string(), "binance".to_string(), "dexscreener".to_string()],
        }
    }
}

// ============================================================================
// Types
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenPrice {
    pub token_address: String,
    pub price_usd: f64,
    pub change_24h: f64,
    pub volume_24h: f64,
    pub market_cap: f64,
    pub source: String,
    pub timestamp: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PriceUpdate {
    pub address: String,
    pub symbol: String,
    pub price_usd: f64,
    pub change_24h: f64,
    pub volume_24h: f64,
    pub market_cap: f64,
}

// ============================================================================
// Price Sources
// ============================================================================

#[async_trait]
pub trait PriceSource: Send + Sync {
    fn name(&self) -> &str;
    async fn get_prices(&self, tokens: &[String]) -> Result<Vec<PriceUpdate>, Box<dyn std::error::Error + Send + Sync>>;
}

pub struct CoinGeckoSource {
    client: reqwest::Client,
    api_key: Option<String>,
}

impl CoinGeckoSource {
    pub fn new(api_key: Option<String>) -> Self {
        Self {
            client: reqwest::Client::new(),
            api_key,
        }
    }
}

#[async_trait]
impl PriceSource for CoinGeckoSource {
    fn name(&self) -> &str { "coingecko" }

    async fn get_prices(&self, tokens: &[String]) -> Result<Vec<PriceUpdate>, Box<dyn std::error::Error + Send + Sync>> {
        // Implementation for CoinGecko API
        let mut updates = Vec::new();
        for token in tokens {
            updates.push(PriceUpdate {
                address: token.clone(),
                symbol: token.clone(),
                price_usd: 0.0,
                change_24h: 0.0,
                volume_24h: 0.0,
                market_cap: 0.0,
            });
        }
        Ok(updates)
    }
}

pub struct DexScreenerSource {
    client: reqwest::Client,
}

impl DexScreenerSource {
    pub fn new() -> Self {
        Self { client: reqwest::Client::new() }
    }
}

#[async_trait]
impl PriceSource for DexScreenerSource {
    fn name(&self) -> &str { "dexscreener" }

    async fn get_prices(&self, tokens: &[String]) -> Result<Vec<PriceUpdate>, Box<dyn std::error::Error + Send + Sync>> {
        let mut updates = Vec::new();
        for token in tokens {
            let url = format!("https://api.dexscreener.com/latest/dex/tokens/{}", token);
            if let Ok(response) = self.client.get(&url).send().await {
                if let Ok(data) = response.json::<serde_json::Value>().await {
                    if let Some(pair) = data.get("pair") {
                        let price_usd = pair.get("priceUsd").and_then(|v| v.as_str())
                            .and_then(|s| s.parse::<f64>().ok()).unwrap_or(0.0);
                        let change_24h = pair.get("priceChange").and_then(|v| v.as_f64()).unwrap_or(0.0);
                        let volume_24h = pair.get("volume").and_then(|v| v.get("h24"))
                            .and_then(|v| v.as_f64()).unwrap_or(0.0);
                        updates.push(PriceUpdate {
                            address: token.clone(),
                            symbol: token.clone(),
                            price_usd,
                            change_24h,
                            volume_24h,
                            market_cap: 0.0,
                        });
                    }
                }
            }
        }
        Ok(updates)
    }
}

// ============================================================================
// Price Feed Service
// ============================================================================

pub struct PriceFeedService {
    config: Config,
    db: PgPool,
    sources: Vec<Box<dyn PriceSource>>,
    state: Arc<RwLock<PriceFeedState>>,
}

#[derive(Debug, Clone)]
pub struct PriceFeedState {
    pub last_update: Option<DateTime<Utc>>,
    pub updates_count: u64,
    pub errors_count: u64,
    pub tokens_tracked: u64,
}

impl PriceFeedService {
    pub async fn new(config: Config) -> Result<Self> {
        let db = PgPoolOptions::new()
            .max_connections(5)
            .connect(&config.database_url)
            .await?;

        let mut sources: Vec<Box<dyn PriceSource>> = vec![];
        for source in &config.sources {
            match source.as_str() {
                "coingecko" => {
                    let api_key = std::env::var("COINGECKO_API_KEY").ok();
                    sources.push(Box::new(CoinGeckoSource::new(api_key)));
                }
                "dexscreener" => sources.push(Box::new(DexScreenerSource::new())),
                _ => warn!("Unknown source: {}", source),
            }
        }

        Ok(Self {
            config,
            db,
            sources,
            state: Arc::new(RwLock::new(PriceFeedState::default())),
        })
    }

    pub async fn start(&self) {
        info!("Starting price feed service...");
        let mut interval = interval(Duration::from_secs(self.config.update_interval));
        loop {
            interval.tick().await;
            if let Err(e) = self.update_prices().await {
                error!("Failed to update prices: {}", e);
                self.state.write().errors_count += 1;
            }
        }
    }

    async fn update_prices(&self) -> Result<()> {
        let tokens: Vec<String> = sqlx::query_scalar(
            "SELECT address FROM tokens WHERE is_verified = true AND is_spam = false LIMIT 100"
        ).fetch_all(&self.db).await?;

        if tokens.is_empty() { return Ok(()); }

        for source in &self.sources {
            match source.get_prices(&tokens).await {
                Ok(updates) => { self.save_prices(&updates, source.name()).await?; break; }
                Err(e) => warn!("Failed from {}: {}", source.name(), e),
            }
        }

        let mut state = self.state.write();
        state.last_update = Some(Utc::now());
        state.updates_count += 1;
        state.tokens_tracked = tokens.len() as u64;
        Ok(())
    }

    async fn save_prices(&self, updates: &[PriceUpdate], source: &str) -> Result<()> {
        for update in updates {
            sqlx::query(
                "UPDATE tokens SET price_usd = $1, price_change_24h = $2, volume_24h = $3, market_cap = $4, updated_at = NOW() WHERE address = $5"
            ).bind(update.price_usd).bind(update.change_24h).bind(update.volume_24h).bind(update.market_cap).bind(&update.address)
            .execute(&self.db).await?;

            sqlx::query(
                "INSERT INTO token_prices (token_address, price_usd, timestamp, source) VALUES ($1, $2, $3, $4)"
            ).bind(&update.address).bind(update.price_usd).bind(Utc::now().timestamp()).bind(source)
            .execute(&self.db).await?;
        }
        Ok(())
    }

    pub fn get_state(&self) -> PriceFeedState { self.state.read().clone() }
}