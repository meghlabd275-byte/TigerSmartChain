//! Transaction risk analysis.
//!
//! The previous version had several defects that made its numbers meaningless:
//!
//! - It parsed `value` as `u64`, but a wei value for one ETH is 10^18 and for
//!   large transfers exceeds `u64::MAX`, so the parse failed and the large-value
//!   check never fired. The literal it compared against,
//!   `1_000_000_000_000_000_0000`, is 10^19, not the 1000 ETH its comment
//!   claimed.
//! - `overall_risk` was the mean of six components. A transaction that is
//!   certainly a sandwich attack — `sandwich_risk = 1.0`, everything else 0 —
//!   scored 0.17, which is "safe". The mean hides a single severe signal.
//! - `analyze_sandwich` compared `value` between the transactions of a block,
//!   but a sandwich is defined by ordering around a victim *swap*, and the
//!   values involved are the attacker's, not the victim's.
//! - `front_run_risk`/`mev_risk` were set from the first four bytes of calldata
//!   alone, so any `approve` was "MEV".
//!
//! This version scores each component from a signal that supports it, combines
//! them with a noisy-OR so one certain signal dominates, and defines the
//! sandwich pattern by swap ordering.

use serde::{Deserialize, Serialize};

/// A risk assessment.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RiskAssessment {
    pub overall_risk: f64,
    pub mev_risk: f64,
    pub front_run_risk: f64,
    pub sandwich_risk: f64,
    pub unusual_pattern_risk: f64,
    pub gas_anomaly_risk: f64,
    pub timing_anomaly_risk: f64,
    pub details: Vec<String>,
}

/// Transaction data to assess.
#[derive(Debug, Clone, Default)]
pub struct TransactionData {
    pub from: String,
    pub to: String,
    /// Value in wei, as a decimal string. Parsed as `u128`.
    pub value: String,
    /// Gas price in wei, as a decimal string.
    pub gas_price: String,
    pub gas_limit: u64,
    pub timestamp: i64,
    /// Calldata, hex.
    pub data: Option<String>,
    pub block_number: u64,
}

/// 4-byte selectors that mark a swap or a liquidity change.
///
/// A front-run or sandwich can only target a trade, so these are the signals
/// that matter. `approve` is deliberately absent: approving a spender moves no
/// value and cannot be sandwiched on its own.
const SWAP_SELECTORS: &[([u8; 4], &str)] = &[
    ([0x38, 0xed, 0x17, 0x39], "swapExactTokensForTokens"),
    ([0x18, 0xcb, 0xaf, 0xe5], "swapExactETHForTokens"),
    ([0x7f, 0xf3, 0x6a, 0xb5], "swapExactETHForTokens (V1)"),
    ([0x88, 0x03, 0xdb, 0xee], "swapExactTokensForETH"),
    ([0x79, 0x1a, 0xc9, 0x47], "swapExactTokensForETH (V1)"),
    ([0xfb, 0x3b, 0xdb, 0x41], "swapExactTokensForTokensSupportingFeeOnTransferTokens"),
    ([0xb6, 0xf9, 0xde, 0x95], "swapExactETHForTokensSupportingFeeOnTransferTokens"),
    ([0x5c, 0x11, 0xd7, 0x95], "swapExactTokensForETHSupportingFeeOnTransferTokens"),
    ([0x88, 0x03, 0xdb, 0xee], "swapExactTokensForETH"),
    ([0x12, 0xaa, 0x3c, 0x8c], "swapExactTokensForTokens (V3)"),
    ([0x5a, 0xe4, 0x01, 0xdc], "exactInputSingle (V3)"),
    ([0xc0, 0x4b, 0x8d, 0x59], "exactInput (V3)"),
    ([0x41, 0x4b, 0xf3, 0x89], "exactOutputSingle (V3)"),
    ([0x09, 0x2f, 0x2a, 0x0d], "exactOutput (V3)"),
    ([0x02, 0x2c, 0x0d, 0x9f], "addLiquidity"),
    ([0xe8, 0xe3, 0x37, 0x00], "addLiquidityETH"),
    ([0xba, 0xa2, 0xab, 0x9d], "removeLiquidity"),
    ([0x02, 0x75, 0x1c, 0xd4], "removeLiquidityETH"),
];

