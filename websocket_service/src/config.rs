//! WebSocket Service Configuration
//! Security-focused configuration management

use serde::{Deserialize, Serialize};

/// WebSocket server configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    /// Server bind address
    pub bind_address: String,
    /// Server port
    pub port: u16,
    /// Maximum connections
    pub max_connections: usize,
    /// Connection timeout in seconds
    pub connection_timeout: u64,
    /// Heartbeat interval in seconds
    pub heartbeat_interval: u64,
    /// Message queue size per connection
    pub message_queue_size: usize,
    /// Maximum message size in bytes
    pub max_message_size: usize,
    /// Enable TLS
    pub tls_enabled: bool,
    /// TLS certificate path
    pub tls_cert_path: Option<String>,
    /// TLS key path
    pub tls_key_path: Option<String>,
    /// Rate limit per connection (messages per second)
    pub rate_limit: usize,
    /// Enable origin check
    pub origin_check: bool,
    /// Allowed origins
    pub allowed_origins: Vec<String>,
    /// RPC URL for blockchain data
    pub rpc_url: String,
    /// WebSocket RPC URL
    pub ws_rpc_url: String,
    /// Chain ID
    pub chain_id: u64,
    /// Enable metrics
    pub metrics_enabled: bool,
    /// Metrics port
    pub metrics_port: u16,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            bind_address: "0.0.0.0".to_string(),
            port: 8546,
            max_connections: 10000,
            connection_timeout: 60,
            heartbeat_interval: 30,
            message_queue_size: 1000,
            max_message_size: 1024 * 1024, // 1MB
            tls_enabled: false,
            tls_cert_path: None,
            tls_key_path: None,
            rate_limit: 100,
            origin_check: true,
            allowed_origins: vec![
                "https://tigerscan.io".to_string(),
                "https://www.tigerscan.io".to_string(),
                "http://localhost:3000".to_string(),
            ],
            rpc_url: "http://localhost:8545".to_string(),
            ws_rpc_url: "ws://localhost:8545".to_string(),
            chain_id: 6666,
            metrics_enabled: true,
            metrics_port: 9090,
        }
    }
}

impl Config {
    /// Load configuration from environment variables
    pub fn from_env() -> Self {
        Self {
            bind_address: std::env::var("WS_BIND_ADDRESS")
                .unwrap_or_else(|_| "0.0.0.0".to_string()),
            port: std::env::var("WS_PORT")
                .unwrap_or_else(|_| "8546".to_string())
                .parse()
                .unwrap_or(8546),
            max_connections: std::env::var("WS_MAX_CONNECTIONS")
                .unwrap_or_else(|_| "10000".to_string())
                .parse()
                .unwrap_or(10000),
            connection_timeout: std::env::var("WS_CONNECTION_TIMEOUT")
                .unwrap_or_else(|_| "60".to_string())
                .parse()
                .unwrap_or(60),
            heartbeat_interval: std::env::var("WS_HEARTBEAT_INTERVAL")
                .unwrap_or_else(|_| "30".to_string())
                .parse()
                .unwrap_or(30),
            message_queue_size: std::env::var("WS_MESSAGE_QUEUE_SIZE")
                .unwrap_or_else(|_| "1000".to_string())
                .parse()
                .unwrap_or(1000),
            max_message_size: std::env::var("WS_MAX_MESSAGE_SIZE")
                .unwrap_or_else(|_| "1048576".to_string())
                .parse()
                .unwrap_or(1048576),
            tls_enabled: std::env::var("WS_TLS_ENABLED")
                .unwrap_or_else(|_| "false".to_string())
                .parse()
                .unwrap_or(false),
            tls_cert_path: std::env::var("WS_TLS_CERT_PATH").ok(),
            tls_key_path: std::env::var("WS_TLS_KEY_PATH").ok(),
            rate_limit: std::env::var("WS_RATE_LIMIT")
                .unwrap_or_else(|_| "100".to_string())
                .parse()
                .unwrap_or(100),
            origin_check: std::env::var("WS_ORIGIN_CHECK")
                .unwrap_or_else(|_| "true".to_string())
                .parse()
                .unwrap_or(true),
            allowed_origins: std::env::var("WS_ALLOWED_ORIGINS")
                .unwrap_or_else(|_| "https://tigerscan.io,https://www.tigerscan.io,http://localhost:3000".to_string())
                .split(',')
                .map(|s| s.to_string())
                .collect(),
            rpc_url: std::env::var("RPC_URL")
                .unwrap_or_else(|_| "http://localhost:8545".to_string()),
            ws_rpc_url: std::env::var("WS_RPC_URL")
                .unwrap_or_else(|_| "ws://localhost:8545".to_string()),
            chain_id: std::env::var("CHAIN_ID")
                .unwrap_or_else(|_| "6666".to_string())
                .parse()
                .unwrap_or(6666),
            metrics_enabled: std::env::var("WS_METRICS_ENABLED")
                .unwrap_or_else(|_| "true".to_string())
                .parse()
                .unwrap_or(true),
            metrics_port: std::env::var("WS_METRICS_PORT")
                .unwrap_or_else(|_| "9090".to_string())
                .parse()
                .unwrap_or(9090),
        }
    }
}