//! Anomaly detection over a stream of block statistics.
//!
//! The previous version compared the last entry to the mean of the whole
//! window, including the last entry itself. On a short window a single spike
//! raises the mean enough to hide itself: with two entries of 100 and 1000, the
//! mean is 550 and the ratio is 1.8, far below the 10x threshold, so the spike
//! was never reported. It also divided by the average gas without checking for
//! zero, producing `inf` and then `NaN` severities.
//!
//! This version compares the current value against the median of the *prior*
//! window, which a single outlier cannot move, and guards every division.

use serde::{Deserialize, Serialize};
use std::collections::VecDeque;

/// An anomaly report.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AnomalyReport {
    pub score: f64,
    pub anomalies: Vec<Anomaly>,
    pub pattern_type: Option<PatternType>,
}

impl AnomalyReport {
    /// An empty report.
    pub fn none() -> Self {
        Self {
            score: 0.0,
            anomalies: Vec::new(),
            pattern_type: None,
        }
    }
}

/// A detected anomaly.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Anomaly {
    pub anomaly_type: AnomalyType,
    pub severity: f64,
    pub description: String,
    pub details: Vec<String>,
}

/// Kinds of anomaly.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AnomalyType {
    UnusualVolume,
    UnusualGas,
    RapidTransactions,
    DormantAccount,
    FlashLoan,
    WhaleMovement,
    SmartMoney,
    ContractCreation,
    SelfDestruct,
    UnverifiedContract,
    Unknown,
}

/// Patterns inferred from the shape of the activity.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PatternType {
    Whale,
    Miner,
    Arbitrage,
    Liquidation,
    Airdrop,
    Distribution,
    Accumulation,
    Unknown,
}

/// One observation.
#[derive(Debug, Clone, PartialEq)]
pub struct HistoryEntry {
    pub timestamp: i64,
    pub volume: f64,
    pub gas: f64,
    pub tx_count: usize,
}

/// The anomaly detector.
pub struct AnomalyDetector {
    history: VecDeque<HistoryEntry>,
    max_history: usize,
    /// A value this many times the prior median is an anomaly.
    volume_multiple: f64,
    gas_multiple: f64,
    tx_multiple: f64,
    /// How many prior entries are needed before a comparison is made.
    min_baseline: usize,
}

impl AnomalyDetector {
    /// A detector with defaults: 10x volume, 5x gas, 5x transaction count.
    pub fn new() -> Self {
        Self {
            history: VecDeque::new(),
            max_history: 1000,
            volume_multiple: 10.0,
            gas_multiple: 5.0,
            tx_multiple: 5.0,
            min_baseline: 3,
        }
    }

    /// Set how many prior entries are needed before comparing.
    pub fn with_min_baseline(mut self, entries: usize) -> Self {
        self.min_baseline = entries;
        self
    }

    /// Append an observation, evicting the oldest beyond the cap.
    pub fn add_entry(&mut self, timestamp: i64, volume: f64, gas: f64, tx_count: usize) {
        self.history.push_back(HistoryEntry {
            timestamp,
            volume,
            gas,
            tx_count,
        });

        while self.history.len() > self.max_history {
            self.history.pop_front();
        }
    }

    /// Number of observations held.
    pub fn len(&self) -> usize {
        self.history.len()
    }

    /// Whether no observations are held.
    pub fn is_empty(&self) -> bool {
        self.history.is_empty()
    }