/// The transaction risk analyzer.
pub struct TransactionRiskAnalyzer {
    /// Gas price above this is anomalous, in wei.
    threshold_high_gas: u128,
    /// A gas price this many times the recent median is an anomaly.
    gas_spike_multiple: f64,
}

impl TransactionRiskAnalyzer {
    /// An analyzer with default thresholds (100 gwei, 5x median).
    pub fn new() -> Self {
        Self {
            threshold_high_gas: 100_000_000_000,
            gas_spike_multiple: 5.0,
        }
    }

    /// Set the absolute high-gas threshold, in wei.
    pub fn with_high_gas_threshold(mut self, wei: u128) -> Self {
        self.threshold_high_gas = wei;
        self
    }

    /// Assess one transaction.
    ///
    /// `recent_median_gas` and `recent_median_value` are the medians over a
    /// recent window; when absent, only the absolute thresholds apply and the
    /// caller is told so in `details`.
    pub fn analyze_with_baseline(
        &self,
        tx: &TransactionData,
        recent_median_gas: Option<u128>,
        recent_median_value: Option<u128>,
    ) -> RiskAssessment {
        let mut details = Vec::new();

        let is_swap = self.swap_kind(&tx.data);
        let value = parse_u128(&tx.value);
        let gas_price = parse_u128(&tx.gas_price);

        // --- Gas anomaly -----------------------------------------------------
        let mut gas_anomaly_risk = 0.0;
        match gas_price {
            Some(price) if price > self.threshold_high_gas => {
                gas_anomaly_risk = 0.8;
                details.push(format!(
                    "Gas price {:.2} gwei is above the {:.2} gwei ceiling",
                    wei_to_gwei(price),
                    wei_to_gwei(self.threshold_high_gas)
                ));
            }
            Some(price) => {
                if let Some(median) = recent_median_gas.filter(|m| *m > 0) {
                    let multiple = price as f64 / median as f64;
                    if multiple >= self.gas_spike_multiple {
                        // Scale from 0 at the multiple to 0.7 at 4x the multiple.
                        gas_anomaly_risk =
                            (0.3 + 0.4 * ((multiple - self.gas_spike_multiple)
                                / (3.0 * self.gas_spike_multiple))
                                .clamp(0.0, 1.0))
                            .min(0.7);
                        details.push(format!(
                            "Gas price is {multiple:.1}x the recent median, consistent with a \
                             priority bid for ordering"
                        ));
                    }
                }
            }
            None => details.push("Gas price could not be parsed; gas risk not assessed".to_string()),
        }

        // --- MEV and front-running -------------------------------------------
        let mut mev_risk = 0.0;
        let mut front_run_risk = 0.0;

        if let Some(kind) = is_swap {
            details.push(format!("{kind} call: a trade that can be front-run"));

            // A trade is front-runnable when the sender paid above the going
            // rate to be ordered early.
            if let (Some(price), Some(median)) = (gas_price, recent_median_gas.filter(|m| *m > 0)) {
                let multiple = price as f64 / median as f64;
                if multiple >= 2.0 {
                    front_run_risk = (0.3 + 0.5 * ((multiple - 2.0) / 8.0).clamp(0.0, 1.0)).min(0.8);
                    details.push(format!(
                        "Trade pays {multiple:.1}x the median gas, indicating a bid for priority"
                    ));
                }
            }

            // MEV exposure scales with the size of the trade.
            if let Some(value) = value {
                mev_risk = match value {
                    v if v >= 100 * ONE_ETHER => 0.7,
                    v if v >= 10 * ONE_ETHER => 0.5,
                    v if v >= ONE_ETHER => 0.3,
                    _ => 0.1,
                };
                if mev_risk >= 0.3 {
                    details.push(format!(
                        "Trade moves {:.2} native units, worth sandwiching",
                        value as f64 / ONE_ETHER as f64
                    ));
                }
            }
        }

        // --- Unusual value ----------------------------------------------------
        let mut unusual_pattern_risk: f64 = 0.0;
        if let Some(value) = value {
            let absolute = if value >= 1_000 * ONE_ETHER {
                unusual_pattern_risk = 0.7;
                details.push(format!(
                    "Large transfer: {:.2} native units",
                    value as f64 / ONE_ETHER as f64
                ));
                0.7
            } else {
                0.0
            };

            if let Some(median) = recent_median_value.filter(|m| *m > 0) {
                let multiple = value as f64 / median as f64;
                if multiple >= 50.0 {
                    let relative = (0.4 + 0.3 * ((multiple - 50.0) / 950.0).clamp(0.0, 1.0)).min(0.7);
                    unusual_pattern_risk = unusual_pattern_risk.max(relative);
                    details.push(format!("Transfer is {multiple:.0}x the recent median"));
                }
            }

            unusual_pattern_risk = unusual_pattern_risk.max(absolute);
        } else {
            details.push("Value could not be parsed; value risk not assessed".to_string());
        }

        // --- Timing ------------------------------------------------------------
        // A transaction cannot be scored for timing against a baseline without
        // knowing the sender's history, so this is reported only when the
        // caller supplies a timestamp and it is in the future.
        let mut timing_anomaly_risk = 0.0;
        if tx.timestamp > 0 {
            let now = now_unix();
            if tx.timestamp > now + 60 {
                timing_anomaly_risk = 0.5;
                details.push("Transaction timestamp is in the future".to_string());
            }
        }

        // Sandwich risk needs a block context; a single transaction cannot show
        // the pattern. `analyze_block` fills this in.
        let sandwich_risk = 0.0;

        let overall_risk = combine(&[
            mev_risk,
            front_run_risk,
            sandwich_risk,
            unusual_pattern_risk,
            gas_anomaly_risk,
            timing_anomaly_risk,
        ]);

        RiskAssessment {
            overall_risk,
            mev_risk,
            front_run_risk,
            sandwich_risk,
            unusual_pattern_risk,
            gas_anomaly_risk,
            timing_anomaly_risk,
            details,
        }
    }

