//! Security Bounty Service for TigerScan

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

// =============================================================================
// TYPES
// =============================================================================

/// Bug Bounty
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BugBounty {
    pub id: String,
    pub severity: Severity,
    pub status: BountyStatus,
    pub reward: u64,
    pub description: String,
    pub reporter: String,
    pub timestamp: i64,
}

/// Severity
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Severity {
    Low,
    Medium,
    High,
    Critical,
}

/// Bounty Status
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum BountyStatus {
    Open,
    InProgress,
    Resolved,
    Paid,
}

/// Bounty Service
pub struct BountyService {
    bounties: HashMap<String, BugBounty>,
    total_paid: u64,
}

impl BountyService {
    pub fn new() -> Self {
        Self {
            bounties: HashMap::new(),
            total_paid: 0,
        }
    }

    /// Submit bounty
    pub fn submit(&mut self, bounty: BugBounty) -> String {
        let id = bounty.id.clone();
        self.bounties.insert(id.clone(), bounty);
        id
    }

    /// Get bounty
    pub fn get(&self, id: &str) -> Option<&BugBounty> {
        self.bounties.get(id)
    }

    /// List bounties
    pub fn list(&self) -> Vec<&BugBounty> {
        self.bounties.values().collect()
    }

    /// Get total paid
    pub fn total_paid(&self) -> u64 {
        self.total_paid
    }
}

impl Default for BountyService {
    fn default() -> Self {
        Self::new()
    }
}

// =============================================================================
// AML
// =============================================================================

/// AML Check
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AMLCheck {
    pub address: String,
    pub risk_score: f64,
    pub flagged: bool,
    pub reasons: Vec<String>,
}

/// AML Service
pub struct AMLService {
    high_risk_addresses: std::collections::HashSet<String>,
}

impl AMLService {
    pub fn new() -> Self {
        Self {
            high_risk_addresses: std::collections::HashSet::new(),
        }
    }

    /// Check address
    pub fn check(&self, address: &str) -> AMLCheck {
        let flagged = self.high_risk_addresses.contains(address);
        AMLCheck {
            address: address.to_string(),
            risk_score: if flagged { 80.0 } else { 0.0 },
            flagged,
            reasons: if flagged { vec!["High risk".to_string()] } else { vec![] },
        }
    }

    /// Add high risk address
    pub fn add_high_risk(&mut self, address: &str) {
        self.high_risk_addresses.insert(address.to_string());
    }
}

impl Default for AMLService {
    fn default() -> Self {
        Self::new()
    }
}