    /// Detect anomalies in the most recent observation.
    ///
    /// The baseline is the median of every entry *before* the last, so the entry
    /// being judged cannot influence its own comparison.
    pub fn detect(&self) -> AnomalyReport {
        if self.history.len() < self.min_baseline + 1 {
            return AnomalyReport::none();
        }

        let current = self.history.back().expect("non-empty");
        let prior: Vec<&HistoryEntry> = self.history.iter().take(self.history.len() - 1).collect();

        let median_volume = median(prior.iter().map(|e| e.volume));
        let median_gas = median(prior.iter().map(|e| e.gas));
        let median_tx = median(prior.iter().map(|e| e.tx_count as f64));

        let mut anomalies = Vec::new();
        let mut score: f64 = 0.0;
        let mut pattern_type = None;

        // --- Volume ---------------------------------------------------------
        if let Some(baseline) = median_volume.filter(|b| *b > 0.0) {
            let ratio = current.volume / baseline;
            if ratio >= self.volume_multiple {
                let severity = severity_for(ratio, self.volume_multiple, 0.9);
                anomalies.push(Anomaly {
                    anomaly_type: AnomalyType::WhaleMovement,
                    severity,
                    description: "Volume spike against the recent median".to_string(),
                    details: vec![format!(
                        "{:.2} against a median of {:.2} ({ratio:.1}x)",
                        current.volume, baseline
                    )],
                });
                score = score.max(severity);
                pattern_type = Some(PatternType::Whale);
            }
        }

        // --- Transaction count ----------------------------------------------
        if let Some(baseline) = median_tx.filter(|b| *b > 0.0) {
            let ratio = current.tx_count as f64 / baseline;
            if ratio >= self.tx_multiple {
                let severity = severity_for(ratio, self.tx_multiple, 0.7);
                anomalies.push(Anomaly {
                    anomaly_type: AnomalyType::RapidTransactions,
                    severity,
                    description: "Transaction-count spike against the recent median".to_string(),
                    details: vec![format!(
                        "{} transactions against a median of {:.0} ({ratio:.1}x)",
                        current.tx_count, baseline
                    )],
                });
                score = score.max(severity);
            }
        }

        // --- Gas ------------------------------------------------------------
        if let Some(baseline) = median_gas.filter(|b| *b > 0.0) {
            let ratio = current.gas / baseline;
            if ratio >= self.gas_multiple {
                let severity = severity_for(ratio, self.gas_multiple, 0.6);
                anomalies.push(Anomaly {
                    anomaly_type: AnomalyType::UnusualGas,
                    severity,
                    description: "Gas-price spike against the recent median".to_string(),
                    details: vec![format!(
                        "{:.2} against a median of {:.2} ({ratio:.1}x)",
                        current.gas, baseline
                    )],
                });
                score = score.max(severity);
            }
        }

        // --- Distribution shape ----------------------------------------------
        // A burst of small transactions with a flat volume is an airdrop or a
        // distribution; a burst of large ones is accumulation.
        if let Some(baseline_tx) = median_tx.filter(|b| *b > 0.0) {
            if pattern_type.is_none() && current.tx_count as f64 >= baseline_tx * self.tx_multiple {
                let per_tx = if current.tx_count > 0 {
                    current.volume / current.tx_count as f64
                } else {
                    0.0
                };

                // A typical entry's value per transaction, for comparison.
                let typical_per_tx = prior
                    .iter()
                    .filter(|e| e.tx_count > 0)
                    .map(|e| e.volume / e.tx_count as f64)
                    .fold(f64::NAN, f64::max);

                pattern_type = Some(
                    if per_tx.is_finite() && typical_per_tx.is_finite() && per_tx < typical_per_tx {
                        PatternType::Airdrop
                    } else {
                        PatternType::Accumulation
                    },
                );
            }
        }

        AnomalyReport {
            score,
            anomalies,
            pattern_type,
        }
    }

    /// Whether a specific pattern is present in the latest observation.
    pub fn detect_pattern(&self, pattern: PatternType) -> bool {
        self.detect().pattern_type == Some(pattern)
    }

    /// Whether the recent gas readings are trending upward.
    ///
    /// Uses the median of the first and second halves of the window, so a
    /// single outlier does not create a trend.
    pub fn gas_is_rising(&self) -> bool {
        if self.history.len() < 4 {
            return false;
        }

        let entries: Vec<&HistoryEntry> = self.history.iter().collect();
        let middle = entries.len() / 2;
        let first_half = median(entries[..middle].iter().map(|e| e.gas));
        let second_half = median(entries[middle..].iter().map(|e| e.gas));

        match (first_half, second_half) {
            (Some(first), Some(second)) if first > 0.0 => second / first >= 2.0,
            _ => false,
        }
    }
}

