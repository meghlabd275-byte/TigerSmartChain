//! TigerScan load balancer.
//!
//! Seven selection algorithms over a set of backends, health checking, and
//! request forwarding that reports the outcome back into the backend's
//! counters.
//!
//! The previous version selected a backend with `timestamp % len`, which
//! ignores the round-robin counter entirely and picks the same backend for a
//! whole second; its weighted round-robin ignored the weights and delegated to
//! that same broken round-robin; its `ip_hash` was a sum of bytes, so any
//! permutation of an address landed on the same backend; and `forward` never
//! sent a request, never incremented `total_requests` and never recorded a
//! failure, so `health_score` always saw zero traffic.

pub mod algo;
pub mod health;

use algo::SelectionState;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::RwLock;

/// Load balancer configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LbConfig {
    pub host: String,
    pub port: u16,
    pub health_check_interval_ms: u64,
    pub max_retries: usize,
    pub timeout_ms: u64,
    /// Consecutive failed health checks before a backend is marked down.
    pub unhealthy_threshold: u32,
    /// Consecutive successful health checks before it is marked up again.
    pub healthy_threshold: u32,
}

impl Default for LbConfig {
    fn default() -> Self {
        Self {
            host: "0.0.0.0".to_string(),
            port: 8080,
            health_check_interval_ms: 5000,
            max_retries: 3,
            timeout_ms: 5000,
            unhealthy_threshold: 2,
            healthy_threshold: 2,
        }
    }
}

/// The selection algorithm.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
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
        Self::HealthScore
    }
}

/// A backend the balancer can send traffic to.
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
    /// Consecutive successful health probes.
    pub consecutive_successes: u32,
    /// Consecutive failed health probes.
    pub consecutive_failures: u32,
}

impl Default for Backend {
    fn default() -> Self {
        Self {
            url: String::new(),
            weight: 100,
            is_healthy: true,
            is_draining: false,
            active_connections: 0,
            total_requests: 0,
            failed_requests: 0,
            avg_latency_ms: 0,
            last_health_check: 0,
            consecutive_successes: 0,
            consecutive_failures: 0,
        }
    }
}

impl Backend {
    /// A healthy backend with the default weight.
    pub fn new(url: &str) -> Self {
        Self {
            url: url.to_string(),
            ..Default::default()
        }
    }

    /// A backend with an explicit weight.
    pub fn with_weight(url: &str, weight: u32) -> Self {
        Self {
            url: url.to_string(),
            weight,
            ..Default::default()
        }
    }

    /// Whether the backend may receive traffic.
    pub fn is_available(&self) -> bool {
        self.is_healthy && !self.is_draining
    }

    /// A 0..=100 score combining latency and failure rate.
    ///
    /// The failure rate is a ratio of failures to requests, and it is only
    /// meaningful once there has been traffic; before that the score depends on
    /// latency alone.
    pub fn health_score(&self) -> f64 {
        if !self.is_available() {
            return 0.0;
        }

        let mut score = 100.0;

        if self.avg_latency_ms > 1000 {
            score -= 30.0;
        } else if self.avg_latency_ms > 500 {
            score -= 15.0;
        } else if self.avg_latency_ms > 200 {
            score -= 5.0;
        }

        if self.total_requests > 0 {
            let failure_rate = (self.failed_requests as f64 / self.total_requests as f64) * 100.0;
            score -= failure_rate;
        }

        score.max(0.0)
    }

    /// Record a completed request.
    pub fn record_success(&mut self, latency_ms: u64) {
        self.total_requests += 1;
        // An exponential moving average keeps recent latency weighted without
        // storing a window.
        self.avg_latency_ms = if self.total_requests == 1 {
            latency_ms
        } else {
            (self.avg_latency_ms * 4 + latency_ms) / 5
        };
        self.active_connections = self.active_connections.saturating_sub(1);
    }

    /// Record a failed request.
    pub fn record_failure(&mut self) {
        self.total_requests += 1;
        self.failed_requests += 1;
        self.active_connections = self.active_connections.saturating_sub(1);
    }
}

