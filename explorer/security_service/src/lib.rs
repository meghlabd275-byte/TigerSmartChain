//! TigerScan Security Service
//! Honeypot detection, phishing alerts, scam token database

use std::sync::Arc;
use std::time::Duration;
use anyhow::Result;
use async_trait::async_trait;
use ethers::providers::{Http, Provider};
use parking_lot::RwLock;
use regex::Regex;
use serde::{Deserialize, Serialize};
use sqlx::postgres::{PgPool, PgPoolOptions};
use tokio::time::interval;
use tracing::{error, info, warn};

#[derive(Debug, Clone, Deserialize)]
pub struct Config {
    pub rpc_url: String,
    pub database_url: String,
    pub update_interval: u64,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            rpc_url: std::env::var("RPC_URL").unwrap_or_else(|_| "http://localhost:8545".to_string()),
            database_url: std::env::var("DATABASE_URL").unwrap_or_else(|_| "postgres://localhost:5432/tigerscan".to_string()),
            update_interval: 300,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SecurityReport {
    pub address: String,
    pub is_safe: bool,
    pub threat_type: Option<String>,
    pub confidence: u32,
    pub warnings: Vec<String>,
    pub details: Vec<SecurityDetail>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SecurityDetail {
    pub category: String,
    pub severity: String,
    pub description: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HoneypotResult {
    pub is_honeypot: bool,
    pub honeypot_score: f64,
    pub reasons: Vec<String>,
}

pub struct SecurityService {
    config: Config,
    db: PgPool,
    rpc: Provider<Http>,
    state: Arc<RwLock<SecurityState>>,
    honeypot_patterns: Vec<Regex>,
    scam_patterns: Vec<Regex>,
}

#[derive(Debug, Clone)]
pub struct SecurityState {
    pub contracts_scanned: u64,
    pub threats_detected: u64,
    pub last_update: u64,
}

impl SecurityService {
    pub async fn new(config: Config) -> Result<Self> {
        let db = PgPoolOptions::new()
            .max_connections(5)
            .connect(&config.database_url)
            .await?;

        let rpc = Provider::<Http>::try_from(config.rpc_url.clone())?;

        let honeypot_patterns = vec![
            Regex::new(r"0x[0-9a-f]{40}.*?0x[0-9a-f]{40}")?,
        ];

        let scam_patterns = vec![
            Regex::new(r"(?i)(airdrop|free|reward|claim|bonus)")?,
            Regex::new(r"(?i)(ponzi|scam|fake|phish)")?,
        ];

        Ok(Self {
            config,
            db,
            rpc,
            state: Arc::new(RwLock::new(SecurityState::default())),
            honeypot_patterns,
            scam_patterns,
        })
    }

    pub async fn analyze_contract(&self, address: &str) -> Result<SecurityReport> {
        info!("Analyzing contract: {}", address);

        let mut report = SecurityReport {
            address: address.to_string(),
            is_safe: true,
            threat_type: None,
            warnings: vec![],
            details: vec![],
        };

        // Check malicious contracts DB
        if self.is_malicious(address).await? {
            report.is_safe = false;
            report.threat_type = Some("malicious".to_string());
            report.warnings.push("Contract is in malicious database".to_string());
            report.details.push(SecurityDetail {
                category: "malicious".to_string(),
                severity: "critical".to_string(),
                description: "This contract is flagged as malicious".to_string(),
            });
        }

        // Check honeypot patterns
        if let Ok(code) = self.get_contract_code(address).await {
            let hp_result = self.detect_honeypot(&code);
            if hp_result.is_honeypot {
                report.is_safe = false;
                report.threat_type = Some("honeypot".to_string());
                report.warnings.extend(hp_result.reasons.clone());
                report.details.push(SecurityDetail {
                    category: "honeypot".to_string(),
                    severity: "critical".to_string(),
                    description: "Potential honeypot detected".to_string(),
                });
            }
        }

        // Check if it's a known scam token
        if self.is_scam_token(address).await? {
            report.is_safe = false;
            report.threat_type = Some("scam_token".to_string());
            report.warnings.push("Token is flagged as scam".to_string());
        }

        // Check phishing URLs
        if self.has_phishing_history(address).await? {
            report.warnings.push("Address has phishing history".to_string());
            report.details.push(SecurityDetail {
                category: "phishing".to_string(),
                severity: "high".to_string(),
                description: "Associated with phishing activities".to_string(),
            });
        }

        self.state.write().contracts_scanned += 1;
        if !report.is_safe {
            self.state.write().threats_detected += 1;
        }

        Ok(report)
    }

    fn detect_honeypot(&self, bytecode: &str) -> HoneypotResult {
        let mut reasons = vec![];
        let mut score = 0.0;

        if bytecode.contains("5a4b") { // Self-destruct
            score += 0.3;
            reasons.push("Contains self-destruct opcode".to_string());
        }

        if bytecode.len() < 100 {
            score += 0.2;
            reasons.push("Very small bytecode".to_string());
        }

        // Check for fake return
        if bytecode.contains("1460006000") {
            score += 0.4;
            reasons.push("Suspicious storage access pattern".to_string());
        }

        HoneypotResult {
            is_honeypot: score > 0.5,
            honeypot_score: score,
            reasons,
        }
    }

    async fn get_contract_code(&self, address: &str) -> Result<String> {
        Ok(String::new())
    }

    async fn is_malicious(&self, address: &str) -> Result<bool> {
        let result: (i64,) = sqlx::query_as(
            "SELECT COUNT(*) FROM malicious_contracts WHERE address = $1 AND is_active = true"
        ).bind(address).fetch_one(&self.db).await?;

        Ok(result.0 > 0)
    }

    async fn is_scam_token(&self, address: &str) -> Result<bool> {
        let result: (i64,) = sqlx::query_as(
            "SELECT COUNT(*) FROM scam_tokens WHERE address = $1 AND is_active = true"
        ).bind(address).fetch_one(&self.db).await?;

        Ok(result.0 > 0)
    }

    async fn has_phishing_history(&self, address: &str) -> Result<bool> {
        let result: (i64,) = sqlx::query_as(
            "SELECT COUNT(*) FROM phishing_urls WHERE $1 = ANY(target_addresses) AND is_active = true"
        ).bind(address).fetch_one(&self.db).await?;

        Ok(result.0 > 0)
    }

    pub async fn update_threat_database(&self) -> Result<()> {
        info!("Updating threat database...");
        // Fetch from threat intelligence feeds
        self.state.write().last_update = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();
        Ok(())
    }

    pub fn get_state(&self) -> SecurityState { self.state.read().clone() }
}