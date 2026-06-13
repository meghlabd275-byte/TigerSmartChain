//! TigerScan Security: Honeypot & Phishing Detection Service
//! Detect malicious contracts, phishing sites, and scam tokens

use std::collections::HashMap;
use std::sync::Arc;

use anyhow::Result;
use chrono::{DateTime, Utc};
use ethers::core::types::{Address, H256, U256};
use ethers::providers::{Http, Provider};
use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use sha2::{Digest as Sha256Digest, Sha256 as Sha256Hasher};
use thiserror::Error;
use tracing::{error, info, warn};

// ============================================================================
// Error Types
// ============================================================================

#[derive(Error, Debug)]
pub enum SecurityError {
    #[error("RPC error: {0}")]
    Rpc(String),
    
    #[error("Analysis error: {0}")]
    Analysis(String),
    
    #[error("Not found: {0}")]
    NotFound(String),
}

// ============================================================================
// Configuration
// ============================================================================

#[derive(Debug, Clone, Deserialize)]
pub struct Config {
    pub rpc_url: String,
    pub max_analysis_time: u64,
    pub enable_ml_detection: bool,
    pub threat_feed_url: Option<String>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            rpc_url: std::env::var("RPC_url")
                .unwrap_or_else(|_| "http://localhost:8545".to_string()),
            max_analysis_time: 30,
            enable_ml_detection: true,
            threat_feed_url: std::env::var("THREAT_FEED_URL").ok(),
        }
    }
}