/// Per-backend traffic counters, exposed to the `/stats` endpoint.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackendStats {
    pub url: String,
    pub requests: u64,
    pub bytes_sent: u64,
    pub bytes_received: u64,
    pub errors: u64,
}

/// The result of forwarding a request.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ForwardedResponse {
    pub backend: String,
    pub status: u16,
    pub body: Vec<u8>,
    pub latency_ms: u64,
}

/// Load balancer errors.
#[derive(Debug, thiserror::Error)]
pub enum LbError {
    #[error("no healthy backend is available")]
    NoBackendAvailable,

    #[error("all {attempts} attempts to reach a backend failed: {last}")]
    AllAttemptsFailed { attempts: usize, last: String },

    #[error("backend returned status {0}")]
    BackendError(u16),

    #[error("http error: {0}")]
    Http(String),
}

/// The load balancer.
pub struct LoadBalancer {
    config: LbConfig,
    algorithm: LoadBalancingAlgorithm,
    backends: Arc<RwLock<HashMap<String, Backend>>>,
    /// Insertion order, so round-robin and hashing are stable across calls.
    order: Arc<RwLock<Vec<String>>>,
    stats: Arc<RwLock<HashMap<String, BackendStats>>>,
    state: Arc<RwLock<SelectionState>>,
    client: reqwest::Client,
}

impl Clone for LoadBalancer {
    fn clone(&self) -> Self {
        Self {
            config: self.config.clone(),
            algorithm: self.algorithm,
            backends: Arc::clone(&self.backends),
            order: Arc::clone(&self.order),
            stats: Arc::clone(&self.stats),
            state: Arc::clone(&self.state),
            client: self.client.clone(),
        }
    }
}

impl LoadBalancer {
    /// Create a balancer.
    pub fn new(config: LbConfig, algorithm: LoadBalancingAlgorithm) -> Self {
        let client = reqwest::Client::builder()
            .timeout(Duration::from_millis(config.timeout_ms))
            .build()
            .unwrap_or_default();

        Self {
            config,
            algorithm,
            backends: Arc::new(RwLock::new(HashMap::new())),
            order: Arc::new(RwLock::new(Vec::new())),
            stats: Arc::new(RwLock::new(HashMap::new())),
            state: Arc::new(RwLock::new(SelectionState::default())),
            client,
        }
    }

    /// The configuration.
    pub fn config(&self) -> &LbConfig {
        &self.config
    }

    /// Add or replace a backend.
    pub async fn add_backend(&self, url: &str, weight: u32) {
        if weight == 0 {
            return;
        }

        let backend = Backend::with_weight(url, weight);

        {
            let mut backends = self.backends.write().await;
            backends.insert(url.to_string(), backend);
        }
        {
            let mut order = self.order.write().await;
            if !order.iter().any(|u| u == url) {
                order.push(url.to_string());
            }
        }
        {
            let mut stats = self.stats.write().await;
            stats.entry(url.to_string()).or_insert_with(|| BackendStats {
                url: url.to_string(),
                requests: 0,
                bytes_sent: 0,
                bytes_received: 0,
                errors: 0,
            });
        }
    }

    /// Remove a backend.
    pub async fn remove_backend(&self, url: &str) {
        self.backends.write().await.remove(url);
        self.order.write().await.retain(|u| u != url);
        self.stats.write().await.remove(url);
    }

    /// Mark a backend as draining, so it stops receiving new traffic but keeps
    /// serving the connections already open.
    pub async fn drain_backend(&self, url: &str) -> bool {
        let mut backends = self.backends.write().await;
        match backends.get_mut(url) {
            Some(backend) => {
                backend.is_draining = true;
                true
            }
            None => false,
        }
    }

    /// Choose a backend for a request.
    pub async fn get_backend(&self, client_ip: Option<&str>) -> Option<Backend> {
        let backends = self.backends.read().await;
        let order = self.order.read().await;
        let mut state = self.state.write().await;

        algo::select(self.algorithm, &backends, &order, client_ip, &mut state)
    }

    /// Number of backends that can currently take traffic.
    pub async fn available_backend_count(&self) -> usize {
        self.backends
            .read()
            .await
            .values()
            .filter(|b| b.is_available())
            .count()
    }

