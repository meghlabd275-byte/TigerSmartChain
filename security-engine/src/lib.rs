//! TigerSmartChain Security Engine
//! 
//! A comprehensive security system for detecting:
//! - Phishing domains and websites
//! - Scam tokens and contracts
//! - Honeypot contracts
//! - Blacklisted addresses
//! - Transaction risk analysis
//! - Anomaly detection

pub mod phishing;
pub mod scam;
pub mod honeypot;
pub mod blacklist;
pub mod transaction_risk;
pub mod anomaly;
pub mod engine;

pub use engine::SecurityEngine;
pub use phishing::PhishingDetector;
pub use scam::ScamDetector;
pub use honeypot::HoneypotDetector;
pub use blacklist::Blacklist;
pub use transaction_risk::TransactionRiskAnalyzer;
pub use anomaly::AnomalyDetector;

/// Security report containing all analysis results
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct SecurityReport {
    pub overall_risk: RiskLevel,
    pub phishing_score: f64,
    pub scam_score: f64,
    pub honeypot_score: f64,
    pub blacklist_match: bool,
    pub transaction_risk: f64,
    pub anomaly_score: f64,
    pub warnings: Vec<String>,
    pub details: SecurityDetails,
}

/// Security details for each check
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct SecurityDetails {
    pub phishing: PhishingResult,
    pub scam: ScamResult,
    pub honeypot: HoneypotResult,
    pub blacklist: BlacklistResult,
    pub transaction: TransactionRiskResult,
    pub anomaly: AnomalyResult,
}

/// Risk level enumeration
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RiskLevel {
    Safe,
    Low,
    Medium,
    High,
    Critical,
}

impl RiskLevel {
    pub fn as_float(&self) -> f64 {
        match self {
            RiskLevel::Safe => 0.0,
            RiskLevel::Low => 0.25,
            RiskLevel::Medium => 0.5,
            RiskLevel::High => 0.75,
            RiskLevel::Critical => 1.0,
        }
    }
}

/// Individual check results
pub type PhishingResult = Option<phishing::PhishingMatch>;
pub type ScamResult = Option<scam::ScamMatch>;
pub type HoneypotResult = Option<honeypot::HoneypotMatch>;
pub type BlacklistResult = Option<blacklist::BlacklistMatch>;
pub type TransactionRiskResult = transaction_risk::RiskAssessment;
pub type AnomalyResult = anomaly::AnomalyReport;