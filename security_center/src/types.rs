//! Security Types for TigerScan

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

// =============================================================================
// THREAT DETECTION
// =============================================================================

/// Security Threat
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SecurityThreat {
    /// Threat type
    pub threat_type: ThreatType,
    /// Severity
    pub severity: ThreatSeverity,
    /// Description
    pub description: String,
    /// Evidence
    pub evidence: HashMap<String, String>,
    /// Recommendation
    pub recommendation: String,
}

/// Threat Type
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum ThreatType {
    Phishing,
    Honeypot,
    RugPull,
    FlashLoan,
    Multisig,
    Proxiable,
    Ownerable,
    Pausable,
    Mintable,
    Blacklist,
    Tax,
    Unknown,
}

/// Threat Severity
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum ThreatSeverity {
    Critical,
    High,
    Medium,
    Low,
    Info,
}

// =============================================================================
// TRANSACTION ANALYSIS
// =============================================================================

/// Transaction Analysis Result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TransactionAnalysis {
    pub hash: String,
    pub status: AnalysisStatus,
    pub threats: Vec<SecurityThreat>,
    pub summary: String,
    pub risk_score: u32,
    pub simulation_result: Option<SimulationResult>,
    pub token_transfers: Vec<TokenTransfer>,
    pub contract_calls: Vec<ContractCall>,
    pub is_suspicious: bool,
}

/// Analysis Status
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum AnalysisStatus {
    Safe,
    Warning,
    Dangerous,
    Unknown,
}

/// Simulation Result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SimulationResult {
    pub success: bool,
    pub gas_used: u64,
    pub balance_change: String,
    pub token_changes: Vec<TokenBalanceChange>,
    pub error: Option<String>,
}

/// Token Transfer
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenTransfer {
    pub token: String,
    pub from: String,
    pub to: String,
    pub amount: String,
    pub direction: TransferDirection,
}

/// Transfer Direction
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum TransferDirection {
    In,
    Out,
}

/// Token Balance Change
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenBalanceChange {
    pub token: String,
    pub owner: String,
    pub before: String,
    pub after: String,
}

/// Contract Call
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContractCall {
    pub to: String,
    pub method: String,
    pub params: HashMap<String, String>,
}

// =============================================================================
// CONTRACT SCAN
// =============================================================================

/// Contract Scan Result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContractScanResult {
    pub address: String,
    pub threats: Vec<SecurityThreat>,
    pub is_verified: bool,
    pub is_open_source: bool,
    pub is_malicious: bool,
    pub score: u32,
    pub audit_status: AuditStatus,
    pub last_scan: i64,
}

/// Audit Status
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum AuditStatus {
    Audited,
    Pending,
    Failed,
    None,
}

// =============================================================================
// WALLET ANALYSIS
// =============================================================================

/// Wallet Analysis
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WalletAnalysis {
    pub address: String,
    /// Wallet age in days
    pub age_days: u64,
    /// First transaction timestamp
    pub first_tx_timestamp: i64,
    /// Total transactions
    pub total_transactions: u64,
    /// Total received
    pub total_received: String,
    /// Total sent
    pub total_sent: String,
    /// Is a contract
    pub is_contract: bool,
    /// Is a multisig
    pub is_multisig: bool,
    /// Multisig owners (if applicable)
    pub multisig_owners: Vec<String>,
    /// Token holdings
    pub token_holdings: Vec<TokenHolding>,
    /// NFT holdings
    pub nft_holdings: Vec<NFTHolding>,
    /// Risk assessment
    pub risk_level: RiskLevel,
}

/// Token Holding
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenHolding {
    pub address: String,
    pub balance: String,
    pub value_usd: f64,
}

/// NFT Holding
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NFTHolding {
    pub collection: String,
    pub token_ids: Vec<String>,
    pub count: u32,
}

/// Risk Level
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum RiskLevel {
    Low,
    Medium,
    High,
    Critical,
}

// =============================================================================
// ALERTS
// =============================================================================

/// Security Alert
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SecurityAlert {
    pub id: String,
    pub alert_type: AlertType,
    pub severity: ThreatSeverity,
    pub title: String,
    pub description: String,
    pub address: Option<String>,
    pub tx_hash: Option<String>,
    pub created_at: i64,
    pub acknowledged: bool,
}

/// Alert Type
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum AlertType {
    LargeTransfer,
    NewToken,
    SuspiciousContract,
    WhaleActivity,
    PhishingDetected,
    FlashLoan,
    RugPull,
    Governance,
}

// =============================================================================
// CONFIG
// =============================================================================

/// Security Center Configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SecurityConfig {
    /// RPC URL
    pub rpc_url: String,
    /// Enable real-time monitoring
    pub enable_monitoring: bool,
    /// Large transfer threshold (USD)
    pub large_transfer_threshold: f64,
    /// Enable contract scanning
    pub enable_contract_scan: bool,
    /// Enable wallet analysis
    pub enable_wallet_analysis: bool,
    /// Alert webhook URL
    pub alert_webhook: Option<String>,
    /// Alert email
    pub alert_email: Option<String>,
    /// Known malicious contracts DB
    pub malicious_db_url: Option<String>,
    /// Known phish DB
    pub phish_db_url: Option<String>,
}

impl Default for SecurityConfig {
    fn default() -> Self {
        Self {
            rpc_url: "http://localhost:8545".to_string(),
            enable_monitoring: true,
            large_transfer_threshold: 10_000.0,
            enable_contract_scan: true,
            enable_wallet_analysis: true,
            alert_webhook: None,
            alert_email: None,
            malicious_db_url: None,
            phish_db_url: None,
        }
    }
}

// =============================================================================
// STATS
// =============================================================================

/// Security Center Statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SecurityStats {
    pub contracts_scanned: u64,
    pub threats_detected: u64,
    pub alerts_sent: u64,
    pub wallets_analyzed: u64,
    pub last_update: i64,
}

impl Default for SecurityStats {
    fn default() -> Self {
        Self {
            contracts_scanned: 0,
            threats_detected: 0,
            alerts_sent: 0,
            wallets_analyzed: 0,
            last_update: chrono::Utc::now().timestamp(),
        }
    }
}