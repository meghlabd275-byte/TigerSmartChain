//! IPFS Client for NFT Metadata

use thiserror::Error;
use reqwest::Client;
use std::time::Duration;
use std::collections::VecDeque;

// =============================================================================
// ERRORS
// =============================================================================

#[derive(Error, Debug)]
pub enum IPFSError {
    #[error("Request error: {0}")]
    RequestError(String),
    #[error("Parse error: {0}")]
    ParseError(String),
    #[error("Gateway error: {0}")]
    GatewayError(String),
}

// =============================================================================
// CLIENT
// =============================================================================

/// IPFS Client
pub struct IPFSClient {
    client: Client,
    gateways: VecDeque<String>,
}

impl IPFSClient {
    /// Create new IPFS client
    pub fn new(gateways: Vec<String>) -> Self {
        let client = Client::builder()
            .timeout(Duration::from_secs(30))
            .build()
            .unwrap_or_else(|_| Client::new());
        
        Self { 
            client, 
            gateways: gateways.into(),
        }
    }

    /// Fetch metadata from IPFS
    pub async fn fetch(&self, cid: &str) -> Result<String, IPFSError> {
        // Try each gateway until one works
        for gateway in &self.gateways {
            let url = format!("{}{}", gateway, cid);
            
            match self.client.get(&url).send().await {
                Ok(response) if response.status().is_success() => {
                    match response.text().await {
                        Ok(text) => return Ok(text),
                        Err(_) => continue,
                    }
                }
                _ => continue,
            }
        }
        
        Err(IPFSError::GatewayError("All gateways failed".to_string()))
    }

    /// Fetch JSON metadata from IPFS
    pub async fn fetch_json<T: serde::de::DeserializeOwned>(&self, cid: &str) -> Result<T, IPFSError> {
        let text = self.fetch(cid).await?;
        serde_json::from_str(&text)
            .map_err(|e| IPFSError::ParseError(e.to_string()))
    }
}