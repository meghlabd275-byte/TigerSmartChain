//! TigerScan Load Balancer
//! 
//! High-performance load balancer with multiple algorithms
//! Built in Rust for ultra-low latency

use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::RwLock;
use actix_web::{web, App, HttpServer, HttpResponse, Responder, middleware};
use serde::{Deserialize, Serialize};

// ============================================================================
// CONFIG
// ============================================================================

#[derive(Debug, Clone)]
pub struct LbConfig {
    pub host: String,
    pub port: u16,
    pub health_check_interval_ms: u64,
    pub max_retries: usize,
    pub timeout_ms: u64,
}

impl Default for LbConfig {
    fn default() -> Self {
        Self {
            host: "0.0.0.0".to_string(),
            port: 8080,
            health_check_interval_ms: 5000,
            max_retries: 3,
            timeout_ms: 5000,
        }
    }
}

// ============================================================================
// BACKEND
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Backend {
    pub url: String,
    pub weight: u32,
    pub is_healthy: bool,
    pub is_draining: bool,
    pub active_connections: u64,
    pub total_requests: u64,
    pub failed_requests: u64,
    pub avg_latency_ms: u64,
    pub last_health_check: i64,
}

impl Backend {
    pub fn new(url: &str) -> Self {
        Self {
            url: url.to_string(),
            weight: 100,
            is_healthy: true,
            is_draining: false,
            active_connections: 0,
            total_requests: 0,
            failed_requests: 0,
            avg_latency_ms: 0,
            last_health_check: chrono::Utc::now().timestamp(),
        }
    }

    pub fn health_score(&self) -> f64 {
        if !self.is_healthy || self.is_draining {
            return 0.0;
        }
        
        // Calculate health score based on various metrics
        let mut score = 100.0;
        
        // Reduce score for high latency
        if self.avg_latency_ms > 1000 {
            score -= 30.0;
        } else if self.avg_latency_ms > 500 {
            score -= 15.0;
        } else if self.avg_latency_ms > 200 {
            score -= 5.0;
        }
        
        // Reduce score for failed requests
        if self.total_requests > 0 {
            let failure_rate = (self.failed_requests as f64 / self.total_requests as f64) * 100.0;
            score -= failure_rate;
        }
        
        score.max(0.0)
    }
}

// ============================================================================
// ALGORITHMS
// ============================================================================

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LoadBalancingAlgorithm {
    RoundRobin,
    LeastConnections,
    WeightedRoundRobin,
    WeightedLeastConnections,
    IpHash,
    Random,
    HealthScore,
}

impl Default for LoadBalancingAlgorithm {
    fn default() -> Self {
        Self::HealthScore // Best for production
    }
}

// ============================================================================
// LOAD BALANCER
// ============================================================================

