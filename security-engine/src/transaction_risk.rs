//! Transaction Risk Analyzer Module
//! 
//! Analyzes transaction risk including MEV, front-running, and unusual patterns.

use serde::{Deserialize, Serialize};

/// Transaction risk assessment
#[derive(Debug, Clone, Serialize, Deserialize)]
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

/// Transaction data for analysis
#[derive(Debug, Clone)]
pub struct TransactionData {
    pub from: String,
    pub to: String,
    pub value: String,
    pub gas_price: String,
    pub gas_limit: u64,
    pub timestamp: i64,
    pub data: Option<String>,
    pub block_number: u64,
}

/// Transaction risk analyzer
pub struct TransactionRiskAnalyzer {
    threshold_high_gas: u64,
    threshold_sandwich: f64,
}

impl TransactionRiskAnalyzer {
    pub fn new() -> Self {
        Self {
            threshold_high_gas: 100_000_000_000, // 100 Gwei
            threshold_sandwich: 0.5,
        }
    }

    /// Analyze transaction risk
    pub fn analyze(&self, tx: &TransactionData) -> RiskAssessment {
        let mut details = Vec::new();
        let mut mev_risk = 0.0;
        let mut front_run_risk = 0.0;
        let mut sandwich_risk = 0.0;
        let mut unusual_pattern_risk = 0.0;
        let mut gas_anomaly_risk = 0.0;
        let mut timing_anomaly_risk = 0.0;

        // Check gas price anomaly
        if let Ok(gas_price) = tx.gas_price.parse::<u64>() {
            if gas_price > self.threshold_high_gas {
                gas_anomaly_risk = 0.8;
                details.push(format!("High gas price: {} Gwei", gas_price / 1e9));
            } else if gas_price > self.threshold_high_gas / 2 {
                gas_anomaly_risk = 0.3;
            }
        }

        // Check for swap transaction (common MEV target)
        if let Some(data) = &tx.data {
            let data_lower = data.to_lowercase();
            if data_lower.starts_with("0x095ea7b3") // approve
                || data_lower.starts_with("0x7ff36ab5") // swapExactETHForTokens
                || data_lower.starts_with("0x38ed1739") // swapExactTokensForETH
            {
                front_run_risk = 0.4;
                mev_risk = 0.3;
                details.push("DEX swap transaction detected".to_string());
            }
        }

        // Check for large value transfer
        if let Ok(value) = tx.value.parse::<u64>() {
            if value > 1_000_000_000_000_000_0000u64 { // > 1000 ETH
                unusual_pattern_risk = 0.7;
                details.push("Large value transfer detected".to_string());
            }
        }

        // Calculate overall risk
        let overall_risk = (mev_risk + front_run_risk + sandwich_risk + unusual_pattern_risk + gas_anomaly_risk + timing_anomaly_risk) / 6.0;

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

    /// Analyze multiple transactions for sandwich attack
    pub fn analyze_sandwich(&self, txs: &[TransactionData]) -> f64 {
        if txs.len() < 2 {
            return 0.0;
        }

        let mut sandwich_risk = 0.0;

        // Check for pattern: small tx surrounded by large txs
        for i in 1..txs.len() - 1 {
            let prev_value: u64 = txs[i - 1].value.parse().unwrap_or(0);
            let curr_value: u64 = txs[i].value.parse().unwrap_or(0);
            let next_value: u64 = txs[i + 1].value.parse().unwrap_or(0);

            if prev_value > curr_value * 10 && next_value > curr_value * 10 {
                sandwich_risk = 0.9;
            }
        }

        sandwich_risk
    }
}

impl Default for TransactionRiskAnalyzer {
    fn default() -> Self {
        Self::new()
    }
}