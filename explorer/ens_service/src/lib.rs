//! TigerScan ENS Resolution Service

use std::sync::Arc;
use anyhow::Result;
use serde::{Deserialize, Serialize};
use sqlx::postgres::{PgPool, PgPoolOptions};
use parking_lot::RwLock;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ENSRecord {
    pub name: String,
    pub address: Option<String>,
    pub resolver: Option<String>,
    pub owner: Option<String>,
    pub ttl: Option<u64>,
    pub content_hash: Option<String>,
}

pub struct ENSService {
    db: PgPool,
    cache: Arc<RwLock<ENSCache>>,
    rpc_url: String,
}

#[derive(Default)]
pub struct ENSCache {
    records: std::collections::HashMap<String, ENSRecord>,
}

impl ENSService {
    pub async fn new(rpc_url: String, db_url: &str) -> Result<Self> {
        let db = PgPoolOptions::new()
            .max_connections(5)
            .connect(db_url)
            .await?;
        Ok(Self {
            db,
            cache: Arc::new(RwLock::new(ENSCache::default())),
            rpc_url,
        })
    }

    pub async fn resolve(&self, name: &str) -> Result<ENSRecord> {
        // Check cache first
        if let Some(record) = self.cache.read().records.get(name) {
            return Ok(record.clone());
        }
        
        // In production, query ENS registry contract
        let record = ENSRecord {
            name: name.to_string(),
            address: None,
            resolver: None,
            owner: None,
            ttl: None,
            content_hash: None,
        };
        
        // Cache it
        self.cache.write().records.insert(name.to_string(), record.clone());
        Ok(record)
    }

    pub async fn reverse_lookup(&self, address: &str) -> Result<Option<String>> {
        let name = format!("{:?}.addr.reverse", address);
        Ok(Some(name))
    }
}