    /// Assess one transaction against no baseline.
    pub fn analyze(&self, tx: &TransactionData) -> RiskAssessment {
        self.analyze_with_baseline(tx, None, None)
    }

    /// Analyze a block's transactions for sandwich attacks.
    ///
    /// A sandwich is two swaps by the same attacker around a victim's swap of
    /// the same pair, with the attacker buying before and selling after. The
    /// detector requires three swaps in sequence: attacker, victim, attacker,
    /// with the two attacker transactions from the same sender and the middle
    /// one from a different sender.
    pub fn analyze_block(&self, txs: &[TransactionData]) -> Vec<SandwichFinding> {
        let mut findings = Vec::new();

        if txs.len() < 3 {
            return findings;
        }

        let swaps: Vec<usize> = txs
            .iter()
            .enumerate()
            .filter(|(_, tx)| self.swap_kind(&tx.data).is_some())
            .map(|(index, _)| index)
            .collect();

        if swaps.len() < 3 {
            return findings;
        }

        for window in swaps.windows(3) {
            let (front, victim, back) = (window[0], window[1], window[2]);

            let front_tx = &txs[front];
            let victim_tx = &txs[victim];
            let back_tx = &txs[back];

            // The attacker is the same account on both ends, and is not the
            // victim. Address comparison is case-insensitive.
            let same_attacker = front_tx.from.eq_ignore_ascii_case(&back_tx.from);
            let distinct_victim = !victim_tx.from.eq_ignore_ascii_case(&front_tx.from);

            if !same_attacker || !distinct_victim {
                continue;
            }

            // All three must touch the same router or pair.
            if !front_tx.to.eq_ignore_ascii_case(&victim_tx.to)
                || !back_tx.to.eq_ignore_ascii_case(&victim_tx.to)
            {
                continue;
            }

            let front_value = parse_u128(&front_tx.value).unwrap_or(0);
            let back_value = parse_u128(&back_tx.value).unwrap_or(0);
            let victim_value = parse_u128(&victim_tx.value).unwrap_or(0);

            // The victim has to be worth sandwiching relative to the attacker's
            // own exposure; a dust trade is not a target.
            let worth_targeting = victim_value > 0
                && (front_value + back_value) > 0
                && victim_value as f64 >= (front_value + back_value) as f64 * 0.01;

            findings.push(SandwichFinding {
                front_run_index: front,
                victim_index: victim,
                back_run_index: back,
                attacker: front_tx.from.clone(),
                victim: victim_tx.from.clone(),
                target: victim_tx.to.clone(),
                victim_value: victim_value.to_string(),
                confidence: if worth_targeting { 0.85 } else { 0.5 },
            });
        }

        findings
    }

