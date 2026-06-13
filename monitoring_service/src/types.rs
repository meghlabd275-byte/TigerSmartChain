//! Monitoring Service Types - Complete implementation with real-time metrics and alerting
//!
//! This module provides:
//! - Real-time metrics collection and aggregation
//! - Prometheus-compatible metrics
//! - Health checking and status monitoring
//! - Alert management with severity levels
//! - Performance tracking

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::time::{SystemTime, UNIX_EPOCH};

// =============================================================================
// ERRORS
// =============================================================================

/// Monitoring Service Errors
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum MonitoringError {
    #[serde(rename = "metric_not_found")]
    MetricNotFound(String),
    #[serde(rename = "alert_not_found")]
    AlertNotFound(String),
    #[serde(rename = "health_check_failed")]
    HealthCheckFailed(String),
    #[serde(rename = "aggregation_error")]
    AggregationError(String),
}

// =============================================================================
// METRICS
// =============================================================================

/// Metric with labels
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Metric {
    /// Metric name
    pub name: String,
    /// Metric value
    pub value: f64,
    /// Timestamp
    pub timestamp: u64,
    /// Labels for filtering
    pub labels: HashMap<String, String>,
}

impl Metric {
    /// Create new metric
    pub fn new(name: String, value: f64) -> Self {
        Self {
            name,
            value,
            timestamp: now_unix(),
            labels: HashMap::new(),
        }
    }

    /// Add label
    pub fn with_label(mut self, key: String, value: String) -> Self {
        self.labels.insert(key, value);
        self
    }
}

/// Counter metric
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Counter {
    pub name: String,
    pub value: f64,
    pub labels: HashMap<String, String>,
}

impl Counter {
    pub fn new(name: String) -> Self {
        Self {
            name,
            value: 0.0,
            labels: HashMap::new(),
        }
    }

    pub fn inc(&mut self, amount: f64) {
        self.value += amount;
    }

    pub fn get(&self) -> f64 {
        self.value
    }
}

/// Gauge metric
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Gauge {
    pub name: String,
    pub value: f64,
    pub labels: HashMap<String, String>,
}

impl Gauge {
    pub fn new(name: String) -> Self {
        Self {
            name,
            value: 0.0,
            labels: HashMap::new(),
        }
    }

    pub fn set(&mut self, value: f64) {
        self.value = value;
    }

    pub fn inc(&mut self) {
        self.value += 1.0;
    }

    pub fn dec(&mut self) {
        self.value -= 1.0;
    }

    pub fn get(&self) -> f64 {
        self.value
    }
}

/// Histogram metric
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Histogram {
    pub name: String,
    pub buckets: HashMap<String, f64>,
    pub sum: f64,
    pub count: u64,
}

impl Histogram {
    pub fn new(name: String) -> Self {
        let mut buckets = HashMap::new();
        buckets.insert("0.005".to_string(), 0.0);
        buckets.insert("0.01".to_string(), 0.0);
        buckets.insert("0.025".to_string(), 0.0);
        buckets.insert("0.05".to_string(), 0.0);
        buckets.insert("0.1".to_string(), 0.0);
        buckets.insert("0.25".to_string(), 0.0);
        buckets.insert("0.5".to_string(), 0.0);
        buckets.insert("1.0".to_string(), 0.0);
        buckets.insert("2.5".to_string(), 0.0);
        buckets.insert("5.0".to_string(), 0.0);
        buckets.insert("10.0".to_string(), 0.0);
        
        Self {
            name,
            buckets,
            sum: 0.0,
            count: 0,
        }
    }

    pub fn observe(&mut self, value: f64) {
        self.sum += value;
        self.count += 1;
        
        for (bucket, count) in self.buckets.iter_mut() {
            if value <= bucket.parse::<f64>().unwrap_or(f64::MAX) {
                *count += 1.0;
            }
        }
    }
}

// =============================================================================
// ALERTS
// =============================================================================

/// Alert severity levels
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum AlertSeverity {
    #[serde(rename = "info")]
    Info,
    #[serde(rename = "warning")]
    Warning,
    #[serde(rename = "error")]
    Error,
    #[serde(rename = "critical")]
    Critical,
}

