//! TigerScan price feed: token prices from external sources, persisted for history.

#![forbid(unsafe_code)]

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
    /// Chain whose token prices the sources should report. DexScreener is
    /// queried per chain, and CoinGecko per platform.
    pub chain_id: String,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            database_url: std::env::var("DATABASE_URL")
                .unwrap_or_else(|_| "postgres://localhost:5432/tigerscan".to_string()),
            update_interval: 60,
            sources: vec![
                "coingecko".to_string(),
                "dexscreener".to_string(),
            ],
            chain_id: std::env::var("CHAIN_ID").unwrap_or_else(|_| "6666".to_string()),
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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GasOracle {
    pub slow: u64,
    pub standard: u64,
    pub fast: u64,
    /// Highest base fee observed in the sampled window.
    pub base_fee: u64,
    pub sampled_at: i64,
}

// ============================================================================
// Price sources
// ============================================================================

#[async_trait]
pub trait PriceSource: Send + Sync {
    fn name(&self) -> &str;

    async fn get_prices(
        &self,
        tokens: &[String],
    ) -> Result<Vec<PriceUpdate>, Box<dyn std::error::Error + Send + Sync>>;
}

/// CoinGecko `simple/token_price`. Requires a platform slug that maps to the
/// chain; `chain_id` is passed through so a deployment can point at whichever
/// platform CoinGecko lists the chain under.
pub struct CoinGeckoSource {
    client: reqwest::Client,
    api_key: Option<String>,
    platform: String,
}

impl CoinGeckoSource {
    pub fn new(api_key: Option<String>, platform: String) -> Self {
        Self {
            client: reqwest::Client::new(),
            api_key,
            platform,
        }
    }

    /// Addresses are lower-cased: CoinGecko's contract lookup is case-sensitive.
    fn contract_list(tokens: &[String]) -> String {
        tokens
            .iter()
            .map(|token| token.to_ascii_lowercase())
            .collect::<Vec<_>>()
            .join(",")
    }
}

#[async_trait]
impl PriceSource for CoinGeckoSource {
    fn name(&self) -> &str {
        "coingecko"
    }

    async fn get_prices(
        &self,
        tokens: &[String],
    ) -> Result<Vec<PriceUpdate>, Box<dyn std::error::Error + Send + Sync>> {
        if tokens.is_empty() {
            return Ok(Vec::new());
        }

        let mut request = self
            .client
            .get(format!(
                "https://api.coingecko.com/api/v3/simple/token_price/{}",
                self.platform
            ))
            .query(&[
                ("contract_addresses", Self::contract_list(tokens)),
                ("vs_currencies", "usd".to_string()),
                ("include_24hr_change", "true".to_string()),
                ("include_24hr_vol", "true".to_string()),
                ("include_market_cap", "true".to_string()),
            ]);

        if let Some(api_key) = &self.api_key {
            request = request.header("x-cg-demo-api-key", api_key);
        }

        let response = request.send().await?;

        if !response.status().is_success() {
            return Err(format!("coingecko returned HTTP {}", response.status()).into());
        }

        let body: serde_json::Value = response.json().await?;

        let mut updates = Vec::new();

        for token in tokens {
            let key = token.to_ascii_lowercase();

            let Some(entry) = body.get(&key) else {
                continue;
            };

            let Some(price_usd) = entry.get("usd").and_then(serde_json::Value::as_f64) else {
                continue;
            };

            updates.push(PriceUpdate {
                address: token.clone(),
                symbol: token.clone(),
                price_usd,
                change_24h: entry
                    .get("usd_24h_change")
                    .and_then(serde_json::Value::as_f64)
                    .unwrap_or(0.0),
                volume_24h: entry
                    .get("usd_24h_vol")
                    .and_then(serde_json::Value::as_f64)
                    .unwrap_or(0.0),
                market_cap: entry
                    .get("usd_market_cap")
                    .and_then(serde_json::Value::as_f64)
                    .unwrap_or(0.0),
            });
        }

        Ok(updates)
    }
}