    /// The swap kind when the calldata begins with a known swap selector.
    pub fn swap_kind(&self, data: &Option<String>) -> Option<&'static str> {
        let data = data.as_deref()?;
        let trimmed = data.trim();
        let digits = trimmed
            .strip_prefix("0x")
            .or_else(|| trimmed.strip_prefix("0X"))
            .unwrap_or(trimmed);

        if digits.len() < 8 {
            return None;
        }

        let selector = [
            u8::from_str_radix(&digits[0..2], 16).ok()?,
            u8::from_str_radix(&digits[2..4], 16).ok()?,
            u8::from_str_radix(&digits[4..6], 16).ok()?,
            u8::from_str_radix(&digits[6..8], 16).ok()?,
        ];

        SWAP_SELECTORS
            .iter()
            .find(|(known, _)| *known == selector)
            .map(|(_, name)| *name)
    }
}

impl Default for TransactionRiskAnalyzer {
    fn default() -> Self {
        Self::new()
    }
}

/// A detected sandwich.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SandwichFinding {
    pub front_run_index: usize,
    pub victim_index: usize,
    pub back_run_index: usize,
    pub attacker: String,
    pub victim: String,
    pub target: String,
    pub victim_value: String,
    pub confidence: f64,
}

/// One wei expressed as the number of wei in one native unit (10^18).
pub const ONE_ETHER: u128 = 1_000_000_000_000_000_000;

/// Combine independent risk signals.
///
/// Each signal is a probability that a distinct problem is present. The chance
/// that at least one is present is `1 - Π(1 - p)`, so a single certain signal
/// yields 1.0 rather than being diluted by five zeros.
pub fn combine(signals: &[f64]) -> f64 {
    let mut safe = 1.0f64;
    for signal in signals {
        let p = signal.clamp(0.0, 1.0);
        safe *= 1.0 - p;
    }
    (1.0 - safe).clamp(0.0, 1.0)
}

/// Parse a decimal string into a `u128`, tolerating a leading `0x`.
pub fn parse_u128(input: &str) -> Option<u128> {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return None;
    }

    if let Some(hex) = trimmed
        .strip_prefix("0x")
        .or_else(|| trimmed.strip_prefix("0X"))
    {
        u128::from_str_radix(hex, 16).ok()
    } else {
        trimmed.parse::<u128>().ok()
    }
}

fn wei_to_gwei(wei: u128) -> f64 {
    wei as f64 / 1e9
}

