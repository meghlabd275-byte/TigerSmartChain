//! The engine that runs the detectors together.
//!
//! The previous version had three problems that made its reports unreliable:
//!
//! - It moved each `Option<…Match>` into the warning builder and then tried to
//!   use it again in the details, which does not compile.
//! - `analyze_transaction` fabricated the transaction it analysed —
//!   `from: "unknown"`, `value: "0"` — and analysed that, so every transaction
//!   scored as safe regardless of its actual content.
//! - The overall level came from the maximum component, which meant a confirmed
//!   blacklist hit and a merely suspicious domain produced the same band.
//!
//! This version takes the data it analyses as a parameter, combines components
//! with a noisy-OR so a single certain signal dominates, and reports the
//! component scores alongside the band.

use crate::anomaly::{AnomalyDetector, AnomalyReport};
use crate::blacklist::{Blacklist, BlacklistCategory, BlacklistMatch};
use crate::honeypot::{HoneypotDetector, HoneypotMatch, SellSimulation};
use crate::phishing::{PhishingDetector, PhishingMatch};
use crate::scam::{ScamDetector, ScamMatch, ScamType};
use crate::transaction_risk::{
    combine, RiskAssessment, SandwichFinding, TransactionData, TransactionRiskAnalyzer,
};
use crate::{RiskLevel, SecurityDetails, SecurityReport};

/// The security engine.
pub struct SecurityEngine {
    phishing: PhishingDetector,
    scam: ScamDetector,
    honeypot: HoneypotDetector,
    blacklist: Blacklist,
    tx_risk: TransactionRiskAnalyzer,
    anomaly: AnomalyDetector,
}

impl SecurityEngine {
    /// An engine with empty lists.
    pub fn new() -> Self {
        Self {
            phishing: PhishingDetector::new(),
            scam: ScamDetector::new(),
            honeypot: HoneypotDetector::new(),
            blacklist: Blacklist::new(),
            tx_risk: TransactionRiskAnalyzer::new(),
            anomaly: AnomalyDetector::new(),
        }
    }

    /// Record a contract as a confirmed scam.
    pub fn report_scam(&mut self, address: &str, scam_type: ScamType, reason: &str) {
        self.scam.report(address, scam_type, reason);
    }

    /// Record a deployer whose contracts are suspect.
    pub fn report_deployer(&mut self, deployer: &str) {
        self.scam.report_deployer(deployer);
    }

    /// Record a contract as a confirmed honeypot.
    pub fn report_honeypot(&mut self, address: &str) {
        self.honeypot.report(address);
    }

    /// Add an address to the blacklist.
    pub fn blacklist_address(
        &mut self,
        address: &str,
        category: BlacklistCategory,
        reason: &str,
        risk_score: f64,
    ) -> bool {
        self.blacklist.add(address, category, reason, risk_score)
    }

    /// Add a domain to the phishing blocklist.
    pub fn block_domain(&mut self, domain: &str) {
        self.phishing.block(domain);
    }

    /// Pull the phishing feeds.
    pub async fn refresh_phishing_feeds(&mut self) -> usize {
        self.phishing.refresh_feeds().await
    }

    /// Feed the anomaly detector a block's statistics.
    pub fn observe_block(&mut self, timestamp: i64, volume: f64, gas: f64, tx_count: usize) {
        self.anomaly.add_entry(timestamp, volume, gas, tx_count);
    }

    /// The current anomaly report.
    pub fn anomaly_report(&self) -> AnomalyReport {
        self.anomaly.detect()
    }

