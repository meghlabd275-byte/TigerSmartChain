//! TigerScan Indexer Service Main Entry Point

use anyhow::Result;
use indexer::{IndexerService, Config};
use tokio::signal;
use tracing::{error, info, Level};
use tracing_subscriber::FmtSubscriber;

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize logging
    let _subscriber = FmtSubscriber::builder()
        .with_max_level(Level::INFO)
        .with_target(true)
        .with_thread_ids(true)
        .with_file(true)
        .with_line_number(true)
        .init();

    info!("TigerScan Indexer Service starting...");

    // Load configuration
    let config = Config::default();

    // Create indexer service
    let mut indexer = IndexerService::new(config).await?;

    // Kept alive so the final progress can be reported after shutdown.
    let state = indexer.state();
    
    tokio::select! {
        result = indexer.start() => {
            if let Err(e) = result {
                error!("Indexer error: {}", e);
            }
        }
        _ = signal::ctrl_c() => {
            info!("Received shutdown signal");
            indexer.stop().await;
        }
    }

    let state = state.read();
    info!(
        indexed_block = state.last_indexed_block,
        current_block = state.current_block,
        errors = state.errors.len(),
        "TigerScan Indexer Service stopped"
    );

    Ok(())
}