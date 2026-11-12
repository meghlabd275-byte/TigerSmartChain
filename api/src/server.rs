//! API Server for TigerScan

use crate::{handlers::*, middleware::*, routes::*};
use std::net::SocketAddr;
use std::sync::Arc;

// =============================================================================
// SERVER
// =============================================================================

/// API Server
pub struct APIServer {
    config: APIConfig,
    state: Arc<AppState>,
}

impl APIServer {
    /// Create new server
    pub fn new(config: APIConfig) -> Self {
        Self::try_new(config).expect("RPC client must be constructible")
    }

    /// Create a server, reporting why the RPC client could not be built.
    pub fn try_new(config: APIConfig) -> Result<Self, String> {
        let state = Arc::new(AppState::new(config.clone()).map_err(|e| e.to_string())?);

        Ok(Self { config, state })
    }

    /// Start server
    pub async fn start(&self) -> Result<(), String> {
        let addr: SocketAddr = format!("{}:{}", self.config.host, self.config.port)
            .parse()
            .map_err(|e| format!("invalid bind address: {e}"))?;

        let app = build_routes()
            .layer(axum::middleware::from_fn_with_state(
                self.state.clone(),
                metrics_middleware,
            ))
            .layer(axum::middleware::from_fn_with_state(
                self.state.clone(),
                rate_limit_middleware,
            ))
            .layer(cors_layer(&self.config.allowed_origins))
            .with_state(self.state.clone());

        let listener = tokio::net::TcpListener::bind(addr)
            .await
            .map_err(|e| e.to_string())?;

        println!("TigerScan API listening on http://{addr}");

        axum::serve(
            listener,
            app.into_make_service_with_connect_info::<SocketAddr>(),
        )
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
    /// JSON-RPC endpoint of the TigerSmartChain node.
    pub rpc_url: String,
    /// Origins allowed to call the API.
    pub allowed_origins: Vec<String>,
}

impl Default for APIConfig {
    fn default() -> Self {
        Self {
            port: 8080,
            host: "0.0.0.0".to_string(),
            max_connections: 1000,
            request_timeout: 30,
            rate_limit: 100,
            rpc_url: std::env::var("RPC_URL")
                .unwrap_or_else(|_| "http://127.0.0.1:8545".to_string()),
            allowed_origins: std::env::var("CORS_ALLOWED_ORIGINS")
                .map(|value| {
                    value
                        .split(',')
                        .map(str::trim)
                        .filter(|origin| !origin.is_empty())
                        .map(str::to_string)
                        .collect()
                })
                .unwrap_or_else(|_| vec!["*".to_string()]),
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