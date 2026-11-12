//! TigerSmartChain Blockchain Node
//! 
//! This is the main entry point for running a TigerSmartChain blockchain node.
//! It provides safety, security, high speed, and ultra low latency through Rust.

use anyhow::{Context, Result};
use clap::Parser;
use log::info;
use std::sync::Arc;
use tokio::signal;

mod config;
mod debug_rpc;
mod filters;
mod limits;
mod node;
mod p2p;
mod rpc_server;
mod ws_server;
mod storage;

pub use config::Config;
pub use node::TigerNode;

/// Resolve the validator key from the flag or from the file it names.
///
/// A key given on the command line is visible in the process list to every
/// account on the host, so the file form is the one to prefer; both are
/// accepted because a container that injects the key as an argument has no
/// other option. Naming both is a configuration error rather than a silent
/// preference, so a node never seals with a key other than the one the
/// operator intended.
fn resolve_validator_key(
    validator_key: Option<String>,
    validator_key_file: Option<&str>,
) -> Result<Option<String>> {
    match (validator_key, validator_key_file) {
        (Some(_), Some(_)) => anyhow::bail!(
            "--validator-key and --validator-key-file are mutually exclusive;              pass only the file so the key stays out of the process list"
        ),
        (Some(key), None) => {
            log::warn!(
                "--validator-key is set on the command line, so the sealing key is \
                 visible to other accounts on this host; use --validator-key-file"
            );
            Ok(Some(key))
        }
        (None, Some(path)) => {
            let contents = std::fs::read_to_string(path)
                .with_context(|| format!("cannot read the validator key file {path}"))?;
            let key = contents.trim();
            if key.is_empty() {
                anyhow::bail!("the validator key file {path} is empty");
            }
            Ok(Some(key.to_string()))
        }
        (None, None) => Ok(None),
    }
}

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
/// Default WebSocket address
pub const DEFAULT_WS_ADDR: &str = ":8546";
/// Default max peers
pub const DEFAULT_MAX_PEERS: usize = 50;
/// Default epoch length
pub const DEFAULT_EPOCH_LENGTH: u64 = 200;
/// Default slot duration in seconds
pub const DEFAULT_SLOT_DURATION_SECS: u64 = 3;
/// Default data directory
pub const DEFAULT_DATA_DIR: &str = "./data";
/// Default per-client RPC request ceiling, in requests per second.
pub const DEFAULT_RPC_RATE_LIMIT: u32 = config::DEFAULT_RPC_RATE_LIMIT;
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

    /// Comma-separated browser origins allowed to call the RPC. Empty denies
    /// every browser origin, which is the safe default.
    #[arg(long)]
    rpc_cors_host: Option<String>,

    /// Maximum JSON-RPC requests per second from one client address. Zero
    /// disables the limit.
    #[arg(long, default_value_t = DEFAULT_RPC_RATE_LIMIT)]
    rpc_rate_limit: u32,

    /// Serve the WebSocket JSON-RPC endpoint, which carries `eth_subscribe`.
    #[arg(long, default_value = "true")]
    ws_enabled: bool,

    /// Address the WebSocket endpoint listens on.
    #[arg(long, default_value = DEFAULT_WS_ADDR)]
    ws_addr: String,

    /// Data directory
    #[arg(long, default_value = DEFAULT_DATA_DIR)]
    data_dir: String,

    /// Cache size in MB
    #[arg(long, default_value_t = DEFAULT_CACHE_SIZE)]
    cache_size: usize,

    /// Validator address
    #[arg(long)]
    validator_addr: Option<String>,

    /// Comma-separated validator addresses that may propose blocks. Without
    /// any, this node follows the chain but never produces a block.
    #[arg(long, value_delimiter = ',')]
    validators: Vec<String>,

    /// Validator private key (hex).
    ///
    /// Passing a key here exposes it to every account on the host through the
    /// process list. Prefer `--validator-key-file`.
    #[arg(long)]
    validator_key: Option<String>,

    /// File holding the validator private key as hex.
    ///
    /// Read at startup and never written back. The file should be readable
    /// only by the account that runs the node.
    #[arg(long)]
    validator_key_file: Option<String>,

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

    // Resolved before the config is built, because reading the key file can
    // fail and a node must not come up half-configured.
    let validator_key = resolve_validator_key(
        args.validator_key.clone(),
        args.validator_key_file.as_deref(),
    )?;

    // Build boot nodes list
    let boot_nodes: Vec<String> = args
        .boot_nodes
        .as_ref()
        .map(|s| s.split(',').map(|s| s.trim().to_string()).collect())
        .unwrap_or_default();

    // Build RPC CORS host
    let rpc_cors_host = args.rpc_cors_host.unwrap_or_default();

    // Create node configuration. Clone the fields that are logged after the
    // config takes ownership of them.
    let listen_addr = args.listen_addr.clone();
    let rpc_addr = args.rpc_addr.clone();
    let metrics_addr = args.metrics_addr.clone();

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
        args.rpc_rate_limit,
        args.ws_enabled,
        args.ws_addr,
        args.data_dir,
        args.cache_size,
        args.validator_addr,
        validator_key,
        args.validators,
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

    // Bring up the peer-to-peer service. A node that cannot bind its listen
    // address still runs: it can follow the chain over RPC and serve clients,
    // which is better than refusing to start.
    match crate::p2p::P2pService::start(Arc::clone(&node)).await {
        Ok(Some(service)) => {
            node.set_p2p(service);
            info!("P2P listening on: {}", listen_addr);
        }
        Ok(None) => log::warn!("P2P is disabled: the listen address could not be bound"),
        Err(e) => log::warn!("P2P failed to start: {e}"),
    }

    info!("TigerSmartChain node started successfully");
    
    if args.rpc_enabled {
        info!("RPC server listening on: {}", rpc_addr);
    }
    
    if args.metrics_enabled {
        info!("Metrics server listening on: {}", metrics_addr);
    }

    // Wait for a shutdown signal. SIGTERM is only available on Unix; on other
    // platforms the Ctrl+C branch alone terminates the select.
    #[cfg(unix)]
    {
        let mut sigterm = tokio::signal::unix::signal(
            tokio::signal::unix::SignalKind::terminate(),
        )
        .context("failed to install SIGTERM handler")?;
        tokio::select! {
            result = signal::ctrl_c() => {
                result.context("failed to listen for Ctrl+C")?;
                info!("Received Ctrl+C, shutting down...");
            }
            _ = sigterm.recv() => {
                info!("Received SIGTERM, shutting down...");
            }
        }
    }
    #[cfg(not(unix))]
    {
        signal::ctrl_c().await.context("failed to listen for Ctrl+C")?;
        info!("Received Ctrl+C, shutting down...");
    }

    // Stop the node gracefully
    info!("Stopping node...");
    node.stop().await.context("Failed to stop node")?;

    info!("TigerSmartChain node stopped successfully");
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;

    fn temp_file(name: &str, contents: &str) -> std::path::PathBuf {
        let path = std::env::temp_dir().join(format!("tiger-key-{}-{name}", std::process::id()));
        std::fs::write(&path, contents).unwrap();
        path
    }

    /// A key file is the way to keep the sealing key out of the process list,
    /// so the flag has to actually load it.
    #[test]
    fn the_validator_key_is_read_from_the_file() {
        let path = temp_file("read", "0xabcd\n");
        let key = resolve_validator_key(None, Some(path.to_str().unwrap()))
            .unwrap()
            .expect("a key file must yield a key");
        assert_eq!(key, "0xabcd", "the trailing newline must be trimmed");
        let _ = std::fs::remove_file(&path);
    }

    /// Naming both sources is ambiguous. Choosing one silently would let a
    /// node seal with a key the operator did not intend, so it is refused.
    #[test]
    fn naming_both_key_sources_is_refused() {
        let path = temp_file("both", "0x01");
        let error = resolve_validator_key(Some("0x02".to_string()), Some(path.to_str().unwrap()))
            .unwrap_err()
            .to_string();
        assert!(
            error.contains("mutually exclusive"),
            "unexpected error: {error}"
        );
        let _ = std::fs::remove_file(&path);
    }

    /// An unreadable or empty key file must fail the startup rather than let
    /// the node run without the key it was told to use.
    #[test]
    fn a_missing_or_empty_key_file_is_fatal() {
        let missing = std::env::temp_dir().join("tiger-key-does-not-exist");
        let _ = std::fs::remove_file(&missing);
        assert!(resolve_validator_key(None, Some(missing.to_str().unwrap())).is_err());

        let empty = temp_file("empty", "   \n");
        let error = resolve_validator_key(None, Some(empty.to_str().unwrap()))
            .unwrap_err()
            .to_string();
        assert!(error.contains("empty"), "unexpected error: {error}");
        let _ = std::fs::remove_file(&empty);
    }

    /// A node with no key configured is a follower, not a misconfiguration.
    #[test]
    fn no_key_configured_yields_no_key() {
        assert!(resolve_validator_key(None, None).unwrap().is_none());
    }
}
