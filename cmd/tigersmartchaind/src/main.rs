//! TigerSmartChain Node Daemon
//! 
//! This is the main entry point for running the TigerSmartChain blockchain daemon.

use anyhow::Result;
use clap::Parser;
use log::info;

mod commands;

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// Chain ID
    #[arg(long, default_value_t = 9001)]
    chain_id: u64,

    /// Data directory
    #[arg(long)]
    datadir: Option<String>,

    /// Config file
    #[arg(long)]
    config: Option<String>,

    /// Verbose logging
    #[arg(long, short)]
    verbosity: bool,

    /// HTTP server host
    #[arg(long, default_value = "127.0.0.1")]
    http_host: String,

    /// HTTP server port
    #[arg(long, default_value_t = 8545)]
    http_port: u16,

    /// WebSocket server host
    #[arg(long, default_value = "127.0.0.1")]
    ws_host: String,

    /// WebSocket server port
    #[arg(long, default_value_t = 8546)]
    ws_port: u16,

    /// Bootnodes (comma separated)
    #[arg(long)]
    bootnodes: Option<String>,

    /// Private key for validator
    #[arg(long)]
    key: Option<String>,

    /// Command to run
    #[arg(default_value = "")]
    command: String,
}

fn main() -> Result<()> {
    // Parse arguments
    let args = Args::parse();

    // Initialize logger
    env_logger::Builder::new()
        .filter_level(if args.verbosity {
            log::LevelFilter::Debug
        } else {
            log::LevelFilter::Info
        })
        .init();

    info!("TigerSmartChain v1.0.0");
    info!("Chain ID: {}", args.chain_id);

    // Handle different commands
    match args.command.as_str() {
        "init" => {
            commands::init_chain(&args)?;
        }
        "start" => {
            commands::start_node(&args)?;
        }
        "validator" => {
            commands::manage_validator(&args)?;
        }
        "version" => {
            println!("TigerSmartChain v1.0.0");
            println!("Rust version: 1.0.0");
        }
        "export" => {
            commands::export_blockchain(&args)?;
        }
        "import" => {
            commands::import_blockchain(&args)?;
        }
        "monitor" => {
            commands::start_monitor(&args)?;
        }
        "console" => {
            commands::start_console(&args)?;
        }
        "attach" => {
            commands::attach_node(&args)?;
        }
        "" => {
            // Default: show info and start
            println!("Starting node...");
            info!("Data directory: {:?}", args.datadir);
            info!("HTTP server: {}:{}", args.http_host, args.http_port);
            info!("WebSocket server: {}:{}", args.ws_host, args.ws_port);
        }
        _ => {
            anyhow::bail!("Unknown command: {}", args.command);
        }
    }

    Ok(())
}