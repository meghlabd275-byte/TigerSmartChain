//! Anomaly Detection Module
//! 
//! Detects unusual patterns and behaviors in blockchain data.

use serde::{Deserialize, Serialize};
use std::collections::VecDeque;

/// Anomaly report
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnomalyReport {
    pub score: f64,
    pub anomalies: Vec<Anomaly>,
    pub pattern_type: Option<PatternType>,
}

/// Detected anomaly
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Anomaly {
    pub anomaly_type: AnomalyType,
    pub severity: f64,
    pub description: String,
    pub details: Vec<String>,
}

/// Anomaly types
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

/// Pattern types
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

/// Anomaly detector
pub struct AnomalyDetector {
    history: VecDeque<HistoryEntry>,
    max_history: usize,
}

#[derive(Debug, Clone)]
struct HistoryEntry {
    timestamp: i64,
    volume: f64,
    gas: f64,
    tx_count: usize,
}

impl AnomalyDetector {
    pub fn new() -> Self {
        Self {
            history: VecDeque::new(),
            max_history: 1000,
        }
    }

    /// Add entry to history
    pub fn add_entry(&mut self, timestamp: i64, volume: f64, gas: f64, tx_count: usize) {
        self.history.push_back(HistoryEntry {
            timestamp,
            volume,
            gas,
            tx_count,
        });

        if self.history.len() > self.max_history {
            self.history.pop_front();
        }
    }

    /// Detect anomalies
    pub fn detect(&self) -> AnomalyReport {
        let mut anomalies = Vec::new();
        let mut pattern_type = None;
        let mut score = 0.0;

        if self.history.is_empty() {
            return AnomalyReport {
                score: 0.0,
                anomalies: vec![],
                pattern_type: None,
            };
        }

        // Calculate averages
        let avg_volume: f64 = self.history.iter().map(|e| e.volume).sum::<f64>() / self.history.len() as f64;
        let avg_gas: f64 = self.history.iter().map(|e| e.gas).sum::<f64>() / self.history.len() as f64;
        let avg_tx: f64 = self.history.iter().map(|e| e.tx_count as f64).sum::<f64>() / self.history.len() as f64;

        // Check for unusual volume
        if let Some(last) = self.history.back() {
            if last.volume > avg_volume * 10.0 {
                anomalies.push(Anomaly {
                    anomaly_type: AnomalyType::WhaleMovement,
                    severity: 0.9,
                    description: "Unusual trading volume detected".to_string(),
                    details: vec![format!("Volume {}x above average", last.volume / avg_volume)],
                });
                score = 0.9;
                pattern_type = Some(PatternType::Whale);
            }

            if last.tx_count as f64 > avg_tx * 5.0 {
                anomalies.push(Anomaly {
                    anomaly_type: AnomalyType::RapidTransactions,
                    severity: 0.7,
                    description: "Rapid transaction spike".to_string(),
                    details: vec![format!("{} txs vs avg {}", last.tx_count, avg_tx)],
                });
                score = score.max(0.7);
            }

            if last.gas > avg_gas * 5.0 {
                anomalies.push(Anomaly {
                    anomaly_type: AnomalyType::UnusualGas,
                    severity: 0.6,
                    description: "Gas price spike".to_string(),
                    details: vec![format!("Gas {}x above average", last.gas / avg_gas)],
                });
                score = score.max(0.6);
            }
        }

        AnomalyReport {
            score,
            anomalies,
            pattern_type,
        }
    }

    /// Check for specific pattern
    pub fn detect_pattern(&self, pattern: PatternType) -> bool {
        match pattern {
            PatternType::Whale => {
                if let Some(last) = self.history.back() {
                    let avg_volume: f64 = self.history.iter().map(|e| e.volume).sum::<f64>() / self.history.len().max(1) as f64;
                    return last.volume > avg_volume * 10.0;
                }
            }
            PatternType::Liquidation => {
                // Check for large liquidations
                if let Some(last) = self.history.back() {
                    return last.tx_count as f64 > 100.0;
                }
            }
            _ => {}
        }
        false
    }
}

impl Default for AnomalyDetector {
    fn default() -> Self {
        Self::new()
    }
}