//! AML Service Types

use serde::{Deserialize, Serialize};

// =============================================================================
// AML SERVICE
// =============================================================================

/// Risk Score
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RiskScore {
    pub address: String,
    pub score: u8,
    pub risk_factors: Vec<String>,
}

/// Transaction Flag
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TransactionFlag {
    pub tx_hash: String,
    pub flag: String,
    pub reason: String,
}

/// AML Report
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AmlReport {
    pub address: String,
    pub risk_score: u8,
    pub flags: Vec<String>,
    pub last_updated: u64,
}

/// AML Service
pub struct Service {
    reports: std::collections::HashMap<String, AmlReport>,
}

impl Service {
    pub fn new() -> Self {
        Self {
            reports: std::collections::HashMap::new(),
        }
    }

    /// Add report
    pub fn add_report(&mut self, address: String, report: AmlReport) {
        self.reports.insert(address, report);
    }

    /// Get report
    pub fn get_report(&self, address: &str) -> Option<&AmlReport> {
        self.reports.get(address)
    }
}

impl Default for Service {
    fn default() -> Self {
        Self::new()
    }
}