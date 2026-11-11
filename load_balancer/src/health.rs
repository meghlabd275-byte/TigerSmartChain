//! Health checking.
//!
//! A backend's health flips only after a run of consecutive results, so a
//! single timeout does not remove a healthy backend from rotation and a single
//! success does not immediately restore a failing one.

use crate::{Backend, LbConfig};

/// Consecutive outcomes tracked per backend.
#[derive(Debug, Default, Clone)]
pub struct HealthCounter {
    pub consecutive_successes: u32,
    pub consecutive_failures: u32,
}

/// Apply one health-check outcome to a backend.
pub fn apply(backend: &mut Backend, healthy: bool, latency_ms: u64, config: &LbConfig) {
    backend.last_health_check = now_unix();

    if healthy {
        backend.consecutive_failures = 0;
        backend.consecutive_successes = backend.consecutive_successes.saturating_add(1);

        if backend.consecutive_successes >= config.healthy_threshold.max(1) {
            backend.is_healthy = true;
        }

        // Fold the probe latency in so the score reflects the live path.
        backend.avg_latency_ms = if backend.avg_latency_ms == 0 {
            latency_ms
        } else {
            (backend.avg_latency_ms * 4 + latency_ms) / 5
        };
    } else {
        backend.consecutive_successes = 0;
        backend.consecutive_failures = backend.consecutive_failures.saturating_add(1);

        if backend.consecutive_failures >= config.unhealthy_threshold.max(1) {
            backend.is_healthy = false;
        }
    }
}

/// Probe a backend's health endpoint.
///
/// Returns whether the probe succeeded and how long it took. A 5xx or a
/// transport error is a failure; a 4xx is also a failure, because a health
/// endpoint that answers 404 is misconfigured.
pub async fn probe(
    client: &reqwest::Client,
    base_url: &str,
) -> (bool, u64) {
    let url = format!("{}/health", base_url.trim_end_matches('/'));
    let started = std::time::Instant::now();

    match client.get(&url).send().await {
        Ok(response) => {
            let elapsed = started.elapsed().as_millis() as u64;
            (response.status().is_success(), elapsed)
        }
        Err(_) => (false, started.elapsed().as_millis() as u64),
    }
}

fn now_unix() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}
