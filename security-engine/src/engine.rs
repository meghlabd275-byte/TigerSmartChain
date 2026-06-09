//! Security Engine - Main Entry Point
//! 
//! Combines all security modules into a unified engine.

use crate::{
    phishing::{PhishingDetector, PhishingMatch},
    scam::{ScamDetector, ScamMatch},
    honeypot::{HoneypotDetector, HoneypotMatch},
    blacklist::{Blacklist, BlacklistMatch},
    transaction_risk::{TransactionRiskAnalyzer, RiskAssessment, TransactionData},
    anomaly::{AnomalyDetector, AnomalyReport},
    SecurityReport, SecurityDetails, RiskLevel,
};

/// Main security engine
pub struct SecurityEngine {
    phishing: PhishingDetector,
    scam: ScamDetector,
    honeypot: HoneypotDetector,
    blacklist: Blacklist,
    tx_risk: TransactionRiskAnalyzer,
    anomaly: AnomalyDetector,
    api_key: Option<String>,
}

impl SecurityEngine {
    pub fn new() -> Self {
        Self {
            phishing: PhishingDetector::new(),
            scam: ScamDetector::new(),
            honeypot: HoneypotDetector::new(),
            blacklist: Blacklist::new(),
            tx_risk: TransactionRiskAnalyzer::new(),
            anomaly: AnomalyDetector::new(),
            api_key: None,
        }
    }

    pub fn with_api_key(mut self, api_key: String) -> Self {
        self.api_key = Some(api_key);
        self
    }

    /// Analyze an address
    pub async fn analyze_address(&mut self, address: &str) -> Result<SecurityReport, Box<dyn std::error::Error + Send + Sync>> {
        // Run all checks in parallel would be better
        let phishing = self.phishing.check_domain(address);
        let blacklist = self.blacklist.check(address);
        
        let phishing_score = if phishing.is_some() { 1.0 } else { 0.0 };
        let blacklist_match = blacklist.is_some();
        
        Ok(SecurityReport {
            overall_risk: self.calculate_risk(phishing_score, 0.0, 0.0, blacklist_match, 0.0, 0.0),
            phishing_score,
            scam_score: 0.0,
            honeypot_score: 0.0,
            blacklist_match,
            transaction_risk: 0.0,
            anomaly_score: 0.0,
            warnings: self.generate_warnings(phishing, blacklist),
            details: SecurityDetails {
                phishing,
                scam: None,
                honeypot: None,
                blacklist,
                transaction: RiskAssessment {
                    overall_risk: 0.0,
                    mev_risk: 0.0,
                    front_run_risk: 0.0,
                    sandwich_risk: 0.0,
                    unusual_pattern_risk: 0.0,
                    gas_anomaly_risk: 0.0,
                    timing_anomaly_risk: 0.0,
                    details: vec![],
                },
                anomaly: AnomalyReport {
                    score: 0.0,
                    anomalies: vec![],
                    pattern_type: None,
                },
            },
        })
    }

    /// Analyze a contract
    pub async fn analyze_contract(&mut self, contract: &str) -> Result<SecurityReport, Box<dyn std::error::Error + Send + Sync>> {
        let scam = self.scam.check_contract(contract);
        let honeypot = self.honeypot.check_contract(contract);
        let blacklist = self.blacklist.check(contract);
        
        let scam_score = if scam.is_some() { 1.0 } else { 0.0 };
        let honeypot_score = if honeypot.is_some() { 1.0 } else { 0.0 };
        let blacklist_match = blacklist.is_some();
        
        Ok(SecurityReport {
            overall_risk: self.calculate_risk(0.0, scam_score, honeypot_score, blacklist_match, 0.0, 0.0),
            phishing_score: 0.0,
            scam_score,
            honeypot_score,
            blacklist_match,
            transaction_risk: 0.0,
            anomaly_score: 0.0,
            warnings: self.generate_contract_warnings(scam, honeypot, blacklist),
            details: SecurityDetails {
                phishing: None,
                scam,
                honeypot,
                blacklist,
                transaction: RiskAssessment {
                    overall_risk: 0.0,
                    mev_risk: 0.0,
                    front_run_risk: 0.0,
                    sandwich_risk: 0.0,
                    unusual_pattern_risk: 0.0,
                    gas_anomaly_risk: 0.0,
                    timing_anomaly_risk: 0.0,
                    details: vec![],
                },
                anomaly: AnomalyReport {
                    score: 0.0,
                    anomalies: vec![],
                    pattern_type: None,
                },
            },
        })
    }