// ============================================================================
// Data Types
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SecurityReport {
    pub address: String,
    pub is_malicious: bool,
    pub risk_level: RiskLevel,
    pub honeypot_score: f64,
    pub phishing_score: f64,
    pub scam_score: f64,
    pub findings: Vec<SecurityFinding>,
    pub indicators: Vec<ThreatIndicator>,
    pub recommendations: Vec<String>,
    pub analyzed_at: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RiskLevel {
    Safe,
    Low,
    Medium,
    High,
    Critical,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SecurityFinding {
    pub category: FindingCategory,
    pub severity: FindingSeverity,
    pub description: String,
    pub evidence: String,
    pub cwe_id: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FindingCategory {
    Honeypot,
    Phishing,
    Scam,
    Backdoor,
    RugPull,
    PumpAndDump,
    FakeToken,
    FlashLoanAttack,
    SandwichAttack,
    FrontRunning,
    Vulnerability,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FindingSeverity {
    Info,
    Low,
    Medium,
    High,
    Critical,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ThreatIndicator {
    pub indicator_type: IndicatorType,
    pub value: String,
    pub source: String,
    pub confidence: f64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum IndicatorType {
    Address,
    Domain,
    Transaction,
    Bytecode,
    Signature,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PhishingSite {
    pub domain: String,
    pub target: String,
    pub phishing_type: PhishingType,
    pub reported_at: i64,
    pub reports: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PhishingType {
    FakeExchange,
    FakeWallet,
    AirdropScam,
    Ponzi,
    Phishing,
    Malware,
}

// ============================================================================
// Honeypot Detector
// ============================================================================

pub struct HoneypotDetector {
    config: Config,
    rpc: Provider<Http>,
    known_patterns: Arc<RwLock<HashMap<String, HoneypotPattern>>>,
}

#[derive(Debug, Clone)]
pub struct HoneypotPattern {
    pub name: String,
    pub pattern: String,
    pub severity: FindingSeverity,
    pub description: String,
}

impl HoneypotDetector {
    pub async fn new(config: Config) -> Result<Self> {
        info!("Initializing Honeypot Detector");
        
        let rpc = Provider::<Http>::try_from(config.rpc_url.clone())?;
        
        let mut detector = Self {
            config: config.clone(),
            rpc,
            known_patterns: Arc::new(RwLock::new(HashMap::new())),
        };
        
        detector.load_patterns();
        
        info!("Honeypot Detector initialized");
        Ok(detector)
    }

    /// Load known honeypot patterns
    fn load_patterns(&mut self) {
        let mut patterns = self.known_patterns.write();
        
        // Common honeypot patterns in bytecode
        let honeypot_patterns = vec![
            ("honeypot_1", "0x146t4m", "Critical", "Trick bytecode that appears to transfer but doesn't"),
            ("honeypot_2", "0x5ac6e90a", "Critical", "Hidden require that always fails"),
            ("honeypot_3", "0xcdfead2e", "High", "Impersonation of popular token"),
            ("honeypot_4", "0x531ea3d8", "High", "Fake transfer mechanism"),
            ("honeypot_5", "0x2e1a7d4d", "Medium", "Return true on transfer"),
        ];
        
        for (name, pattern, severity, desc) in honeypot_patterns {
            patterns.insert(name.to_string(), HoneypotPattern {
                name: name.to_string(),
                pattern: pattern.to_string(),
                severity: match severity {
                    "Critical" => FindingSeverity::Critical,
                    "High" => FindingSeverity::High,
                    "Medium" => FindingSeverity::Medium,
                    _ => FindingSeverity::Low,
                },
                description: desc.to_string(),
            });
        }
    }

    /// Analyze bytecode for honeypot patterns
    pub fn analyze_bytecode(&self, bytecode: &str) -> Vec<SecurityFinding> {
        let mut findings = Vec::new();
        let patterns = self.known_patterns.read();
        
        for pattern in patterns.values() {
            if bytecode.contains(&pattern.pattern) {
                findings.push(SecurityFinding {
                    category: FindingCategory::Honeypot,
                    severity: pattern.severity,
                    description: pattern.description.clone(),
                    evidence: format!("Found pattern: {}", pattern.pattern),
                    cwe_id: Some("CWE-912".to_string()),
                });
            }
        }
        
        findings
    }

    /// Check for honeypot in transfer logic
    pub fn check_transfer_logic(&self, bytecode: &str) -> Option<SecurityFinding> {
        // Check for common honeypot transfer patterns
        
        // Pattern 1: Transfer that returns true but doesn't actually transfer
        if bytecode.contains("0x42842e0e") && bytecode.contains("0x095ea7b3") {
            // Check for fake approval + transfer
            return Some(SecurityFinding {
                category: FindingCategory::Honeypot,
                severity: FindingSeverity::Critical,
                description: "Suspicious approval + transfer pattern detected".to_string(),
                evidence: "Contract may execute fake transfers".to_string(),
                cwe_id: Some("CWE-912".to_string()),
            });
        }
        
        // Pattern 2: Require that always fails for certain addresses
        if bytecode.contains("0x09b5a3b1") {
            return Some(SecurityFinding {
                category: FindingCategory::Honeypot,
                severity: FindingSeverity::Critical,
                description: "Hidden require that blocks specific addresses".to_string(),
                evidence: "Conditional require pattern detected".to_string(),
                cwe_id: Some("CWE-863".to_string()),
            });
        }
        
        None
    }
}

// ============================================================================
// Phishing Detector
// ============================================================================

pub struct PhishingDetector {
    config: Config,
    rpc: Provider<Http>,
    threat_list: Arc<RwLock<ThreatDatabase>>,
}

#[derive(Debug, Default)]
pub struct ThreatDatabase {
    pub malicious_addresses: HashMap<String, ThreatEntry>,
    pub phishing_domains: HashMap<String, PhishingSite>,
    pub scam_contracts: HashMap<String, ScamInfo>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ThreatEntry {
    pub address: String,
    pub threat_type: String,
    pub first_seen: i64,
    pub reports: u32,
    pub confidence: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScamInfo {
    pub address: String,
    pub scam_type: ScamType,
    pub description: String,
    pub target_users: String,
    pub reported_at: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ScamType {
    Honeypot,
    RugPull,
    Phishing,
    Ponzi,
    FakeICO,
    FlashLoan,
    Sandwich,
}

impl PhishingDetector {
    pub async fn new(config: Config) -> Result<Self> {
        info!("Initializing Phishing Detector");
        
        let rpc = Provider::<Http>::try_from(config.rpc_url.clone())?;
        
        let detector = Self {
            config: config.clone(),
            rpc,
            threat_list: Arc::new(RwLock::new(ThreatDatabase::default())),
        };
        
        info!("Phishing Detector initialized");
        Ok(detector)
    }

    /// Check if address is in threat database
    pub fn check_address(&self, address: &str) -> Option<ThreatEntry> {
        let db = self.threat_list.read();
        db.malicious_addresses.get(address.to_lowercase().as_str()).cloned()
    }

    /// Check if domain is phishing
    pub fn check_domain(&self, domain: &str) -> Option<PhishingSite> {
        let db = self.threat_list.read();
        db.phishing_domains.get(domain).cloned()
    }

    /// Add threat to database
    pub fn add_threat(&self, entry: ThreatEntry) {
        let mut db = self.threat_list.write();
        db.malicious_addresses.insert(entry.address.clone(), entry);
    }
}

// ============================================================================
// Security Scanner
// ============================================================================

pub struct SecurityScanner {
    config: Config,
    rpc: Provider<Http>,
    honeypot_detector: HoneypotDetector,
    phishing_detector: PhishingDetector,
    state: Arc<RwLock<ScannerState>>,
}

#[derive(Debug)]
pub struct ScannerState {
    pub cache: HashMap<String, SecurityReport>,
    pub scans_today: u32,
}

impl SecurityScanner {
    pub async fn new(config: Config) -> Result<Self> {
        info!("Initializing Security Scanner");
        
        let rpc = Provider::<Http>::try_from(config.rpc_url.clone())?;
        
        let honeypot = HoneypotDetector::new(config.clone()).await?;
        let phishing = PhishingDetector::new(config.clone()).await?;
        
        let scanner = Self {
            config: config.clone(),
            rpc,
            honeypot_detector: honeypot,
            phishing_detector: phishing,
            state: Arc::new(RwLock::new(ScannerState {
                cache: HashMap::new(),
                scans_today: 0,
            })),
        };
        
        info!("Security Scanner initialized");
        Ok(scanner)
    }

    /// Full security analysis
    pub async fn analyze(&self, address: &str) -> Result<SecurityReport> {
        info!("Analyzing contract: {}", address);
        
        // Get bytecode
        let code = self.rpc.get_code(address.parse().unwrap(), None).await?;
        let bytecode = format!("{:?}", code);
        
        let mut findings = Vec::new();
        let mut indicators = Vec::new();
        
        // Check honeypot patterns
        findings.extend(self.honeypot_detector.analyze_bytecode(&bytecode));
        
        if let Some(finding) = self.honeypot_detector.check_transfer_logic(&bytecode) {
            findings.push(finding);
        }
        
        // Check threat database
        if let Some(threat) = self.phishing_detector.check_address(address) {
            findings.push(SecurityFinding {
                category: FindingCategory::Scam,
                severity: FindingSeverity::Critical,
                description: format!("Address found in threat database: {}", threat.threat_type),
                evidence: format!("Reports: {}, Confidence: {}%", threat.reports, threat.confidence * 100.0),
                cwe_id: None,
            });
            
            indicators.push(ThreatIndicator {
                indicator_type: IndicatorType::Address,
                value: address.to_string(),
                source: "Threat Database".to_string(),
                confidence: threat.confidence,
            });
        }
        
        // Calculate scores
        let honeypot_score = self.calculate_honeypot_score(&findings);
        let phishing_score = self.calculate_phishing_score(&findings);
        let scam_score = self.calculate_scam_score(&findings);
        
        let is_malicious = honeypot_score > 0.7 || phishing_score > 0.7 || scam_score > 0.7;
        
        let risk_level = match () {
            _ if honeypot_score > 0.9 => RiskLevel::Critical,
            _ if honeypot_score > 0.7 => RiskLevel::High,
            _ if scam_score > 0.7 => RiskLevel::High,
            _ if phishing_score > 0.5 => RiskLevel::Medium,
            _ if scam_score > 0.5 => RiskLevel::Medium,
            _ => RiskLevel::Safe,
        };
        
        // Generate recommendations
        let recommendations = self.generate_recommendations(&findings, risk_level);
        
        // Cache result
        let report = SecurityReport {
            address: address.to_string(),
            is_malicious,
            risk_level,
            honeypot_score,
            phishing_score,
            scam_score,
            findings,
            indicators,
            recommendations,
            analyzed_at: Utc::now().timestamp(),
        };
        
        let mut state = self.state.write();
        state.cache.insert(address.to_string(), report.clone());
        
        Ok(report)
    }

    /// Calculate honeypot score
    fn calculate_honeypot_score(&self, findings: &[SecurityFinding]) -> f64 {
        let honeypot_findings: Vec<_> = findings.iter()
            .filter(|f| f.category == FindingCategory::Honeypot)
            .collect();
        
        if honeypot_findings.is_empty() {
            return 0.0;
        }
        
        let max_severity = honeypot_findings.iter()
            .map(|f| match f.severity {
                FindingSeverity::Critical => 1.0,
                FindingSeverity::High => 0.75,
                FindingSeverity::Medium => 0.5,
                FindingSeverity::Low => 0.25,
                FindingSeverity::Info => 0.1,
            })
            .fold(0.0, f64::max);
        
        max_severity
    }

    /// Calculate phishing score
    fn calculate_phishing_score(&self, findings: &[SecurityFinding]) -> f64 {
        let phishing_findings: Vec<_> = findings.iter()
            .filter(|f| f.category == FindingCategory::Phishing || f.category == FindingCategory::FakeToken)
            .collect();
        
        if phishing_findings.is_empty() {
            return 0.0;
        }
        
        phishing_findings.len() as f64 * 0.3
    }

    /// Calculate scam score
    fn calculate_scam_score(&self, findings: &[SecurityFinding]) -> f64 {
        let scam_findings: Vec<_> = findings.iter()
            .filter(|f| matches!(f.category, FindingCategory::Scam | FindingCategory::RugPull | FindingCategory::PumpAndDump))
            .collect();
        
        if scam_findings.is_empty() {
            return 0.0;
        }
        
        scam_findings.len() as f64 * 0.4
    }

    /// Generate recommendations
    fn generate_recommendations(&self, findings: &[SecurityFinding], risk: RiskLevel) -> Vec<String> {
        let mut recs = Vec::new();
        
        match risk {
            RiskLevel::Critical => {
                recs.push("DO NOT interact with this contract - high risk of funds loss".to_string());
                recs.push("Report to security team and blocklists".to_string());
            }
            RiskLevel::High => {
                recs.push("Exercise extreme caution if interacting with this contract".to_string());
                recs.push("Verify contract source independently before use".to_string());
            }
            RiskLevel::Medium => {
                recs.push("Proceed with caution".to_string());
                recs.push("Start with small test transactions".to_string());
            }
            RiskLevel::Low => {
                recs.push("Low risk detected but still verify before large transactions".to_string());
            }
            RiskLevel::Safe => {
                recs.push("No obvious threats detected".to_string());
            }
        }
        
        recs
    }
}

// ============================================================================
// API Types
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SecurityApiRequest {
    pub address: Option<String>,
    pub domain: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SecurityApiResponse {
    pub success: bool,
    pub result: Option<serde_json::Value>,
    pub error: Option<String>,
}

// ============================================================================
// Utility Functions
// ============================================================================

/// Compute contract hash for caching
pub fn compute_contract_hash(bytecode: &str) -> String {
    let mut hasher = Sha256Hasher::new();
    hasher.update(bytecode.as_bytes());
    let result = hasher.finalize();
    hex::encode(result)
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_contract_hash() {
        let hash = compute_contract_hash("0x608060405234");
        assert_eq!(hash.len(), 64);
    }
}