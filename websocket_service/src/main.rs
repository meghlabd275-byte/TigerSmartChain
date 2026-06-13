//! WebSocket Service Main Entry Point
//! Production-grade WebSocket server for TigerScan

#![forbid(unsafe_code)]

use tiger_websocket_service::{Config, Server};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize logging
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info"))
        .init();
    
    log::info!("Starting TigerScan WebSocket Service...");
    
    // Load configuration
    let config = Config::from_env();
    log::info!("Configuration loaded: max_connections={}, port={}", 
         config.max_connections, config.port);
    
    // Create and start server
    let mut server = Server::new(config);
    
    if let Err(e) = server.start().await {
        log::error!("Server error: {}", e);
        std::process::exit(1);
    }
    
    log::info!("WebSocket service stopped");
    Ok(())
}