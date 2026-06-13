//! TigerScan DeFi Analytics Service
//! Production-grade DeFi analytics - TVL, lending rates, pool analytics, yield farming

use std::sync::Arc;
use std::time::Duration;

use anyhow::Result;
use chrono::{DateTime, Utc};
use ethers::providers::{Http, Provider};
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
    pub rpc_url: String,
    pub database_url: String,
    pub update_interval: u64,
    pub defi_protocols: Vec<ProtocolConfig>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            rpc_url: std::env::var("RPC_URL").unwrap_or_else(|_| "http://localhost:8545".to_string()),
            database_url: std::env::var("DATABASE_URL")
                .unwrap_or_else(|_| "postgres://localhost:5432/tigerscan".to_string()),
            update_interval: 300,
            defi_protocols: vec![
                ProtocolConfig {
                    name: "PancakeSwap".to_string(),
                    factory: "0xcA143Ce32Fe78f1f7019d7d551a6402fC2270E62".to_string(),
                    router: "0x10ED43C718714eb63d5aA6B79E2b10d5bFe2f3D9".to_string(),
                    protocol_type: "DEX".to_string(),
                },
                ProtocolConfig {
                    name: "Biswap".to_string(),
                    factory: "0x858E3312ed3A87694751DaEcF21E823e580B7A92".to_string(),
                    router: "0x3a6d15cde7F37B647C67AbA4b2b201A3B2F2Ed57".to_string(),
                    protocol_type: "DEX".to_string(),
                },
            ],
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct ProtocolConfig {
    pub name: String,
    pub factory: String,
    pub router: String,
    pub protocol_type: String,
}