    /// Analyze an address or domain.
    pub fn analyze_address(&self, address: &str) -> SecurityReport {
        let phishing = self.phishing.check_domain(address);
        let blacklist = self.blacklist.check(address);

        let phishing_score = phishing
            .as_ref()
            .map(|m| m.confidence)
            .unwrap_or(0.0);
        let blacklist_score = blacklist
            .as_ref()
            .map(|m| m.risk_score)
            .unwrap_or(0.0);

        let mut warnings = Vec::new();
        if let Some(m) = &phishing {
            warnings.push(format!(
                "Lookalike or reported domain {} impersonating {} ({} confidence)",
                m.domain, m.target, m.confidence
            ));
        }
        if let Some(m) = &blacklist {
            warnings.push(format!(
                "Blacklisted address {}: {} ({:?})",
                m.address, m.reason, m.category
            ));
        }

        let risk_score = combine(&[phishing_score, blacklist_score]);

        SecurityReport {
            overall_risk: RiskLevel::from_score(risk_score),
            risk_score,
            phishing_score,
            scam_score: 0.0,
            honeypot_score: 0.0,
            blacklist_match: blacklist.is_some(),
            transaction_risk: 0.0,
            anomaly_score: 0.0,
            warnings,
            details: SecurityDetails {
                phishing,
                scam: Vec::new(),
                honeypot: None,
                blacklist,
                transaction: None,
                anomaly: AnomalyReport::none(),
            },
        }
    }

    /// Analyze a contract address, its bytecode, and its deployer.
    ///
    /// Bytecode is optional; when supplied it is scanned for scam capabilities.
    /// Without it the analysis is limited to the reported lists, and the report
    /// says so.
    pub fn analyze_contract(
        &self,
        address: &str,
        bytecode: Option<&str>,
        deployer: Option<&str>,
    ) -> SecurityReport {
        let mut matches: Vec<ScamMatch> = Vec::new();
        let mut warnings: Vec<String> = Vec::new();

        if let Some(reported) = self.scam.check_contract(address) {
            warnings.push(format!(
                "Reported scam contract {}: {}",
                reported.contract,
                reported.evidence.join("; ")
            ));
            matches.push(reported);
        }

        if let Some(deployer) = deployer {
            if let Some(found) = self.scam.check_deployer(deployer) {
                warnings.push(format!(
                    "Deployer {deployer} has previously deployed reported scams"
                ));
                matches.push(found);
            }
        }

        if let Some(bytecode) = bytecode {
            for found in self.scam.analyze_bytecode(bytecode) {
                warnings.push(format!(
                    "Bytecode capability: {}",
                    found.evidence.join("; ")
                ));
                matches.push(found);
            }
        } else if self.scam.check_contract(address).is_none() {
            warnings.push(
                "No bytecode supplied: capability analysis was not performed".to_string(),
            );
        }

        let honeypot = self.honeypot.check_contract(address);
        if let Some(found) = &honeypot {
            warnings.push(format!(
                "Confirmed honeypot {}: {}",
                found.contract,
                found.evidence.join("; ")
            ));
        }

        let blacklist = self.blacklist.check(address);
        if let Some(found) = &blacklist {
            warnings.push(format!(
                "Blacklisted address {}: {} ({:?})",
                found.address, found.reason, found.category
            ));
        }

        // The strongest scam finding drives the scam score.
        let scam_score = matches
            .iter()
            .map(|m| m.confidence)
            .reduce(f64::max)
            .unwrap_or(0.0);
        let honeypot_score = honeypot.as_ref().map(|m| m.confidence).unwrap_or(0.0);
        let blacklist_score = blacklist.as_ref().map(|m| m.risk_score).unwrap_or(0.0);

        let risk_score = combine(&[scam_score, honeypot_score, blacklist_score]);

        SecurityReport {
            overall_risk: RiskLevel::from_score(risk_score),
            risk_score,
            phishing_score: 0.0,
            scam_score,
            honeypot_score,
            blacklist_match: blacklist.is_some(),
            transaction_risk: 0.0,
            anomaly_score: 0.0,
            warnings,
            details: SecurityDetails {
                phishing: None,
                scam: matches,
                honeypot,
                blacklist,
                transaction: None,
                anomaly: AnomalyReport::none(),
            },
        }
    }