pub struct LoadBalancer {
    config: LbConfig,
    algorithm: LoadBalancingAlgorithm,
    backends: Arc<RwLock<HashMap<String, Backend>>>,
    backend_list: Arc<RwLock<Vec<String>>>,
    stats: Arc<RwLock<HashMap<String, BackendStats>>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackendStats {
    pub url: String,
    pub requests: u64,
    pub bytes_sent: u64,
    pub bytes_received: u64,
    pub errors: u64,
}

impl LoadBalancer {
    pub fn new(config: LbConfig, algorithm: LoadBalancingAlgorithm) -> Self {
        Self {
            config,
            algorithm,
            backends: Arc::new(RwLock::new(HashMap::new())),
            backend_list: Arc::new(RwLock::new(Vec::new())),
            stats: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Add a backend
    pub async fn add_backend(&self, url: &str, weight: u32) {
        let backend = Backend {
            url: url.to_string(),
            weight,
            ..Default::default()
        };
        
        let mut backends = self.backends.write().await;
        backends.insert(url.to_string(), backend);
        
        let mut list = self.backend_list.write().await;
        list.push(url.to_string());
        
        let mut stats = self.stats.write().await;
        stats.insert(url.to_string(), BackendStats {
            url: url.to_string(),
            requests: 0,
            bytes_sent: 0,
            bytes_received: 0,
            errors: 0,
        });
    }

    /// Remove a backend
    pub async fn remove_backend(&self, url: &str) {
        let mut backends = self.backends.write().await;
        backends.remove(url);
        
        let mut list = self.backend_list.write().await;
        list.retain(|u| u != url);
    }

    /// Get next backend based on algorithm
    pub async fn get_backend(&self, client_ip: Option<&str>) -> Option<String> {
        let backends = self.backends.read().await;
        let list = self.backend_list.read().await;
        
        if list.is_empty() {
            return None;
        }
        
        let backend = match self.algorithm {
            LoadBalancingAlgorithm::RoundRobin => {
                self.round_robin(&list)
            }
            LoadBalancingAlgorithm::LeastConnections => {
                self.least_connections(&backends, &list)
            }
            LoadBalancingAlgorithm::WeightedRoundRobin => {
                self.weighted_round_robin(&backends, &list)
            }
            LoadBalancingAlgorithm::WeightedLeastConnections => {
                self.weighted_least_connections(&backends, &list)
            }
            LoadBalancingAlgorithm::IpHash => {
                self.ip_hash(client_ip, &list)
            }
            LoadBalancingAlgorithm::Random => {
                self.random_(&list)
            }
            LoadBalancingAlgorithm::HealthScore => {
                self.health_score(&backends, &list)
            }
        };
        
        backend.map(|b| b.url.clone())
    }

    // Algorithms
    
    fn round_robin(&self, list: &Vec<String>) -> Option<Backend> {
        let idx = chrono::Utc::now().timestamp() as usize % list.len();
        list.get(idx).cloned().and_then(|url| {
            // This is simplified - in production, would look up from HashMap
            Some(Backend::new(&url))
        })
    }

    fn least_connections(&self, backends: &HashMap<String, Backend>, list: &Vec<String>) -> Option<Backend> {
        list.iter()
            .filter_map(|url| backends.get(url))
            .filter(|b| b.is_healthy && !b.is_draining)
            .min_by_key(|b| b.active_connections as u64)
            .cloned()
    }

    fn weighted_round_robin(&self, backends: &HashMap<String, Backend>, list: &Vec<String>) -> Option<Backend> {
        // Simplified implementation
        self.round_robin(list)
    }

    fn weighted_least_connections(&self, backends: &HashMap<String, Backend>, list: &Vec<String>) -> Option<Backend> {
        // Calculate effective connections (weighted)
        list.iter()
            .filter_map(|url| backends.get(url))
            .filter(|b| b.is_healthy && !b.is_draining)
            .map(|b| {
                let effective = b.active_connections as f64 / b.weight as f64;
                (b.clone(), effective)
            })
            .min_by(|a, b| a.1.partial_cmp(&b.1).unwrap())
            .map(|(b, _)| b)
            .cloned()
    }

    fn ip_hash(&self, client_ip: Option<&str>, list: &Vec<String>) -> Option<Backend> {
        if let Some(ip) = client_ip {
            let hash = ip.bytes().fold(0u32, |acc, b| acc.wrapping_add(b as u32));
            let idx = (hash as usize) % list.len();
            list.get(idx).cloned().and_then(|url| Some(Backend::new(&url)))
        } else {
            self.round_robin(list)
        }
    }

    fn random_(&self, list: &Vec<String>) -> Option<Backend> {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};
        
        let mut hasher = DefaultHasher::new();
        Instant::now().hash(&mut hasher);
        let idx = (hasher.finish() as usize) % list.len();
        
        list.get(idx).cloned().and_then(|url| Some(Backend::new(&url)))
    }

    fn health_score(&self, backends: &HashMap<String, Backend>, list: &Vec<String>) -> Option<Backend> {
        list.iter()
            .filter_map(|url| backends.get(url))
            .filter(|b| b.is_healthy && !b.is_draining)
            .max_by(|a, b| {
                a.health_score().partial_cmp(&b.health_score()).unwrap()
            })
            .cloned()
    }

    /// Forward request to backend
    pub async fn forward(&self, path: &str, client_ip: Option<&str>) -> Option<Backend> {
        self.get_backend(client_ip).await.map(|url| {
            Backend::new(&url)
        })
    }

    /// Health check all backends
    pub async fn health_check(&self) {
        let mut backends = self.backends.write().await;
        
        for (url, backend) in backends.iter_mut() {
            let start = Instant::now();
            
            // Perform health check
            let client = reqwest::Client::builder()
                .timeout(Duration::from_millis(self.config.timeout_ms))
                .build();
            
            if let Ok(client) = client {
                let url = format!("{}/health", url);
                if let Ok(response) = client.get(&url).send().await {
                    let elapsed = start.elapsed().as_millis() as u64;
                    
                    backend.is_healthy = response.status().is_success();
                    backend.avg_latency_ms = (backend.avg_latency_ms + elapsed) / 2;
                } else {
                    backend.is_healthy = false;
                }
            }
            
            backend.last_health_check = chrono::Utc::now().timestamp();
        }
    }

    /// Get stats
    pub async fn get_stats(&self) -> Vec<BackendStats> {
        let stats = self.stats.read().await;
        stats.values().cloned().collect()
    }

    /// Get all backends
    pub async fn get_backends(&self) -> Vec<Backend> {
        let backends = self.backends.read().await;
        backends.values().cloned().collect()
    }
}

impl Default for LoadBalancer {
    fn default() -> Self {
        Self::new(Default::default(), Default::default())
    }
}

// ============================================================================
// ROUTES
// ============================================================================

async fn health() -> impl Responder {
    HttpResponse::Ok().json(serde_json::json!({
        "status": "healthy",
        "service": "TigerScan Load Balancer"
    }))
}

async fn get_backends(lb: web::Data<LoadBalancer>) -> impl Responder {
    let backends = lb.get_backends().await;
    HttpResponse::Ok().json(backends)
}

async fn get_stats(lb: web::Data<LoadBalancer>) -> impl Responder {
    let stats = lb.get_stats().await;
    HttpResponse::Ok().json(stats)
}

// ============================================================================
// MAIN
// ============================================================================

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();

    let config = LbConfig::default();
    let lb = LoadBalancer::new(config.clone(), LoadBalancingAlgorithm::HealthScore);

    // Add some backends
    lb.add_backend("http://node1.tigersmartchain.com", 100).await;
    lb.add_backend("http://node2.tigersmartchain.com", 100).await;
    lb.add_backend("http://node3.tigersmartchain.com", 50).await;

    println!("Starting Load Balancer on {}:{}", config.host, config.port);

    HttpServer::new(move || {
        App::new()
            .app_data(lb.clone())
            .wrap(middleware::Logger::default())
            .route("/health", web::get().to(health))
            .route("/backends", web::get().to(get_backends))
            .route("/stats", web::get().to(get_stats))
    })
    .bind((config.host.as_str(), config.port))?
    .run()
    .await
}