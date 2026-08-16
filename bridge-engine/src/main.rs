//! TigerSmartChain Bridge Engine - Main Binary

use tigersmartchain_bridge::{BridgeEngine, Chain, ChainConfig, BridgeConfig, FeeConfig};
use clap::{Parser, ValueEnum};
use tracing_subscriber::{fmt, EnvFilter};

#[derive(Parser, Debug)]
#[command(name = "tigersmartchain-bridge")]
#[command(about = "TigerSmartChain Bridge Engine", long_about = None)]
struct Cli {
    /// Source chain
    #[arg(short, long)]
    from: ChainArg,

    /// Destination chain
    #[arg(short, long)]
    to: ChainArg,

    /// Token address
    #[arg(short, long)]
    token: String,

    /// Recipient address
    #[arg(short, long)]
    recipient: String,

    /// Amount
    #[arg(short, long)]
    amount: String,

    /// Config file
    #[arg(short, long)]
    config: Option<String>,

    /// Enable verbose output
    #[arg(short, long)]
    verbose: bool,
}

#[derive(Debug, Clone, Copy, ValueEnum)]
enum ChainArg {
    Tigersmartchain,
    Ethereum,
    Polygon,
    Arbitrum,
    Optimism,
    Base,
}

impl From<ChainArg> for Chain {
    fn from(arg: ChainArg) -> Self {
        match arg {
            ChainArg::Tigersmartchain => Chain::TigerSmartChain,
            ChainArg::Ethereum => Chain::Ethereum,
            ChainArg::Polygon => Chain::Polygon,
            ChainArg::Arbitrum => Chain::Arbitrum,
            ChainArg::Optimism => Chain::Optimism,
            ChainArg::Base => Chain::Base,
        }
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();
    
    let filter = if cli.verbose {
        EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("debug"))
    } else {
        EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"))
    };
    
    fmt()
        .with_env_filter(filter)
        .with_target(false)
        .init();

    // Create default config
    let config = BridgeConfig {
        chains: vec![
            ChainConfig {
                chain: Chain::TigerSmartChain,
                rpc_url: "http://localhost:8545".to_string(),
                contract_address: "0x0000000000000000000000000000000000000001".to_string(),
                start_block: 0,
            },
            ChainConfig {
                chain: Chain::Ethereum,
                rpc_url: "https://eth-mainnet.alchemyapi.io".to_string(),
                contract_address: "0x0000000000000000000000000000000000000002".to_string(),
                start_block: 15000000,
            },
        ],
        relayers: vec![],
        validators: vec![],
        relayers_pubkeys: vec![],
        signature_threshold: 3,
        confirmation_blocks: 15,
        fee: FeeConfig {
            flat_fee: "1000000000000000".to_string(),
            percentage_fee: 0.003,
            min_fee: "1000000000000000".to_string(),
            max_fee: "100000000000000000".to_string(),
        },
        database_url: std::env::var("DATABASE_URL").unwrap_or_default(),
    };

    // Create bridge engine
    let mut bridge = BridgeEngine::new(config);
    // Initialize providers (and optional Postgres pool). Failures here are
    // non-fatal for a dry-run initiate; log and continue.
    if let Err(e) = bridge.init().await {
        eprintln!("warn: bridge init: {}", e);
    }

    // Execute transfer
    let source: Chain = cli.from.into();
    let dest: Chain = cli.to.into();

    println!("Initiating transfer from {:?} to {:?}", source, dest);
    println!("Token: {}", cli.token);
    println!("Recipient: {}", cli.recipient);
    println!("Amount: {}", cli.amount);

    let token_type = tigersmartchain_bridge::TokenType::ERC20;
    let transfer = bridge
        .initiate_transfer(
            source,
            dest,
            "0x0000000000000000000000000000000000000000".to_string(),
            cli.recipient.clone(),
            cli.token.clone(),
            token_type,
            cli.amount.clone(),
            None,
        )
        .await
        .map_err(|e| -> Box<dyn std::error::Error> { e.to_string().into() })?;

    println!("\nTransfer initiated successfully");
    println!("  Transfer ID : {}", transfer.id);
    println!("  Source      : {:?}", transfer.source_chain);
    println!("  Destination : {:?}", transfer.destination_chain);
    println!("  Status      : {:?}", transfer.status);
    println!("  Timestamp   : {}", transfer.timestamp);

    Ok(())
}