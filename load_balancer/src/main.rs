//! Load balancer server.
//!
//! Backends come from `LB_BACKENDS` (comma-separated URLs, each optionally
//! `url=weight`). Nothing is hardcoded: an empty list means the balancer starts
//! with no backends and refuses traffic until some are added.

use actix_web::{middleware, web, App, HttpResponse, HttpServer, Responder};
use std::sync::Arc;
use std::time::Duration;
use tiger_load_balancer::{LbConfig, LoadBalancer, LoadBalancingAlgorithm};

/// Backends parsed from the environment.
fn backends_from_env() -> Vec<(String, u32)> {
    std::env::var("LB_BACKENDS")
        .unwrap_or_default()
        .split(',')
        .filter_map(|entry| {
            let entry = entry.trim();
            if entry.is_empty() {
                return None;
            }
            match entry.split_once('=') {
                Some((url, weight)) => Some((url.trim().to_string(), weight.trim().parse().ok()?)),
                None => Some((entry.to_string(), 100)),
            }
        })
        .collect()
}

/// The algorithm named by `LB_ALGORITHM`, defaulting to health score.
fn algorithm_from_env() -> LoadBalancingAlgorithm {
    match std::env::var("LB_ALGORITHM")
        .unwrap_or_default()
        .to_ascii_lowercase()
        .as_str()
    {
        "round_robin" => LoadBalancingAlgorithm::RoundRobin,
        "least_connections" => LoadBalancingAlgorithm::LeastConnections,
        "weighted_round_robin" => LoadBalancingAlgorithm::WeightedRoundRobin,
        "weighted_least_connections" => LoadBalancingAlgorithm::WeightedLeastConnections,
        "ip_hash" => LoadBalancingAlgorithm::IpHash,
        "random" => LoadBalancingAlgorithm::Random,
        _ => LoadBalancingAlgorithm::HealthScore,
    }
}

async fn health(lb: web::Data<Arc<LoadBalancer>>) -> impl Responder {
    let available = lb.available_backend_count().await;
    HttpResponse::Ok().json(serde_json::json!({
        "status": if available > 0 { "healthy" } else { "degraded" },
        "service": "TigerScan Load Balancer",
        "available_backends": available,
    }))
}

async fn list_backends(lb: web::Data<Arc<LoadBalancer>>) -> impl Responder {
    HttpResponse::Ok().json(lb.get_backends().await)
}

async fn stats(lb: web::Data<Arc<LoadBalancer>>) -> impl Responder {
    HttpResponse::Ok().json(lb.get_stats().await)
}

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();

    let config = LbConfig {
        host: std::env::var("LB_HOST").unwrap_or_else(|_| "0.0.0.0".to_string()),
        port: std::env::var("LB_PORT")
            .ok()
            .and_then(|p| p.parse().ok())
            .unwrap_or(8080),
        ..Default::default()
    };

    let lb = Arc::new(LoadBalancer::new(config.clone(), algorithm_from_env()));

    for (url, weight) in backends_from_env() {
        lb.add_backend(&url, weight).await;
    }

    log::info!(
        "load balancer listening on {}:{} with {} backends",
        config.host,
        config.port,
        lb.get_backends().await.len()
    );

    // Health checking runs on its own timer for the life of the process.
    let health_lb = Arc::clone(&lb);
    let interval = Duration::from_millis(config.health_check_interval_ms);
    tokio::spawn(async move {
        let mut ticker = tokio::time::interval(interval);
        loop {
            ticker.tick().await;
            health_lb.health_check_all().await;
        }
    });

    let server_lb = Arc::clone(&lb);
    HttpServer::new(move || {
        App::new()
            .app_data(web::Data::new(Arc::clone(&server_lb)))
            .wrap(middleware::Logger::default())
            .route("/health", web::get().to(health))
            .route("/backends", web::get().to(list_backends))
            .route("/stats", web::get().to(stats))
    })
    .bind((config.host.as_str(), config.port))?
    .run()
    .await
}
