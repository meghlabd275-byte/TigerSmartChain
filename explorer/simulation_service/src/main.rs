//! TigerScan Simulation Service Main Entry Point

use anyhow::Result;
use simulation::{SimulationService, Config};
use tokio::signal;
use tracing::{error, info, Level};
use tracing_subscriber::FmtSubscriber;

#[tokio::main]
async fn main() -> Result<()> {
    let subscriber = FmtSubscriber::builder()
        .with_max_level(Level::INFO)
        .with_target(true)
        .init();

    info!("TigerScan Simulation Service starting...");

    let config = Config::default();
    let simulation = SimulationService::new(config).await?;

    tokio::select! {
        result = async {
            loop {
                tokio::time::sleep(std::time::Duration::from_secs(60)).await;
                info!("Simulation service running");
            }
        } => {},
        _ = signal::ctrl_c() => {
            info!("Received shutdown signal");
        }
    }

    info!("TigerScan Simulation Service stopped");

    Ok(())
}