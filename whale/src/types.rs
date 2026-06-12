//! Whale Types

use serde::{Deserialize, Serialize};

// =============================================================================
// WHALE
// =============================================================================

/// Whale
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Whale {
    pub address: String,
    pub balance: u64,
    pub tx_count: u64,
    pub first_seen: u64,
    pub last_active: u64,
    pub labels: Vec<String>,
}

/// Whale Alert
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WhaleAlert {
    pub id: String,
    pub whale: String,
    pub tx_hash: String,
    pub amount: u64,
    pub token: String,
    pub alert_type: String,
    pub timestamp: u64,
}

/// Whale Tracker
pub struct Tracker {
    whales: std::collections::HashMap<String, Whale>,
    alerts: Vec<WhaleAlert>,
}

impl Tracker {
    pub fn new() -> Self {
        Self {
            whales: std::collections::HashMap::new(),
            alerts: vec![],
        }
    }

    /// Add whale
    pub fn add_whale(&mut self, whale: Whale) {
        self.whales.insert(whale.address.clone(), whale);
    }

    /// Get whale
    pub fn get_whale(&self, address: &str) -> Option<&Whale> {
        self.whales.get(address)
    }

    /// Add alert
    pub fn add_alert(&mut self, alert: WhaleAlert) {
        self.alerts.push(alert);
    }

    /// Get alerts
    pub fn get_alerts(&self, address: &str) -> Vec<&WhaleAlert> {
        self.alerts
            .iter()
            .filter(|a| a.whale == address)
            .collect()
    }
}

impl Default for Tracker {
    fn default() -> Self {
        Self::new()
    }
}