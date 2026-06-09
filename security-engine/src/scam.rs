//! Scam Detector Module
//! 
//! Detects scam tokens and malicious contracts.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Scam detection result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScamMatch {
    pub contract: String,
    pub scam_type: ScamType,
    pub confidence: f64,
    pub evidence: Vec<String>,
    pub reported_at: Option<i64>,
    pub source: String,
}

/// Types of scams
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ScamType {
    RugPull,
    Honeypot,
    FakeToken,
    FlashLoan,
    Ponzi,
    Phishing,
    FakeExchange,
    FakeStaking,
    ICO,
    Migrator,
    Unknown,
}

/// Scam detector
pub struct ScamDetector {
    known_scams: HashMap<String, ScamType>,
    patterns: Vec<ScamPattern>,
    client: reqwest::Client,
}

/// Pattern for scam detection
#[derive(Debug, Clone)]
struct ScamPattern {
    scam_type: ScamType,
    pattern: String,
    description: String,
}

impl ScamDetector {
    pub fn new() -> Self {
        Self {
            known_scams: Self::default_scams(),
            patterns: Self::default_patterns(),
            client: reqwest::Client::new(),
        }
    }

    fn default_scams() -> HashMap<String, ScamType> {
        let mut map = HashMap::new();
        // Add known scam contracts (would be fetched from API in production)
        // These are example addresses
        map.insert("0x000000000000000000000000000000000000dEaD".to_string(), ScamType::RugPull);
        map
    }

    fn default_patterns() -> Vec<ScamPattern> {
        vec![
            ScamPattern {
                scam_type: ScamType::RugPull,
                pattern: "mint".to_string(),
                description: "Unlimited mint capability".to_string(),
            },
            ScamPattern {
                scam_type: ScamType::RugPull,
                pattern: "setTaxPercent".to_string(),
                description: "Adjustable tax that can be increased".to_string(),
            },
            ScamPattern {
                scam_type: ScamType::FakeToken,
                pattern: "fake".to_string(),
                description: "Impersonating known token".to_string(),
            },
            ScamPattern {
                scam_type: ScamType::Honeypot,
                pattern: "buy".to_string(),
                description: "Can buy but cannot sell".to_string(),
            },
        ]
    }

    /// Check if a contract is a known scam
    pub fn check_contract(&self, address: &str) -> Option<ScamMatch> {
        let addr = address.to_lowercase();
        
        if let Some(&scam_type) = self.known_scams.get(&addr) {
            return Some(ScamMatch {
                contract: address.to_string(),
                scam_type,
                confidence: 1.0,
                evidence: vec!["Known scam contract".to_string()],
                reported_at: None,
                source: "local_blacklist".to_string(),
            });
        }

        None
    }

    /// Analyze contract code for scam indicators
    pub fn analyze_code(&self, bytecode: &str) -> Vec<ScamMatch> {
        let mut matches = Vec::new();
        let bytecode_lower = bytecode.to_lowercase();
        
        for pattern in &self.patterns {
            if bytecode_lower.contains(&pattern.pattern.to_lowercase()) {
                matches.push(ScamMatch {
                    contract: "unknown".to_string(),
                    scam_type: pattern.scam_type,
                    confidence: 0.5,
                    evidence: vec![pattern.description.clone()],
                    reported_at: None,
                    source: "pattern_match".to_string(),
                });
            }
        }

        matches
    }

    /// Analyze token metadata for scam indicators
    pub fn analyze_token(&self, name: &str, symbol: &str) -> Option<ScamMatch> {
        let name_lower = name.to_lowercase();
        let symbol_lower = symbol.to_lowercase();
        
        // Check for impersonation
        let impersonation_patterns = ["bnb", "btc", "eth", "usdt", "usdc", "busd"];
        for imp in &impersonation_patterns {
            if (name_lower.contains(imp) || symbol_lower.contains(imp)) 
                && !name_lower.contains("fake") 
                && !name_lower.contains("test") {
                return Some(ScamMatch {
                    contract: "unknown".to_string(),
                    scam_type: ScamType::FakeToken,
                    confidence: 0.7,
                    evidence: vec![format!("Impersonating {}", imp)],
                    reported_at: None,
                    source: "impersonation_detection".to_string(),
                });
            }
        }

        None
    }

    /// Get all scam types
    pub fn get_scam_types() -> Vec<&'static str> {
        vec![
            "rug_pull",
            "honeypot",
            "fake_token",
            "flash_loan",
            "ponzi",
            "phishing",
            "fake_exchange",
            "fake_staking",
            "ico_scam",
            "migrator_scam",
        ]
    }
}

impl Default for ScamDetector {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_known_scam() {
        let detector = ScamDetector::new();
        
        let result = detector.check_contract("0x000000000000000000000000000000000000dEaD");
        assert!(result.is_some());
    }

    #[test]
    fn test_token_impersonation() {
        let detector = ScamDetector::new();
        
        let result = detector.analyze_token("BNB Token", "BNB");
        assert!(result.is_some());
    }
}