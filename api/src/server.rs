//! API Server for TigerScan

use crate::{handlers::*, routes::*, middleware::*};
use axum::Router;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::sync::RwLock;
use std::collections::HashMap;

// =============================================================================
// SERVER
// =============================================================================

/// API Server
pub struct APIServer {
    config: APIConfig,
    state: Arc<AppState>,
    stats: Arc<RwLock<ServerStats>>,
}

/// Server Statistics
#[derive(Debug, Clone)]
pub struct ServerStats {
    pub requests: HashMap<String, usize>,
    pub start_time: std::time::Instant,
}

impl Default for ServerStats {
    fn default() -> Self {
        Self {
            requests: HashMap::new(),
            start_time: std::time::Instant::now(),
        }
    }
}

impl APIServer {
    /// Create new server
    pub fn new(config: APIConfig) -> Self {
        let state = Arc::new(AppState::new(config.clone()));
        
        Self {
            config,
            state,
            stats: Arc::new(RwLock::new(ServerStats::default())),
        }
    }

    /// Start server
    pub async fn start(&self) -> Result<(), String> {
        let addr = SocketAddr::from(([0, 0, 0, 0], self.config.port));
        
        println!("Starting API server on {}", addr);
        
        let app = build_routes()
            .with_state(self.state.clone());
        
        let listener = tokio::net::TcpListener::bind(addr)
            .await
            .map_err(|e| e.to_string())?;
        
        axum::serve(listener, app)
            .await
            .map_err(|e| e.to_string())?;
        
        Ok(())
    }

    /// Get config
    pub fn config(&self) -> &APIConfig {
        &self.config
    }
}

// =============================================================================
// CONFIG
// =============================================================================

/// API Configuration
#[derive(Debug, Clone)]
pub struct APIConfig {
    pub port: u16,
    pub host: String,
    pub max_connections: usize,
    pub request_timeout: u64,
    pub rate_limit: usize,
}

impl Default for APIConfig {
    fn default() -> Self {
        Self {
            port: 8080,
            host: "0.0.0.0".to_string(),
            max_connections: 1000,
            request_timeout: 30,
            rate_limit: 100,
        }
    }
}

// =============================================================================
// BUILDER
// =============================================================================

/// Server Builder
pub struct APIServerBuilder {
    config: APIConfig,
}

impl APIServerBuilder {
    pub fn new() -> Self {
        Self {
            config: APIConfig::default(),
        }
    }

    pub fn port(mut self, port: u16) -> Self {
        self.config.port = port;
        self
    }

    pub fn host(mut self, host: &str) -> Self {
        self.config.host = host.to_string();
        self
    }

    pub fn build(self) -> APIServer {
        APIServer::new(self.config)
    }
}

impl Default for APIServerBuilder {
    fn default() -> Self {
        Self::new()
    }
}

// =============================================================================
// TESTS
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_server() {
        let server = APIServer::new(APIConfig::default());
        assert_eq!(server.config().port, 8080);
    }

    #[test]
    fn test_builder() {
        let server = APIServerBuilder::new()
            .port(9090)
            .build();
        
        assert_eq!(server.config().port, 9090);
    }
}