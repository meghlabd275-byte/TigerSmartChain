//! Security Center Implementation

use crate::types::*;
use chrono::Utc;
use std::sync::Arc;
use std::collections::HashMap;
use tokio::sync::RwLock;
use thiserror::Error;
use reqwest::Client;
use serde_json::{json, Value};
use std::time::Duration;

// =============================================================================
// ERRORS
// =============================================================================

#[derive(Error, Debug)]
pub enum SecurityError {
    #[error("RPC error: {0}")]
    RPCError(String),
    #[error("Analysis error: {0}")]
    AnalysisError(String),
    #[error("Detection error: {0}")]
    DetectionError(String),
}

// =============================================================================
// SERVICE
// =============================================================================

/// Security Center Service
pub struct SecurityCenter {
    config: SecurityConfig,
    client: Client,
    malicious_cache: Arc<RwLock<HashMap<String, bool>>>,
    stats: Arc<RwLock<SecurityStats>>,
}

impl SecurityCenter {
    /// Create new security center
    pub fn new(config: SecurityConfig) -> Self {
        let client = Client::builder()
            .timeout(Duration::from_secs(30))
            .build()
            .unwrap_or_else(|_| Client::new());
        
        Self {
            config: config.clone(),
            client,
            malicious_cache: Arc::new(RwLock::new(HashMap::new())),
            stats: Arc::new(RwLock::new(SecurityStats::default())),
        }
    }

    /// Analyze a transaction
    pub async fn analyze_transaction(&self, tx_hash: &str) -> Result<TransactionAnalysis, SecurityError> {
        // Get transaction and receipt
        let (tx, receipt) = self.get_transaction(tx_hash).await?;
        
        // Get traces
        let traces = self.get_traces(tx_hash).await.ok();
        
        // Analyze
        let mut threats = Vec::new();
        let mut token_transfers = Vec::new();
        let mut contract_calls = Vec::new();
        let mut is_suspicious = false;
        
        // Check if to address is malicious
        if let Some(to) = &tx.to {
            if self.is_malicious(to).await {
                threats.push(SecurityThreat {
                    threat_type: ThreatType::Phishing,
                    severity: ThreatSeverity::Critical,
                    description: "Transaction to known malicious contract".to_string(),
                    evidence: [("address".to_string(), to.clone())].into(),
                    recommendation: "Do not interact with this contract".to_string(),
                });
                is_suspicious = true;
            }
        }
        
        // Check for large transfer
        let value_wei = u128::from_str_radix(tx.value.trim_start_matches("0x"), 16).unwrap_or(0);
        let value_eth = value_wei as f64 / 1e18;
        
        if value_eth > 100.0 {
            threats.push(SecurityThreat {
                threat_type: ThreatType::Unknown,
                severity: ThreatSeverity::High,
                description: format!("Large transfer detected: {} ETH", value_eth),
                evidence: [("value".to_string(), tx.value.clone())].into(),
                recommendation: "Verify the recipient is trusted".to_string(),
            });
        }
        
        // Check for suspicious contract calls
        if let Some(input) = &tx.input {
            if input.len() > 10 {
                let method = format!("0x{}", &input[2..10]);
                
                // Check for suspicious methods
                let suspicious_methods = [
                    "0x095ea7b3", // approve
                    "0x23b872dd", // transferFrom
                    "0xa9059cbb", // transfer
                    "0x2e1a7d4d", // deposit
                    "0xd0e30db0", // deposit (ETH)
                    "0xf242dab3", // execute
                    "0x4ce6f06d", // sweep
                ];
                
                if suspicious_methods.contains(&method.as_str()) {
                    contract_calls.push(ContractCall {
                        to: tx.to.clone().unwrap_or_default(),
                        method: method.clone(),
                        params: HashMap::new(),
                    });
                }
            }
        }
        
        // Calculate risk score
        let risk_score = self.calculate_risk_score(&threats);
        
        // Determine status
        let status = match risk_score {
            0..=20 => AnalysisStatus::Safe,
            21..=50 => AnalysisStatus::Warning,
            51..=100 => AnalysisStatus::Dangerous,
            _ => AnalysisStatus::Unknown,
        };
        
        Ok(TransactionAnalysis {
            hash: tx_hash.to_string(),
            status,
            threats: threats.clone(),
            summary: if threats.is_empty() {
                "Transaction appears safe".to_string()
            } else {
                format!("{} threat(s) detected", threats.len())
            },
            risk_score,
            simulation_result: None,
            token_transfers,
            contract_calls,
            is_suspicious,
        })
    }