fn now_unix() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tx(from: &str, to: &str, value: &str, gas: &str, data: Option<&str>) -> TransactionData {
        TransactionData {
            from: from.to_string(),
            to: to.to_string(),
            value: value.to_string(),
            gas_price: gas.to_string(),
            gas_limit: 21000,
            timestamp: 0,
            data: data.map(|d| d.to_string()),
            block_number: 1,
        }
    }

    #[test]
    fn a_plain_transfer_is_low_risk() {
        let analyzer = TransactionRiskAnalyzer::new();
        let assessment = analyzer.analyze(&tx("0xa", "0xb", "1000000000000000", "1000000000", None));

        assert!(assessment.overall_risk < 0.2, "{assessment:?}");
    }

    #[test]
    fn a_large_value_transfer_is_flagged() {
        let analyzer = TransactionRiskAnalyzer::new();
        // 5000 native units, which is 5 * 10^21 wei: far beyond u64::MAX, so
        // the old u64 parse would have failed silently.
        let value = format!("{}", 5000 * ONE_ETHER);
        let assessment = analyzer.analyze(&tx("0xa", "0xb", &value, "1000000000", None));

        assert!(assessment.unusual_pattern_risk >= 0.7, "{assessment:?}");
    }

    #[test]
    fn a_value_larger_than_u64_is_parsed_correctly() {
        // 10^21 wei does not fit in u64; the old analyzer's parse failed here.
        assert_eq!(parse_u128("1000000000000000000000"), Some(1_000_000_000_000_000_000_000));
        assert!(parse_u128("1000000000000000000000").unwrap() > u64::MAX as u128);
    }

    #[test]
    fn a_high_gas_price_is_flagged() {
        let analyzer = TransactionRiskAnalyzer::new();
        // 200 gwei, above the 100 gwei ceiling.
        let assessment = analyzer.analyze(&tx("0xa", "0xb", "1", "200000000000", None));

        assert!(assessment.gas_anomaly_risk >= 0.8, "{assessment:?}");
    }

    #[test]
    fn a_gas_spike_against_the_median_is_flagged() {
        let analyzer = TransactionRiskAnalyzer::new();
        let assessment = analyzer.analyze_with_baseline(
            &tx("0xa", "0xb", "1", "30000000000", None), // 30 gwei
            Some(1_000_000_000),                          // median 1 gwei
            None,
        );

        assert!(assessment.gas_anomaly_risk > 0.0, "{assessment:?}");
    }

    #[test]
    fn a_swap_is_recognised_from_its_selector() {
        let analyzer = TransactionRiskAnalyzer::new();

        assert_eq!(
            analyzer.swap_kind(&Some("0x38ed1739000000000000000000000000".to_string())),
            Some("swapExactTokensForTokens")
        );
        assert_eq!(
            analyzer.swap_kind(&Some("0x5ae401dc00000000".to_string())),
            Some("exactInputSingle (V3)")
        );
        assert_eq!(analyzer.swap_kind(&None), None);
        assert_eq!(analyzer.swap_kind(&Some("0x".to_string())), None);
        assert_eq!(analyzer.swap_kind(&Some("0x1234".to_string())), None);
    }

    #[test]
    fn an_approve_is_not_treated_as_a_swap() {
        // The old analyzer set mev_risk and front_run_risk for approve.
        let analyzer = TransactionRiskAnalyzer::new();
        let approve = "0x095ea7b30000000000000000000000000000000000000000000000000000000000000001";

        assert_eq!(analyzer.swap_kind(&Some(approve.to_string())), None);

        let assessment = analyzer.analyze(&tx("0xa", "0xb", "0", "1000000000", Some(approve)));
        assert_eq!(assessment.mev_risk, 0.0);
        assert_eq!(assessment.front_run_risk, 0.0);
    }

    #[test]
    fn a_high_gas_swap_raises_front_run_risk() {
        let analyzer = TransactionRiskAnalyzer::new();
        let assessment = analyzer.analyze_with_baseline(
            &tx(
                "0xa",
                "0xb",
                &(10 * ONE_ETHER).to_string(),
                "50000000000", // 50 gwei
                Some("0x38ed1739"),
            ),
            Some(1_000_000_000), // median 1 gwei, so 50x
            None,
        );

        assert!(assessment.front_run_risk > 0.3, "{assessment:?}");
        assert!(assessment.mev_risk >= 0.3, "{assessment:?}");
    }

    #[test]
    fn one_certain_signal_is_not_diluted_by_the_others() {
        // The old mean-based scoring gave a certain sandwich 0.17.
        assert_eq!(combine(&[0.0, 0.0, 1.0, 0.0, 0.0, 0.0]), 1.0);

        let two = combine(&[0.5, 0.5, 0.0, 0.0, 0.0, 0.0]);
        assert!((two - 0.75).abs() < 1e-9, "{two}");

        assert_eq!(combine(&[0.0; 6]), 0.0);
    }

    #[test]
    fn a_sandwich_is_detected_from_ordering() {
        let analyzer = TransactionRiskAnalyzer::new();
        let txs = vec![
            tx("0xattacker", "0xrouter", &(10 * ONE_ETHER).to_string(), "1", Some("0x38ed1739")),
            tx("0xvictim", "0xrouter", &(5 * ONE_ETHER).to_string(), "1", Some("0x38ed1739")),
            tx("0xattacker", "0xrouter", &(10 * ONE_ETHER).to_string(), "1", Some("0x38ed1739")),
        ];

        let findings = analyzer.analyze_block(&txs);
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].attacker, "0xattacker");
        assert_eq!(findings[0].victim, "0xvictim");
        assert!(findings[0].confidence >= 0.85);
    }

    #[test]
    fn three_unrelated_swaps_are_not_a_sandwich() {
        let analyzer = TransactionRiskAnalyzer::new();
        let txs = vec![
            tx("0xa", "0xrouter", "1", "1", Some("0x38ed1739")),
            tx("0xb", "0xrouter", "1", "1", Some("0x38ed1739")),
            tx("0xc", "0xrouter", "1", "1", Some("0x38ed1739")),
        ];

        assert!(analyzer.analyze_block(&txs).is_empty());
    }

    #[test]
    fn the_same_account_trading_twice_is_not_a_sandwich() {
        let analyzer = TransactionRiskAnalyzer::new();
        let txs = vec![
            tx("0xa", "0xrouter", "1", "1", Some("0x38ed1739")),
            tx("0xa", "0xrouter", "1", "1", Some("0x38ed1739")),
            tx("0xa", "0xrouter", "1", "1", Some("0x38ed1739")),
        ];

        assert!(analyzer.analyze_block(&txs).is_empty());
    }

    #[test]
    fn swaps_on_different_targets_are_not_a_sandwich() {
        let analyzer = TransactionRiskAnalyzer::new();
        let txs = vec![
            tx("0xattacker", "0xrouterA", "10", "1", Some("0x38ed1739")),
            tx("0xvictim", "0xrouterB", "5", "1", Some("0x38ed1739")),
            tx("0xattacker", "0xrouterA", "10", "1", Some("0x38ed1739")),
        ];

        assert!(analyzer.analyze_block(&txs).is_empty());
    }

    #[test]
    fn fewer_than_three_transactions_cannot_be_a_sandwich() {
        let analyzer = TransactionRiskAnalyzer::new();
        assert!(analyzer.analyze_block(&[]).is_empty());
        assert!(analyzer
            .analyze_block(&[tx("0xa", "0xb", "1", "1", Some("0x38ed1739"))])
            .is_empty());
    }

    #[test]
    fn an_unparseable_value_is_reported_rather_than_scored_as_zero() {
        let analyzer = TransactionRiskAnalyzer::new();
        let assessment = analyzer.analyze(&tx("0xa", "0xb", "not a number", "1000000000", None));

        assert!(assessment
            .details
            .iter()
            .any(|d| d.contains("Value could not be parsed")));
    }

    #[test]
    fn a_future_timestamp_is_flagged() {
        let analyzer = TransactionRiskAnalyzer::new();
        let mut transaction = tx("0xa", "0xb", "1", "1", None);
        transaction.timestamp = now_unix() + 3600;

        let assessment = analyzer.analyze(&transaction);
        assert!(assessment.timing_anomaly_risk > 0.0);
    }

    #[test]
    fn hex_and_decimal_amounts_both_parse() {
        assert_eq!(parse_u128("0x10"), Some(16));
        assert_eq!(parse_u128("16"), Some(16));
        assert_eq!(parse_u128("0X10"), Some(16));
        assert_eq!(parse_u128(""), None);
        assert_eq!(parse_u128("12.5"), None);
    }
}
