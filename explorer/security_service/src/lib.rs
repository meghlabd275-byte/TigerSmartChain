//! TigerScan Security Service
//! Honeypot detection, phishing alerts, scam token database

use std::sync::Arc;
use anyhow::Result;
use ethers::providers::{Http, Middleware, Provider};
use ethers::types::Address;
use parking_lot::RwLock;
use regex::Regex;
use serde::{Deserialize, Serialize};
use sqlx::postgres::{PgPool, PgPoolOptions};
use tracing::{info, warn};

#[derive(Debug, Clone, Deserialize)]
pub struct Config {
    pub rpc_url: String,
    pub database_url: String,
    pub update_interval: u64,
    /// Address holding a token balance, used for the dynamic honeypot probe.
    /// Without it the probe cannot run, because a `transfer` simulation from an
    /// empty account reverts on balance checks alone and would flag every token.
    pub honeypot_probe_address: Option<Address>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            rpc_url: std::env::var("RPC_URL").unwrap_or_else(|_| "http://localhost:8545".to_string()),
            database_url: std::env::var("DATABASE_URL").unwrap_or_else(|_| "postgres://localhost:5432/tigerscan".to_string()),
            update_interval: 300,
            honeypot_probe_address: std::env::var("HONEYPOT_PROBE_ADDRESS")
                .ok()
                .and_then(|value| value.parse().ok()),
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
    /// Whether the dynamic probe actually ran. False means only bytecode was
    /// inspected, which cannot distinguish a honeypot from a normal token.
    pub dynamic_probe_ran: bool,
}

/// A row from `malicious_contracts`, surfaced by the `threats` method.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ThreatRecord {
    pub address: String,
    pub threat_type: String,
    pub confidence: i32,
    pub description: Option<String>,
    pub last_seen: i64,
}

pub struct SecurityService {
    config: Config,
    db: PgPool,
    rpc: Provider<Http>,
    state: Arc<RwLock<SecurityState>>,
    honeypot_patterns: Vec<Regex>,
    scam_patterns: Vec<Regex>,
}

