//! NFT Metadata Indexer Implementation

use crate::types::*;
use crate::ipfs::IPFSClient;
use crate::arweave::ArweaveClient;
use chrono::Utc;
use std::sync::Arc;
use tokio::sync::RwLock;
use thiserror::Error;
use reqwest::Client;
use std::collections::HashMap;
use std::time::Duration;

// =============================================================================
// ERRORS
// =============================================================================

#[derive(Error, Debug)]
pub enum NFTMetadataError {
    #[error("Request error: {0}")]
    RequestError(String),
    #[error("Parse error: {0}")]
    ParseError(String),
    #[error("Not found: {0}")]
    NotFound(String),
    #[error("Unsupported URI: {0}")]
    UnsupportedURI(String),
}

// =============================================================================
// INDEXER
// =============================================================================

/// NFT Metadata Indexer
pub struct NFTMetadataIndexer {
    config: NFTMetadataConfig,
    client: Client,
    ipfs_client: IPFSClient,
    arweave_client: ArweaveClient,
    cache: Arc<RwLock<HashMap<String, NFTMetadata>>>,
}

impl NFTMetadataIndexer {
    /// Create new indexer
    pub fn new(config: NFTMetadataConfig) -> Self {
        let client = Client::builder()
            .timeout(Duration::from_secs(config.timeout_secs))
            .build()
            .unwrap_or_else(|_| Client::new());
        
        Self {
            config: config.clone(),
            client,
            ipfs_client: IPFSClient::new(config.ipfs_gateways),
            arweave_client: ArweaveClient::new(&config.arweave_gateway),
            cache: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Fetch metadata for a token
    pub async fn fetch_metadata(&self, collection: &str, token_id: &str, metadata_uri: &str) -> Result<NFTMetadata, NFTMetadataError> {
        let cache_key = format!("{}:{}", collection, token_id);
        
        // Check cache first
        if self.config.enable_cache {
            let cache = self.cache.read().await;
            if let Some(cached) = cache.get(&cache_key) {
                return Ok(cached.clone());
            }
        }

        // Fetch from URI
        let metadata = self.fetch_from_uri(metadata_uri).await?;
        let mut metadata = metadata;
        metadata.token_id = token_id.to_string();
        metadata.collection = collection.to_string();

        // Cache it
        if self.config.enable_cache {
            let mut cache = self.cache.write().await;
            cache.insert(cache_key, metadata.clone());
        }

        Ok(metadata)
    }

    /// Fetch metadata from URI (IPFS, Arweave, HTTP)
    async fn fetch_from_uri(&self, uri: &str) -> Result<NFTMetadata, NFTMetadataError> {
        let uri = uri.trim();
        
        if uri.starts_with("ipfs://") {
            // IPFS URI
            let cid = uri.strip_prefix("ipfs://").unwrap_or(uri);
            let text = self.ipfs_client.fetch(cid).await
                .map_err(|e| NFTMetadataError::RequestError(e.to_string()))?;
            serde_json::from_str(&text)
                .map_err(|e| NFTMetadataError::ParseError(e.to_string()))
        } else if uri.starts_with("arweave://") {
            // Arweave URI
            let tx_id = uri.strip_prefix("arweave://").unwrap_or(uri);
            let text = self.arweave_client.fetch(tx_id).await
                .map_err(|e| NFTMetadataError::RequestError(e.to_string()))?;
            serde_json::from_str(&text)
                .map_err(|e| NFTMetadataError::ParseError(e.to_string()))
        } else if uri.starts_with("http://") || uri.starts_with("https://") {
            // HTTP URI
            let response = self.client
                .get(uri)
                .send()
                .await
                .map_err(|e| NFTMetadataError::RequestError(e.to_string()))?;
            
            if !response.status().is_success() {
                return Err(NFTMetadataError::RequestError(format!("HTTP error: {}", response.status())));
            }

            // Try JSON first
            if let Ok(metadata) = response.json().await {
                return Ok(metadata);
            }

            // Fall back to text
            let text = response.text().await
                .map_err(|e| NFTMetadataError::ParseError(e.to_string()))?;
            serde_json::from_str(&text)
                .map_err(|e| NFTMetadataError::ParseError(e.to_string()))
        } else {
            Err(NFTMetadataError::UnsupportedURI(uri.to_string()))
        }
    }

    /// Calculate rarity for a collection
    pub async fn calculate_rarity(&self, collection: &str, token_ids: &[String]) -> Result<Vec<NFTRarity>, NFTMetadataError> {
        let mut rarities = Vec::new();
        
        // This would require fetching metadata for each token
        // For now, return empty result
        // In production, this would:
        // 1. Fetch metadata for all tokens
        // 2. Calculate trait frequencies
        // 3. Calculate rarity scores
        
        for token_id in token_ids {
            rarities.push(NFTRarity {
                token_id: token_id.clone(),
                collection: collection.to_string(),
                rarity_score: 1.0,
                rank: 0,
                trait_rarities: Vec::new(),
            });
        }
        
        Ok(rarities)
    }

    /// Get cached metadata
    pub async fn get_cached(&self, collection: &str, token_id: &str) -> Option<NFTMetadata> {
        let cache_key = format!("{}:{}", collection, token_id);
        self.cache.read().await.get(&cache_key).cloned()
    }

    /// Clear cache
    pub async fn clear_cache(&self) {
        self.cache.write().await.clear();
    }
}