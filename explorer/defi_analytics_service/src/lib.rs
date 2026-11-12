//! TigerScan DeFi Analytics Service
//! Production-grade DeFi analytics - TVL, lending rates, pool analytics, yield farming

use std::sync::Arc;
use std::time::Duration;

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

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
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

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct LendingRate {
    pub protocol: String,
    pub token: String,
    pub supply_rate: f64,
    pub borrow_rate: f64,
    pub utilization: f64,
    pub total_supply: f64,
    pub total_borrow: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
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
    /// The borrowed asset, identified by its contract address.
    pub asset: String,
    pub amount: f64,
    pub fee: f64,
    pub profit_usd: f64,
    pub timestamp: i64,
}

// ============================================================================
// DeFi Analytics Service
// ============================================================================

pub struct DeFiAnalyticsService {
    config: Config,
    db: PgPool,
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

        Ok(Self {
            config,
            db,
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

            // The 24h change is measured against this protocol's own snapshot
            // for yesterday, not against the network-wide daily aggregate.
            let previous_tvl: Option<f64> = sqlx::query_scalar(
                "SELECT tvl::float8 FROM defi_tvl_history \
                 WHERE protocol = $1 AND date = CURRENT_DATE - 1",
            )
            .bind(&protocol.name)
            .fetch_optional(&self.db)
            .await?;

            let tvl_change_24h = match previous_tvl {
                Some(previous) if previous > 0.0 => ((tvl - previous) / previous) * 100.0,
                _ => 0.0,
            };

            // The snapshot is written first so that a failure while writing the
            // current value cannot leave the change computed from stale data.
            sqlx::query(
                "INSERT INTO defi_tvl_history (protocol, tvl, date) VALUES ($1, $2, CURRENT_DATE) \
                 ON CONFLICT (protocol, date) DO UPDATE SET tvl = EXCLUDED.tvl",
            )
            .bind(&protocol.name)
            .bind(tvl)
            .execute(&self.db)
            .await?;

            sqlx::query(
                "INSERT INTO defi_tvl (protocol, tvl, tvl_change_24h, updated_at) \
                 VALUES ($1, $2, $3, NOW()) \
                 ON CONFLICT (protocol) DO UPDATE SET tvl = EXCLUDED.tvl, \
                 tvl_change_24h = EXCLUDED.tvl_change_24h, updated_at = NOW()",
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

    /// Sum the USD liquidity of the pairs that belong to one factory.
    async fn calculate_protocol_tvl(&self, factory: &str) -> Result<f64> {
        let total_liquidity: Option<f64> = sqlx::query_scalar(
            "SELECT COALESCE(SUM(liquidity_usd), 0)::float8 FROM dex_pairs \
             WHERE factory_address = $1",
        )
        .bind(factory)
        .fetch_one(&self.db)
        .await?;

        Ok(total_liquidity.unwrap_or(0.0))
    }

    pub async fn update_pools(&self) -> Result<()> {
        info!("Updating pool data...");

        // `fee_bps` is the pool's own fee tier, so the fee taken over the last
        // day is the pool's volume times its own rate.
        let pools: Vec<(String, f64, f64, i32)> = sqlx::query_as(
            "SELECT pair_address, volume_24h::float8, liquidity_usd::float8, fee_bps \
             FROM dex_pairs WHERE liquidity_usd > 1000",
        )
        .fetch_all(&self.db)
        .await?;

        for (pair, volume, liquidity, fee_bps) in pools {
            let fee_24h = volume * (f64::from(fee_bps) / 10_000.0);

            // Annualised fees over the pool's liquidity, the standard
            // "fee APR" figure that DEX front ends quote.
            let apy = if liquidity > 0.0 {
                (fee_24h * 365.0 / liquidity) * 100.0
            } else {
                0.0
            };

            sqlx::query(
                "UPDATE dex_pairs SET apy = $1, fee_24h = $2, updated_at = NOW() \
                 WHERE pair_address = $3",
            )
            .bind(apy)
            .bind(fee_24h)
            .bind(&pair)
            .execute(&self.db)
            .await?;

            self.state.write().pools_tracked += 1;
        }

        Ok(())
    }

    /// Find flash loans in the recently indexed blocks.
    ///
    /// A flash loan is a borrow and a repayment of at least the borrowed
    /// amount inside one transaction, so it is read out of the trace rather
    /// than guessed from the transaction value.
    pub async fn detect_flash_loans(&self) -> Result<usize> {
        info!("Detecting flash loans...");

        let candidates: Vec<(String, i64)> = sqlx::query_as(
            "SELECT transaction_hash, block_number FROM internal_transactions \
             WHERE block_number > (SELECT COALESCE(MAX(number), 0) - 10 FROM blocks) \
             GROUP BY transaction_hash, block_number",
        )
        .fetch_all(&self.db)
        .await?;

        let mut detected = 0;

        for (transaction_hash, block_number) in candidates {
            let traces: Vec<(String, Option<String>, Option<String>, f64, f64)> = sqlx::query_as(
                "SELECT call_type, from_address, to_address, \
                        COALESCE(value, 0)::float8, COALESCE(CAST(gas_used AS NUMERIC), 0)::float8 \
                 FROM internal_transactions \
                 WHERE transaction_hash = $1 AND success \
                 ORDER BY trace_index",
            )
            .bind(&transaction_hash)
            .fetch_all(&self.db)
            .await?;

            let Some(loan) = self.identify_flash_loan(&transaction_hash, block_number, &traces)
            else {
                continue;
            };

            // The unique key is the transaction and the asset, so a re-run of
            // the detector updates the same row instead of duplicating it.
            sqlx::query(
                "INSERT INTO flash_loans \
                 (transaction_hash, block_number, borrower, asset, amount, fee, profit_usd) \
                 VALUES ($1, $2, $3, $4, $5, $6, $7) \
                 ON CONFLICT (transaction_hash, asset) DO UPDATE \
                 SET amount = EXCLUDED.amount, fee = EXCLUDED.fee, \
                     profit_usd = EXCLUDED.profit_usd",
            )
            .bind(&loan.transaction_hash)
            .bind(loan.block_number as i64)
            .bind(&loan.borrower)
            .bind(&loan.asset)
            .bind(loan.amount)
            .bind(loan.fee)
            .bind(loan.profit_usd)
            .execute(&self.db)
            .await?;

            detected += 1;
        }

        Ok(detected)
    }

    /// Match the borrow and repayment legs of a flash loan inside one trace.
    ///
    /// The borrower is the account that received the borrowed asset and paid it
    /// back, so the two legs are matched on the pair of addresses. A repayment
    /// smaller than the borrow is not a flash loan; it is a liquidation.
    fn identify_flash_loan(
        &self,
        transaction_hash: &str,
        block_number: i64,
        traces: &[(String, Option<String>, Option<String>, f64, f64)],
    ) -> Option<FlashLoan> {
        for (_, _, to_address, amount, _) in traces {
            let Some(asset) = to_address.as_ref() else {
                continue;
            };

            if *amount <= 0.0 {
                continue;
            }

            let borrower = traces
                .iter()
                .find(|(call_type, _, _, _, _)| call_type == "CALL")
                .and_then(|(_, from, _, _, _)| from.clone());

            let Some(borrower) = borrower else {
                continue;
            };

            let repaid: f64 = traces
                .iter()
                .filter(|(_, from, to, _, _)| {
                    from.as_deref() == Some(borrower.as_str())
                        && to.as_deref() == Some(asset.as_str())
                })
                .map(|(_, _, _, value, _)| *value)
                .sum();

            if repaid >= *amount {
                return Some(FlashLoan {
                    transaction_hash: transaction_hash.to_string(),
                    block_number: block_number as u64,
                    borrower,
                    asset: asset.clone(),
                    amount: *amount,
                    fee: 0.0,
                    profit_usd: 0.0,
                    timestamp: Utc::now().timestamp(),
                });
            }
        }

        None
    }

    pub async fn get_top_yields(&self, limit: usize) -> Result<Vec<YieldOpportunity>> {
        // `apy_7d` comes from the pool's 7-day volume, not from its 24h
        // volume; substituting one for the other would overstate a pool that
        // had a single busy day.
        let yields: Vec<YieldOpportunity> = sqlx::query_as(
            r#"
            SELECT dp.protocol_name AS protocol, dp.pair_address AS pool,
                   dp.token0_symbol AS token0, dp.token1_symbol AS token1,
                   COALESCE(dp.apy, 0)::float8 AS apy,
                   COALESCE(
                       CASE WHEN dp.liquidity_usd > 0
                            THEN (dp.volume_7d * (dp.fee_bps::float8 / 10000.0) * 365.0 / 7.0
                                  / dp.liquidity_usd) * 100.0
                            ELSE 0
                       END, 0)::float8 AS apy_7d,
                   dp.liquidity_usd::float8 AS tvl,
                   CASE WHEN dp.liquidity_usd >= 1000000 THEN 'low'
                        WHEN dp.liquidity_usd >= 100000 THEN 'medium'
                        ELSE 'high'
                   END AS risk_level
            FROM dex_pairs dp
            WHERE dp.apy > 0 AND dp.liquidity_usd > 10000
            ORDER BY dp.apy DESC
            LIMIT $1
            "#,
        )
        .bind(limit as i64)
        .fetch_all(&self.db)
        .await?;

        Ok(yields)
    }

    /// Current TVL per configured protocol, with its 24h change.
    pub async fn get_tvl_data(&self) -> Result<Vec<TVLData>> {
        let rows: Vec<(String, f64, f64, f64)> = sqlx::query_as(
            "SELECT protocol, tvl::float8, COALESCE(tvl_change_24h, 0)::float8, \
                    COALESCE(volume_24h, 0)::float8 \
             FROM defi_tvl ORDER BY tvl DESC",
        )
        .fetch_all(&self.db)
        .await?;

        let mut data = Vec::with_capacity(rows.len());

        for (protocol, tvl, tvl_change_24h, volume_24h) in rows {
            let tvl_change_7d = self.tvl_change(&protocol, 7).await?;
            let tokens = self.protocol_tokens(&protocol).await?;

            data.push(TVLData {
                protocol,
                tvl,
                tvl_change_24h,
                tvl_change_7d,
                volume_24h,
                tokens,
            });
        }

        Ok(data)
    }

    /// Percentage change of a protocol's TVL against its snapshot `days` ago.
    async fn tvl_change(&self, protocol: &str, days: i32) -> Result<f64> {
        let current: Option<f64> = sqlx::query_scalar(
            "SELECT tvl::float8 FROM defi_tvl_history \
             WHERE protocol = $1 ORDER BY date DESC LIMIT 1",
        )
        .bind(protocol)
        .fetch_optional(&self.db)
        .await?;

        let past: Option<f64> = sqlx::query_scalar(
            "SELECT tvl::float8 FROM defi_tvl_history \
             WHERE protocol = $1 AND date <= CURRENT_DATE - $2 \
             ORDER BY date DESC LIMIT 1",
        )
        .bind(protocol)
        .bind(days)
        .fetch_optional(&self.db)
        .await?;

        match (current, past) {
            (Some(current), Some(past)) if past > 0.0 => Ok(((current - past) / past) * 100.0),
            _ => Ok(0.0),
        }
    }

    /// The largest tokens by liquidity in a protocol's pools.
    async fn protocol_tokens(&self, protocol: &str) -> Result<Vec<TokenTVL>> {
        let total: Option<f64> = sqlx::query_scalar(
            "SELECT COALESCE(SUM(liquidity_usd), 0)::float8 FROM dex_pairs \
             WHERE protocol_name = $1",
        )
        .bind(protocol)
        .fetch_one(&self.db)
        .await?;

        let total = total.unwrap_or(0.0);

        let rows: Vec<(String, Option<String>, f64)> = sqlx::query_as(
            "SELECT token0_address, token0_symbol, COALESCE(SUM(liquidity_usd), 0)::float8 \
             FROM dex_pairs WHERE protocol_name = $1 GROUP BY token0_address, token0_symbol \
             UNION ALL \
             SELECT token1_address, token1_symbol, COALESCE(SUM(liquidity_usd), 0)::float8 \
             FROM dex_pairs WHERE protocol_name = $1 GROUP BY token1_address, token1_symbol",
        )
        .bind(protocol)
        .fetch_all(&self.db)
        .await?;

        let mut by_token: std::collections::BTreeMap<String, (String, f64)> =
            std::collections::BTreeMap::new();

        for (address, symbol, tvl) in rows {
            let entry = by_token
                .entry(address)
                .or_insert_with(|| (symbol.unwrap_or_default(), 0.0));

            entry.1 += tvl;
        }

        let mut tokens: Vec<TokenTVL> = by_token
            .into_iter()
            .map(|(address, (symbol, tvl))| TokenTVL {
                address,
                symbol,
                tvl,
                weight: if total > 0.0 { tvl / total } else { 0.0 },
            })
            .collect();

        tokens.sort_by(|left, right| {
            right
                .tvl
                .partial_cmp(&left.tvl)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        tokens.truncate(20);

        Ok(tokens)
    }

    /// The pools with the most liquidity.
    pub async fn get_top_pools(&self, limit: usize) -> Result<Vec<PoolData>> {
        let pools: Vec<PoolData> = sqlx::query_as(
            "SELECT pair_address AS address, COALESCE(protocol_name, '') AS protocol, \
                    COALESCE(token0_symbol, '') AS token0, COALESCE(token1_symbol, '') AS token1, \
                    COALESCE(reserve0, 0)::float8 AS reserve0, \
                    COALESCE(reserve1, 0)::float8 AS reserve1, \
                    COALESCE(liquidity_usd, 0)::float8 AS liquidity_usd, \
                    COALESCE(volume_24h, 0)::float8 AS volume_24h, \
                    COALESCE(volume_change_24h, 0)::float8 AS volume_change_24h, \
                    COALESCE(fee_24h, 0)::float8 AS fee_24h, \
                    COALESCE(apy, 0)::float8 AS apy \
             FROM dex_pairs ORDER BY liquidity_usd DESC LIMIT $1",
        )
        .bind(limit as i64)
        .fetch_all(&self.db)
        .await?;

        Ok(pools)
    }

    /// Pools whose token symbols match a search term.
    pub async fn search_pools(&self, query: &str) -> Result<Vec<PoolData>> {
        // `ILIKE` on the escaped pattern keeps a user-supplied `%` or `_` from
        // turning into a wildcard.
        let pattern = format!(
            "%{}%",
            query
                .replace('\\', "\\\\")
                .replace('%', "\\%")
                .replace('_', "\\_")
        );

        let pools: Vec<PoolData> = sqlx::query_as(
            "SELECT pair_address AS address, COALESCE(protocol_name, '') AS protocol, \
                    COALESCE(token0_symbol, '') AS token0, COALESCE(token1_symbol, '') AS token1, \
                    COALESCE(reserve0, 0)::float8 AS reserve0, \
                    COALESCE(reserve1, 0)::float8 AS reserve1, \
                    COALESCE(liquidity_usd, 0)::float8 AS liquidity_usd, \
                    COALESCE(volume_24h, 0)::float8 AS volume_24h, \
                    COALESCE(volume_change_24h, 0)::float8 AS volume_change_24h, \
                    COALESCE(fee_24h, 0)::float8 AS fee_24h, \
                    COALESCE(apy, 0)::float8 AS apy \
             FROM dex_pairs \
             WHERE token0_symbol ILIKE $1 ESCAPE '\\' OR token1_symbol ILIKE $1 ESCAPE '\\' \
             ORDER BY liquidity_usd DESC LIMIT 50",
        )
        .bind(pattern)
        .fetch_all(&self.db)
        .await?;

        Ok(pools)
    }

    pub async fn get_lending_rates(&self) -> Result<Vec<LendingRate>> {
        // Get lending rates from database (populated by lending protocol indexer)
        let rates: Vec<LendingRate> = sqlx::query_as(
            "SELECT protocol, token, supply_rate::float8, borrow_rate::float8, \
             utilization::float8, total_supply::float8, total_borrow::float8 \
             FROM lending_rates \
             ORDER BY protocol, token"
        )
        .fetch_all(&self.db)
        .await?;

        Ok(rates)
    }

    pub async fn get_tvl_history(&self, protocol: &str, days: i32) -> Result<Vec<TVLHistory>> {
        // The daily snapshot table is the only source that can answer a range
        // query; `defi_tvl` holds the current value only.
        let history: Vec<TVLHistory> = sqlx::query_as(
            "SELECT protocol, tvl::float8 AS tvl, date::text AS date \
             FROM defi_tvl_history \
             WHERE protocol = $1 AND date >= CURRENT_DATE - $2::integer \
             ORDER BY date",
        )
        .bind(protocol)
        .bind(days)
        .fetch_all(&self.db)
        .await?;

        Ok(history)
    }

    pub fn get_state(&self) -> AnalyticsState {
        self.state.read().clone()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct TVLHistory {
    pub protocol: String,
    pub tvl: f64,
    pub date: String,
}