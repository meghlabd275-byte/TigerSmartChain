//! TigerScan indexer daemon.
//!
//! Reads configuration from the environment (the compose file sets
//! `INDEXER_RPC_URL`, `INDEXER_CHAIN_ID`, `DATABASE_URL` and friends), connects
//! to the database and the node, then runs the block-processing loop until it
//! is signalled to stop.

use tiger_indexer::{IndexerBuilder, IndexerConfig};

#[tokio::main]
async fn main() {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();

    let mut config = IndexerConfig::default();
    if let Ok(url) = std::env::var("INDEXER_RPC_URL") {
        config.rpc_url = url;
    }
    if let Ok(url) = std::env::var("INDEXER_WS_URL") {
        config.ws_url = url;
    }
    if let Ok(chain_id) = std::env::var("INDEXER_CHAIN_ID") {
        if let Ok(value) = chain_id.parse() {
            config.chain_id = value;
        }
    }
    if let Ok(batch) = std::env::var("INDEXER_BATCH_SIZE") {
        if let Ok(value) = batch.parse() {
            config.batch_size = value;
        }
    }
    if let Ok(workers) = std::env::var("INDEXER_WORKERS") {
        if let Ok(value) = workers.parse() {
            config.parallel_workers = value;
        }
    }
    if let Ok(depth) = std::env::var("INDEXER_CONFIRMATION_DEPTH") {
        if let Ok(value) = depth.parse() {
            config.confirmations = value;
        }
    }
    if let Ok(start) = std::env::var("INDEXER_START_BLOCK") {
        if let Ok(value) = start.parse() {
            config.start_block = value;
        }
    }

    log::info!(
        "starting indexer: chain {} rpc {} from block {}",
        config.chain_id,
        config.rpc_url,
        config.start_block
    );

    let mut indexer = IndexerBuilder::new()
        .chain_id(config.chain_id)
        .rpc_url(&config.rpc_url)
        .ws_url(&config.ws_url)
        .start_block(config.start_block)
        .batch_size(config.batch_size)
        .build();

    // A database is required: without one the indexer can read the chain but
    // cannot store anything, so failing at startup is correct.
    if let Err(e) = indexer.connect_database().await {
        log::error!("cannot reach the database: {e}");
        std::process::exit(1);
    }

    if let Err(e) = indexer.start().await {
        log::error!("indexer stopped: {e}");
        std::process::exit(1);
    }
}