impl Default for AnomalyDetector {
    fn default() -> Self {
        Self::new()
    }
}

/// The median of a set of values, ignoring non-finite ones.
///
/// Returns `None` for an empty set. An even-sized set takes the lower of the
/// two middle values, so the result is always an observed value.
pub fn median(values: impl Iterator<Item = f64>) -> Option<f64> {
    let mut finite: Vec<f64> = values.filter(|v| v.is_finite()).collect();
    if finite.is_empty() {
        return None;
    }

    finite.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    Some(finite[(finite.len() - 1) / 2])
}

/// Severity for a ratio above a threshold.
///
/// The severity rises from half the maximum at the threshold to the maximum at
/// ten times the threshold, so a marginal breach is reported as a marginal
/// anomaly rather than as a maximum-severity one.
fn severity_for(ratio: f64, threshold: f64, maximum: f64) -> f64 {
    if !ratio.is_finite() || threshold <= 0.0 {
        return 0.0;
    }

    let excess = (ratio - threshold) / (9.0 * threshold);
    (maximum * (0.5 + 0.5 * excess.clamp(0.0, 1.0))).clamp(0.0, maximum)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A detector primed with a flat baseline.
    fn primed(volume: f64, gas: f64, tx_count: usize) -> AnomalyDetector {
        let mut detector = AnomalyDetector::new();
        for _ in 0..10 {
            detector.add_entry(0, volume, gas, tx_count);
        }
        detector
    }

    #[test]
    fn a_flat_series_has_no_anomalies() {
        let mut detector = primed(100.0, 10.0, 50);
        detector.add_entry(0, 100.0, 10.0, 50);

        let report = detector.detect();
        assert!(report.anomalies.is_empty(), "{report:?}");
        assert_eq!(report.score, 0.0);
    }

    #[test]
    fn a_single_spike_is_detected_in_a_short_window() {
        // The old mean-based test could not detect this: with two entries of
        // 100 and 1000 the mean is 550, so the ratio was only 1.8.
        let mut detector = AnomalyDetector::new();
        detector.add_entry(0, 100.0, 10.0, 50);
        detector.add_entry(0, 100.0, 10.0, 50);
        detector.add_entry(0, 100.0, 10.0, 50);
        detector.add_entry(0, 1000.0, 10.0, 50);

        let report = detector.detect();
        assert_eq!(report.anomalies.len(), 1);
        assert_eq!(report.anomalies[0].anomaly_type, AnomalyType::WhaleMovement);
        assert!(report.anomalies[0].severity > 0.0);
    }

    #[test]
    fn the_median_is_not_moved_by_an_outlier() {
        assert_eq!(median([1.0, 2.0, 3.0, 4.0, 1000.0].into_iter()), Some(3.0));
        assert_eq!(median([5.0].into_iter()), Some(5.0));
        assert_eq!(median(std::iter::empty()), None);
    }

    #[test]
    fn the_median_ignores_non_finite_values() {
        assert_eq!(median([1.0, f64::NAN, 3.0].into_iter()), Some(1.0));
        assert_eq!(median([f64::INFINITY].into_iter()), None);
    }

    #[test]
    fn a_zero_baseline_does_not_produce_a_division_by_zero() {
        // The old code divided by the average gas unconditionally.
        let mut detector = AnomalyDetector::new();
        for _ in 0..5 {
            detector.add_entry(0, 0.0, 0.0, 0);
        }
        detector.add_entry(0, 1000.0, 500.0, 10);

        let report = detector.detect();
        for anomaly in &report.anomalies {
            assert!(anomaly.severity.is_finite(), "{anomaly:?}");
        }
        assert!(report.score.is_finite());
    }

    #[test]
    fn too_short_a_history_reports_nothing() {
        let mut detector = AnomalyDetector::new();
        detector.add_entry(0, 1000.0, 10.0, 50);

        assert_eq!(detector.detect(), AnomalyReport::none());
    }

    #[test]
    fn a_gas_spike_is_detected() {
        let mut detector = primed(100.0, 10.0, 50);
        detector.add_entry(0, 100.0, 500.0, 50);

        let report = detector.detect();
        assert!(report
            .anomalies
            .iter()
            .any(|a| a.anomaly_type == AnomalyType::UnusualGas));
    }

    #[test]
    fn a_transaction_count_spike_is_detected() {
        let mut detector = primed(100.0, 10.0, 50);
        detector.add_entry(0, 100.0, 10.0, 5000);

        let report = detector.detect();
        assert!(report
            .anomalies
            .iter()
            .any(|a| a.anomaly_type == AnomalyType::RapidTransactions));
    }

    #[test]
    fn a_volume_spike_sets_the_whale_pattern() {
        let mut detector = primed(100.0, 10.0, 50);
        detector.add_entry(0, 100_000.0, 10.0, 50);

        assert_eq!(detector.detect().pattern_type, Some(PatternType::Whale));
        assert!(detector.detect_pattern(PatternType::Whale));
    }

    #[test]
    fn severity_scales_with_the_size_of_the_breach() {
        let marginal = severity_for(10.0, 10.0, 0.9);
        let extreme = severity_for(100.0, 10.0, 0.9);

        assert!(marginal < extreme, "{marginal} vs {extreme}");
        assert!(extreme <= 0.9);
        assert!(marginal >= 0.45);
    }

    #[test]
    fn severity_is_finite_for_an_infinite_ratio() {
        assert!(severity_for(f64::INFINITY, 10.0, 0.9).is_finite());
    }

    #[test]
    fn the_history_is_capped() {
        let mut detector = AnomalyDetector::new();
        for i in 0..1200 {
            detector.add_entry(i, 1.0, 1.0, 1);
        }

        assert_eq!(detector.len(), 1000);
    }

    #[test]
    fn gas_is_rising_is_detected_only_for_a_sustained_increase() {
        let mut flat = AnomalyDetector::new();
        for _ in 0..10 {
            flat.add_entry(0, 1.0, 10.0, 1);
        }
        assert!(!flat.gas_is_rising());

        let mut rising = AnomalyDetector::new();
        for _ in 0..5 {
            rising.add_entry(0, 1.0, 10.0, 1);
        }
        for _ in 0..5 {
            rising.add_entry(0, 1.0, 100.0, 1);
        }
        assert!(rising.gas_is_rising());
    }

    #[test]
    fn a_single_gas_outlier_does_not_create_a_trend() {
        let mut detector = AnomalyDetector::new();
        for _ in 0..5 {
            detector.add_entry(0, 1.0, 10.0, 1);
        }
        detector.add_entry(0, 1.0, 100_000.0, 1); // one spike
        for _ in 0..4 {
            detector.add_entry(0, 1.0, 10.0, 1);
        }

        assert!(!detector.gas_is_rising());
    }

    #[test]
    fn a_burst_of_small_transactions_reads_as_an_airdrop() {
        let mut detector = AnomalyDetector::new();
        for _ in 0..10 {
            detector.add_entry(0, 1_000_000.0, 10.0, 10);
        }
        // Many more transactions but a similar total volume: the value per
        // transaction collapsed.
        detector.add_entry(0, 1_000_000.0, 10.0, 10_000);

        assert_eq!(detector.detect().pattern_type, Some(PatternType::Airdrop));
    }

    #[test]
    fn an_empty_detector_reports_nothing() {
        let detector = AnomalyDetector::new();
        assert!(detector.is_empty());
        assert_eq!(detector.detect(), AnomalyReport::none());
        assert!(!detector.gas_is_rising());
    }
}
