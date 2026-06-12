//! Analytics Service Types

use serde::{Deserialize, Serialize};

// =============================================================================
// ANALYTICS SERVICE
// =============================================================================

/// Chart Data
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChartData {
    pub labels: Vec<String>,
    pub values: Vec<f64>,
    pub timestamps: Vec<u64>,
}

/// Metric
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Metric {
    pub name: String,
    pub value: f64,
    pub change_24h: f64,
    pub trend: String,
}

/// Dashboard
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Dashboard {
    pub id: String,
    pub name: String,
    pub metrics: Vec<Metric>,
    pub charts: Vec<ChartData>,
}

/// Analytics Service
pub struct Service {
    dashboards: std::collections::HashMap<String, Dashboard>,
}

impl Service {
    pub fn new() -> Self {
        Self {
            dashboards: std::collections::HashMap::new(),
        }
    }

    /// Add dashboard
    pub fn add_dashboard(&mut self, dashboard: Dashboard) {
        self.dashboards.insert(dashboard.id.clone(), dashboard);
    }

    /// Get dashboard
    pub fn get_dashboard(&self, id: &str) -> Option<&Dashboard> {
        self.dashboards.get(id)
    }
}

impl Default for Service {
    fn default() -> Self {
        Self::new()
    }
}