    /// Analyze a transaction
    pub async fn analyze_transaction(&mut self, tx_hash: &str) -> Result<SecurityReport, Box<dyn std::error::Error + Send + Sync>> {
        // Would fetch transaction data from RPC in production
        let tx_data = TransactionData {
            from: "unknown".to_string(),
            to: "unknown".to_string(),
            value: "0".to_string(),
            gas_price: "0".to_string(),
            gas_limit: 0,
            timestamp: 0,
            data: None,
            block_number: 0,
        };
        
        let tx_risk = self.tx_risk.analyze(&tx_data);
        
        Ok(SecurityReport {
            overall_risk: self.calculate_risk(0.0, 0.0, 0.0, false, tx_risk.overall_risk, 0.0),
            phishing_score: 0.0,
            scam_score: 0.0,
            honeypot_score: 0.0,
            blacklist_match: false,
            transaction_risk: tx_risk.overall_risk,
            anomaly_score: 0.0,
            warnings: tx_risk.details.clone(),
            details: SecurityDetails {
                phishing: None,
                scam: None,
                honeypot: None,
                blacklist: None,
                transaction: tx_risk,
                anomaly: AnomalyReport {
                    score: 0.0,
                    anomalies: vec![],
                    pattern_type: None,
                },
            },
        })
    }

    /// Analyze a token
    pub async fn analyze_token(&mut self, token: &str) -> Result<SecurityReport, Box<dyn std::error::Error + Send + Sync>> {
        let scam = self.scam.check_contract(token);
        let honeypot = self.honeypot.check_contract(token);
        let blacklist = self.blacklist.check(token);
        
        let scam_score = if scam.is_some() { 1.0 } else { 0.0 };
        let honeypot_score = if honeypot.is_some() { 1.0 } else { 0.0 };
        let blacklist_match = blacklist.is_some();
        
        Ok(SecurityReport {
            overall_risk: self.calculate_risk(0.0, scam_score, honeypot_score, blacklist_match, 0.0, 0.0),
            phishing_score: 0.0,
            scam_score,
            honeypot_score,
            blacklist_match,
            transaction_risk: 0.0,
            anomaly_score: 0.0,
            warnings: self.generate_contract_warnings(scam, honeypot, blacklist),
            details: SecurityDetails {
                phishing: None,
                scam,
                honeypot,
                blacklist,
                transaction: RiskAssessment {
                    overall_risk: 0.0,
                    mev_risk: 0.0,
                    front_run_risk: 0.0,
                    sandwich_risk: 0.0,
                    unusual_pattern_risk: 0.0,
                    gas_anomaly_risk: 0.0,
                    timing_anomaly_risk: 0.0,
                    details: vec![],
                },
                anomaly: AnomalyReport {
                    score: 0.0,
                    anomalies: vec![],
                    pattern_type: None,
                },
            },
        })
    }

    /// Calculate overall risk level
    fn calculate_risk(
        &self,
        phishing: f64,
        scam: f64,
        honeypot: f64,
        blacklist: bool,
        tx_risk: f64,
        anomaly: f64,
    ) -> RiskLevel {
        let scores = vec![phishing, scam, honeypot, if blacklist { 0.8 } else { 0.0 }, tx_risk, anomaly];
        let max_score = scores.iter().cloned().fold(0.0f64, |a, b| a.max(b));
        
        match max_score {
            s if s >= 0.8 => RiskLevel::Critical,
            s if s >= 0.6 => RiskLevel::High,
            s if s >= 0.4 => RiskLevel::Medium,
            s if s >= 0.2 => RiskLevel::Low,
            _ => RiskLevel::Safe,
        }
    }

    /// Generate warnings for address
    fn generate_warnings(
        &self,
        phishing: Option<PhishingMatch>,
        blacklist: Option<BlacklistMatch>,
    ) -> Vec<String> {
        let mut warnings = Vec::new();
        
        if let Some(m) = phishing {
            warnings.push(format!("Phishing domain detected: {} (target: {})", m.domain, m.target));
        }
        
        if let Some(m) = blacklist {
            warnings.push(format!("Blacklisted address: {} ({})", m.address, m.category));
        }
        
        warnings
    }

    /// Generate warnings for contract
    fn generate_contract_warnings(
        &self,
        scam: Option<ScamMatch>,
        honeypot: Option<HoneypotMatch>,
        blacklist: Option<BlacklistMatch>,
    ) -> Vec<String> {
        let mut warnings = Vec::new();
        
        if let Some(m) = scam {
            warnings.push(format!("Scam contract: {} ({:?})", m.contract, m.scam_type));
        }
        
        if let Some(m) = honeypot {
            warnings.push(format!("Honeypot contract: {} ({:?})", m.contract, m.honeypot_type));
        }
        
        if let Some(m) = blacklist {
            warnings.push(format!("Blacklisted: {} ({})", m.address, m.category));
        }
        
        warnings
    }
}

impl Default for SecurityEngine {
    fn default() -> Self {
        Self::new()
    }
}