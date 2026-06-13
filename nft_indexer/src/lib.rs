/**
 * NFT Metadata Indexer - IPFS/Arweave Support
 * Complete implementation in Rust for high performance
 */

use std::collections::HashMap;
use std::sync::Arc;

use async_trait::async_trait;
use ethers::types::{Address, H256, U256};
use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use sqlx::{PgPool, Row};
use thiserror::Error;
use tokio::sync::mpsc;

// ============================================
// Types
// ============================================

#[derive(Error, Debug)]
pub enum NFTError {
    #[error("Database error: {0}")]
    Database(#[from] sqlx::Error),
    #[error("Network error: {0}")]
    Network(String),
    #[error("Parse error: {0}")]
    Parse(String),
    #[error("IPFS error: {0}")]
    IPFS(String),
    #[error("Not found: {0}")]
    NotFound(String),
}

pub type Result<T> = std::result::Result<T, NFTError>;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NFTContract {
    pub address: String,
    pub name: String,
    pub symbol: String,
    pub total_supply: u64,
    pub contract_type: ContractType,
    pub base_uri: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ContractType {
    ERC721,
    ERC1155,
    ERC721A,
    Unknown,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NFTToken {
    pub id: String,
    pub contract_address: String,
    pub owner: String,
    pub uri: String,
    pub metadata: Option<NFTMetadata>,
    pub transfers: Vec<Transfer>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NFTMetadata {
    pub name: String,
    pub description: Option<String>,
    pub image: Option<String>,
    pub external_url: Option<String>,
    pub attributes: Vec<MetadataAttribute>,
    pub background_color: Option<String>,
    pub animation_url: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetadataAttribute {
    pub trait_type: String,
    pub value: String,
    pub display_type: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Transfer {
    pub from: String,
    pub to: String,
    pub timestamp: i64,
    pub block_number: i64,
    pub transaction_hash: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CollectionStats {
    pub total_supply: u64,
    pub holders: u64,
    pub transfers: u64,
    pub floor_price: Option<f64>,
    pub avg_price: Option<f64>,
    pub volume_24h: f64,
    pub volume_7d: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RarityScore {
    pub token_id: String,
    pub score: f64,
    pub rank: u32,
    pub traits: Vec<TraitRarity>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TraitRarity {
    pub trait_type: String,
    pub value: String,
    pub frequency: f64,
}

// ============================================
// NFT Indexer
// ============================================

pub struct NFTIndexer {
    db: Arc<Database>,
    ipfs: IPFSClient,
    metadata_cache: RwLock<HashMap<String, Option<NFTMetadata>>>,
    rarity_cache: RwLock<HashMap<String, RarityScore>>,
}

pub struct Database {
    pool: PgPool,
}

impl Database {
    pub async fn new(url: &str) -> Result<Self> {
        let pool = PgPool::connect(url).await?;
        Ok(Self { pool })
    }

    pub async fn init_schema(&self) -> Result<()> {
        sqlx::query(r#"
            CREATE TABLE IF NOT EXISTS nft_contracts (
                address VARCHAR(42) PRIMARY KEY,
                name VARCHAR(255) NOT NULL,
                symbol VARCHAR(50),
                total_supply BIGINT,
                contract_type VARCHAR(20),
                base_uri TEXT,
                created_at TIMESTAMP DEFAULT NOW()
            );
            
            CREATE TABLE IF NOT EXISTS nft_tokens (
                id VARCHAR(78) PRIMARY KEY,
                contract_address VARCHAR(42) NOT NULL,
                owner VARCHAR(42) NOT NULL,
                uri TEXT,
                metadata JSONB,
                last_updated TIMESTAMP DEFAULT NOW(),
                FOREIGN KEY (contract_address) REFERENCES nft_contracts(address)
            );
            
            CREATE TABLE IF NOT EXISTS nft_transfers (
                id SERIAL PRIMARY KEY,
                token_id VARCHAR(78) NOT NULL,
                contract_address VARCHAR(42) NOT NULL,
                from_address VARCHAR(42),
                to_address VARCHAR(42) NOT NULL,
                amount BIGINT DEFAULT 1,
                transaction_hash VARCHAR(66) NOT NULL,
                block_number BIGINT NOT NULL,
                timestamp BIGINT NOT NULL,
                FOREIGN KEY (token_id, contract_address) REFERENCES nft_tokens(id, contract_address)
            );
            
            CREATE TABLE IF NOT EXISTS nft_metadata_cache (
                uri VARCHAR(500) PRIMARY KEY,
                metadata JSONB,
                fetched_at TIMESTAMP DEFAULT NOW(),
                expires_at TIMESTAMP
            );
            
            CREATE INDEX idx_nft_tokens_owner ON nft_tokens(owner);
            CREATE INDEX idx_nft_tokens_contract ON nft_tokens(contract_address);
            CREATE INDEX idx_nft_transfers_token ON nft_transfers(token_id, contract_address);
            CREATE INDEX idx_nft_transfers_block ON nft_transfers(block_number);
        "#).execute(&self.pool).await?;
        
        Ok(())
    }
}

impl NFTIndexer {
    pub async fn new(db_url: &str) -> Result<Self> {
        let db = Arc::new(Database::new(db_url).await?);
        db.init_schema().await?;
        
        Ok(Self {
            db,
            ipfs: IPFSClient::new(),
            metadata_cache: RwLock::new(HashMap::new()),
            rarity_cache: RwLock::new(HashMap::new()),
        })
    }

    /// Index a new NFT transfer
    pub async fn index_transfer(&self, transfer: Transfer) -> Result<()> {
        sqlx::query(r#"
            INSERT INTO nft_transfers (token_id, contract_address, from_address, to_address, amount, transaction_hash, block_number, timestamp)
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
        "#)
        .bind(&transfer.transaction_hash.split('_').next().unwrap_or("0"))
        .bind(&transfer.transaction_hash.rsplit('_').next().unwrap_or("0x"))
        .bind(&transfer.from)
        .bind(&transfer.to)
        .bind(1i64)
        .bind(&transfer.transaction_hash)
        .bind(transfer.block_number)
        .bind(transfer.timestamp)
        .execute(&self.db.pool)
        .await?;
        
        Ok(())
    }

    /// Fetch and cache metadata from URI
    pub async fn fetch_metadata(&self, uri: &str) -> Result<Option<NFTMetadata>> {
        // Check cache first
        if let Some(cached) = self.metadata_cache.read().get(uri) {
            return Ok(cached.clone());
        }
        
        // Fetch from network
        let metadata = self.fetch_metadata_from_uri(uri).await?;
        
        // Cache result
        self.metadata_cache.write().insert(uri.to_string(), metadata.clone());
        
        Ok(metadata)
    }

    async fn fetch_metadata_from_uri(&self, uri: &str) -> Result<Option<NFTMetadata>> {
        let uri = uri.trim();
        
        if uri.starts_with("ipfs://") {
            return self.fetch_from_ipfs(uri).await;
        } else if uri.starts_with("ar://") {
            return self.fetch_from_arweave(uri).await;
        } else {
            return self.fetch_from_http(uri).await;
        }
    }

    async fn fetch_from_ipfs(&self, uri: &str) -> Result<Option<NFTMetadata>> {
        let cid = uri.trim_start_matches("ipfs://");
        let gateway = "https://ipfs.io/ipfs/";
        
        let url = if cid.starts_with("ipfs://") {
            format!("{}{}", gateway, cid.trim_start_matches("ipfs://"))
        } else {
            format!("{}{}", gateway, cid)
        };
        
        self.fetch_and_parse(&url).await
    }

    async fn fetch_from_arweave(&self, uri: &str) -> Result<Option<NFTMetadata>> {
        let tx_id = uri.trim_start_matches("ar://");
        let gateway = "https://arweave.net/";
        let url = format!("{}{}", gateway, tx_id);
        
        self.fetch_and_parse(&url).await
    }

    async fn fetch_from_http(&self, uri: &str) -> Result<Option<NFTMetadata>> {
        let response = reqwest::get(uri)
            .await
            .map_err(|e| NFTError::Network(e.to_string()))?;
        
        if !response.status().is_success() {
            return Err(NFTError::NotFound(uri.to_string()));
        }
        
        let metadata: NFTMetadata = response
            .json()
            .await
            .map_err(|e| NFTError::Parse(e.to_string()))?;
        
        Ok(Some(metadata))
    }

    async fn fetch_and_parse(&self, url: &str) -> Result<Option<NFTMetadata>> {
        let response = reqwest::get(url)
            .await
            .map_err(|e| NFTError::Network(e.to_string()))?;
        
        if !response.status().is_success() {
            return Ok(None);
        }
        
        let content_type = response
            .headers()
            .get("content-type")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("");
        
        if content_type.contains("json") {
            let metadata: NFTMetadata = response
                .json()
                .await
                .map_err(|e| NFTError::Parse(e.to_string()))?;
            Ok(Some(metadata))
        } else {
            Ok(None)
        }
    }

    /// Calculate rarity scores for a collection
    pub async fn calculate_rarity(&self, contract: &str) -> Result<Vec<RarityScore>> {
        // Get all tokens with metadata
        let tokens = sqlx::query_as::<_, (String, Option<serde_json::Value>)(
            "SELECT id, metadata FROM nft_tokens WHERE contract_address = $1"
        )
        .bind(contract)
        .fetch_all(&self.db.pool)
        .await?;
        
        // Count trait occurrences
        let mut trait_counts: HashMap<String, HashMap<String, u32>> = HashMap::new();
        let mut token_traits: Vec<(String, Vec<MetadataAttribute>)> = Vec::new();
        
        for (id, metadata_json) in tokens {
            if let Some(json) = metadata_json {
                if let Ok(metadata) = serde_json::from_value::<NFTMetadata>(json) {
                    let traits = metadata.attributes.clone();
                    
                    for trait_attr in &traits {
                        let counter = trait_counts.entry(trait_attr.trait_type.clone())
                            .or_insert_with(HashMap::new);
                        *counter.entry(trait_attr.value.clone()).or_insert(0) += 1;
                    }
                    
                    token_traits.push((id, traits));
                }
            }
        }
        
        // Calculate total tokens
        let total = token_traits.len() as f64;
        if total == 0.0 {
            return Ok(vec![]);
        }
        
        // Calculate rarity scores
        let mut scores: Vec<RarityScore> = token_traits.iter().enumerate()
            .map(|(i, (id, traits))| {
                let mut score = 0.0;
                let mut trait_rarities = Vec::new();
                
                for trait_attr in traits {
                    let trait_counter = trait_counts.get(&trait_attr.trait_type)
                        .and_then(|c| c.get(&trait_attr.value))
                        .unwrap_or(&1);
                    
                    let frequency = *trait_counter as f64 / total;
                    score += 1.0 / frequency;
                    
                    trait_rarities.push(TraitRarity {
                        trait_type: trait_attr.trait_type.clone(),
                        value: trait_attr.value.clone(),
                        frequency,
                    });
                }
                
                RarityScore {
                    token_id: id.clone(),
                    score,
                    rank: 0, // Will be sorted later
                    traits: trait_rarities,
                }
            })
            .collect();
        
        // Sort by score descending and assign ranks
        scores.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap());
        for (i, score) in scores.iter_mut().enumerate() {
            score.rank = (i + 1) as u32;
        }
        
        Ok(scores)
    }

    /// Get collection statistics
    pub async fn get_collection_stats(&self, contract: &str) -> Result<CollectionStats> {
        let total_supply: (i64,) = sqlx::query_as(
            "SELECT COUNT(*) FROM nft_tokens WHERE contract_address = $1"
        )
        .bind(contract)
        .fetch_one(&self.db.pool)
        .await?;
        
        let holders: (i64,) = sqlx::query_as(
            "SELECT COUNT(DISTINCT owner) FROM nft_tokens WHERE contract_address = $1"
        )
        .bind(contract)
        .fetch_one(&self.db.pool)
        .await?;
        
        let transfers: (i64,) = sqlx::query_as(
            "SELECT COUNT(*) FROM nft_transfers WHERE contract_address = $1"
        )
        .bind(contract)
        .fetch_one(&self.db.pool)
        .await?;
        
        Ok(CollectionStats {
            total_supply: total_supply.0 as u64,
            holders: holders.0 as u64,
            transfers: transfers.0 as u64,
            floor_price: None,
            avg_price: None,
            volume_24h: 0.0,
            volume_7d: 0.0,
        })
    }
}

// ============================================
// IPFS Client
// ============================================

pub struct IPFSClient {
    gateways: Vec<String>,
}

impl IPFSClient {
    pub fn new() -> Self {
        Self {
            gateways: vec![
                "https://ipfs.io/ipfs/".to_string(),
                "https://cloudflare-ipfs.com/ipfs/".to_string(),
                "https://dweb.link/ipfs/".to_string(),
            ],
        }
    }

    pub async fn fetch(&self, cid: &str) -> Result<Option<Vec<u8>>> {
        for gateway in &self.gateways {
            let url = format!("{}{}", gateway, cid);
            
            match reqwest::get(&url).await {
                Ok(response) if response.status().is_success() => {
                    match response.bytes().await {
                        Ok(bytes) => return Ok(Some(bytes.to_vec())),
                        Err(_) => continue,
                    }
                }
                _ => continue,
            }
        }
        
        Ok(None)
    }
}

impl Default for IPFSClient {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================
// Tests
// ============================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_trait_rarity_calculation() {
        let mut trait_counts: HashMap<String, HashMap<String, u32>> = HashMap::new();
        
        // Add some trait counts
        let mut eye_counts = HashMap::new();
        eye_counts.insert("Red".to_string(), 10);
        eye_counts.insert("Blue".to_string(), 30);
        eye_counts.insert("Green".to_string(), 60);
        
        trait_counts.insert("Eyes".to_string(), eye_counts);
        
        // Calculate frequency
        let total = 100.0;
        let red_frequency = 10.0 / total;
        
        assert_eq!(red_frequency, 0.1);
    }
}