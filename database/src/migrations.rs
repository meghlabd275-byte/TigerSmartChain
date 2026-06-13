//! Database Migrations for TigerScan
//! 
//! Migration management and execution functions.

use crate::schema::*;
use thiserror::Error;
use tokio::postgres::Client;

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

/// Migration runner
pub struct MigrationRunner {
    client: Client,
}

impl MigrationRunner {
    /// Create new migration runner
    pub fn new(client: Client) -> Self {
        Self { client }
    }

    /// Run all migrations
    pub async fn run_all(&self) -> Result<(), MigrationError> {
        // Create extensions first
        self.create_extensions().await?;
        
        // Run all table schemas
        for sql in ALL_TABLES {
            self.execute(sql).await?;
        }
        
        Ok(())
    }

    /// Create database extensions
    async fn create_extensions(&self) -> Result<(), MigrationError> {
        let extensions = r#"
            CREATE EXTENSION IF NOT EXISTS "uuid-ossp";
            CREATE EXTENSION IF NOT EXISTS "pgcrypto";
            CREATE EXTENSION IF NOT EXISTS "btree_gin";
        "#;
        
        self.execute(extensions).await
    }

    /// Execute a single migration
    async fn execute(&self, sql: &str) -> Result<(), MigrationError> {
        self.client
            .simple_query(sql)
            .await
            .map_err(|e| MigrationError::Failed(e.to_string()))?;
        
        Ok(())
    }

    /// Get migration version
    pub async fn get_version(&self) -> Result<i64, MigrationError> {
        // In production, track migration version in a table
        Ok(1)
    }

    /// Rollback to version
    pub async fn rollback_to(&self, _version: i64) -> Result<(), MigrationError> {
        // In production, implement proper rollback
        Ok(())
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