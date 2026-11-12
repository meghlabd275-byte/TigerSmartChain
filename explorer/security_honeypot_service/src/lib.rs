//! TigerScan Security: Honeypot & Phishing Detection Service
//! Detect malicious contracts, phishing sites, and scam tokens

use std::collections::HashMap;
use std::sync::Arc;

use anyhow::Result;
use chrono::{DateTime, Utc};
use ethers::core::types::{Address, H256, U256};
use ethers::providers::{Http, Provider, Middleware};
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
    /// keccak256 of the analysed runtime code, so a caller can pin it.
    pub code_hash: String,
    /// The 4-byte dispatch selectors found in the runtime code.
    pub selectors: Vec<String>,
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

    /// Load the selectors this detector recognises.
    ///
    /// Each value is the real `keccak256(signature)[..4]` of a function whose
    /// presence in a dispatcher indicates owner-controlled behaviour.
    fn load_patterns(&mut self) {
        let mut patterns = self.known_patterns.write();

        let rules: Vec<(&str, &str, &str, &str)> = vec![
            ("setMaxTxAmount", "0xec28438a", "Medium", "Owner can cap the size of a single transfer"),
            ("blacklist", "0xf9f92be4", "High", "Owner can block an address from transferring"),
            ("addBotToBlackList", "0x4303443d", "High", "Owner can block an address from transferring"),
            ("_isBlacklisted", "0x1cdd3be3", "High", "Transfer path consults a blacklist"),
            ("setTradingEnabled", "0xc2e5ec04", "Medium", "Owner can switch trading on and off"),
            ("enableTrading", "0x8a8c523c", "Low", "Trading is disabled until the owner enables it"),
            ("setTaxFee", "0xc4081a4c", "Medium", "Owner can raise the transfer fee after launch"),
            ("setFees", "0x0b78f9c0", "Medium", "Owner can raise transfer fees after launch"),
            ("removeAllFee", "0x301370af", "Low", "Fees can be removed entirely"),
            ("transferOwnership", "0xf2fde38b", "Low", "Ownership can be transferred to another account"),
            ("renounceOwnership", "0x715018a6", "Low", "Owner can renounce control"),
            ("mint", "0x40c10f19", "Medium", "Supply is not fixed; new tokens can be minted"),
            ("setSwapAndLiquifyEnabled", "0xc49b9a80", "Low", "Owner can switch automatic swaps on and off"),
        ];

        for (name, selector, severity, desc) in rules {
            patterns.insert(name.to_string(), HoneypotPattern {
                name: name.to_string(),
                pattern: selector.to_string(),
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

    /// Findings for a runtime bytecode blob.
    ///
    /// Selectors are decoded first, then matched against the known rules, so a
    /// coincidental byte sequence cannot raise a finding on its own.
    pub fn analyze_bytecode(&self, bytecode: &[u8]) -> Vec<SecurityFinding> {
        let selectors = extract_selectors(bytecode);
        let present: std::collections::HashSet<String> = selectors.into_iter().collect();
        let patterns = self.known_patterns.read();

        let mut findings: Vec<SecurityFinding> = patterns
            .values()
            .filter(|pattern| present.contains(pattern.pattern.trim_start_matches("0x")))
            .map(|pattern| SecurityFinding {
                category: FindingCategory::Honeypot,
                severity: pattern.severity,
                description: pattern.description.clone(),
                evidence: format!(
                    "selector {} ({}) is in the dispatch table",
                    pattern.pattern, pattern.name
                ),
                cwe_id: Some("CWE-284".to_string()),
            })
            .collect();

        findings.sort_by(|a, b| a.evidence.cmp(&b.evidence));
        findings
    }


    /// Whether a transfer path is gated by an owner-controlled switch.
    ///
    /// `selectors` are the 4-byte dispatch entries decoded from runtime code.
    pub fn check_transfer_logic(&self, selectors: &[String]) -> Option<SecurityFinding> {
        let present: std::collections::HashSet<&str> =
            selectors.iter().map(String::as_str).collect();

        let blacklist = present.contains("f9f92be4")
            || present.contains("4303443d")
            || present.contains("1cdd3be3");

        let trading_switch = present.contains("c2e5ec04") || present.contains("8a8c523c");

        if blacklist && trading_switch {
            return Some(SecurityFinding {
                category: FindingCategory::Honeypot,
                severity: FindingSeverity::Critical,
                description:
                    "Transfers can be blocked and trading can be switched off by the owner"
                        .to_string(),
                evidence: "blacklist selector together with a trading switch".to_string(),
                cwe_id: Some("CWE-284".to_string()),
            });
        }

        if blacklist {
            return Some(SecurityFinding {
                category: FindingCategory::Honeypot,
                severity: FindingSeverity::High,
                description: "The owner can block an address from transferring".to_string(),
                evidence: "blacklist selector in the dispatch table".to_string(),
                cwe_id: Some("CWE-284".to_string()),
            });
        }

        None
    }
}

/// Decode the 4-byte selectors in a contract's dispatch table.
///
/// A selector appears in runtime code as `PUSH4 <selector>`. Scanning for that
/// pattern finds the dispatcher entries and, in practice, little else.
pub fn extract_selectors(bytecode: &[u8]) -> Vec<String> {
    let mut selectors = Vec::new();
    let mut seen = std::collections::HashSet::new();

    let mut index = 0;
    while index + 4 < bytecode.len() {
        if bytecode[index] == 0x63 {
            let hexed = hex::encode(&bytecode[index + 1..index + 5]);
            if seen.insert(hexed.clone()) {
                selectors.push(hexed);
            }
            index += 5;
        } else {
            index += 1;
        }
    }

    selectors
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
        
        let address: Address = address
            .trim()
            .parse()
            .map_err(|error| SecurityError::Analysis(format!("invalid address: {error}")))?;

        let code = self
            .rpc
            .get_code(address, None)
            .await
            .map_err(|error| SecurityError::Rpc(error.to_string()))?;

        if code.is_empty() {
            return Err(anyhow::anyhow!("{address:#x} has no deployed bytecode"));
        }

        let bytecode: &[u8] = code.as_ref();
        let selectors = extract_selectors(bytecode);

        let mut findings = self.honeypot_detector.analyze_bytecode(bytecode);
        let mut indicators = Vec::new();

        if let Some(finding) = self.honeypot_detector.check_transfer_logic(&selectors) {
            findings.push(finding);
        }

        
        let address_hex = format!("{address:#x}");

        // Check threat database
        if let Some(threat) = self.phishing_detector.check_address(&address_hex) {
            findings.push(SecurityFinding {
                category: FindingCategory::Scam,
                severity: FindingSeverity::Critical,
                description: format!("Address is on the threat list: {}", threat.threat_type),
                evidence: format!(
                    "{} reports, confidence {:.0}%",
                    threat.reports,
                    threat.confidence * 100.0
                ),
                cwe_id: None,
            });
            
            indicators.push(ThreatIndicator {
                indicator_type: IndicatorType::Address,
                value: format!("{address:#x}"),
                source: "threat list".to_string(),
                confidence: threat.confidence,
            });
        }
        
        // Calculate scores
        let honeypot_score = self.calculate_honeypot_score(&findings);
        let phishing_score = self.calculate_phishing_score(&findings);
        let scam_score = self.calculate_scam_score(&findings);
        
        let is_malicious = honeypot_score >= 0.75 || phishing_score >= 0.75 || scam_score >= 0.75;
        
        let risk_level = if honeypot_score >= 1.0 {
            RiskLevel::Critical
        } else if honeypot_score.max(phishing_score).max(scam_score) >= 0.75 {
            RiskLevel::High
        } else if honeypot_score.max(phishing_score).max(scam_score) >= 0.5 {
            RiskLevel::Medium
        } else if honeypot_score.max(phishing_score).max(scam_score) > 0.0 {
            RiskLevel::Low
        } else {
            RiskLevel::Safe
        };
        
        // Generate recommendations
        let recommendations = self.generate_recommendations(&findings, risk_level);
        
        // Cache result
        let report = SecurityReport {
            address: format!("{address:#x}"),
            code_hash: format!("0x{}", hex::encode(ethers::utils::keccak256(bytecode))),
            selectors: selectors
                .iter()
                .map(|selector| format!("0x{selector}"))
                .collect(),
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
        state.cache.insert(format!("{address:#x}"), report.clone());
        state.scans_today += 1;
        
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
    fn contract_hash_is_sha256_hex() {
        // NIST vector: SHA-256("abc").
        assert_eq!(
            compute_contract_hash("abc"),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }

    #[test]
    fn selectors_are_extracted_from_push4_operands() {
        let bytecode = [0x63, 0xa9, 0x05, 0x9c, 0xbb, 0x63, 0x70, 0xa0, 0x82, 0x31];

        assert_eq!(extract_selectors(&bytecode), vec!["a9059cbb", "70a08231"]);
    }

    #[test]
    fn a_repeated_selector_is_reported_once() {
        let bytecode = [0x63, 0xa9, 0x05, 0x9c, 0xbb, 0x63, 0xa9, 0x05, 0x9c, 0xbb];

        assert_eq!(extract_selectors(&bytecode).len(), 1);
    }

    #[test]
    fn a_blacklist_selector_is_reported() {
        let patterns = Arc::new(RwLock::new(HashMap::new()));
        patterns.write().insert(
            "blacklist".to_string(),
            HoneypotPattern {
                name: "blacklist".to_string(),
                pattern: "0xf9f92be4".to_string(),
                severity: FindingSeverity::High,
                description: "Owner can block an address".to_string(),
            },
        );

        let detector = HoneypotDetector {
            config: Config::default(),
            rpc: Provider::<Http>::try_from("http://127.0.0.1:8545").unwrap(),
            known_patterns: patterns,
        };

        let bytecode = [0x63, 0xf9, 0xf9, 0x2b, 0xe4];

        let findings = detector.analyze_bytecode(&bytecode);

        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].severity, FindingSeverity::High);
    }

    #[test]
    fn a_plain_erc20_raises_no_finding() {
        let patterns = Arc::new(RwLock::new(HashMap::new()));
        patterns.write().insert(
            "blacklist".to_string(),
            HoneypotPattern {
                name: "blacklist".to_string(),
                pattern: "0xf9f92be4".to_string(),
                severity: FindingSeverity::High,
                description: "Owner can block an address".to_string(),
            },
        );

        let detector = HoneypotDetector {
            config: Config::default(),
            rpc: Provider::<Http>::try_from("http://127.0.0.1:8545").unwrap(),
            known_patterns: patterns,
        };

        // transfer(address,uint256) and balanceOf(address).
        let bytecode = [0x63, 0xa9, 0x05, 0x9c, 0xbb, 0x63, 0x70, 0xa0, 0x82, 0x31];

        assert!(detector.analyze_bytecode(&bytecode).is_empty());
    }

    #[test]
    fn a_blacklist_together_with_a_trading_switch_is_critical() {
        let detector = HoneypotDetector {
            config: Config::default(),
            rpc: Provider::<Http>::try_from("http://127.0.0.1:8545").unwrap(),
            known_patterns: Arc::new(RwLock::new(HashMap::new())),
        };

        let finding = detector
            .check_transfer_logic(&["f9f92be4".to_string(), "c2e5ec04".to_string()])
            .expect("blacklist plus trading switch must be reported");

        assert_eq!(finding.severity, FindingSeverity::Critical);
    }

    #[test]
    fn a_trading_switch_alone_is_not_a_finding() {
        let detector = HoneypotDetector {
            config: Config::default(),
            rpc: Provider::<Http>::try_from("http://127.0.0.1:8545").unwrap(),
            known_patterns: Arc::new(RwLock::new(HashMap::new())),
        };

        assert!(detector
            .check_transfer_logic(&["c2e5ec04".to_string()])
            .is_none());
    }
}