    /// Scan a contract for threats
    pub async fn scan_contract(&self, address: &str) -> Result<ContractScanResult, SecurityError> {
        let mut threats = Vec::new();
        
        // Update stats
        {
            let mut stats = self.stats.write().await;
            stats.contracts_scanned += 1;
        }
        
        // Check if malicious
        if self.is_malicious(address).await {
            threats.push(SecurityThreat {
                threat_type: ThreatType::Phishing,
                severity: ThreatSeverity::Critical,
                description: "Contract is flagged as malicious".to_string(),
                evidence: [("address".to_string(), address.to_string())].into(),
                recommendation: "Do not interact with this contract".to_string(),
            });
        }
        
        // Get contract code
        let code = self.get_code(address).await?;
        
        if code == "0x" {
            return Ok(ContractScanResult {
                address: address.to_string(),
                threats,
                is_verified: false,
                is_open_source: false,
                is_malicious: !threats.is_empty(),
                score: 100,
                audit_status: AuditStatus::None,
                last_updated: Utc::now().timestamp(),
            });
        }
        
        // Check for malicious patterns
        let code_lower = code.to_lowercase();
        
        // Check for honeypot patterns
        if code_lower.contains("0x094e5f1c") && code_lower.contains("0x5c60f5") {
            // Potential honeypot - blocks certain addresses
            threats.push(SecurityThreat {
                threat_type: ThreatType::Honeypot,
                severity: ThreatSeverity::High,
                description: "Contract may block certain addresses".to_string(),
                evidence: HashMap::new(),
                recommendation: "Verify contract behavior before interacting".to_string(),
            });
        }
        
        // Check for owner-only functions
        if code_lower.contains("0x00fdd58e") || code_lower.contains("0x8f2ecfd9") {
            threats.push(SecurityThreat {
                threat_type: ThreatType::Ownerable,
                severity: ThreatSeverity::Medium,
                description: "Contract has owner-only functions".to_string(),
                evidence: HashMap::new(),
                recommendation: "Check who owns the contract".to_string(),
            });
        }
        
        // Check for pausable
        if code_lower.contains("0x5d1f5c4e") || code_lower.contains("0x3f7062") {
            threats.push(SecurityThreat {
                threat_type: ThreatType::Pausable,
                severity: ThreatSeverity::Medium,
                description: "Contract can be paused".to_string(),
                evidence: HashMap::new(),
                recommendation: "Be aware contract can be paused".to_string(),
            });
        }
        
        // Check for mintable
        if code_lower.contains("0x40c10f19") || code_lower.contains("0x0c55699c") {
            threats.push(SecurityThreat {
                threat_type: ThreatType::Mintable,
                severity: ThreatSeverity::Medium,
                description: "Contract can mint tokens".to_string(),
                evidence: HashMap::new(),
                recommendation: "Verify token supply is capped".to_string(),
            });
        }
        
        // Calculate score
        let score = match threats.iter().map(|t| t.severity as u32).max() {
            Some(s) => s,
            None => 0,
        };
        
        // Update stats
        {
            let mut stats = self.stats.write().await;
            stats.threats_detected += threats.len() as u64;
            stats.last_update = Utc::now().timestamp();
        }
        
        Ok(ContractScanResult {
            address: address.to_string(),
            threats,
            is_verified: false,
            is_open_source: false,
            is_malicious: score >= 80,
            score,
            audit_status: AuditStatus::None,
            last_updated: Utc::now().timestamp(),
        })
    }