    /// Analyze a token contract, its metadata, and its bytecode.
    pub fn analyze_token(
        &self,
        address: &str,
        name: Option<&str>,
        symbol: Option<&str>,
        bytecode: Option<&str>,
    ) -> SecurityReport {
        let mut report = self.analyze_contract(address, bytecode, None);

        if let (Some(name), Some(symbol)) = (name, symbol) {
            if let Some(found) = self.scam.analyze_token(name, symbol) {
                report.warnings.push(format!(
                    "Token metadata: {}",
                    found.evidence.join("; ")
                ));
                report.scam_score = report.scam_score.max(found.confidence);
                report.details.scam.push(found);
            }
        }

        if let Some(bytecode) = bytecode {
            if !self.scam.is_erc20(bytecode) {
                report
                    .warnings
                    .push("Contract does not expose the full ERC-20 interface".to_string());
            }
        }

        report.risk_score = combine(&[
            report.scam_score,
            report.honeypot_score,
            if report.blacklist_match { 0.8 } else { 0.0 },
        ]);
        report.overall_risk = RiskLevel::from_score(report.risk_score);

        report
    }

    /// Judge a buy/sell simulation of a token.
    pub fn analyze_honeypot_simulation(
        &self,
        address: &str,
        simulation: &SellSimulation,
    ) -> SecurityReport {
        let honeypot = self.honeypot.simulate_sell(address, simulation);
        let mut warnings = Vec::new();

        if let Some(found) = &honeypot {
            warnings.push(format!(
                "Honeypot behaviour: {} ({})",
                found.evidence.join("; "),
                found.honeypot_type as u8
            ));
        }

        let honeypot_score = honeypot.as_ref().map(|m| m.confidence).unwrap_or(0.0);
        let risk_score = combine(&[honeypot_score]);

        SecurityReport {
            overall_risk: RiskLevel::from_score(risk_score),
            risk_score,
            phishing_score: 0.0,
            scam_score: 0.0,
            honeypot_score,
            blacklist_match: false,
            transaction_risk: 0.0,
            anomaly_score: 0.0,
            warnings,
            details: SecurityDetails {
                phishing: None,
                scam: Vec::new(),
                honeypot,
                blacklist: None,
                transaction: None,
                anomaly: AnomalyReport::none(),
            },
        }
    }

    /// Analyze a transaction.
    ///
    /// The transaction is the caller's; nothing is fabricated. Baselines are
    /// optional and, when absent, the report says the relative checks were not
    /// performed.
    pub fn analyze_transaction(
        &self,
        tx: &TransactionData,
        recent_median_gas: Option<u128>,
        recent_median_value: Option<u128>,
    ) -> SecurityReport {
        let assessment = self
            .tx_risk
            .analyze_with_baseline(tx, recent_median_gas, recent_median_value);

        let mut warnings = assessment.details.clone();

        // The participants may be known-bad independently of the transaction.
        let mut blacklist_match = false;
        let mut blacklist_score: f64 = 0.0;

        for address in [&tx.from, &tx.to] {
            if let Some(found) = self.blacklist.check(address) {
                warnings.push(format!(
                    "Transaction participant {} is blacklisted: {}",
                    found.address, found.reason
                ));
                blacklist_match = true;
                blacklist_score = blacklist_score.max(found.risk_score);
            }
        }

        let anomaly = self.anomaly.detect();
        let risk_score = combine(&[assessment.overall_risk, blacklist_score, anomaly.score]);

        SecurityReport {
            overall_risk: RiskLevel::from_score(risk_score),
            risk_score,
            phishing_score: 0.0,
            scam_score: 0.0,
            honeypot_score: 0.0,
            blacklist_match,
            transaction_risk: assessment.overall_risk,
            anomaly_score: anomaly.score,
            warnings,
            details: SecurityDetails {
                phishing: None,
                scam: Vec::new(),
                honeypot: None,
                blacklist: None,
                transaction: Some(assessment),
                anomaly,
            },
        }
    }