/// DexScreener token endpoint. The response holds a `pairs` array, and the
/// deepest-liquidity pair is the one worth reporting.
pub struct DexScreenerSource {
    client: reqwest::Client,
}

impl DexScreenerSource {
    pub fn new() -> Self {
        Self {
            client: reqwest::Client::new(),
        }
    }

    /// Price fields arrive as decimal strings, not numbers.
    fn decimal(value: Option<&serde_json::Value>) -> Option<f64> {
        match value {
            Some(serde_json::Value::String(text)) => text.parse::<f64>().ok(),
            Some(serde_json::Value::Number(number)) => number.as_f64(),
            _ => None,
        }
    }

    fn deepest_pair(pairs: &[serde_json::Value]) -> Option<&serde_json::Value> {
        pairs.iter().max_by(|left, right| {
            let left_liquidity = Self::decimal(left.get("liquidity").and_then(|l| l.get("usd")))
                .unwrap_or(0.0);
            let right_liquidity =
                Self::decimal(right.get("liquidity").and_then(|l| l.get("usd"))).unwrap_or(0.0);

            left_liquidity
                .partial_cmp(&right_liquidity)
                .unwrap_or(std::cmp::Ordering::Equal)
        })
    }
}

impl Default for DexScreenerSource {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl PriceSource for DexScreenerSource {
    fn name(&self) -> &str {
        "dexscreener"
    }