    /// Forward a request, retrying on another backend up to `max_retries`.
    ///
    /// Each attempt increments the backend's request counters and decrements
    /// its in-flight count, so `health_score` sees real traffic. A 5xx response
    /// counts as a failure and is retried; a 4xx is returned to the caller
    /// unchanged, because it is the client's fault and retrying it would be
    /// pointless.
    pub async fn forward(
        &self,
        path: &str,
        client_ip: Option<&str>,
    ) -> Result<ForwardedResponse, LbError> {
        let mut last_error = String::new();

        for attempt in 0..=self.config.max_retries {
            let backend = match self.get_backend(client_ip).await {
                Some(backend) => backend,
                None => {
                    return if attempt == 0 {
                        Err(LbError::NoBackendAvailable)
                    } else {
                        Err(LbError::AllAttemptsFailed {
                            attempts: attempt,
                            last: last_error,
                        })
                    };
                }
            };

            self.begin_request(&backend.url).await;

            let url = format!("{}{}", backend.url.trim_end_matches('/'), path);
            let started = std::time::Instant::now();

            match self.client.get(&url).send().await {
                Ok(response) => {
                    let latency = started.elapsed().as_millis() as u64;
                    let status = response.status().as_u16();
                    let body = response.bytes().await.map(|b| b.to_vec()).unwrap_or_default();

                    if status >= 500 {
                        self.finish_failure(&backend.url).await;
                        last_error = format!("{url} returned {status}");
                        continue;
                    }

                    self.finish_success(&backend.url, latency, body.len()).await;

                    return Ok(ForwardedResponse {
                        backend: backend.url,
                        status,
                        body,
                        latency_ms: latency,
                    });
                }
                Err(e) => {
                    self.finish_failure(&backend.url).await;
                    last_error = e.to_string();
                }
            }
        }

        Err(LbError::AllAttemptsFailed {
            attempts: self.config.max_retries + 1,
            last: last_error,
        })
    }

    async fn begin_request(&self, url: &str) {
        let mut backends = self.backends.write().await;
        if let Some(backend) = backends.get_mut(url) {
            backend.active_connections += 1;
        }
    }

    async fn finish_success(&self, url: &str, latency_ms: u64, bytes: usize) {
        {
            let mut backends = self.backends.write().await;
            if let Some(backend) = backends.get_mut(url) {
                backend.record_success(latency_ms);
            }
        }
        let mut stats = self.stats.write().await;
        if let Some(entry) = stats.get_mut(url) {
            entry.requests += 1;
            entry.bytes_received += bytes as u64;
        }
    }

    async fn finish_failure(&self, url: &str) {
        {
            let mut backends = self.backends.write().await;
            if let Some(backend) = backends.get_mut(url) {
                backend.record_failure();
            }
        }
        let mut stats = self.stats.write().await;
        if let Some(entry) = stats.get_mut(url) {
            entry.requests += 1;
            entry.errors += 1;
        }
    }

    /// Traffic counters for every backend.
    pub async fn get_stats(&self) -> Vec<BackendStats> {
        self.stats.read().await.values().cloned().collect()
    }

    /// Every backend, healthy or not.
    pub async fn get_backends(&self) -> Vec<Backend> {
        self.backends.read().await.values().cloned().collect()
    }

    /// Probe every backend's `/health` endpoint and apply the outcome.
    ///
    /// This is the loop the server runs on a timer. It uses the same
    /// consecutive-outcome rule as [`LoadBalancer::apply_health_result`], so a
    /// single blip does not change routing.
    pub async fn health_check_all(&self) {
        let urls: Vec<String> = self.order.read().await.clone();

        for url in urls {
            let (healthy, latency_ms) = health::probe(&self.client, &url).await;
            self.apply_health_result(&url, healthy, latency_ms).await;
        }
    }