    /// Analyze a block for sandwich attacks.
    pub fn analyze_block(&self, txs: &[TransactionData]) -> Vec<SandwichFinding> {
        self.tx_risk.analyze_block(txs)
    }

    /// Whether a phishing domain is protected from being flagged.
    pub fn is_protected_domain(&self, domain: &str) -> bool {
        self.phishing.is_protected(domain)
    }

    /// Number of blacklisted addresses.
    pub fn blacklist_size(&self) -> usize {
        self.blacklist.count()
    }

    /// Number of reported scams.
    pub fn reported_scam_count(&self) -> usize {
        self.scam.known_count()
    }

    /// Number of confirmed honeypots.
    pub fn reported_honeypot_count(&self) -> usize {
        self.honeypot.known_count()
    }

    /// Number of blocked phishing domains.
    pub fn blocked_domain_count(&self) -> usize {
        self.phishing.blocked_count()
    }
}

impl Default for SecurityEngine {
    fn default() -> Self {
        Self::new()
    }
}

/// Convenience: the phishing match for an address, if any.
pub fn phishing_match(engine: &SecurityEngine, domain: &str) -> Option<PhishingMatch> {
    engine.analyze_address(domain).details.phishing
}

/// Convenience: the blacklist match for an address, if any.
pub fn blacklist_match(engine: &SecurityEngine, address: &str) -> Option<BlacklistMatch> {
    engine.analyze_address(address).details.blacklist
}

/// Convenience: the honeypot match for a simulation, if any.
pub fn honeypot_match(
    engine: &SecurityEngine,
    address: &str,
    simulation: &SellSimulation,
) -> Option<HoneypotMatch> {
    engine
        .analyze_honeypot_simulation(address, simulation)
        .details
        .honeypot
}

