//! Arweave Client for NFT Metadata

use thiserror::Error;
use reqwest::Client;
use std::time::Duration;

// =============================================================================
// ERRORS
// =============================================================================

#[derive(Error, Debug)]
pub enum ArweaveError {
    #[error("Request error: {0}")]
    RequestError(String),
    #[error("Parse error: {0}")]
    ParseError(String),
    #[error("Not found: {0}")]
    NotFound(String),
}

// =============================================================================
// CLIENT
// =============================================================================

/// Arweave Client
pub struct ArweaveClient {
    client: Client,
    gateway: String,
}

impl ArweaveClient {
    /// Create new Arweave client
    pub fn new(gateway: &str) -> Self {
        let client = Client::builder()
            .timeout(Duration::from_secs(30))
            .build()
            .unwrap_or_else(|_| Client::new());
        
        Self {
            client,
            gateway: gateway.to_string(),
        }
    }

    /// Fetch metadata from Arweave
    pub async fn fetch(&self, tx_id: &str) -> Result<String, ArweaveError> {
        let url = format!("{}/{}", self.gateway, tx_id);
        
        let response = self.client
            .get(&url)
            .send()
            .await
            .map_err(|e| ArweaveError::RequestError(e.to_string()))?;

        if response.status() == 404 {
            return Err(ArweaveError::NotFound(tx_id.to_string()));
        }

        if !response.status().is_success() {
            return Err(ArweaveError::RequestError(format!("HTTP error: {}", response.status())));
        }

        response
            .text()
            .await
            .map_err(|e| ArweaveError::ParseError(e.to_string()))
    }

    /// Fetch JSON metadata from Arweave
    pub async fn fetch_json<T: serde::de::DeserializeOwned>(&self, tx_id: &str) -> Result<T, ArweaveError> {
        let text = self.fetch(tx_id).await?;
        serde_json::from_str(&text)
            .map_err(|e| ArweaveError::ParseError(e.to_string()))
    }

    /// Get transaction status
    pub async fn get_status(&self, tx_id: &str) -> Result<ArweaveStatus, ArweaveError> {
        let url = format!("{}/status/{}", self.gateway, tx_id);
        
        let response = self.client
            .get(&url)
            .send()
            .await
            .map_err(|e| ArweaveError::RequestError(e.to_string()))?;

        if response.status() == 404 {
            return Err(ArweaveError::NotFound(tx_id.to_string()));
        }

        response
            .json()
            .await
            .map_err(|e| ArweaveError::ParseError(e.to_string()))
    }
}

// =============================================================================
// STATUS
// =============================================================================

/// Arweave Transaction Status
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ArweaveStatus {
    pub block_indep_hash: String,
    pub block_height: u64,
    pub confirmations: u32,
}