    /// Apply a health check outcome to a backend.
    ///
    /// A backend flips only after `healthy_threshold` consecutive successes or
    /// `unhealthy_threshold` consecutive failures, so one dropped packet does
    /// not remove a backend from rotation.
    pub async fn apply_health_result(&self, url: &str, healthy: bool, latency_ms: u64) {
        let mut backends = self.backends.write().await;
        let backend = match backends.get_mut(url) {
            Some(backend) => backend,
            None => return,
        };

        health::apply(backend, healthy, latency_ms, &self.config);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use actix_web::{web, App, HttpResponse, HttpServer};

    fn config() -> LbConfig {
        LbConfig {
            health_check_interval_ms: 60_000,
            timeout_ms: 2000,
            max_retries: 2,
            unhealthy_threshold: 2,
            healthy_threshold: 2,
            ..Default::default()
        }
    }

    #[tokio::test]
    async fn a_backend_can_be_added_listed_and_removed() {
        let lb = LoadBalancer::new(config(), LoadBalancingAlgorithm::RoundRobin);
        lb.add_backend("http://127.0.0.1:1", 100).await;
        lb.add_backend("http://127.0.0.1:2", 100).await;

        assert_eq!(lb.get_backends().await.len(), 2);
        assert_eq!(lb.available_backend_count().await, 2);
        assert_eq!(lb.get_stats().await.len(), 2);

        lb.remove_backend("http://127.0.0.1:1").await;
        assert_eq!(lb.get_backends().await.len(), 1);
        assert_eq!(lb.get_stats().await.len(), 1);
    }

    #[tokio::test]
    async fn a_zero_weight_backend_is_rejected() {
        let lb = LoadBalancer::new(config(), LoadBalancingAlgorithm::RoundRobin);
        lb.add_backend("http://127.0.0.1:1", 0).await;
        assert!(lb.get_backends().await.is_empty());
    }

    #[tokio::test]
    async fn selection_returns_none_when_no_backend_is_available() {
        let lb = LoadBalancer::new(config(), LoadBalancingAlgorithm::RoundRobin);
        assert!(lb.get_backend(None).await.is_none());

        lb.add_backend("http://127.0.0.1:1", 100).await;
        lb.drain_backend("http://127.0.0.1:1").await;
        assert!(lb.get_backend(None).await.is_none());
    }

    #[tokio::test]
    async fn health_flips_only_after_the_configured_run() {
        let lb = LoadBalancer::new(config(), LoadBalancingAlgorithm::RoundRobin);
        lb.add_backend("http://127.0.0.1:1", 100).await;

        // One failure is not enough.
        lb.apply_health_result("http://127.0.0.1:1", false, 0).await;
        assert!(lb.get_backend(None).await.is_some());

        // The second consecutive failure takes it out of rotation.
        lb.apply_health_result("http://127.0.0.1:1", false, 0).await;
        assert!(lb.get_backend(None).await.is_none());

        // It takes two successes to bring it back.
        lb.apply_health_result("http://127.0.0.1:1", true, 10).await;
        assert!(lb.get_backend(None).await.is_none());
        lb.apply_health_result("http://127.0.0.1:1", true, 10).await;
        assert!(lb.get_backend(None).await.is_some());
    }

    #[tokio::test]
    async fn a_successful_request_is_forwarded_and_counted() {
        // A real server on a real port, so the request path is exercised end
        // to end rather than through a stub.
        let server = HttpServer::new(|| {
            App::new().route(
                "/health",
                web::get().to(|| async { HttpResponse::Ok().body("ok") }),
            )
        })
        .bind(("127.0.0.1", 0))
        .unwrap();

        let addr = server.addrs()[0];
        let handle = server.run();
        tokio::spawn(handle);

        let lb = LoadBalancer::new(config(), LoadBalancingAlgorithm::RoundRobin);
        lb.add_backend(&format!("http://{addr}"), 100).await;

        let response = lb.forward("/health", None).await.expect("request succeeds");
        assert_eq!(response.status, 200);
        assert_eq!(response.body, b"ok");

        // The counters must reflect the traffic.
        let backends = lb.get_backends().await;
        assert_eq!(backends[0].total_requests, 1);
        assert_eq!(backends[0].failed_requests, 0);
        assert_eq!(backends[0].active_connections, 0);

        let stats = lb.get_stats().await;
        assert_eq!(stats[0].requests, 1);
        assert_eq!(stats[0].errors, 0);
        assert_eq!(stats[0].bytes_received, 2);
    }

    #[tokio::test]
    async fn a_failing_request_is_retried_and_recorded_as_a_failure() {
        let lb = LoadBalancer::new(config(), LoadBalancingAlgorithm::RoundRobin);
        // Nothing is listening here, so every attempt fails at the transport.
        lb.add_backend("http://127.0.0.1:9", 100).await;

        let result = lb.forward("/health", None).await;
        assert!(matches!(result, Err(LbError::AllAttemptsFailed { .. })));

        let backends = lb.get_backends().await;
        assert_eq!(backends[0].total_requests, 3);
        assert_eq!(backends[0].failed_requests, 3);
        assert_eq!(backends[0].active_connections, 0);

        let stats = lb.get_stats().await;
        assert_eq!(stats[0].errors, 3);
    }

    #[tokio::test]
    async fn a_server_error_is_retried_on_another_backend() {
        // Two real servers: the first always fails with 500, the second works.
        let bad = HttpServer::new(|| {
            App::new().route(
                "/health",
                web::get().to(|| async { HttpResponse::InternalServerError().finish() }),
            )
        })
        .bind(("127.0.0.1", 0))
        .unwrap();
        let bad_addr = bad.addrs()[0];
        tokio::spawn(bad.run());

        let good = HttpServer::new(|| {
            App::new().route(
                "/health",
                web::get().to(|| async { HttpResponse::Ok().body("healthy") }),
            )
        })
        .bind(("127.0.0.1", 0))
        .unwrap();
        let good_addr = good.addrs()[0];
        tokio::spawn(good.run());

        let lb = LoadBalancer::new(config(), LoadBalancingAlgorithm::RoundRobin);
        lb.add_backend(&format!("http://{bad_addr}"), 100).await;
        lb.add_backend(&format!("http://{good_addr}"), 100).await;

        let response = lb.forward("/health", None).await.expect("the retry succeeds");
        assert_eq!(response.status, 200);
        assert_eq!(response.body, b"healthy");
        assert_eq!(response.backend, format!("http://{good_addr}"));

        // The failing backend must have recorded the error.
        let stats = lb.get_stats().await;
        let bad_stats = stats
            .iter()
            .find(|s| s.url == format!("http://{bad_addr}"))
            .unwrap();
        assert_eq!(bad_stats.errors, 1);
    }

    #[tokio::test]
    async fn health_score_improves_when_latency_drops() {
        let mut backend = Backend::new("http://127.0.0.1:1");
        backend.avg_latency_ms = 1500;
        let slow = backend.health_score();

        backend.avg_latency_ms = 20;
        let fast = backend.health_score();

        assert!(fast > slow);
    }

    #[tokio::test]
    async fn a_failure_rate_lowers_the_score() {
        let mut backend = Backend::new("http://127.0.0.1:1");
        backend.avg_latency_ms = 20;
        backend.total_requests = 100;
        backend.failed_requests = 30;

        assert!(backend.health_score() < 80.0);
    }

    #[tokio::test]
    async fn an_unavailable_backend_scores_zero() {
        let mut backend = Backend::new("http://127.0.0.1:1");
        backend.is_healthy = false;
        assert_eq!(backend.health_score(), 0.0);

        backend.is_healthy = true;
        backend.is_draining = true;
        assert_eq!(backend.health_score(), 0.0);
    }

    #[tokio::test]
    async fn forwarding_with_no_backends_reports_the_cause() {
        let lb = LoadBalancer::new(config(), LoadBalancingAlgorithm::RoundRobin);
        assert!(matches!(
            lb.forward("/health", None).await,
            Err(LbError::NoBackendAvailable)
        ));
    }

    #[tokio::test]
    async fn record_success_tracks_a_running_latency_average() {
        let mut backend = Backend::new("http://127.0.0.1:1");
        backend.active_connections = 1;

        backend.record_success(100);
        assert_eq!(backend.avg_latency_ms, 100);
        assert_eq!(backend.total_requests, 1);
        assert_eq!(backend.active_connections, 0);

        backend.active_connections = 1;
        backend.record_success(200);
        // The moving average weights the new sample by one fifth.
        assert_eq!(backend.avg_latency_ms, 120);
        assert_eq!(backend.total_requests, 2);
    }
}
