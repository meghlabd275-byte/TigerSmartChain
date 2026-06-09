//! Phishing Detector Module
//! 
//! Detects phishing domains and websites that impersonate TigerSmartChain or related services.

use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use lazy_static::lazy_static;
use regex::Regex;

/// Phishing detection result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PhishingMatch {
    pub domain: String,
    pub target: String,
    pub similarity: f64,
    pub detection_method: String,
    pub confidence: f64,
    pub reported: bool,
    pub source: String,
}

/// Phishing detector
pub struct PhishingDetector {
    known_phishing: HashSet<String>,
    target_patterns: Vec<TargetPattern>,
    api_key: Option<String>,
    client: reqwest::Client,
}

/// Target pattern for detection
#[derive(Debug, Clone)]
struct TargetPattern {
    name: String,
    domain: String,
    keywords: Vec<String>,
}

lazy_static! {
    static ref PHISHING_DOMAINS: HashSet<String> = {
        let mut set = HashSet::new();
        // Common phishing domains (would be fetched from API in production)
        set.insert("tigersmartchaiin.com".to_string());
        set.insert("tigersmartchian.com".to_string());
        set.insert("tigersmartchain.io".to_string());
        set.insert("tigersmart-chain.com".to_string());
        set.insert("tigersmartcahin.com".to_string());
        set.insert("tigersmartchain-defi.com".to_string());
        set.insert("tigersmart-nft.com".to_string());
        set.insert("tigersmartdapp.com".to_string());
        set
    };

    static ref PHISHING_REGEX: Regex = Regex::new(
        r"(?i)(tiger|bsc|binance)(smart)?(chain|scan|dapp|defi|nft|wallet|swap|bridge|farm|pool)([0-9a-z\-]{0,20})?\.(com|io|net|org|info|xyz|cc|top|site|online|app)"
    ).unwrap();

    static ref TYPOSQUATTING_REGEX: Regex = Regex::new(
        r"(?i)(tiger|bsc|binance).*(1|l|i|0|o|a|e)"
    ).unwrap();
}

impl PhishingDetector {
    pub fn new() -> Self {
        Self {
            known_phishing: PHISHING_DOMAINS.clone(),
            target_patterns: Self::default_targets(),
            api_key: None,
            client: reqwest::Client::new(),
        }
    }

    fn default_targets() -> Vec<TargetPattern> {
        vec![
            TargetPattern {
                name: "TigerSmartChain".to_string(),
                domain: "tigersmartchain.com".to_string(),
                keywords: vec!["tiger".to_string(), "tigersmartchain".to_string()],
            },
            TargetPattern {
                name: "TigerScan".to_string(),
                domain: "scan.tigersmartchain.com".to_string(),
                keywords: vec!["scan".to_string(), "explorer".to_string()],
            },
            TargetPattern {
                name: "BNB Chain".to_string(),
                domain: "bnbchain.org".to_string(),
                keywords: vec!["bnb".to_string(), "binance".to_string()],
            },
        ]
    }

    pub fn with_api_key(mut self, api_key: String) -> Self {
        self.api_key = Some(api_key);
        self
    }

    /// Check if a domain is a known phishing site
    pub fn check_domain(&self, domain: &str) -> Option<PhishingMatch> {
        let domain_lower = domain.to_lowercase();
        
        // Direct match
        if self.known_phishing.contains(&domain_lower) {
            return Some(PhishingMatch {
                domain: domain.to_string(),
                target: "TigerSmartChain".to_string(),
                similarity: 1.0,
                detection_method: "known_phishing".to_string(),
                confidence: 1.0,
                reported: true,
                source: "local_blacklist".to_string(),
            });
        }

        // Regex pattern match
        if PHISHING_REGEX.is_match(&domain_lower) {
            // Find the target being impersonated
            let target = self.find_target(&domain_lower);
            return Some(PhishingMatch {
                domain: domain.to_string(),
                target,
                similarity: 0.9,
                detection_method: "pattern_match".to_string(),
                confidence: 0.9,
                reported: false,
                source: "regex_detection".to_string(),
            });
        }

        // Typo-squatting detection
        if self.check_typosquatting(&domain_lower) {
            return Some(PhishingMatch {
                domain: domain.to_string(),
                target: "TigerSmartChain".to_string(),
                similarity: 0.7,
                detection_method: "typosquatting".to_string(),
                confidence: 0.7,
                reported: false,
                source: "typosquatting".to_string(),
            });
        }

        None
    }

    /// Check for typo-squatting
    fn check_typosquatting(&self, domain: &str) -> bool {
        // Simple check: contains numbers or common typos
        if domain.contains("1") || domain.contains("l") || domain.contains("0") || domain.contains("o") {
            for target in &self.target_patterns {
                let target_name = target.name.to_lowercase();
                let target_domain = target.domain.to_lowercase();
                
                // Levenshtein distance check would be better
                if domain.contains(&target_name.replace("tiger", "tiger")) {
                    if domain.len() > target_domain.len() + 3 {
                        return true;
                    }
                }
            }
        }
        false
    }

    /// Find the target being impersonated
    fn find_target(&self, domain: &str) -> String {
        for target in &self.target_patterns {
            if domain.contains(&target.name.to_lowercase()) {
                return target.name.clone();
            }
        }
        "Unknown".to_string()
    }

    /// Analyze a URL for phishing indicators
    pub fn analyze_url(&self, url: &str) -> Option<PhishingMatch> {
        // Extract domain from URL
        if let Ok(parsed) = url::Url::parse(url) {
            if let Some(host) = parsed.host_str() {
                return self.check_domain(host);
            }
        }
        
        // Try as direct domain
        self.check_domain(url)
    }

    /// Fetch external phishing lists (requires API key)
    pub async fn fetch_phishing_list(&self) -> Result<Vec<String>, reqwest::Error> {
        // This would fetch from external APIs in production
        // Example: PhishTank, OpenPhish, etc.
        Ok(vec![])
    }
}

impl Default for PhishingDetector {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_known_phishing() {
        let detector = PhishingDetector::new();
        
        assert!(detector.check_domain("tigersmartchaiin.com").is_some());
        assert!(detector.check_domain("tigersmartchian.com").is_some());
    }

    #[test]
    fn test_safe_domain() {
        let detector = PhishingDetector::new();
        
        assert!(detector.check_domain("google.com").is_none());
        assert!(detector.check_domain("github.com").is_none());
    }

    #[test]
    fn test_url_analysis() {
        let detector = PhishingDetector::new();
        
        let result = detector.analyze_url("https://tigersmartchaiin.com/login");
        assert!(result.is_some());
    }
}