    async fn get_prices(
        &self,
        tokens: &[String],
    ) -> Result<Vec<PriceUpdate>, Box<dyn std::error::Error + Send + Sync>> {
        let mut updates = Vec::new();

        for token in tokens {
            let response = self
                .client
                .get(format!(
                    "https://api.dexscreener.com/latest/dex/tokens/{token}"
                ))
                .send()
                .await?;

            if !response.status().is_success() {
                warn!(token, status = %response.status(), "dexscreener request failed");
                continue;
            }

            let body: serde_json::Value = response.json().await?;

            let Some(pairs) = body.get("pairs").and_then(serde_json::Value::as_array) else {
                continue;
            };

            let Some(pair) = Self::deepest_pair(pairs) else {
                continue;
            };

            let Some(price_usd) = Self::decimal(pair.get("priceUsd")) else {
                continue;
            };

            let symbol = pair
                .get("baseToken")
                .and_then(|token| token.get("symbol"))
                .and_then(serde_json::Value::as_str)
                .unwrap_or(token)
                .to_string();

            updates.push(PriceUpdate {
                address: token.clone(),
                symbol,
                price_usd,
                change_24h: Self::decimal(pair.get("priceChange").and_then(|c| c.get("h24")))
                    .unwrap_or(0.0),
                volume_24h: Self::decimal(pair.get("volume").and_then(|v| v.get("h24")))
                    .unwrap_or(0.0),
                market_cap: Self::decimal(pair.get("marketCap")).unwrap_or(0.0),
            });
        }

        Ok(updates)
    }
}

// ============================================================================
// Price feed service
// ============================================================================

pub struct PriceFeedService {
    config: Config,
    db: PgPool,
    sources: Vec<Box<dyn PriceSource>>,
    state: Arc<RwLock<PriceFeedState>>,
}

#[derive(Debug, Clone, Default)]
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
                    let platform = std::env::var("COINGECKO_PLATFORM")
                        .unwrap_or_else(|_| config.chain_id.clone());
                    sources.push(Box::new(CoinGeckoSource::new(api_key, platform)));
                }
                "dexscreener" => sources.push(Box::new(DexScreenerSource::new())),
                other => warn!("unknown price source: {other}"),
            }
        }

        if sources.is_empty() {
            return Err(anyhow::anyhow!(
                "no usable price source configured: {:?}",
                config.sources
            ));
        }

        Ok(Self {
            config,
            db,
            sources,
            state: Arc::new(RwLock::new(PriceFeedState::default())),
        })
    }

    /// Poll every configured source until one answers, then persist the result.
    pub async fn start(&self) {
        info!("starting price feed service");

        let mut ticker = interval(Duration::from_secs(self.config.update_interval));

        loop {
            ticker.tick().await;

            if let Err(error) = self.update_prices().await {
                error!(%error, "price update failed");
                self.state.write().errors_count += 1;
            }
        }
    }

    async fn update_prices(&self) -> Result<()> {
        let tokens: Vec<String> = sqlx::query_scalar(
            "SELECT address FROM tokens WHERE is_verified = true AND is_spam = false LIMIT 100",
        )
        .fetch_all(&self.db)
        .await?;

        if tokens.is_empty() {
            return Ok(());
        }

        for source in &self.sources {
            match source.get_prices(&tokens).await {
                Ok(updates) if !updates.is_empty() => {
                    self.save_prices(&updates, source.name()).await?;

                    let mut state = self.state.write();
                    state.last_update = Some(Utc::now());
                    state.updates_count += 1;
                    state.tokens_tracked = tokens.len() as u64;

                    return Ok(());
                }
                Ok(_) => warn!(source = source.name(), "source returned no prices"),
                Err(error) => warn!(source = source.name(), %error, "source failed"),
            }
        }

        Err(anyhow::anyhow!("every price source failed"))
    }

    async fn save_prices(&self, updates: &[PriceUpdate], source: &str) -> Result<()> {
        let timestamp = Utc::now().timestamp();

        for update in updates {
            let mut transaction = self.db.begin().await?;

            sqlx::query(
                "UPDATE tokens SET price_usd = $1, price_change_24h = $2, volume_24h = $3, \
                 market_cap = $4, updated_at = NOW() WHERE address = $5",
            )
            .bind(update.price_usd)
            .bind(update.change_24h)
            .bind(update.volume_24h)
            .bind(update.market_cap)
            .bind(&update.address)
            .execute(&mut *transaction)
            .await?;

            // A second write in the same second for the same token is expected
            // when several sources agree, and must not abort the batch.
            sqlx::query(
                "INSERT INTO token_prices (token_address, price_usd, timestamp, source) \
                 VALUES ($1, $2, $3, $4) ON CONFLICT (token_address, timestamp) DO NOTHING",
            )
            .bind(&update.address)
            .bind(update.price_usd)
            .bind(timestamp)
            .bind(source)
            .execute(&mut *transaction)
            .await?;

            transaction.commit().await?;
        }

        Ok(())
    }

    /// Current prices for the given token addresses.
    pub async fn get_prices(&self, addresses: &[String]) -> Result<Vec<PriceUpdate>> {
        if addresses.is_empty() {
            return Ok(Vec::new());
        }

        let rows: Vec<(String, String, Option<f64>, Option<f64>, Option<f64>, Option<f64>)> =
            sqlx::query_as(
                "SELECT address, symbol, price_usd::float8, price_change_24h::float8, \
                 volume_24h::float8, market_cap::float8 FROM tokens WHERE address = ANY($1)",
            )
            .bind(addresses)
            .fetch_all(&self.db)
            .await?;

        Ok(rows
            .into_iter()
            .filter_map(
                |(address, symbol, price_usd, change_24h, volume_24h, market_cap)| {
                    // A token with no recorded price is reported as absent
                    // rather than as a price of zero.
                    let price_usd = price_usd?;

                    Some(PriceUpdate {
                        address,
                        symbol,
                        price_usd,
                        change_24h: change_24h.unwrap_or(0.0),
                        volume_24h: volume_24h.unwrap_or(0.0),
                        market_cap: market_cap.unwrap_or(0.0),
                    })
                },
            )
            .collect())
    }

    /// Persisted price history for one token over the last `days` days.
    pub async fn get_price_history(&self, address: &str, days: i64) -> Result<Vec<TokenPrice>> {
        let since = Utc::now().timestamp() - days.max(1) * 86_400;

        let rows: Vec<(String, f64, i64, Option<String>)> = sqlx::query_as(
            "SELECT token_address, price_usd::float8, timestamp, source FROM token_prices \
             WHERE token_address = $1 AND timestamp >= $2 ORDER BY timestamp ASC",
        )
        .bind(address)
        .bind(since)
        .fetch_all(&self.db)
        .await?;

        Ok(rows
            .into_iter()
            .map(|(token_address, price_usd, timestamp, source)| TokenPrice {
                token_address,
                price_usd,
                change_24h: 0.0,
                volume_24h: 0.0,
                market_cap: 0.0,
                source: source.unwrap_or_default(),
                timestamp,
            })
            .collect())
    }

    /// Gas price percentiles from the gas prices recorded in the last hour.
    pub async fn get_gas_oracle(&self) -> Result<GasOracle> {
        let since = Utc::now().timestamp() - 3_600;

        let prices: Vec<i64> = sqlx::query_scalar(
            "SELECT gas_price FROM gas_prices WHERE timestamp >= $1 ORDER BY gas_price ASC",
        )
        .bind(since)
        .fetch_all(&self.db)
        .await?;

        if prices.is_empty() {
            return Err(anyhow::anyhow!(
                "no gas prices recorded in the last hour"
            ));
        }

        let percentile = |fraction: f64| -> u64 {
            let index = ((prices.len() as f64 - 1.0) * fraction).round() as usize;
            prices[index.min(prices.len() - 1)].max(0) as u64
        };

        let base_fee: Option<i64> = sqlx::query_scalar(
            "SELECT MAX(base_fee) FROM gas_prices WHERE timestamp >= $1",
        )
        .bind(since)
        .fetch_one(&self.db)
        .await?;

        Ok(GasOracle {
            slow: percentile(0.10),
            standard: percentile(0.50),
            fast: percentile(0.90),
            base_fee: base_fee.unwrap_or(0).max(0) as u64,
            sampled_at: Utc::now().timestamp(),
        })
    }

    pub fn get_state(&self) -> PriceFeedState {
        self.state.read().clone()
    }

    pub fn config(&self) -> &Config {
        &self.config
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn price_fields_are_read_from_decimal_strings() {
        let value = serde_json::json!("0.0000001234");

        assert_eq!(
            DexScreenerSource::decimal(Some(&value)),
            Some(0.0000001234)
        );
    }

    #[test]
    fn numeric_price_fields_are_also_accepted() {
        let value = serde_json::json!(12.5);

        assert_eq!(DexScreenerSource::decimal(Some(&value)), Some(12.5));
    }

    #[test]
    fn a_missing_price_field_is_none() {
        assert_eq!(DexScreenerSource::decimal(None), None);
    }

    #[test]
    fn the_deepest_pair_is_selected() {
        let pairs = vec![
            serde_json::json!({ "priceUsd": "1.0", "liquidity": { "usd": 100.0 } }),
            serde_json::json!({ "priceUsd": "2.0", "liquidity": { "usd": 9000.0 } }),
            serde_json::json!({ "priceUsd": "3.0", "liquidity": { "usd": 50.0 } }),
        ];

        let selected = DexScreenerSource::deepest_pair(&pairs).expect("a pair must be selected");

        assert_eq!(selected.get("priceUsd").unwrap(), "2.0");
    }

    #[test]
    fn coin_gecko_lowercases_contract_addresses() {
        let tokens = vec![
            "0xAbCdEf0000000000000000000000000000000001".to_string(),
            "0x1234".to_string(),
        ];

        assert_eq!(
            CoinGeckoSource::contract_list(&tokens),
            "0xabcdef0000000000000000000000000000000001,0x1234"
        );
    }

    #[test]
    fn the_default_config_has_usable_sources() {
        let config = Config::default();

        assert!(!config.sources.is_empty());
        assert!(config.update_interval > 0);
    }
}
