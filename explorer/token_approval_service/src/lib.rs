//! TigerScan Token Approval Manager Service
//! Full token approval tracking and revocation
//! Uses Rust for maximum performance

use std::collections::HashMap;
use std::sync::Arc;

use anyhow::Result;
use chrono::{DateTime, Utc};
use ethers::core::abi::{Abi, Token};
use ethers::core::k256::sha2::Sha256;
use ethers::core::types::{Address, H256, U256};
use ethers::providers::{Http, Provider};
use ethers::types::{Filter, Log, Transaction};
use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use sha2::{Digest as Sha256Digest, Sha256 as Sha256Hasher};
use thiserror::Error;
use tracing::{error, info, warn};

// ============================================================================
// Error Types
// ============================================================================

#[derive(Error, Debug)]
pub enum ApprovalError {
    #[error("RPC error: {0}")]
    Rpc(String),
    
    #[error("Token error: {0}")]
    Token(String),
    
    #[error("Approval not found: {0}")]
    NotFound(String),
    
    #[error("Revocation error: {0}")]
    Revocation(String),
    
    #[error("Invalid input: {0}")]
    InvalidInput(String),
}

// ============================================================================
// Configuration
// ============================================================================

#[derive(Debug, Clone, Deserialize)]
pub struct Config {
    pub rpc_url: String,
    pub database_url: String,
    pub max_approvals: usize,
    pub scan_from_block: u64,
    pub enable_notifications: bool,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            rpc_url: std::env::var("RPC_URL")
                .unwrap_or_else(|_| "http://localhost:8545".to_string()),
            database_url: std::env::var("DATABASE_URL")
                .unwrap_or_else(|_| "postgres://localhost:5432/tigerscan".to_string()),
            max_approvals: 100000,
            scan_from_block: 0,
            enable_notifications: true,
        }
    }
}

