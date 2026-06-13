//! ENS Database Operations
//! PostgreSQL database for ENS records

use async_trait::async_trait;
use sqlx::{PgPool, Row};

use crate::errors::{Error, Result};
use crate::types::{ENSRecord, ENSDomain};

pub struct Database {
    pool: PgPool,
}

impl Database {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
    
    /// Initialize database schema
    pub async fn init(&self) -> Result<()> {
        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS ens_records (
                id SERIAL PRIMARY KEY,
                name VARCHAR(255) UNIQUE NOT NULL,
                address VARCHAR(42),
                resolver VARCHAR(42),
                owner VARCHAR(42),
                ttl BIGINT,
                content_hash VARCHAR(64),
                avatar VARCHAR(255),
                description TEXT,
                email VARCHAR(255),
                url VARCHAR(255),
                version INTEGER DEFAULT 0,
                created_at TIMESTAMP DEFAULT NOW(),
                updated_at TIMESTAMP DEFAULT NOW()
            )
            "#,
        )
        .execute(&self.pool)
        .await
        .map_err(|e| Error::database(format!("Failed to create table: {}", e)))?;
        
        sqlx::query(
            "CREATE INDEX IF NOT EXISTS idx_ens_name ON ens_records(name)"
        )
        .execute(&self.pool)
        .await
        .map_err(|e| Error::database(format!("Failed to create index: {}", e)))?;
        
        Ok(())
    }
    
    /// Save ENS record
    pub async fn save_record(&self, record: &ENSRecord) -> Result<()> {
        sqlx::query(
            r#"
            INSERT INTO ens_records (name, address, resolver, owner, ttl, content_hash, 
                avatar, description, email, url, updated_at)
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, NOW())
            ON CONFLICT (name) DO UPDATE SET
                address = EXCLUDED.address,
                resolver = EXCLUDED.resolver,
                owner = EXCLUDED.owner,
                ttl = EXCLUDED.ttl,
                content_hash = EXCLUDED.content_hash,
                avatar = EXCLUDED.avatar,
                description = EXCLUDED.description,
                email = EXCLUDED.email,
                url = EXCLUDED.url,
                updated_at = NOW()
            "#,
        )
        .bind(&record.name)
        .bind(&record.address)
        .bind(&record.resolver)
        .bind(&record.owner)
        .bind(&record.ttl)
        .bind(&record.content_hash)
        .bind(&record.avatar)
        .bind(&record.description)
        .bind(&record.email)
        .bind(&record.url)
        .execute(&self.pool)
        .await
        .map_err(|e| Error::database(format!("Failed to save record: {}", e)))?;
        
        Ok(())
    }
    
    /// Get ENS record
    pub async fn get_record(&self, name: &str) -> Result<Option<ENSRecord>> {
        let row = sqlx::query(
            "SELECT name, address, resolver, owner, ttl, content_hash, avatar, description, email, url FROM ens_records WHERE name = $1"
        )
        .bind(name)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| Error::database(format!("Failed to get record: {}", e)))?;
        
        Ok(row.map(|r| ENSRecord {
            name: r.get(0),
            address: r.get(1),
            resolver: r.get(2),
            owner: r.get(3),
            ttl: r.get(4),
            content_hash: r.get(5),
            text_records: None,
            coin_addresses: None,
            interface: None,
            abi: None,
            avatar: r.get(6),
            email: r.get(8),
            description: r.get(7),
            notice: None,
            keywords: None,
            url: r.get(9),
            version: None,
            created_at: None,
            updated_at: None,
        }))
    }
    
    /// Get domain info
    pub async fn get_domain(&self, name: &str) -> Result<Option<ENSDomain>> {
        let row = sqlx::query(
            "SELECT name, owner, resolver, ttl, expiry_date FROM ens_domains WHERE name = $1"
        )
        .bind(name)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| Error::database(format!("Failed to get domain: {}", e)))?;
        
        Ok(row.map(|r| ENSDomain {
            name: r.get(0),
            label_hash: None,
            name_hash: None,
            owner: r.get(1),
            resolver: r.get(2),
            ttl: r.get(3),
            is_eth_2ld: name.ends_with(".eth"),
            registration_date: None,
            expiry_date: r.get(4),
            is_available: r.get::<Option<String>>(1).is_none(),
        }))
    }
}