/// Convenience: a risk assessment for a transaction.
pub fn transaction_assessment(
    engine: &SecurityEngine,
    tx: &TransactionData,
) -> Option<RiskAssessment> {
    engine.analyze_transaction(tx, None, None).details.transaction
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::honeypot::CallOutcome;

    const SCAM: &str = "0x1111111111111111111111111111111111111111";
    const CLEAN: &str = "0x2222222222222222222222222222222222222222";

    fn swap_tx(from: &str, value: &str, gas: &str) -> TransactionData {
        TransactionData {
            from: from.to_string(),
            to: "0xrouter".to_string(),
            value: value.to_string(),
            gas_price: gas.to_string(),
            gas_limit: 200_000,
            timestamp: 0,
            data: Some("0x38ed1739".to_string()),
            block_number: 1,
        }
    }

    #[test]
    fn a_clean_address_reports_safe() {
        let engine = SecurityEngine::new();
        let report = engine.analyze_address(CLEAN);

        assert_eq!(report.overall_risk, RiskLevel::Safe);
        assert_eq!(report.risk_score, 0.0);
        assert!(report.warnings.is_empty());
    }

    #[test]
    fn a_blacklisted_address_is_reported_at_its_risk_score() {
        let mut engine = SecurityEngine::new();
        engine.blacklist_address(CLEAN, BlacklistCategory::Sanctioned, "OFAC SDN", 1.0);

        let report = engine.analyze_address(CLEAN);
        assert_eq!(report.overall_risk, RiskLevel::Critical);
        assert!(report.blacklist_match);
        assert_eq!(report.risk_score, 1.0);
        assert!(report.warnings[0].contains("OFAC SDN"));
    }

    #[test]
    fn the_real_domain_is_safe_and_a_lookalike_is_not() {
        let engine = SecurityEngine::new();

        assert_eq!(
            engine.analyze_address("tigersmartchain.com").overall_risk,
            RiskLevel::Safe
        );
        assert!(engine.is_protected_domain("tigersmartchain.com"));

        let lookalike = engine.analyze_address("tigersmartchian.com");
        assert!(lookalike.risk_score > 0.0);
    }

    #[test]
    fn a_reported_scam_contract_is_critical() {
        let mut engine = SecurityEngine::new();
        engine.report_scam(SCAM, ScamType::RugPull, "drained the liquidity pool");

        let report = engine.analyze_contract(SCAM, None, None);
        assert_eq!(report.overall_risk, RiskLevel::Critical);
        assert_eq!(report.scam_score, 1.0);
        assert_eq!(report.details.scam.len(), 1);
        assert_eq!(engine.reported_scam_count(), 1);
    }

    #[test]
    fn a_flagged_deployer_is_reported_without_the_contract_being_listed() {
        let mut engine = SecurityEngine::new();
        engine.report_deployer(CLEAN);

        let report = engine.analyze_contract(SCAM, None, Some(CLEAN));
        assert!(report.scam_score >= 0.8);
        assert!(report.warnings.iter().any(|w| w.contains("Deployer")));
    }

    #[test]
    fn capability_analysis_flags_a_rug_pull_shape() {
        let engine = SecurityEngine::new();

        let mut code: Vec<u8> = Vec::new();
        for selector in [[0x40, 0xc1, 0x0f, 0x19], [0x69, 0x3c, 0x7d, 0x41]] {
            code.push(0x63);
            code.extend_from_slice(&selector);
            code.push(0x14);
            code.push(0x61);
            code.extend_from_slice(&[0x00, 0x2a]);
            code.push(0x57);
        }

        let report = engine.analyze_contract(CLEAN, Some(&hex::encode(code)), None);
        assert!(report.scam_score >= 0.7);
        assert!(report.warnings.iter().any(|w| w.contains("capability")));
    }

    #[test]
    fn a_missing_bytecode_is_disclosed_rather_than_assumed_clean() {
        let engine = SecurityEngine::new();
        let report = engine.analyze_contract(CLEAN, None, None);

        assert_eq!(report.overall_risk, RiskLevel::Safe);
        assert!(report
            .warnings
            .iter()
            .any(|w| w.contains("capability analysis was not performed")));
    }

    #[test]
    fn a_confirmed_honeypot_raises_the_risk() {
        let mut engine = SecurityEngine::new();
        engine.report_honeypot(SCAM);

        let report = engine.analyze_contract(SCAM, None, None);
        assert_eq!(report.honeypot_score, 1.0);
        assert_eq!(report.overall_risk, RiskLevel::Critical);
    }

    #[test]
    fn a_honeypot_simulation_is_judged_on_its_outcome() {
        let engine = SecurityEngine::new();

        let simulation = SellSimulation {
            buy: CallOutcome::success("1000"),
            sell: CallOutcome::revert("execution reverted"),
            sell_from_non_owner: None,
        };

        let report = engine.analyze_honeypot_simulation(SCAM, &simulation);
        assert_eq!(report.honeypot_score, 0.95);
        assert_eq!(report.overall_risk, RiskLevel::Critical);

        let clean = SellSimulation {
            buy: CallOutcome::success("1000"),
            sell: CallOutcome::success("1000"),
            sell_from_non_owner: Some(CallOutcome::success("1000")),
        };
        assert_eq!(
            engine.analyze_honeypot_simulation(SCAM, &clean).overall_risk,
            RiskLevel::Safe
        );
    }

    #[test]
    fn the_transaction_analyzed_is_the_one_supplied() {
        // The old engine fabricated from="unknown", value="0" and analysed that.
        let engine = SecurityEngine::new();
        let report = engine.analyze_transaction(
            &swap_tx("0xattacker", &(100 * crate::transaction_risk::ONE_ETHER).to_string(), "1000000000"),
            None,
            None,
        );

        let assessment = report.details.transaction.unwrap();
        // A 100-unit swap is worth sandwiching, so MEV risk is present.
        assert!(assessment.mev_risk >= 0.5, "{assessment:?}");
        assert!(report.transaction_risk > 0.0);
    }

    #[test]
    fn a_blacklisted_participant_raises_transaction_risk() {
        let attacker = "0x9999999999999999999999999999999999999999";
        let mut engine = SecurityEngine::new();
        engine.blacklist_address(attacker, BlacklistCategory::Hack, "exploit", 1.0);

        let report =
            engine.analyze_transaction(&swap_tx(attacker, "1000", "1000000000"), None, None);

        assert!(report.blacklist_match);
        assert_eq!(report.overall_risk, RiskLevel::Critical);
    }

    #[test]
    fn a_plain_transaction_stays_safe() {
        let engine = SecurityEngine::new();
        let tx = TransactionData {
            from: CLEAN.to_string(),
            to: SCAM.to_string(),
            value: "1000000000000000".to_string(),
            gas_price: "1000000000".to_string(),
            gas_limit: 21000,
            timestamp: 0,
            data: None,
            block_number: 1,
        };

        let report = engine.analyze_transaction(&tx, None, None);
        assert_eq!(report.overall_risk, RiskLevel::Safe);
    }

    #[test]
    fn a_sandwich_in_a_block_is_found() {
        let engine = SecurityEngine::new();
        let txs = vec![
            swap_tx("0xattacker", "1000", "1"),
            swap_tx("0xvictim", "500", "1"),
            swap_tx("0xattacker", "1000", "1"),
        ];

        let findings = engine.analyze_block(&txs);
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].victim, "0xvictim");
    }

    #[test]
    fn token_metadata_impersonation_is_reported() {
        let engine = SecurityEngine::new();
        let report = engine.analyze_token(CLEAN, Some("Totally Legit"), Some("WETH"), None);

        assert!(report.scam_score >= 0.8);
        assert!(report.warnings.iter().any(|w| w.contains("metadata")));
    }

    #[test]
    fn a_genuine_wrapped_token_is_not_reported() {
        let engine = SecurityEngine::new();
        let report = engine.analyze_token(CLEAN, Some("Wrapped Ether"), Some("WETH"), None);

        assert_eq!(report.scam_score, 0.0);
    }

    #[test]
    fn a_non_erc20_contract_is_disclosed() {
        let engine = SecurityEngine::new();
        let code = hex::encode([0x00u8; 8]);
        let report = engine.analyze_token(CLEAN, None, None, Some(&code));

        assert!(report
            .warnings
            .iter()
            .any(|w| w.contains("does not expose the full ERC-20 interface")));
    }

    #[test]
    fn block_observations_drive_the_anomaly_score() {
        let mut engine = SecurityEngine::new();
        for _ in 0..10 {
            engine.observe_block(0, 100.0, 10.0, 50);
        }

        assert_eq!(engine.anomaly_report().score, 0.0);

        engine.observe_block(0, 100_000.0, 10.0, 50);
        assert!(engine.anomaly_report().score > 0.0);
    }

    #[test]
    fn a_domain_can_be_blocked_and_is_then_reported() {
        let mut engine = SecurityEngine::new();
        assert_eq!(engine.blocked_domain_count(), 0);

        engine.block_domain("evil.example");
        assert_eq!(engine.blocked_domain_count(), 1);

        let report = engine.analyze_address("evil.example");
        assert_eq!(report.phishing_score, 1.0);
        assert_eq!(report.overall_risk, RiskLevel::Critical);
    }

    #[test]
    fn the_engine_starts_with_every_list_empty() {
        let engine = SecurityEngine::new();

        assert_eq!(engine.blacklist_size(), 0);
        assert_eq!(engine.reported_scam_count(), 0);
        assert_eq!(engine.reported_honeypot_count(), 0);
        assert_eq!(engine.blocked_domain_count(), 0);
    }
}
