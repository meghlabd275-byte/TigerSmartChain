//! TigerSmartChain Blockchain Node
//! 
//! This is the main entry point for running a TigerSmartChain blockchain node.
//! It provides safety, security, high speed, and ultra low latency through Rust.

use anyhow::{Context, Result};
use clap::Parser;
use log::info;
use std::sync::Arc;
use tokio::signal;
use tokio::sync::RwLock;

mod config;
mod node;

pub use config::Config;
pub use node::TigerNode;

/// Default network ID
pub const DEFAULT_NETWORK_ID: u64 = 1;
/// Default chain ID
pub const DEFAULT_CHAIN_ID: u64 = 1;
/// Default P2P listen address
pub const DEFAULT_LISTEN_ADDR: &str = ":30303";
/// Default RPC address
pub const DEFAULT_RPC_ADDR: &str = ":8545";
/// Default metrics address
pub const DEFAULT_METRICS_ADDR: &str = ":9090";
/// Default max peers
pub const DEFAULT_MAX_PEERS: usize = 50;
/// Default epoch length
pub const DEFAULT_EPOCH_LENGTH: u64 = 200;
/// Default slot duration in seconds
pub const DEFAULT_SLOT_DURATION_SECS: u64 = 3;
/// Default data directory
pub const DEFAULT_DATA_DIR: &str = "./data";
/// Default cache size in MB
pub const DEFAULT_CACHE_SIZE: usize = 1024;

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// Network ID
    #[arg(long, default_value_t = DEFAULT_NETWORK_ID)]
    network_id: u64,

    /// Chain ID
    #[arg(long, default_value_t = DEFAULT_CHAIN_ID)]
    chain_id: u64,

    /// Network type (mainnet, testnet)
    #[arg(long, default_value = "mainnet")]
    network: String,

    /// P2P listen address
    #[arg(long, default_value = DEFAULT_LISTEN_ADDR)]
    listen_addr: String,

    /// Boot nodes (comma-separated)
    #[arg(long)]
    boot_nodes: Option<String>,

    /// Maximum number of peers
    #[arg(long, default_value_t = DEFAULT_MAX_PEERS)]
    max_peers: usize,

    /// Enable RPC server
    #[arg(long, default_value = "true")]
    rpc_enabled: bool,

    /// RPC listen address
    #[arg(long, default_value = DEFAULT_RPC_ADDR)]
    rpc_addr: String,

    /// RPC CORS host
    #[arg(long)]
    rpc_cors_host: Option<String>,

    /// Data directory
    #[arg(long, default_value = DEFAULT_DATA_DIR)]
    data_dir: String,

    /// Cache size in MB
    #[arg(long, default_value_t = DEFAULT_CACHE_SIZE)]
    cache_size: usize,

    /// Validator address
    #[arg(long)]
    validator_addr: Option<String>,

    /// Validator private key (hex)
    #[arg(long)]
    validator_key: Option<String>,

    /// Epoch length
    #[arg(long, default_value_t = DEFAULT_EPOCH_LENGTH)]
    epoch_length: u64,

    /// Slot duration in seconds
    #[arg(long, default_value_t = DEFAULT_SLOT_DURATION_SECS)]
    slot_duration_secs: u64,

    /// Enable metrics
    #[arg(long, default_value = "false")]
    metrics_enabled: bool,

    /// Metrics listen address
    #[arg(long, default_value = DEFAULT_METRICS_ADDR)]
    metrics_addr: String,

    /// Genesis file path
    #[arg(long)]
    genesis_file: Option<String>,

    /// Verbose logging
    #[arg(short, long, default_value = "false")]
    verbose: bool,
}

#[tokio::main]
async fn main() -> Result<()> {
    // Parse command line arguments
    let args = Args::parse();

    // Initialize logger
    let log_level = if args.verbose {
        log::LevelFilter::Debug
    } else {
        log::LevelFilter::Info
    };
    env_logger::Builder::new()
        .filter_level(log_level)
        .format_timestamp_millis()
        .init();

    info!("Starting TigerSmartChain Node v{}", env!("CARGO_PKG_VERSION"));
    info!("Network: {}, Chain ID: {}", args.network, args.chain_id);

    // Build boot nodes list
    let boot_nodes: Vec<String> = args
        .boot_nodes
        .as_ref()
        .map(|s| s.split(',').map(|s| s.trim().to_string()).collect())
        .unwrap_or_default();

    // Build RPC CORS host
    let rpc_cors_host = args.rpc_cors_host.unwrap_or_default();

    // Create node configuration
    let config = Config::new(
        args.network_id,
        args.chain_id,
        args.network,
        args.listen_addr,
        boot_nodes,
        args.max_peers,
        args.rpc_enabled,
        args.rpc_addr,
        rpc_cors_host,
        args.data_dir,
        args.cache_size,
        args.validator_addr,
        args.validator_key,
        args.epoch_length,
        args.slot_duration_secs,
        args.metrics_enabled,
        args.metrics_addr,
        args.genesis_file,
    );

    // Create and start the node
    let node = TigerNode::new(config)
        .await
        .context("Failed to create node")?;

    let node = Arc::new(node);
    info!("Node initialized successfully");

    // Start the node
    node.start().await.context("Failed to start node")?;

    info!("TigerSmartChain node started successfully");
    info!("P2P listening on: {}", args.listen_addr);
    
    if args.rpc_enabled {
        info!("RPC server listening on: {}", args.rpc_addr);
    }
    
    if args.metrics_enabled {
        info!("Metrics server listening on: {}", args.metrics_addr);
    }

    // Wait for shutdown signal
    tokio::select! {
        _ = signal::ctrl_c() => {
            info!("Received Ctrl+C, shutting down...");
        }
        _ = tokio::signal::signal(tokio::signal::SignalKind::terminate()) => {
            info!("Received SIGTERM, shutting down...");
        }
    }

    // Stop the node gracefully
    info!("Stopping node...");
    node.stop().await.context("Failed to stop node")?;

    info!("TigerSmartChain node stopped successfully");
    Ok(())
}