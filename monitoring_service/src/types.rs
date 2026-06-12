//! Monitoring Service Types

use serde::{Deserialize, Serialize};

// =============================================================================
// MONITORING SERVICE
// =============================================================================

/// Metric
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Metric {
    pub name: String,
    pub value: f64,
    pub timestamp: u64,
}

/// Alert
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Alert {
    pub id: String,
    pub severity: String,
    pub message: String,
    pub timestamp: u64,
    pub resolved: bool,
}

/// Health Check
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthCheck {
    pub service: String,
    pub status: String,
    pub latency_ms: u64,
    pub last_check: u64,
}

/// Monitoring Service
pub struct Service {
    metrics: std::collections::HashMap<String, Vec<Metric>>,
    alerts: std::collections::HashMap<String, Alert>,
}

impl Service {
    pub fn new() -> Self {
        Self {
            metrics: std::collections::HashMap::new(),
            alerts: std::collections::HashMap::new(),
        }
    }

    /// Record metric
    pub fn record_metric(&mut self, name: String, value: f64) {
        let metric = Metric {
            name: name.clone(),
            value,
            timestamp: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs(),
        };
        self.metrics.entry(name).or_insert_with(Vec::new).push(metric);
    }

    /// Create alert
    pub fn create_alert(&mut self, alert: Alert) {
        self.alerts.insert(alert.id.clone(), alert);
    }
}

impl Default for Service {
    fn default() -> Self {
        Self::new()
    }
}