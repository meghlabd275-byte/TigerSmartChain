//! TigerSmartChain security engine.
//!
//! Six detectors, each of which reports what it can actually establish:
//!
//! | Detector | Signal |
//! |---|---|
//! | [`phishing`] | Blocklist hits and lookalike domains (edit distance, homoglyphs, brand-on-foreign-suffix) |
//! | [`scam`] | Reported contracts, deployer history, and capability combinations read from real 4-byte selectors |
//! | [`honeypot`] | Buy/sell simulation results, with capability analysis as a pre-filter |
//! | [`blacklist`] | Reported addresses, case-insensitively |
//! | [`transaction_risk`] | MEV, front-running and sandwich patterns from calldata and gas |
//! | [`anomaly`] | Volume, gas and transaction-count deviations against a median baseline |
//!
//! Every list starts empty. A detector that ships with a hardcoded "known
//! scams" list is wrong within a day, and the previous version of this crate
//! shipped exactly that: one entry, the burn address. The lists are filled by
//! [`SecurityEngine::report_scam`], [`SecurityEngine::report_honeypot`],
//! [`SecurityEngine::blacklist_address`], and, for domains,
//! [`SecurityEngine::refresh_phishing_feeds`].

pub mod anomaly;
pub mod blacklist;
pub mod engine;
pub mod honeypot;
pub mod phishing;
pub mod scam;
pub mod transaction_risk;

pub use anomaly::{AnomalyDetector, AnomalyReport};
pub use blacklist::{Blacklist, BlacklistCategory};
pub use engine::SecurityEngine;
pub use honeypot::{CallOutcome, HoneypotDetector, SellSimulation};
pub use phishing::PhishingDetector;
pub use scam::{ScamDetector, ScamType};
pub use transaction_risk::{
    SandwichFinding, TransactionData, TransactionRiskAnalyzer,
};

/// A complete security report.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct SecurityReport {
    pub overall_risk: RiskLevel,
    /// The single highest component score, which drives `overall_risk`.
    pub risk_score: f64,
    pub phishing_score: f64,
    pub scam_score: f64,
    pub honeypot_score: f64,
    pub blacklist_match: bool,
    pub transaction_risk: f64,
    pub anomaly_score: f64,
    pub warnings: Vec<String>,
    pub details: SecurityDetails,
}

/// The per-detector results behind a report.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct SecurityDetails {
    pub phishing: Option<phishing::PhishingMatch>,
    pub scam: Vec<scam::ScamMatch>,
    pub honeypot: Option<honeypot::HoneypotMatch>,
    pub blacklist: Option<blacklist::BlacklistMatch>,
    pub transaction: Option<transaction_risk::RiskAssessment>,
    pub anomaly: AnomalyReport,
}

impl SecurityDetails {
    /// Details with nothing found.
    pub fn empty() -> Self {
        Self {
            phishing: None,
            scam: Vec::new(),
            honeypot: None,
            blacklist: None,
            transaction: None,
            anomaly: AnomalyReport::none(),
        }
    }
}

/// The overall risk band.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RiskLevel {
    Safe,
    Low,
    Medium,
    High,
    Critical,
}

impl RiskLevel {
    /// The band a 0..=1 score falls in.
    pub fn from_score(score: f64) -> Self {
        match score {
            s if s >= 0.8 => RiskLevel::Critical,
            s if s >= 0.6 => RiskLevel::High,
            s if s >= 0.4 => RiskLevel::Medium,
            s if s >= 0.2 => RiskLevel::Low,
            _ => RiskLevel::Safe,
        }
    }

    /// The lower bound of the band.
    pub fn as_float(self) -> f64 {
        match self {
            RiskLevel::Safe => 0.0,
            RiskLevel::Low => 0.2,
            RiskLevel::Medium => 0.4,
            RiskLevel::High => 0.6,
            RiskLevel::Critical => 0.8,
        }
    }

    /// Whether this band is at least as severe as `other`.
    pub fn is_at_least(self, other: RiskLevel) -> bool {
        self >= other
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn risk_bands_cover_the_unit_interval() {
        assert_eq!(RiskLevel::from_score(0.0), RiskLevel::Safe);
        assert_eq!(RiskLevel::from_score(0.19), RiskLevel::Safe);
        assert_eq!(RiskLevel::from_score(0.2), RiskLevel::Low);
        assert_eq!(RiskLevel::from_score(0.39), RiskLevel::Low);
        assert_eq!(RiskLevel::from_score(0.4), RiskLevel::Medium);
        assert_eq!(RiskLevel::from_score(0.6), RiskLevel::High);
        assert_eq!(RiskLevel::from_score(0.8), RiskLevel::Critical);
        assert_eq!(RiskLevel::from_score(1.0), RiskLevel::Critical);
    }

    #[test]
    fn a_score_is_never_below_its_bands_floor() {
        for step in 0..=100 {
            let score = step as f64 / 100.0;
            let level = RiskLevel::from_score(score);
            assert!(
                score + 1e-9 >= level.as_float(),
                "score {score} fell into {level:?}"
            );
        }
    }

    #[test]
    fn severity_ordering_holds() {
        assert!(RiskLevel::Critical > RiskLevel::High);
        assert!(RiskLevel::High > RiskLevel::Medium);
        assert!(RiskLevel::Medium > RiskLevel::Low);
        assert!(RiskLevel::Low > RiskLevel::Safe);
        assert!(RiskLevel::High.is_at_least(RiskLevel::Medium));
        assert!(!RiskLevel::Safe.is_at_least(RiskLevel::Low));
    }

    #[test]
    fn empty_details_are_all_absent() {
        let details = SecurityDetails::empty();
        assert!(details.phishing.is_none());
        assert!(details.scam.is_empty());
        assert!(details.honeypot.is_none());
        assert!(details.blacklist.is_none());
        assert!(details.transaction.is_none());
        assert_eq!(details.anomaly.score, 0.0);
    }
}