// ============================================================================
// Types
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TVLData {
    pub protocol: String,
    pub tvl: f64,
    pub tvl_change_24h: f64,
    pub tvl_change_7d: f64,
    pub volume_24h: f64,
    pub tokens: Vec<TokenTVL>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenTVL {
    pub address: String,
    pub symbol: String,
    pub tvl: f64,
    pub weight: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PoolData {
    pub address: String,
    pub protocol: String,
    pub token0: String,
    pub token1: String,
    pub reserve0: f64,
    pub reserve1: f64,
    pub liquidity_usd: f64,
    pub volume_24h: f64,
    pub volume_change_24h: f64,
    pub fee_24h: f64,
    pub apy: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LendingRate {
    pub protocol: String,
    pub token: String,
    pub supply_rate: f64,
    pub borrow_rate: f64,
    pub utilization: f64,
    pub total_supply: f64,
    pub total_borrow: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct YieldOpportunity {
    pub protocol: String,
    pub pool: String,
    pub token0: String,
    pub token1: String,
    pub apy: f64,
    pub apy_7d: f64,
    pub tvl: f64,
    pub risk_level: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FlashLoan {
    pub transaction_hash: String,
    pub block_number: u64,
    pub borrower: String,
    pub tokens: Vec<String>,
    pub amounts: Vec<f64>,
    pub profit_usd: f64,
    pub timestamp: i64,
}

// ============================================================================
// DeFi Analytics Service
// ============================================================================

pub struct DeFiAnalyticsService {
    config: Config,
    db: PgPool,
    rpc: Provider<Http>,
    state: Arc<RwLock<AnalyticsState>>,
}

#[derive(Debug, Clone)]
pub struct AnalyticsState {
    pub protocols_tracked: u64,
    pub pools_tracked: u64,
    pub updates_count: u64,
    pub errors: u64,
    pub last_update: Option<DateTime<Utc>>,
}

impl DeFiAnalyticsService {
    pub async fn new(config: Config) -> Result<Self> {
        let db = PgPoolOptions::new()
            .max_connections(10)
            .connect(&config.database_url)
            .await?;

        let rpc = Provider::<Http>::try_from(config.rpc_url.clone())?;

        Ok(Self {
            config,
            db,
            rpc,
            state: Arc::new(RwLock::new(AnalyticsState {
                protocols_tracked: 0,
                pools_tracked: 0,
                updates_count: 0,
                errors: 0,
                last_update: None,
            })),
        })
    }

    pub async fn start(&self) {
        info!("Starting DeFi analytics service...");

        let mut interval = interval(Duration::from_secs(self.config.update_interval));

        loop {
            interval.tick().await;

            if let Err(e) = self.update_tvl().await {
                error!("Failed to update TVL: {}", e);
                self.state.write().errors += 1;
            }

            if let Err(e) = self.update_pools().await {
                error!("Failed to update pools: {}", e);
                self.state.write().errors += 1;
            }

            if let Err(e) = self.detect_flash_loans().await {
                warn!("Failed to detect flash loans: {}", e);
            }

            {
                let mut state = self.state.write();
                state.updates_count += 1;
                state.last_update = Some(Utc::now());
            }
        }
    }

    pub async fn update_tvl(&self) -> Result<()> {
        info!("Updating TVL data...");

        for protocol in &self.config.defi_protocols {
            let tvl = self.calculate_protocol_tvl(&protocol.factory).await?;

            // Get previous TVL for change calculation
            let previous_tvl: Option<f64> = sqlx::query_scalar(
                "SELECT tvl FROM analytics_daily WHERE date = CURRENT_DATE - INTERVAL '1 day'"
            )
            .fetch_optional(&self.db)
            .await?;

            let tvl_change_24h = if let Some(prev) = previous_tvl {
                if prev > 0.0 {
                    ((tvl - prev) / prev) * 100.0
                } else {
                    0.0
                }
            } else {
                0.0
            };

            // Save to database
            sqlx::query(
                "INSERT INTO defi_tvl (protocol, tvl, tvl_change_24h, updated_at) VALUES ($1, $2, $3, NOW()) ON CONFLICT (protocol) DO UPDATE SET tvl = EXCLUDED.tvl, tvl_change_24h = EXCLUDED.tvl_change_24h, updated_at = NOW()"
            )
            .bind(&protocol.name)
            .bind(tvl)
            .bind(tvl_change_24h)
            .execute(&self.db)
            .await?;

            self.state.write().protocols_tracked += 1;
        }

        Ok(())
    }

    async fn calculate_protocol_tvl(&self, _factory: &str) -> Result<f64> {
        // Get all DEX pairs and sum their liquidity
        let total_liquidity: Option<f64> = sqlx::query_scalar(
            "SELECT COALESCE(SUM(liquidity_usd), 0) FROM dex_pairs"
        )
        .fetch_optional(&self.db)
        .await?;

        Ok(total_liquidity.unwrap_or(0.0))
    }

    pub async fn update_pools(&self) -> Result<()> {
        info!("Updating pool data...");

        // Get all DEX pairs
        let pools: Vec<(String, String, String, String, f64, f64, f64, f64)> = sqlx::query_as(
            "SELECT pair_address, token0_address, token1_address, token0_symbol, reserve0, reserve1, liquidity_usd, volume_24h FROM dex_pairs WHERE liquidity_usd > 1000"
        )
        .fetch_all(&self.db)
        .await?;

        for (pair, token0, token1, symbol0, reserve0, reserve1, liquidity, volume) in pools {
            // Calculate APY (simplified formula: fee volume * 0.003 * 365 / liquidity)
            let fee_24h = volume * 0.003;
            let apy = if liquidity > 0.0 {
                (fee_24h * 365.0 / liquidity) * 100.0
            } else {
                0.0
            };

            // Update pool with APY
            sqlx::query(
                "UPDATE dex_pairs SET apy = $1, updated_at = NOW() WHERE pair_address = $2"
            )
            .bind(apy)
            .bind(&pair)
            .execute(&self.db)
            .await?;

            self.state.write().pools_tracked += 1;
        }

        Ok(())
    }

    pub async fn detect_flash_loans(&self) -> Result<()> {
        // Look for flash loan patterns in recent transactions
        // Flash loans typically: borrow + swap + repay in single tx
        
        info!("Detecting flash loans...");

        // Get recent large transactions that could be flash loans
        let txs: Vec<(String, String, f64)> = sqlx::query_as(
            r#"
            SELECT t.hash, t.from_address, CAST(t.value AS NUMERIC)
            FROM transactions t
            WHERE t.block_number > (SELECT MAX(number) - 10 FROM blocks)
            AND CAST(t.value AS NUMERIC) > 1000000000000000000
            "#
        )
        .fetch_all(&self.db)
        .await?;

        for (hash, from, value) in txs {
            // Check for flash loan pattern (would need internal txs)
            // For now, just log potential flash loans
            info!("Potential flash loan: {} - {} TGR", hash, value);
        }

        Ok(())
    }

    pub async fn get_top_yields(&self, limit: usize) -> Result<Vec<YieldOpportunity>> {
        let yields: Vec<YieldOpportunity> = sqlx::query_as(
            r#"
            SELECT dp.protocol_name, dp.pair_address, dp.token0_symbol, dp.token1_symbol, 
                   dp.apy, 0.0 as apy_7d, dp.liquidity_usd, 'medium' as risk_level
            FROM dex_pairs dp
            WHERE dp.apy > 0 AND dp.liquidity_usd > 10000
            ORDER BY dp.apy DESC
            LIMIT $1
            "#
        )
        .bind(limit as i64)
        .fetch_all(&self.db)
        .await?;

        Ok(yields)
    }

    pub async fn get_lending_rates(&self) -> Result<Vec<LendingRate>> {
        // Get lending rates from database (populated by lending protocol indexer)
        let rates: Vec<LendingRate> = sqlx::query_as(
            "SELECT protocol, token, supply_rate, borrow_rate, utilization, total_supply, total_borrow FROM lending_rates"
        )
        .fetch_all(&self.db)
        .await?;

        Ok(rates)
    }

    pub async fn get_tvl_history(&self, protocol: &str, days: i32) -> Result<Vec<TVLHistory>> {
        let history: Vec<TVLHistory> = sqlx::query_as(
            "SELECT protocol, tvl, date FROM defi_tvl WHERE protocol = $1 AND date >= CURRENT_DATE - ($2 || ' days')::interval ORDER BY date"
        )
        .bind(protocol)
        .bind(days.to_string())
        .fetch_all(&self.db)
        .await?;

        Ok(history)
    }

    pub fn get_state(&self) -> AnalyticsState {
        self.state.read().clone()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TVLHistory {
    pub protocol: String,
    pub tvl: f64,
    pub date: String,
}