impl AlertSeverity {
    pub fn as_str(&self) -> &str {
        match self {
            AlertSeverity::Info => "info",
            AlertSeverity::Warning => "warning",
            AlertSeverity::Error => "error",
            AlertSeverity::Critical => "critical",
        }
    }
}

/// Alert
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Alert {
    /// Unique alert ID
    pub id: String,
    /// Alert name
    pub name: String,
    /// Severity level
    pub severity: AlertSeverity,
    /// Alert message
    pub message: String,
    /// Timestamp
    pub timestamp: u64,
    /// Whether resolved
    pub resolved: bool,
    /// Resolution timestamp
    pub resolved_at: Option<u64>,
    /// Labels
    pub labels: HashMap<String, String>,
}

impl Alert {
    /// Create new alert
    pub fn new(id: String, name: String, severity: AlertSeverity, message: String) -> Self {
        Self {
            id,
            name,
            severity,
            message,
            timestamp: now_unix(),
            resolved: false,
            resolved_at: None,
            labels: HashMap::new(),
        }
    }

    /// Resolve alert
    pub fn resolve(&mut self) {
        self.resolved = true;
        self.resolved_at = Some(now_unix());
    }
}

// =============================================================================
// HEALTH CHECK
// =============================================================================

/// Health check status
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum HealthStatus {
    #[serde(rename = "healthy")]
    Healthy,
    #[serde(rename = "degraded")]
    Degraded,
    #[serde(rename = "unhealthy")]
    Unhealthy,
}

/// Health check result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthCheck {
    /// Service name
    pub service: String,
    /// Health status
    pub status: HealthStatus,
    /// Latency in milliseconds
    pub latency_ms: u64,
    /// Last check timestamp
    pub last_check: u64,
    /// Optional error message
    pub error: Option<String>,
    /// Optional metadata
    pub metadata: HashMap<String, String>,
}

impl HealthCheck {
    /// Create new health check
    pub fn new(service: String) -> Self {
        Self {
            service,
            status: HealthStatus::Healthy,
            latency_ms: 0,
            last_check: now_unix(),
            error: None,
            metadata: HashMap::new(),
        }
    }

    /// Mark as healthy
    pub fn healthy(mut self, latency_ms: u64) -> Self {
        self.status = HealthStatus::Healthy;
        self.latency_ms = latency_ms;
        self.last_check = now_unix();
        self
    }

    /// Mark as degraded
    pub fn degraded(mut self, latency_ms: u64, error: String) -> Self {
        self.status = HealthStatus::Degraded;
        self.latency_ms = latency_ms;
        self.last_check = now_unix();
        self.error = Some(error);
        self
    }

    /// Mark as unhealthy
    pub fn unhealthy(mut self, error: String) -> Self {
        self.status = HealthStatus::Unhealthy;
        self.last_check = now_unix();
        self.error = Some(error);
        self
    }
}

// =============================================================================
// MONITORING SERVICE
// =============================================================================

/// Complete monitoring service
pub struct Service {
    /// Metrics by name
    metrics: HashMap<String, Vec<Metric>>,
    /// Counters
    counters: HashMap<String, Counter>,
    /// Gauges
    gauges: HashMap<String, Gauge>,
    /// Histograms
    histograms: HashMap<String, Histogram>,
    /// Alerts
    alerts: HashMap<String, Alert>,
    /// Health checks
    health_checks: HashMap<String, HealthCheck>,
}

impl Service {
    /// Create new monitoring service
    pub fn new() -> Self {
        Self {
            metrics: HashMap::new(),
            counters: HashMap::new(),
            gauges: HashMap::new(),
            histograms: HashMap::new(),
            alerts: HashMap::new(),
            health_checks: HashMap::new(),
        }
    }

    /// Record metric
    pub fn record_metric(&mut self, name: String, value: f64) {
        let metric = Metric::new(name.clone(), value);
        self.metrics.entry(name).or_insert_with(Vec::new).push(metric);
    }

    /// Record metric with labels
    pub fn record_metric_with_labels(&mut self, name: String, value: f64, labels: HashMap<String, String>) {
        let mut metric = Metric::new(name.clone(), value);
        metric.labels = labels;
        self.metrics.entry(name).or_insert_with(Vec::new).push(metric);
    }

