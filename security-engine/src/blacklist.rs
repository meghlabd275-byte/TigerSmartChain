//! Blacklist Module
//! 
//! Manages blacklisted addresses and contracts.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Blacklist entry
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BlacklistMatch {
    pub address: String,
    pub category: BlacklistCategory,
    pub reason: String,
    pub reported_at: i64,
    pub source: String,
    pub risk_score: f64,
}

/// Blacklist categories
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BlacklistCategory {
    Scam,
    Phishing,
    Hack,
    Mixer,
    Exploit,
    Ransomware,
    Fake,
    Malicious,
    Unknown,
}

/// Blacklist manager
pub struct Blacklist {
    entries: HashMap<String, BlacklistEntry>,
    by_category: HashMap<BlacklistCategory, Vec<String>>,
}

#[derive(Debug, Clone)]
struct BlacklistEntry {
    address: String,
    category: BlacklistCategory,
    reason: String,
    reported_at: i64,
    source: String,
    risk_score: f64,
}

impl Blacklist {
    pub fn new() -> Self {
        Self {
            entries: HashMap::new(),
            by_category: HashMap::new(),
        }
    }

    /// Check if address is blacklisted
    pub fn check(&self, address: &str) -> Option<BlacklistMatch> {
        let addr = address.to_lowercase();
        if let Some(entry) = self.entries.get(&addr) {
            Some(BlacklistMatch {
                address: entry.address.clone(),
                category: entry.category,
                reason: entry.reason.clone(),
                reported_at: entry.reported_at,
                source: entry.source.clone(),
                risk_score: entry.risk_score,
            })
        } else {
            None
        }
    }

    /// Add address to blacklist
    pub fn add(&mut self, address: String, category: BlacklistCategory, reason: String, risk_score: f64) {
        let addr = address.to_lowercase();
        let reported_at = chrono::Utc::now().timestamp();
        
        let entry = BlacklistEntry {
            address: addr.clone(),
            category,
            reason,
            reported_at,
            source: "manual".to_string(),
            risk_score,
        };
        
        self.entries.insert(addr.clone(), entry);
        self.by_category
            .entry(category)
            .or_insert_with(Vec::new)
            .push(addr);
    }

    /// Get all addresses in category
    pub fn by_category(&self, category: BlacklistCategory) -> Vec<String> {
        self.by_category
            .get(&category)
            .cloned()
            .unwrap_or_default()
    }

    /// Get total count
    pub fn count(&self) -> usize {
        self.entries.len()
    }
}

impl Default for Blacklist {
    fn default() -> Self {
        Self::new()
    }
}