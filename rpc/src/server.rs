//! RPC Server

use crate::{types::*, handler::*};
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::net::{TcpListener, TcpStream};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

// =============================================================================
// SERVER
// =============================================================================

/// RPC Server
pub struct RPCServer {
    config: RPCConfig,
    handler: Arc<RPCHandler>,
}

/// RPC Config
#[derive(Debug, Clone)]
pub struct RPCConfig {
    pub host: String,
    pub port: u16,
    pub cors_enabled: bool,
    pub max_request_size: usize,
}

impl Default for RPCConfig {
    fn default() -> Self {
        Self {
            host: "0.0.0.0".to_string(),
            port: 8545,
            cors_enabled: true,
            max_request_size: 10 * 1024 * 1024,
        }
    }
}

impl RPCServer {
    pub fn new(config: RPCConfig) -> Self {
        Self {
            config,
            handler: Arc::new(RPCHandler::new()),
        }
    }

    /// Start server
    pub async fn start(&self) -> Result<(), String> {
        let addr = format!("{}:{}", self.config.host, self.config.port);
        let listener = TcpListener::bind(&addr).await.map_err(|e| e.to_string())?;
        
        log::info!("RPC server listening on {}", addr);
        
        loop {
            match listener.accept().await {
                Ok((stream, addr)) => {
                    let handler = self.handler.clone();
                    tokio::spawn(async move {
                        if let Err(e) = Self::handle_connection(stream, handler).await {
                            log::error!("Connection error: {}", e);
                        }
                    });
                }
                Err(e) => {
                    log::error!("Accept error: {}", e);
                }
            }
        }
    }

    async fn handle_connection(stream: TcpStream, handler: Arc<RPCHandler>) -> Result<(), String> {
        let mut buffer = vec![0u8; 1024 * 1024];
        let n = stream.read(&mut buffer).await.map_err(|e| e.to_string())?;
        buffer.truncate(n);
        
        let request: RPCRequest = serde_json::from_slice(&buffer)
            .map_err(|e| e.to_string())?;
        
        let response = handler.handle(&request);
        
        let response_bytes = serde_json::to_vec(&response)
            .map_err(|e| e.to_string())?;
        
        stream.write_all(&response_bytes).await.map_err(|e| e.to_string())?;
        
        Ok(())
    }
}

// =============================================================================
// BUILDER
// =============================================================================

/// Builder
pub struct RPCServerBuilder {
    config: RPCConfig,
}

impl RPCServerBuilder {
    pub fn new() -> Self {
        Self { config: RPCConfig::default() }
    }

    pub fn port(mut self, port: u16) -> Self {
        self.config.port = port;
        self
    }

    pub fn build(self) -> RPCServer {
        RPCServer::new(self.config)
    }
}

impl Default for RPCServerBuilder {
    fn default() -> Self {
        Self::new()
    }
}