    /// Get counter
    pub fn get_counter(&mut self, name: &str) -> &mut Counter {
        self.counters.entry(name.to_string()).or_insert_with(|| Counter::new(name.to_string()))
    }

    /// Get gauge
    pub fn get_gauge(&mut self, name: &str) -> &mut Gauge {
        self.gauges.entry(name.to_string()).or_insert_with(|| Gauge::new(name.to_string()))
    }

    /// Get histogram
    pub fn get_histogram(&mut self, name: &str) -> &mut Histogram {
        self.histograms.entry(name.to_string()).or_insert_with(|| Histogram::new(name.to_string()))
    }

    /// Create alert
    pub fn create_alert(&mut self, alert: Alert) {
        self.alerts.insert(alert.id.clone(), alert);
    }

    /// Get alert
    pub fn get_alert(&self, id: &str) -> Option<&Alert> {
        self.alerts.get(id)
    }

    /// Resolve alert
    pub fn resolve_alert(&mut self, id: &str) -> Result<(), MonitoringError> {
        let alert = self.alerts.get_mut(id)
            .ok_or_else(|| MonitoringError::AlertNotFound(id.to_string()))?;
        alert.resolve();
        Ok(())
    }

    /// Get active alerts
    pub fn active_alerts(&self) -> Vec<&Alert> {
        self.alerts.values().filter(|a| !a.resolved).collect()
    }

    /// Record health check
    pub fn record_health_check(&mut self, check: HealthCheck) {
        self.health_checks.insert(check.service.clone(), check);
    }

    /// Get health status
    pub fn health_status(&self) -> HealthStatus {
        let mut worst = HealthStatus::Healthy;
        
        for check in self.health_checks.values() {
            match check.status {
                HealthStatus::Unhealthy => return HealthStatus::Unhealthy,
                HealthStatus::Degraded => worst = HealthStatus::Degraded,
                _ => {}
            }
        }
        
        worst
    }

    /// Get metrics for Prometheus export
    pub fn prometheus_metrics(&self) -> String {
        let mut output = String::new();
        
        // Counters
        for (name, counter) in &self.counters {
            output.push_str(&format!("{} {}\n", name, counter.value));
        }
        
        // Gauges
        for (name, gauge) in &self.gauges {
            output.push_str(&format!("{} {}\n", name, gauge.value));
        }
        
        // Histograms
        for (name, hist) in &self.histograms {
            output.push_str(&format!("{}_sum {}\n", name, hist.sum));
            output.push_str(&format!("{}_count {}\n", name, hist.count));
            for (bucket, count) in &hist.buckets {
                output.push_str(&format!("{}_bucket{{le=\"{}\"}} {}\n", name, bucket, count));
            }
        }
        
        output
    }

    /// Get all metrics
    pub fn all_metrics(&self, name: &str) -> Option<&Vec<Metric>> {
        self.metrics.get(name)
    }

    /// Get average metric value
    pub fn average_metric(&self, name: &str) -> Option<f64> {
        let metrics = self.metrics.get(name)?;
        if metrics.is_empty() {
            return Some(0.0);
        }
        
        let sum: f64 = metrics.iter().map(|m| m.value).sum();
        Some(sum / metrics.len() as f64)
    }

    /// Get latest metric value
    pub fn latest_metric(&self, name: &str) -> Option<f64> {
        self.metrics.get(name).and_then(|v| v.last().map(|m| m.value))
    }

    /// Get metrics in time range
    pub fn metrics_in_range(&self, name: &str, from: u64, to: u64) -> Vec<&Metric> {
        self.metrics.get(name)
            .map(|v| v.iter().filter(|m| m.timestamp >= from && m.timestamp <= to).collect())
            .unwrap_or_default()
    }

    /// Clear old metrics
    pub fn clear_old_metrics(&mut self, before: u64) {
        for metrics in self.metrics.values_mut() {
            metrics.retain(|m| m.timestamp >= before);
        }
    }
}

impl Default for Service {
    fn default() -> Self {
        Self::new()
    }
}

// =============================================================================
// HELPER FUNCTIONS
// =============================================================================

/// Get current Unix timestamp
fn now_unix() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs()
}