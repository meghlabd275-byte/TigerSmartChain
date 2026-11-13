//! TigerScan API server daemon.
//!
//! Configuration comes from the environment (`API_PORT`, `API_HOST`, `RPC_URL`,
//! `CORS_ALLOWED_ORIGINS`), which is what the compose file sets.

use tiger_api::{APIServer, APIServerBuilder};

#[tokio::main]
async fn main() {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();

    let mut builder = APIServerBuilder::new();

    if let Ok(port) = std::env::var("API_PORT") {
        match port.parse::<u16>() {
            Ok(value) => builder = builder.port(value),
            Err(_) => {
                log::error!("API_PORT is not a port number: {port}");
                std::process::exit(1);
            }
        }
    }
    if let Ok(host) = std::env::var("API_HOST") {
        builder = builder.host(&host);
    }

    let server = builder.build();
    let config = server.config().clone();

    log::info!(
        "starting TigerScan API on {}:{} (node rpc {})",
        config.host,
        config.port,
        config.rpc_url
    );

    if let Err(e) = server.start().await {
        log::error!("api server stopped: {e}");
        std::process::exit(1);
    }
}
