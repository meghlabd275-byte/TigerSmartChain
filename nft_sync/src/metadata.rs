//! NFT Metadata Fetcher - IPFS/Arweave/Http support

use crate::types::*;
use thiserror::Error;
use reqwest::Client;
use serde::{Deserialize, Serialize};

// =============================================================================
// ERRORS
// =============================================================================

#[derive(Error, Debug)]
pub enum MetadataError {
    #[error("Fetch error: {0}")]
    FetchError(String),
    #[error("Parse error: {0}")]
    ParseError(String),
    #[error("Unsupported URI scheme: {0}")]
    UnsupportedScheme(String),
}

// =============================================================================
// METADATA FETCHER
// =============================================================================

/// NFT Metadata Fetcher
pub struct MetadataFetcher {
    client: Client,
    /// IPFS gateway URL
    ipfs_gateway: String,
    /// Arweave gateway URL
    arweave_gateway: String,
    /// HTTP timeout
    timeout_secs: u64,
}

impl MetadataFetcher {
    /// Create new fetcher
    pub fn new() -> Self {
        Self {
            client: Client::builder()
                .timeout(std::time::Duration::from_secs(30))
                .build()
                .unwrap_or_default(),
            ipfs_gateway: "https://ipfs.io/ipfs/".to_string(),
            arweave_gateway: "https://arweave.net/".to_string(),
            timeout_secs: 30,
        }
    }

    /// Create with custom gateways
    pub fn with_gateways(ipfs_gateway: &str, arweave_gateway: &str) -> Self {
        Self {
            client: Client::builder()
                .timeout(std::time::Duration::from_secs(30))
                .build()
                .unwrap_or_default(),
            ipfs_gateway: ipfs_gateway.to_string(),
            arweave_gateway: arweave_gateway.to_string(),
            timeout_secs: 30,
        }
    }

    /// Fetch metadata from URI
    pub async fn fetch(&self, uri: &str) -> Result<NFTMetadata, MetadataError> {
        // Parse URI scheme
        if uri.starts_with("ipfs://") {
            return self.fetch_ipfs(uri).await;
        } else if uri.starts_with("ar://") {
            return self.fetch_arweave(uri).await;
        } else if uri.starts_with("http://") || uri.starts_with("https://") {
            return self.fetch_http(uri).await;
        } else {
            return Err(MetadataError::UnsupportedScheme(uri.to_string()));
        }
    }

    /// Fetch from IPFS
    async fn fetch_ipfs(&self, uri: &str) -> Result<NFTMetadata, MetadataError> {
        // Remove ipfs:// prefix and get the CID
        let path = uri.trim_start_matches("ipfs://");
        
        // Construct gateway URL
        let url = format!("{}{}", self.ipfs_gateway, path);
        
        self.fetch_http(&url).await
    }

    /// Fetch from Arweave
    async fn fetch_arweave(&self, uri: &str) -> Result<NFTMetadata, MetadataError> {
        // Remove ar:// prefix and get the transaction ID
        let tx_id = uri.trim_start_matches("ar://");
        
        // Construct gateway URL
        let url = format!("{}{}", self.arweave_gateway, tx_id);
        
        self.fetch_http(&url).await
    }

    /// Fetch from HTTP/HTTPS
    async fn fetch_http(&self, url: &str) -> Result<NFTMetadata, MetadataError> {
        let response = self.client
            .get(url)
            .send()
            .await
            .map_err(|e| MetadataError::FetchError(e.to_string()))?;

        if !response.status().is_success() {
            return Err(MetadataError::FetchError(format!(
                "HTTP error: {}",
                response.status()
            )));
        }

        let content_type = response.headers()
            .get("content-type")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("");

        // Check if response is JSON
        if content_type.contains("application/json") {
            let metadata: NFTMetadata = response
                .json()
                .await
                .map_err(|e| MetadataError::ParseError(e.to_string()))?;
            
            return Ok(metadata);
        }

        // For other content types, return basic metadata
        let text = response
            .text()
            .await
            .map_err(|e| MetadataError::ParseError(e.to_string()))?;

        Ok(NFTMetadata {
            name: None,
            description: None,
            image: Some(url.to_string()),
            image_data: None,
            external_url: None,
            attributes: vec![],
            background_color: None,
            animation_url: None,
            youtube_url: None,
            external_metadata: Some(text),
        })
    }

    /// Batch fetch metadata for multiple URIs
    pub async fn fetch_batch(&self, uris: &[String]) -> Vec<Result<NFTMetadata, MetadataError>> {
        let mut results = Vec::new();
        
        for uri in uris {
            results.push(self.fetch(uri).await);
        }
        
        results
    }
}

// =============================================================================
// TYPES
// =============================================================================

/// NFT Metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NFTMetadata {
    /// Name of the NFT
    pub name: Option<String>,
    /// Description of the NFT
    pub description: Option<String>,
    /// Image URL
    pub image: Option<String>,
    /// Base64 encoded image data
    pub image_data: Option<String>,
    /// External URL
    pub external_url: Option<String>,
    /// Attributes/Traits
    pub attributes: Vec<NFTAttribute>,
    /// Background color
    pub background_color: Option<String>,
    /// Animation URL
    pub animation_url: Option<String>,
    /// YouTube URL
    pub youtube_url: Option<String>,
    /// Additional raw metadata
    #[serde(flatten)]
    pub external_metadata: Option<serde_json::Value>,
}

/// NFT Attribute/Trait
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NFTAttribute {
    /// Trait type (e.g., "Background", "Type")
    #[serde(rename = "trait_type")]
    pub trait_type: Option<String>,
    /// Display type
    #[serde(rename = "display_type")]
    pub display_type: Option<String>,
    /// Value
    pub value: Option<serde_json::Value>,
    /// Max value (for numeric traits)
    #[serde(rename = "max_value")]
    pub max_value: Option<f64>,
}

// =============================================================================
// TESTS
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_fetcher_creation() {
        let fetcher = MetadataFetcher::new();
        assert_eq!(fetcher.ipfs_gateway, "https://ipfs.io/ipfs/");
    }

    #[test]
    fn test_with_gateways() {
        let fetcher = MetadataFetcher::with_gateways(
            "https://custom-ipfs.io/ipfs/",
            "https://custom-arweave.net/",
        );
        
        assert_eq!(fetcher.ipfs_gateway, "https://custom-ipfs.io/ipfs/");
        assert_eq!(fetcher.arweave_gateway, "https://custom-arweave.net/");
    }
}