    /// Analyze a wallet
    pub async fn analyze_wallet(&self, address: &str) -> Result<WalletAnalysis, SecurityError> {
        // Get first tx and other data
        let (first_tx, balance, code, nonce) = self.get_wallet_data(address).await?;
        
        let is_contract = code != "0x";
        let mut is_multisig = false;
        let mut multisig_owners = Vec::new();
        
        // Check if it's a Gnosis Safe
        if is_contract && code.len() > 100 {
            // Check for Safe interface
            // In production, properly check for Gnosis Safe
            if code.to_lowercase().contains("0xb63b350") {
                is_multisig = true;
                // Get owners
                multisig_owners = self.get_safe_owners(address).await.ok().unwrap_or_default();
            }
        }
        
        // Calculate age
        let age_days = if let Some(first) = first_tx {
            let now = Utc::now().timestamp();
            let age = now - first;
            (age / 86400) as u64
        } else {
            0
        };
        
        // Determine risk level
        let risk_level = match (age_days, is_multisig, !multisig_owners.is_empty()) {
            (0..=1, _, _) => RiskLevel::High,
            (_, true, _) => RiskLevel::Medium,
            (_, _, true) => RiskLevel::Medium,
            (365..=, false, false) => RiskLevel::Low,
            _ => RiskLevel::Medium,
        };
        
        // Update stats
        {
            let mut stats = self.stats.write().await;
            stats.wallets_analyzed += 1;
            stats.last_update = Utc::now().timestamp();
        }
        
        Ok(WalletAnalysis {
            address: address.to_string(),
            age_days,
            first_tx_timestamp: first_tx.unwrap_or(0),
            total_transactions: nonce,
            total_received: balance.clone(),
            total_sent: "0x0".to_string(),
            is_contract,
            is_multisig,
            multisig_owners,
            token_holdings: Vec::new(),
            nft_holdings: Vec::new(),
            risk_level,
        })
    }

    /// Check if address is malicious
    async fn is_malicious(&self, address: &str) -> bool {
        let addr = address.to_lowercase();
        
        // Check cache first
        {
            let cache = self.malicious_cache.read().await;
            if let Some(result) = cache.get(&addr) {
                return *result;
            }
        }
        
        // In production, check against known malicious contracts DB
        // For now, return false
        let result = false;
        
        // Cache result
        {
            let mut cache = self.malicious_cache.write().await;
            cache.insert(addr, result);
        }
        
        result
    }

    /// Calculate risk score from threats
    fn calculate_risk_score(&self, threats: &[SecurityThreat]) -> u32 {
        let mut score = 0u32;
        for threat in threats {
            score += match threat.severity {
                ThreatSeverity::Critical => 100,
                ThreatSeverity::High => 75,
                ThreatSeverity::Medium => 50,
                ThreatSeverity::Low => 25,
                ThreatSeverity::Info => 10,
            };
        }
        score.min(100)
    }

    /// Get transaction data
    async fn get_transaction(&self, tx_hash: &str) -> Result<(TxData, TxReceipt), SecurityError> {
        let request = json!({
            "jsonrpc": "2.0",
            "method": "eth_getTransactionByHash",
            "params": [tx_hash],
            "id": 1
        });

        let response = self.client
            .post(&self.config.rpc_url)
            .json(&request)
            .send()
            .await
            .map_err(|e| SecurityError::RPCError(e.to_string()))?
            .json::<Value>()
            .await
            .map_err(|e| SecurityError::RPCError(e.to_string()))?;

        let result = response.get("result")
            .ok_or_else(|| SecurityError::RPCError("No result".to_string()))?;

        let tx = TxData {
            hash: result.get("hash").and_then(|v| v.as_str()).unwrap_or("").to_string(),
            from: result.get("from").and_then(|v| v.as_str()).unwrap_or("").to_string(),
            to: result.get("to").and_then(|v| v.as_str()).map(|s| s.to_string()),
            value: result.get("value").and_then(|v| v.as_str()).unwrap_or("0x0").to_string(),
            input: result.get("input").and_then(|v| v.as_str()).unwrap_or("0x").to_string(),
        };

        let request2 = json!({
            "jsonrpc": "2.0",
            "method": "eth_getTransactionReceipt",
            "params": [tx_hash],
            "id": 2
        });

        let response2 = self.client
            .post(&self.config.rpc_url)
            .json(&request2)
            .send()
            .await
            .map_err(|e| SecurityError::RPCError(e.to_string()))?
            .json::<Value>()
            .await
            .map_err(|e| SecurityError::RPCError(e.to_string()))?;

        let result2 = response2.get("result")
            .ok_or_else(|| SecurityError::RPCError("No result".to_string()))?;

        let receipt = TxReceipt {
            status: result2.get("status").and_then(|v| v.as_str()).unwrap_or("0x1").to_string(),
        };

        Ok((tx, receipt))
    }

