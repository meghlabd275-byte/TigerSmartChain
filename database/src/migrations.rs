//! Database Migrations for TigerScan
//!
//! Migration management and execution functions.

use crate::schema::*;
use thiserror::Error;

// =============================================================================
// ERRORS
// =============================================================================

#[derive(Error, Debug)]
pub enum MigrationError {
    #[error("Migration failed: {0}")]
    Failed(String),
    #[error("Connection error: {0}")]
    ConnectionError(String),
}

// =============================================================================
// MIGRATION RUNNER
// =============================================================================

/// The name of the table that records which migrations have run.
pub const MIGRATIONS_TABLE: &str = "schema_migrations";

/// Migration runner.
///
/// Each schema statement runs in its own transaction and its name is recorded
/// in `schema_migrations`, so re-running the runner is a no-op and a partial
/// failure leaves the database at the last statement that committed.
pub struct MigrationRunner {
    pool: sqlx::PgPool,
}

impl MigrationRunner {
    /// Create a migration runner over a connection pool.
    pub fn new(pool: sqlx::PgPool) -> Self {
        Self { pool }
    }

    /// Run every migration that has not run yet.
    pub async fn run_all(&self) -> Result<Vec<String>, MigrationError> {
        self.ensure_migrations_table().await?;
        self.create_extensions().await?;

        let applied = self.applied().await?;
        let mut newly_applied = Vec::new();

        for (index, sql) in ALL_TABLES.iter().enumerate() {
            let name = format!("table_{index:03}");

            if applied.contains(&name) {
                continue;
            }

            let mut transaction = self
                .pool
                .begin()
                .await
                .map_err(|error| MigrationError::ConnectionError(error.to_string()))?;

            sqlx::query(sql)
                .execute(&mut *transaction)
                .await
                .map_err(|error| MigrationError::Failed(format!("{name}: {error}")))?;

            sqlx::query(&format!(
                "INSERT INTO {MIGRATIONS_TABLE} (name) VALUES ($1) ON CONFLICT (name) DO NOTHING"
            ))
            .bind(&name)
            .execute(&mut *transaction)
            .await
            .map_err(|error| MigrationError::Failed(format!("{name} record: {error}")))?;

            transaction
                .commit()
                .await
                .map_err(|error| MigrationError::Failed(format!("{name} commit: {error}")))?;

            newly_applied.push(name);
        }

        Ok(newly_applied)
    }

    /// Create the table that records applied migrations.
    async fn ensure_migrations_table(&self) -> Result<(), MigrationError> {
        sqlx::query(&format!(
            "CREATE TABLE IF NOT EXISTS {MIGRATIONS_TABLE} (
                 name TEXT PRIMARY KEY,
                 applied_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
             )"
        ))
        .execute(&self.pool)
        .await
        .map_err(|error| MigrationError::Failed(error.to_string()))?;

        Ok(())
    }

    /// Create the database extensions the schema depends on.
    async fn create_extensions(&self) -> Result<(), MigrationError> {
        for extension in ["uuid-ossp", "pgcrypto", "btree_gin"] {
            // The identifier is a fixed literal from this list, never a value
            // from a request, so the interpolation cannot be influenced.
            sqlx::query(&format!("CREATE EXTENSION IF NOT EXISTS \"{extension}\""))
                .execute(&self.pool)
                .await
                .map_err(|error| MigrationError::Failed(format!("{extension}: {error}")))?;
        }

        Ok(())
    }

    /// The names of the migrations that have already run.
    pub async fn applied(&self) -> Result<std::collections::HashSet<String>, MigrationError> {
        use sqlx::Row;

        let rows = sqlx::query(&format!("SELECT name FROM {MIGRATIONS_TABLE}"))
            .fetch_all(&self.pool)
            .await
            .map_err(|error| MigrationError::Failed(error.to_string()))?;

        Ok(rows
            .into_iter()
            .map(|row| row.get::<String, _>("name"))
            .collect())
    }

    /// The number of migrations that have run.
    pub async fn version(&self) -> Result<i64, MigrationError> {
        Ok(self.applied().await?.len() as i64)
    }

    /// Drop every migration's objects and clear the record.
    ///
    /// The tables are dropped in reverse dependency order so that a table which
    /// references an earlier one is removed first.
    pub async fn rollback_all(&self) -> Result<(), MigrationError> {
        let tables = [
            "pending_transactions",
            "dex_pairs",
            "sourcify_metadata",
            "alerts",
            "webhooks",
            "api_keys",
            "analytics",
            "nft_approvals",
            "token_approvals",
            "contract_creations",
            "uncles",
            "block_rewards",
            "addresses",
            "nft_transfers",
            "nft_tokens",
            "nft_collections",
            "token_holders",
            "token_transfers",
            "tokens",
            "verified_sources",
            "contracts",
            "state_diffs",
            "traces",
            "logs",
            "receipts",
            "transactions",
            "blocks",
        ];

        for table in tables {
            sqlx::query(&format!("DROP TABLE IF EXISTS {table} CASCADE"))
                .execute(&self.pool)
                .await
                .map_err(|error| MigrationError::Failed(format!("{table}: {error}")))?;
        }

        sqlx::query(&format!("DELETE FROM {MIGRATIONS_TABLE}"))
            .execute(&self.pool)
            .await
            .map_err(|error| MigrationError::Failed(error.to_string()))?;

        Ok(())
    }

    /// Seed the analytics counters with a zero row per metric.
    pub async fn seed(&self) -> Result<u64, MigrationError> {
        let mut inserted = 0;

        for statement in SEED_DATA {
            let result = sqlx::query(statement)
                .execute(&self.pool)
                .await
                .map_err(|error| MigrationError::Failed(error.to_string()))?;

            inserted += result.rows_affected();
        }

        Ok(inserted)
    }
}

// =============================================================================
// SEED DATA
// =============================================================================

/// Seed essential data
pub const SEED_DATA: &[&str] = &[
    // Insert initial analytics metrics
    "INSERT INTO analytics (metric_name, metric_value, timestamp) VALUES ('total_blocks', '0', EXTRACT(EPOCH FROM NOW())) ON CONFLICT DO NOTHING",
    "INSERT INTO analytics (metric_name, metric_value, timestamp) VALUES ('total_transactions', '0', EXTRACT(EPOCH FROM NOW())) ON CONFLICT DO NOTHING",
    "INSERT INTO analytics (metric_name, metric_value, timestamp) VALUES ('total_addresses', '0', EXTRACT(EPOCH FROM NOW())) ON CONFLICT DO NOTHING",
    "INSERT INTO analytics (metric_name, metric_value, timestamp) VALUES ('total_contracts', '0', EXTRACT(EPOCH FROM NOW())) ON CONFLICT DO NOTHING",
];

use crate::schema::ALL_TABLES;