// ============================================================================
// Data Types
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenApproval {
    pub id: String,
    pub token_address: String,
    pub owner: String,
    pub spender: String,
    pub amount: String,
    pub block_number: u64,
    pub transaction_hash: String,
    pub log_index: u32,
    pub timestamp: i64,
    pub approval_type: ApprovalType,
    pub is_revoked: bool,
    pub revocation_tx: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ApprovalType {
    Approve,
    IncreaseAllowance,
    DecreaseAllowance,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApprovalStats {
    pub total_approvals: usize,
    pub unique_tokens: usize,
    pub unique_owners: usize,
    pub unique_spenders: usize,
    pub risky_approvals: usize,
    pub large_approvals: usize,
    pub top_tokens: Vec<TokenStats>,
    pub top_spenders: Vec<SpenderStats>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenStats {
    pub address: String,
    pub approvals: usize,
    pub total_value: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SpenderStats {
    pub address: String,
    pub approvals: usize,
    pub total_value: String,
    pub is_verified: bool,
    pub risk_score: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApprovalRisk {
    pub spender: String,
    pub risk_level: RiskLevel,
    pub risk_factors: Vec<String>,
    pub recommendation: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RiskLevel {
    Low,
    Medium,
    High,
    Critical,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RevocationRequest {
    pub token_address: String,
    pub owner: String,
    pub spender: String,
    pub private_key: String,
    pub nonce: Option<u64>,
    pub gas_price: Option<String>,
}

// ============================================================================
// Token Approval Service
// ============================================================================

pub struct ApprovalService {
    config: Config,
    rpc: Provider<Http>,
    state: Arc<RwLock<ApprovalState>>,
}

#[derive(Debug)]
pub struct ApprovalState {
    pub approvals: HashMap<String, TokenApproval>,
    pub by_owner: HashMap<String, HashMap<String, Vec<String>>>,
    pub by_token: HashMap<String, HashMap<String, Vec<String>>>,
    pub by_spender: HashMap<String, Vec<String>>,
    pub last_scan_block: u64,
}

impl ApprovalService {
    pub async fn new(config: Config) -> Result<Self> {
        info!("Initializing Token Approval Service");
        
        let rpc = Provider::<Http>::try_from(config.rpc_url.clone())?;
        
        let service = Self {
            config: config.clone(),
            rpc,
            state: Arc::new(RwLock::new(ApprovalState {
                approvals: HashMap::new(),
                by_owner: HashMap::new(),
                by_token: HashMap::new(),
                by_spender: HashMap::new(),
                last_scan_block: config.scan_from_block,
            })),
        };
        
        info!("Token Approval Service initialized");
        Ok(service)
    }

    /// Scan for token approvals from logs
    pub async fn scan_approvals(&self, from_block: u64, to_block: u64) -> Result<Vec<TokenApproval>> {
        info!("Scanning approvals from block {} to {}", from_block, to_block);
        
        // Create filter for Approval events
        let filter = Filter::new()
            .from_block(from_block)
            .to_block(to_block)
            .address("0x".parse::<Address>().ok())
            .topic0(H256::from_slice(&hex::decoded("0x8c5be1e5ebec7d5bd14f5bdaa510684b1e5c059b1e5c059b1e5c059b1e5c059b1e").unwrap()));
        
        // Get logs
        let logs = self.rpc.get_logs(&filter).await?;
        
        let mut approvals = Vec::new();
        
        for log in logs {
            if let Some(approval) = self.parse_approval_log(&log)? {
                approvals.push(approval);
            }
        }
        
        // Update state
        let mut state = self.state.write();
        for approval in &approvals {
            state.approvals.insert(approval.id.clone(), approval.clone());
            
            state.by_owner
                .entry(approval.owner.clone())
                .or_insert_with(HashMap::new)
                .entry(approval.token_address.clone())
                .or_insert_with(Vec::new)
                .push(approval.id.clone());
            
            state.by_token
                .entry(approval.token_address.clone())
                .or_insert_with(HashMap::new())
                .entry(approval.owner.clone())
                .or_insert_with(Vec::new)
                .push(approval.id.clone());
            
            state.by_spender
                .entry(approval.spender.clone())
                .push(approval.id.clone());
        }
        
        state.last_scan_block = to_block;
        
        Ok(approvals)
    }

    /// Parse approval log
    fn parse_approval_log(&self, log: &Log) -> Result<Option<TokenApproval>> {
        if log.topics.len() < 3 {
            return Ok(None);
        }
        
        // Parse Approval(address,address,uint256)
        let owner = format!("{:?}", Address::from(log.topics[1]));
        let spender = format!("{:?}", Address::from(log.topics[2]));
        
        let amount = if log.data.0.len() >= 32 {
            let mut bytes = [0u8; 32];
            bytes.copy_from_slice(&log.data.0[..32]);
            U256::from_big_endian(&bytes).to_string()
        } else {
            "0".to_string()
        };
        
        let approval = TokenApproval {
            id: format!("{:?}-{:?}", log.transaction_hash, log.log_index),
            token_address: format!("{:?}", log.address),
            owner,
            spender,
            amount,
            block_number: log.block_number.unwrap_or_default().as_u64(),
            transaction_hash: format!("{:?}", log.transaction_hash),
            log_index: log.log_index,
            timestamp: 0, // Would need to fetch block timestamp
            approval_type: ApprovalType::Approve,
            is_revoked: false,
            revocation_tx: None,
        };
        
        Ok(Some(approval))
    }

    /// Get approvals for an owner
    pub fn get_by_owner(&self, owner: &str) -> Vec<TokenApproval> {
        let state = self.state.read();
        
        state.by_owner
            .get(owner)
            .map(|tokens| {
                tokens.values()
                    .flat_map(|ids| {
                        ids.iter()
                            .filter_map(|id| state.approvals.get(id).cloned())
                    })
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Get approvals for a token
    pub fn get_by_token(&self, token: &str) -> Vec<TokenApproval> {
        let state = self.state.read();
        
        state.by_token
            .get(token)
            .map(|owners| {
                owners.values()
                    .flat_map(|ids| {
                        ids.iter()
                            .filter_map(|id| state.approvals.get(id).cloned())
                    })
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Get approvals for a spender
    pub fn get_by_spender(&self, spender: &str) -> Vec<TokenApproval> {
        let state = self.state.read();
        
        state.by_spender
            .get(spender)
            .map(|ids| {
                ids.iter()
                    .filter_map(|id| state.approvals.get(id).cloned())
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Get all approvals for an owner on a specific token
    pub fn get_approvals(&self, owner: &str, token: &str) -> Vec<TokenApproval> {
        let state = self.state.read();
        
        state.by_owner
            .get(owner)
            .and_then(|tokens| tokens.get(token))
            .map(|ids| {
                ids.iter()
                    .filter_map(|id| state.approvals.get(id).filter(|a| !a.is_revoked).cloned())
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Get approval statistics
    pub fn get_stats(&self) -> ApprovalStats {
        let state = self.state.read();
        
        let total = state.approvals.len();
        
        let mut unique_tokens = HashSet::new();
        let mut unique_owners = HashSet::new();
        let mut unique_spenders = HashSet::new();
        let mut risky = 0;
        let mut large = 0;
        
        let mut token_counts: HashMap<String, usize> = HashMap::new();
        let mut spender_counts: HashMap<String, usize> = HashMap::new();
        
        for approval in state.approvals.values() {
            unique_tokens.insert(approval.token_address.clone());
            unique_owners.insert(approval.owner.clone());
            unique_spenders.insert(approval.spender.clone());
            
            // Check for large approvals (unlimited or > 1M)
            if approval.amount == u128::MAX.to_string() || 
               U256::from_dec_str(&approval.amount).unwrap_or_default() > U256::from(1_000_000_000_000_000_000u64) {
                large += 1;
            }
            
            *token_counts.entry(approval.token_address.clone()).or_insert(0) += 1;
            *spender_counts.entry(approval.spender.clone()).or_insert(0) += 1;
        }
        
        // Calculate risky (unverified spenders with large allowances)
        risky = large;
        
        // Top tokens
        let mut top_tokens: Vec<_> = token_counts.into_iter()
            .map(|(address, count)| TokenStats {
                address,
                approvals: count,
                total_value: "0".to_string(),
            })
            .collect();
        
        top_tokens.sort_by(|a, b| b.approvals.cmp(&a.approvals));
        top_tokens.truncate(10);
        
        // Top spenders
        let mut top_spenders: Vec<_> = spender_counts.into_iter()
            .map(|(address, count)| SpenderStats {
                address,
                approvals: count,
                total_value: "0".to_string(),
                is_verified: false,
                risk_score: 0.0,
            })
            .collect();
        
        top_spenders.sort_by(|a, b| b.approvals.cmp(&a.approvals));
        top_spenders.truncate(10);
        
        ApprovalStats {
            total_approvals: total,
            unique_tokens: unique_tokens.len(),
            unique_owners: unique_owners.len(),
            unique_spenders: unique_spenders.len(),
            risky_approvals: risky,
            large_approvals: large,
            top_tokens,
            top_spenders,
        }
    }

    /// Analyze risk for spender
    pub fn analyze_risk(&self, spender: &str) -> ApprovalRisk {
        let approvals = self.get_by_spender(spender);
        
        let mut risk_factors = Vec::new();
        let mut total_value = U256::zero();
        let mut unlimited_count = 0;
        
        for approval in &approvals {
            let amount = U256::from_dec_str(&approval.amount).unwrap_or_default();
            total_value += amount;
            
            if approval.amount == u128::MAX.to_string() {
                unlimited_count += 1;
                risk_factors.push("Unlimited approval detected".to_string());
            }
            
            if amount > U256::from(1_000_000_000_000_000_000u64) {
                risk_factors.push("Large approval amount".to_string());
            }
        }
        
        let risk_level = if unlimited_count > 0 || total_value > U256::from(1_000_000_000_000_000_000_000u64) {
            RiskLevel::Critical
        } else if approvals.len() > 10 || total_value > U256::from(1_000_000_000_000_000_000u64) {
            RiskLevel::High
        } else if approvals.len() > 5 {
            RiskLevel::Medium
        } else {
            RiskLevel::Low
        };
        
        let recommendation = match risk_level {
            RiskLevel::Critical => "Revoke immediately - this spender has unlimited or very large allowances".to_string(),
            RiskLevel::High => "Review and consider revoking if not needed".to_string(),
            RiskLevel::Medium => "Monitor activity - revoke if contract is unused".to_string(),
            RiskLevel::Low => "No immediate action needed".to_string(),
        };
        
        ApprovalRisk {
            spender: spender.to_string(),
            risk_level,
            risk_factors,
            recommendation,
        }
    }

    /// Generate revocation transaction data
    pub fn generate_revocation(&self, owner: &str, token: &str, spender: &str) -> Result<String> {
        // ERC-20 approve(spender, 0)
        let selector = "0x095ea7b3"; // approve(address,uint256)
        let spender_addr = Address::from(spender);
        let mut data = [0u8; 32];
        
        // Encode spender address
        spender_addr.to_big_endian(&mut data);
        
        // Amount = 0
        let amount = U256::zero();
        amount.to_big_endian(&mut data[12..].try_into().unwrap());
        
        Ok(format!("{}000000000000000000000000{}{}", 
            selector, 
            hex::encode(spender_addr.as_bytes()),
            "0000000000000000000000000000000000000000000000000000000000000000"
        ))
    }
}

// ============================================================================
// API Types
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApprovalApiRequest {
    pub owner: Option<String>,
    pub token: Option<String>,
    pub spender: Option<String>,
    pub risk_threshold: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApprovalApiResponse {
    pub success: bool,
    pub result: Option<serde_json::Value>,
    pub error: Option<String>,
}

// ============================================================================
// Utility Functions
// ============================================================================

/// Generate approval ID
pub fn generate_approval_id(tx_hash: &str, log_index: u32) -> String {
    format!("{}-{}", tx_hash, log_index)
}

/// Check if approval is unlimited
pub fn is_unlimited(amount: &str) -> bool {
    amount == u128::MAX.to_string() || 
    amount == "0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff" ||
    amount == "0x".to_string()
}

/// Format amount with decimals
pub fn format_amount(amount: &str, decimals: u8) -> String {
    let value = U256::from_dec_str(amount).unwrap_or_default();
    let divisor = U256::from(10).pow(U256::from(decimals));
    
    let integer = value / divisor;
    let fractional = value % divisor;
    
    if fractional.is_zero() {
        format!("{}", integer)
    } else {
        format!("{}.{}", integer, fractional)
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_unlimited() {
        assert!(is_unlimited(&u128::MAX.to_string()));
        assert!(is_unlimited("0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff"));
        assert!(!is_unlimited("1000"));
    }

    #[test]
    fn test_approval_id() {
        let id = generate_approval_id("0x1234", 1);
        assert_eq!(id, "0x1234-1");
    }
}