    /// Get traces
    async fn get_traces(&self, tx_hash: &str) -> Result<Value, SecurityError> {
        let request = json!({
            "jsonrpc": "2.0",
            "method": "trace_transaction",
            "params": [tx_hash],
            "id": 1
        });

        let response = self.client
            .post(&self.config.rpc_url)
            .json(&request)
            .send()
            .await
            .map_err(|e| SecurityError::RPCError(e.to_string()))?
            .json::<Value>()
            .await
            .map_err(|e| SecurityError::RPCError(e.to_string()))?;

        Ok(response.get("result").cloned().unwrap_or(json!([])))
    }

    /// Get code
    async fn get_code(&self, address: &str) -> Result<String, SecurityError> {
        let request = json!({
            "jsonrpc": "2.0",
            "method": "eth_getCode",
            "params": [address, "latest"],
            "id": 1
        });

        let response = self.client
            .post(&self.config.rpc_url)
            .json(&request)
            .send()
            .await
            .map_err(|e| SecurityError::RPCError(e.to_string()))?
            .json::<Value>()
            .await
            .map_err(|e| SecurityError::RPCError(e.to_string()))?;

        Ok(response.get("result")
            .and_then(|v| v.as_str())
            .unwrap_or("0x")
            .to_string())
    }

    /// Get wallet data
    async fn get_wallet_data(&self, address: &str) -> Result<(Option<i64>, String, String, u64), SecurityError> {
        // Get balance
        let request = json!({
            "jsonrpc": "2.0",
            "method": "eth_getBalance",
            "params": [address, "latest"],
            "id": 1
        });

        let balance = self.client
            .post(&self.config.rpc_url)
            .json(&request)
            .send()
            .await
            .map_err(|e| SecurityError::RPCError(e.to_string()))?
            .json::<Value>()
            .await
            .map_err(|e| SecurityError::RPCError(e.to_string()))?
            .get("result")
            .and_then(|v| v.as_str())
            .unwrap_or("0x0")
            .to_string();

        let code = self.get_code(address).await?;

        let request2 = json!({
            "jsonrpc": "2.0",
            "method": "eth_getTransactionCount",
            "params": [address, "latest"],
            "id": 2
        });

        let nonce = self.client
            .post(&self.config.rpc_url)
            .json(&request2)
            .send()
            .await
            .map_err(|e| SecurityError::RPCError(e.to_string()))?
            .json::<Value>()
            .await
            .map_err(|e| SecurityError::RPCError(e.to_string()))?
            .get("result")
            .and_then(|v| v.as_str())
            .map(|s| u64::from_str_radix(s.trim_start_matches("0x"), 16).unwrap_or(0))
            .unwrap_or(0);

        Ok((None, balance, code, nonce))
    }

    /// Get Safe owners
    async fn get_safe_owners(&self, address: &str) -> Result<Vec<String>, SecurityError> {
        // In production, properly query Safe contract
        Ok(Vec::new())
    }

    /// Get statistics
    pub async fn get_stats(&self) -> SecurityStats {
        self.stats.read().await.clone()
    }
}

// =============================================================================
// HELPER STRUCTS
// =============================================================================

struct TxData {
    hash: String,
    from: String,
    to: Option<String>,
    value: String,
    input: String,
}

struct TxReceipt {
    status: String,
}