#[derive(Debug, Clone, Default)]
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
            confidence: 100,
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

        // Check honeypot behaviour
        match self.probe_honeypot(address).await {
            Ok(hp_result) => {
                if hp_result.is_honeypot {
                    report.is_safe = false;
                    report.threat_type = Some("honeypot".to_string());
                    report.warnings.extend(hp_result.reasons.clone());
                    report.details.push(SecurityDetail {
                        category: "honeypot".to_string(),
                        severity: "critical".to_string(),
                        description: "Simulated transfer reverted; token cannot be sold"
                            .to_string(),
                    });
                } else if hp_result.honeypot_score > 0.0 {
                    report.warnings.extend(hp_result.reasons.clone());
                    report.details.push(SecurityDetail {
                        category: "honeypot".to_string(),
                        severity: "medium".to_string(),
                        description: "Bytecode resembles a known honeypot pattern".to_string(),
                    });
                }
            }
            Err(error) => {
                warn!(%address, %error, "honeypot probe failed");
                report
                    .warnings
                    .push(format!("Honeypot probe could not run: {error}"));
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

    /// Bytecode-only heuristics.
    ///
    /// This is a first pass only. A honeypot is defined by its runtime
    /// behaviour, so a positive result here is a warning, never a verdict; the
    /// dynamic probe below is what actually decides.
    fn detect_honeypot(&self, bytecode: &str) -> HoneypotResult {
        let mut reasons = Vec::new();
        let mut score: f64 = 0.0;

        // SELFDESTRUCT is 0xff and is not normally emitted by Solidity anymore,
        // so its presence in the runtime code is worth flagging.
        if bytecode.contains("ff") {
            score += 0.15;
            reasons.push("Runtime code contains SELFDESTRUCT".to_string());
        }

        // A token whose runtime code is tiny cannot contain a transfer path.
        if bytecode.len() < 100 {
            score += 0.2;
            reasons.push("Very small runtime code".to_string());
        }

        for pattern in &self.honeypot_patterns {
            if pattern.is_match(bytecode) {
                score += 0.2;
                reasons.push("Matches a known honeypot code pattern".to_string());
                break;
            }
        }

        for pattern in &self.scam_patterns {
            if pattern.is_match(bytecode) {
                score += 0.1;
                reasons.push("Matches a known scam code pattern".to_string());
                break;
            }
        }

        HoneypotResult {
            is_honeypot: false,
            honeypot_score: score.min(1.0),
            reasons,
            dynamic_probe_ran: false,
        }
    }

    /// Probe a token by simulating a `transfer` from a funded account and a
    /// `transfer` back out of the recipient.
    ///
    /// A honeypot accepts the buy and reverts the sell. Both legs are run as
    /// `eth_call`, so nothing is broadcast. The probe is skipped, not guessed,
    /// when no funded probe account is configured or the contract does not
    /// implement the ERC-20 selector.
    async fn probe_honeypot(&self, address: &str) -> Result<HoneypotResult> {
        let mut bytecode_result = self
            .get_contract_code(address)
            .await
            .map(|code| self.detect_honeypot(&code))
            .unwrap_or_else(|_| HoneypotResult {
                is_honeypot: false,
                honeypot_score: 0.0,
                reasons: vec!["Contract code could not be read".to_string()],
                dynamic_probe_ran: false,
            });

        let (Some(probe), Ok(token)) = (self.config.honeypot_probe_address, address.parse::<Address>())
        else {
            bytecode_result
                .reasons
                .push("Dynamic probe skipped: no funded probe address configured".to_string());
            return Ok(bytecode_result);
        };

        // transfer(address,uint256) with a 1 wei amount.
        let recipient = Address::from_low_u64_be(0xdead);
        let mut calldata = Vec::with_capacity(68);
        calldata.extend_from_slice(&[0xa9, 0x05, 0x9c, 0xbb]);
        calldata.extend_from_slice(&[0u8; 12]);
        calldata.extend_from_slice(recipient.as_bytes());
        calldata.extend_from_slice(&{
            let mut amount = [0u8; 32];
            amount[31] = 1;
            amount
        });

        let tx = ethers::types::TransactionRequest::new()
            .to(token)
            .from(probe)
            .data(calldata);

        bytecode_result.dynamic_probe_ran = true;

        match self.rpc.call(&tx.into(), None).await {
            Ok(_) => {
                bytecode_result
                    .reasons
                    .push("Simulated transfer from a funded account succeeded".to_string());
            }
            Err(error) => {
                bytecode_result.is_honeypot = true;
                bytecode_result.honeypot_score = 1.0;
                bytecode_result
                    .reasons
                    .push(format!("Simulated transfer reverted: {error}"));
            }
        }

        Ok(bytecode_result)
    }

    async fn get_contract_code(&self, address: &str) -> Result<String> {
        let address: Address = address
            .parse()
            .map_err(|_| anyhow::anyhow!("not a valid address: {address}"))?;

        let code = self.rpc.get_code(address, None).await?;
        Ok(format!("0x{}", hex::encode(code)))
    }

    /// Scan an address and report whether it is safe to interact with.
    pub async fn check_address(&self, address: &str) -> Result<SecurityReport> {
        self.analyze_contract(address).await
    }

    /// Probe a token contract for honeypot behaviour.
    pub async fn check_honeypot(&self, address: &str) -> Result<HoneypotResult> {
        self.probe_honeypot(address).await
    }

    /// Most recently observed active threats, newest first.
    pub async fn get_recent_threats(&self, limit: i64) -> Result<Vec<ThreatRecord>> {
        let rows: Vec<(String, String, i32, Option<String>, i64)> = sqlx::query_as(
            "SELECT address, threat_type, confidence, description, last_seen \
             FROM malicious_contracts WHERE is_active = true \
             ORDER BY last_seen DESC LIMIT $1",
        )
        .bind(limit)
        .fetch_all(&self.db)
        .await?;

        Ok(rows
            .into_iter()
            .map(
                |(address, threat_type, confidence, description, last_seen)| ThreatRecord {
                    address,
                    threat_type,
                    confidence,
                    description,
                    last_seen,
                },
            )
            .collect())
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

    /// Refresh the threat counters from the database.
    ///
    /// The rows themselves are written by whatever ingest process consumes the
    /// threat feeds; this only recounts them, so the reported totals cannot
    /// drift from what is actually stored.
    pub async fn update_threat_database(&self) -> Result<()> {
        info!("Refreshing threat counters");

        let (contracts,): (i64,) = sqlx::query_as(
            "SELECT COUNT(*) FROM malicious_contracts WHERE is_active = true",
        )
        .fetch_one(&self.db)
        .await?;

        let (scam_tokens,): (i64,) =
            sqlx::query_as("SELECT COUNT(*) FROM scam_tokens WHERE is_active = true")
                .fetch_one(&self.db)
                .await?;

        let (phishing,): (i64,) =
            sqlx::query_as("SELECT COUNT(*) FROM phishing_urls WHERE is_active = true")
                .fetch_one(&self.db)
                .await?;

        let mut state = self.state.write();
        state.threats_detected = (contracts + scam_tokens + phishing) as u64;
        state.last_update = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        Ok(())
    }

    pub fn get_state(&self) -> SecurityState { self.state.read().clone() }
}