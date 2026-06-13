//! TigerScan DeFi Analytics Service
//! Production-grade Rust service for protocol TVL, DEX volumes, lending rates, pool analytics

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
use tracing::{error, info};

// ============================================================================
// Error Types
// ============================================================================

#[derive(Error, Debug)]
pub enum DefiServiceError {
    #[error("Database error: {0}")]
    Database(#[from] sqlx::Error),
    
    #[error("HTTP request error: {0}")]
    Http(#[from] reqwest::Error),
    
    #[error("Protocol not found: {0}")]
    NotFound(String),
    
    #[error("Invalid data: {0}")]
    InvalidData(String),
}

// ============================================================================
// Data Models
// ============================================================================

/// DeFi Protocol
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DefiProtocol {
    pub id: String,
    pub name: String,
    pub category: String, // lending, dex, yield, bridge, nft
    pub logo_url: Option<String>,
    pub description: Option<String>,
    pub website: Option<String>,
    pub tvl: f64,
    pub tvl_change_24h: f64,
    pub volume_24h: f64,
    pub users_24h: u64,
    pub active_positions: u64,
    pub verified: bool,
    pub listed_at: DateTime<Utc>,
}

/// DEX Pool
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DefiPool {
    pub id: String,
    pub protocol: String,
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
    pub apy: f64,
    pub apr: f64,
}

/// Lending Pool
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LendingPool {
    pub id: String,
    pub protocol: String,
    pub token: String,
    pub collateral_factor: f64,
    pub reserve_size: f64,
    pub utilization_rate: f64,
    pub supply_rate: f64,
    pub borrow_rate: f64,
    pub liquidation_threshold: f64,
    pub liquidity: f64,
}

/// Yield Pool
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct YieldPool {
    pub id: String,
    pub protocol: String,
    pub token: String,
    pub tvl: f64,
    pub apy: f64,
    pub reward_token: Option<String>,
    pub reward_apy: f64,
    pub lock_period: u32,
    pub risk_level: String,
}

/// TVL Data Point
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TVLDataPoint {
    pub timestamp: DateTime<Utc>,
    pub total_tvl: f64,
    pub by_protocol: Vec<ProtocolTVL>,
    pub by_token: Vec<TokenTVL>,
}

/// Protocol TVL
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProtocolTVL {
    pub protocol: String,
    pub tvl: f64,
    pub change_24h: f64,
}

/// Token TVL
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenTVL {
    pub token: String,
    pub tvl: f64,
    pub percentage: f64,
}

/// Volume Data
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VolumeData {
    pub timestamp: DateTime<Utc>,
    pub volume: f64,
    pub transactions: u64,
    pub users: u64,
}

/// Flash Loan Alert
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FlashLoanAlert {
    pub transaction_hash: String,
    pub block_number: u64,
    pub timestamp: DateTime<Utc>,
    pub pool: String,
    pub token: String,
    pub amount: f64,
    pub profit: f64,
    pub gas_used: u64,
    pub gas_fee: f64,
}

/// Whale Transaction
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WhaleTransaction {
    pub hash: String,
    pub block_number: u64,
    pub timestamp: DateTime<Utc>,
    pub from: String,
    pub to: String,
    pub value_usd: f64,
    pub token: String,
    pub transaction_type: String,
}

// ============================================================================
// Configuration
// ============================================================================

#[derive(Debug, Clone, Deserialize)]
pub struct Config {
    pub database: DatabaseConfig,
    pub defi: DefiConfig,
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
pub struct DefiConfig {
    pub update_interval: u64,
    pub min_whale_value: f64,
    pub flash_loan_threshold: f64,
    pub dex_list: Vec<DexConfig>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct DexConfig {
    pub name: String,
    pub factory_address: String,
    pub router_address: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ServerConfig {
    pub host: String,
    pub port: u16,
}

// ============================================================================
// Defi Service
// ============================================================================

pub struct DefiService {
    pool: PgPool,
    config: Config,
    protocols_cache: Arc<RwLock<Vec<DefiProtocol>>>,
    pools_cache: Arc<RwLock<Vec<DefiPool>>>,
    tvl_cache: Arc<RwLock<Option<TVLDataPoint>>>,
    metrics: Arc<ServiceMetrics>,
    shutdown_tx: mpsc::Sender<()>,
}

#[derive(Default)]
pub struct ServiceMetrics {
    pub tvl_updates: parking_lot::RwLock<u64>,
    pub pool_updates: parking_lot::RwLock<u64>,
    pub flash_loan_alerts: parking_lot::RwLock<u64>,
    pub whale_alerts: parking_lot::RwLock<u64>,
    pub errors: parking_lot::RwLock<u64>,
}

impl DefiService {
    pub async fn new(config: Config) -> Result<Self, DefiServiceError> {
        let pool = PgPoolOptions::new()
            .max_connections(config.database.max_connections)
            .acquire_timeout(Duration::from_secs(30))
            .connect(&config.database.connection_string())
            .await?;
        
        let (shutdown_tx, _) = mpsc::channel::<()>(1);
        
        Ok(Self {
            pool,
            config: config.clone(),
            protocols_cache: Arc::new(RwLock::new(Vec::new())),
            pools_cache: Arc::new(RwLock::new(Vec::new())),
            tvl_cache: Arc::new(RwLock::new(None)),
            metrics: Arc::new(ServiceMetrics::default()),
            shutdown_tx,
        })
    }
    
    /// Start the DeFi service
    pub async fn run(&self) -> Result<()> {
        info!("Starting DeFi service");
        
        // Initial data load
        self.load_protocols().await?;
        self.load_pools().await?;
        self.calculate_tvl().await?;
        
        // Start update tasks
        let pool = self.pool.clone();
        let config = self.config.defi.clone();
        let protocols_cache = self.protocols_cache.clone();
        let pools_cache = self.pools_cache.clone();
        let tvl_cache = self.tvl_cache.clone();
        let metrics = self.metrics.clone();
        
        // TVL and pool update task
        tokio::spawn(async move {
            let mut interval = interval(Duration::from_secs(config.update_interval));
            
            loop {
                interval.tick().await;
                
                if let Err(e) = Self::update_defi_data(
                    &pool,
                    &config,
                    &protocols_cache,
                    &pools_cache,
                    &tvl_cache,
                    &metrics,
                ).await {
                    error!("DeFi update error: {}", e);
                }
            }
        });
        
        // Flash loan detection task
        let flash_pool = self.pool.clone();
        let flash_config = self.config.defi.clone();
        let flash_metrics = self.metrics.clone();
        
        tokio::spawn(async move {
            let mut interval = interval(Duration::from_secs(30));
            
            loop {
                interval.tick().await;
                
                if let Err(e) = Self::detect_flash_loans(
                    &flash_pool,
                    &flash_config,
                    &flash_metrics,
                ).await {
                    error!("Flash loan detection error: {}", e);
                }
            }
        });
        
        // Whale detection task
        let whale_pool = self.pool.clone();
        let whale_config = self.config.defi.clone();
        let whale_metrics = self.metrics.clone();
        
        tokio::spawn(async move {
            let mut interval = interval(Duration::from_secs(60));
            
            loop {
                interval.tick().await;
                
                if let Err(e) = Self::detect_whales(
                    &whale_pool,
                    &whale_config,
                    &whale_metrics,
                ).await {
                    error!("Whale detection error: {}", e);
                }
            }
        });
        
        // Start API server
        self.start_server().await?;
        
        Ok(())
    }
    
    /// Load protocols from database
    async fn load_protocols(&self) -> Result<(), DefiServiceError> {
        let protocols: Vec<DefiProtocol> = sqlx::query_as(
            "SELECT id, name, category, logo_url, description, website, tvl, tvl_change_24h,
                    volume_24h, users_24h, active_positions, verified, listed_at
             FROM defi_protocols
             WHERE verified = true"
        )
        .fetch_all(&self.pool)
        .await?;
        
        *self.protocols_cache.write() = protocols;
        
        Ok(())
    }
    
    /// Load pools from database
    async fn load_pools(&self) -> Result<(), DefiServiceError> {
        let pools: Vec<DefiPool> = sqlx::query_as(
            "SELECT id, protocol, dex_name, token0, token1, reserve0, reserve1, total_supply,
                    volume_24h, volume_7d, fees_24h, liquidity, apy, apr
             FROM defi_pools
             WHERE liquidity > 0
             ORDER BY liquidity DESC
             LIMIT 1000"
        )
        .fetch_all(&self.pool)
        .await?;
        
        *self.pools_cache.write() = pools;
        
        Ok(())
    }
    
    /// Calculate total TVL
    async fn calculate_tvl(&self) -> Result<(), DefiServiceError> {
        // Get TVL by protocol
        let by_protocol: Vec<(String, f64)> = sqlx::query(
            "SELECT protocol, SUM(liquidity) FROM defi_pools GROUP BY protocol"
        )
        .fetch_all(&self.pool)
        .await?
        .into_iter()
        .map(|row| (row.get(0), row.get(1)))
        .collect();
        
        // Get TVL by token
        let by_token: Vec<(String, f64)> = sqlx::query(
            "SELECT token, SUM(liquidity) FROM (
                SELECT token0 as token, reserve0 as liquidity FROM defi_pools
                UNION ALL
                SELECT token1 as token, reserve1 as liquidity FROM defi_pools
            ) t
            GROUP BY token"
        )
        .fetch_all(&self.pool)
        .await?
        .into_iter()
        .map(|row| (row.get(0), row.get(1)))
        .collect();
        
        let total_tvl: f64 = by_protocol.iter().map(|(_, tvl)| tvl).sum();
        
        let data_point = TVLDataPoint {
            timestamp: Utc::now(),
            total_tvl,
            by_protocol: by_protocol.iter().map(|(p, t)| ProtocolTVL {
                protocol: p.clone(),
                tvl: *t,
                change_24h: 0.0,
            }).collect(),
            by_token: by_token.iter().map(|(t, v)| TokenTVL {
                token: t.clone(),
                tvl: *v,
                percentage: if total_tvl > 0.0 { v / total_tvl * 100.0 } else { 0.0 },
            }).collect(),
        };
        
        *self.tvl_cache.write() = Some(data_point);
        
        Ok(())
    }
    
    /// Update DeFi data periodically
    async fn update_defi_data(
        pool: &PgPool,
        config: &DefiConfig,
        protocols_cache: &Arc<RwLock<Vec<DefiProtocol>>>,
        pools_cache: &Arc<RwLock<Vec<DefiPool>>>,
        tvl_cache: &Arc<RwLock<Option<TVLDataPoint>>>,
        metrics: &Arc<ServiceMetrics>,
    ) -> Result<()> {
        // Fetch latest data from DEX APIs
        for dex in &config.dex_list {
            info!("Updating {} pools", dex.name);
            // Fetch pool data from DEX
        }
        
        // Calculate TVL
        let total_tvl: f64 = pools_cache.read().iter().map(|p| p.liquidity).sum();
        
        let data_point = TVLDataPoint {
            timestamp: Utc::now(),
            total_tvl,
            by_protocol: vec![],
            by_token: vec![],
        };
        
        *tvl_cache.write() = Some(data_point);
        
        metrics.tvl_updates.write().inc();
        metrics.pool_updates.write().inc();
        
        Ok(())
    }
    
    /// Detect flash loan attacks
    async fn detect_flash_loans(
        pool: &PgPool,
        config: &DefiConfig,
        metrics: &Arc<ServiceMetrics>,
    ) -> Result<()> {
        // Get recent large transactions
        let transactions: Vec<(String, u64, String, String, f64)> = sqlx::query(
            "SELECT hash, block_number, from_address, to_address, value_usd
             FROM transactions
             WHERE block_number > (SELECT MAX(block_number) FROM blocks) - 10
             AND value_usd > $1"
        )
        .bind(config.flash_loan_threshold)
        .fetch_all(pool)
        .await?
        .into_iter()
        .map(|row| (row.get(0), row.get(1), row.get(2), row.get(3), row.get(4)))
        .collect();
        
        for (hash, block, from, to, value) in transactions {
            // Check if it looks like a flash loan
            // In production, check for:
            // 1. Borrow and repay in same block
            // 2. Multiple token swaps
            // 3. Arbitrage opportunities
            
            let alert = FlashLoanAlert {
                transaction_hash: hash,
                block_number: block,
                timestamp: Utc::now(),
                pool: to.clone(),
                token: from.clone(),
                amount: value,
                profit: 0.0,
                gas_used: 0,
                gas_fee: 0.0,
            };
            
            // Store alert
            sqlx::query(
                "INSERT INTO flash_loan_alerts (transaction_hash, block_number, timestamp, pool, token, amount, profit, gas_used, gas_fee)
                 VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)"
            )
            .bind(&alert.transaction_hash)
            .bind(alert.block_number)
            .bind(alert.timestamp)
            .bind(&alert.pool)
            .bind(&alert.token)
            .bind(alert.amount)
            .bind(alert.profit)
            .bind(alert.gas_used)
            .bind(alert.gas_fee)
            .execute(pool)
            .await?;
            
            metrics.flash_loan_alerts.write().inc();
        }
        
        Ok(())
    }
    
    /// Detect whale transactions
    async fn detect_whales(
        pool: &PgPool,
        config: &DefiConfig,
        metrics: &Arc<ServiceMetrics>,
    ) -> Result<()> {
        let transactions: Vec<(String, u64, String, String, String, f64)> = sqlx::query(
            "SELECT hash, block_number, from_address, to_address, token, value_usd
             FROM token_transfers
             WHERE value_usd > $1
             AND block_number > (SELECT MAX(block_number) FROM blocks) - 100
             ORDER BY value_usd DESC
             LIMIT 100"
        )
        .bind(config.min_whale_value)
        .fetch_all(pool)
        .await?
        .into_iter()
        .map(|row| (row.get(0), row.get(1), row.get(2), row.get(3), row.get(4), row.get(5)))
        .collect();
        
        for (hash, block, from, to, token, value) in transactions {
            let whale = WhaleTransaction {
                hash,
                block_number: block,
                timestamp: Utc::now(),
                from,
                to,
                value_usd: value,
                token,
                transaction_type: "transfer".to_string(),
            };
            
            sqlx::query(
                "INSERT INTO whale_transactions (hash, block_number, timestamp, from_address, to_address, value_usd, token, transaction_type)
                 VALUES ($1, $2, $3, $4, $5, $6, $7, $8)"
            )
            .bind(&whale.hash)
            .bind(whale.block_number)
            .bind(whale.timestamp)
            .bind(&whale.from)
            .bind(&whale.to)
            .bind(whale.value_usd)
            .bind(&whale.token)
            .bind(&whale.transaction_type)
            .execute(pool)
            .await?;
            
            metrics.whale_alerts.write().inc();
        }
        
        Ok(())
    }
    
    /// Start API server
    async fn start_server(&self) -> Result<()> {
        info!("Starting DeFi API server");
        // In production, implement REST API
        Ok(())
    }
    
    // ============================================================================
    // Public API Methods
    // ============================================================================
    
    /// Get all protocols
    pub async fn get_protocols(&self) -> Vec<DefiProtocol> {
        self.protocols_cache.read().clone()
    }
    
    /// Get protocol by name
    pub async fn get_protocol(&self, name: &str) -> Option<DefiProtocol> {
        self.protocols_cache.read().iter()
            .find(|p| p.name == name)
            .cloned()
    }
    
    /// Get all pools
    pub async fn get_pools(&self, protocol: Option<&str>, limit: usize) -> Vec<DefiPool> {
        let pools = self.pools_cache.read();
        
        pools.iter()
            .filter(|p| protocol.map_or(true, |proto| p.protocol == proto))
            .take(limit)
            .cloned()
            .collect()
    }
    
    /// Get pool by address
    pub async fn get_pool(&self, address: &str) -> Option<DefiPool> {
        self.pools_cache.read().iter()
            .find(|p| p.id == address)
            .cloned()
    }
    
    /// Get TVL data
    pub async fn get_tvl(&self) -> Option<TVLDataPoint> {
        self.tvl_cache.read().clone()
    }
    
    /// Get volume history
    pub async fn get_volume_history(
        &self,
        from: DateTime<Utc>,
        to: DateTime<Utc>,
    ) -> Result<Vec<VolumeData>, DefiServiceError> {
        let volumes: Vec<VolumeData> = sqlx::query_as(
            "SELECT timestamp, volume, transactions, users
             FROM defi_volume_history
             WHERE timestamp >= $1 AND timestamp <= $2
             ORDER BY timestamp ASC"
        )
        .bind(from)
        .bind(to)
        .fetch_all(&self.pool)
        .await?;
        
        Ok(volumes)
    }
    
    /// Get flash loan alerts
    pub async fn get_flash_loans(&self, limit: usize) -> Result<Vec<FlashLoanAlert>, DefiServiceError> {
        let alerts: Vec<FlashLoanAlert> = sqlx::query_as(
            "SELECT transaction_hash, block_number, timestamp, pool, token, amount, profit, gas_used, gas_fee
             FROM flash_loan_alerts
             ORDER BY block_number DESC
             LIMIT $1"
        )
        .bind(limit as i64)
        .fetch_all(&self.pool)
        .await?;
        
        Ok(alerts)
    }
    
    /// Get whale transactions
    pub async fn get_whales(&self, min_value: f64, limit: usize) -> Result<Vec<WhaleTransaction>, DefiServiceError> {
        let whales: Vec<WhaleTransaction> = sqlx::query_as(
            "SELECT hash, block_number, timestamp, from_address, to_address, value_usd, token, transaction_type
             FROM whale_transactions
             WHERE value_usd >= $1
             ORDER BY value_usd DESC
             LIMIT $2"
        )
        .bind(min_value)
        .bind(limit as i64)
        .fetch_all(&self.pool)
        .await?;
        
        Ok(whales)
    }
    
    /// Get service metrics
    pub fn get_metrics(&self) -> ServiceMetricsResponse {
        let m = &*self.metrics;
        ServiceMetricsResponse {
            tvl_updates: *m.tvl_updates.read(),
            pool_updates: *m.pool_updates.read(),
            flash_loan_alerts: *m.flash_loan_alerts.read(),
            whale_alerts: *m.whale_alerts.read(),
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
    pub tvl_updates: u64,
    pub pool_updates: u64,
    pub flash_loan_alerts: u64,
    pub whale_alerts: u64,
    pub errors: u64,
}

// ============================================================================
// Main Entry Point
// ============================================================================

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::registry()
        .with(tracing_subscriber::EnvFilter::try_from_default_env()
            .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")))
        .with(tracing_subscriber::fmt::layer())
        .init();
    
    info!("Starting TigerScan DeFi Service");
    
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
        },
        defi: DefiConfig {
            update_interval: 60,
            min_whale_value: 10000.0,
            flash_loan_threshold: 100000.0,
            dex_list: vec![],
        },
        server: ServerConfig {
            host: "0.0.0.0".to_string(),
            port: 8084,
        },
    };
    
    let service = DefiService::new(config).await?;
    service.run().await?;
    
    Ok(())
}