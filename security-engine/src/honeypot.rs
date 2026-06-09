//! Honeypot Detector Module
//! 
//! Detects honeypot contracts that allow buying but prevent selling.

use serde::{Deserialize, Serialize};

/// Honeypot detection result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HoneypotMatch {
    pub contract: String,
    pub honeypot_type: HoneypotType,
    pub confidence: f64,
    pub evidence: Vec<String>,
    pub sell_tax: Option<f64>,
    pub transfer_tax: Option<f64>,
}

/// Types of honeypots
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HoneypotType {
    CannotSell,
    HighSellTax,
    BlockedTransfer,
    TimeLock,
    OnlyOwner,
    Fake,
    Unknown,
}

/// Honeypot detector
pub struct HoneypotDetector {
    known_honeypots: std::collections::HashSet<String>,
}

impl HoneypotDetector {
    pub fn new() -> Self {
        Self {
            known_honeypots: std::collections::HashSet::new(),
        }
    }

    /// Check if contract is a known honeypot
    pub fn check_contract(&self, address: &str) -> Option<HoneypotMatch> {
        let addr = address.to_lowercase();
        if self.known_honeypots.contains(&addr) {
            Some(HoneypotMatch {
                contract: address.to_string(),
                honeypot_type: HoneypotType::Unknown,
                confidence: 1.0,
                evidence: vec!["Known honeypot contract".to_string()],
                sell_tax: None,
                transfer_tax: None,
            })
        } else {
            None
        }
    }

    /// Analyze contract bytecode for honeypot indicators
    pub fn analyze_bytecode(&self, bytecode: &str) -> Option<HoneypotMatch> {
        let bc = bytecode.to_lowercase();
        
        // Check for sell restrictions
        if bc.contains("sell") && !bc.contains("transfer") {
            return Some(HoneypotMatch {
                contract: "unknown".to_string(),
                honeypot_type: HoneypotType::CannotSell,
                confidence: 0.6,
                evidence: vec!["Sell function missing or restricted".to_string()],
                sell_tax: None,
                transfer_tax: None,
            });
        }

        None
    }
}

impl Default for HoneypotDetector {
    fn default() -> Self {
        Self::new()
    }
}