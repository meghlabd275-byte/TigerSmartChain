//! TigerScan Governance DAO Service
//! Proposals, voting, and delegate management

use std::collections::HashMap;
use std::sync::Arc;

use anyhow::Result;
use chrono::{DateTime, Utc};
use ethers::core::types::{Address, H256, U256};
use ethers::providers::{Http, Provider};
use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use thiserror::Error;
use tracing::{error, info, warn};

// ============================================================================
// Error Types
// ============================================================================

#[derive(Error, Debug)]
pub enum GovernanceError {
    #[error("RPC error: {0}")]
    Rpc(String),
    
    #[error("Proposal not found: {0}")]
    NotFound(String),
    
    #[error("Vote error: {0}")]
    Vote(String),
    
    #[error("Unauthorized: {0}")]
    Unauthorized(String),
}

// ============================================================================
// Configuration
// ============================================================================

#[derive(Debug, Clone, Deserialize)]
pub struct Config {
    pub rpc_url: String,
    pub governance_address: String,
    pub token_address: String,
    pub quorum: u64,
    pub voting_period: u64,
    pub execution_delay: u64,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            rpc_url: std::env::var("RPC_URL")
                .unwrap_or_else(|_| "http://localhost:8545".to_string()),
            governance_address: std::env::var("GOVERNANCE_ADDRESS").unwrap_or_default(),
            token_address: std::env::var("TOKEN_ADDRESS").unwrap_or_default(),
            quorum: 4_000_000_000_000_000_000u64, // 4M votes
            voting_period: 5,
            execution_delay: 2,
        }
    }
}

// ============================================================================
// Data Types
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Proposal {
    pub id: u64,
    pub title: String,
    pub description: String,
    pub targets: Vec<String>,
    pub values: Vec<String>,
    pub signatures: Vec<String>,
    pub calldatas: Vec<String>,
    pub proposer: String,
    pub status: ProposalStatus,
    pub for_votes: String,
    pub against_votes: String,
    pub abstain_votes: String,
    pub start_block: u64,
    pub end_block: u64,
    pub execution_block: u64,
    pub created_at: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProposalStatus {
    Pending,
    Active,
    Canceled,
    Defeated,
    Succeeded,
    Queued,
    Executed,
    Expired,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Vote {
    pub proposal_id: u64,
    pub voter: String,
    pub support: VoteChoice,
    pub votes: String,
    pub reason: Option<String>,
    pub timestamp: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VoteChoice {
    Against,
    For,
    Abstain,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Delegate {
    pub address: String,
    pub delegatee: Option<String>,
    pub votes: String,
    pub tokens: String,
    pub delegated_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GovernanceStats {
    pub total_proposals: u64,
    pub active_proposals: u64,
    pub passed_proposals: u64,
    pub failed_proposals: u64,
    pub total_voters: u64,
    pub total_delegates: u64,
    pub quorum_reached: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProposalVotes {
    pub proposal_id: u64,
    pub votes: Vec<Vote>,
    pub for_votes: String,
    pub against_votes: String,
    pub abstain_votes: String,
    pub voter_count: u64,
}

// ============================================================================
// Governance Service
// ============================================================================

pub struct GovernanceService {
    config: Config,
    rpc: Provider<Http>,
    state: Arc<RwLock<GovernanceState>>,
}

#[derive(Debug)]
pub struct GovernanceState {
    pub proposals: HashMap<u64, Proposal>,
    pub votes: HashMap<u64, HashMap<String, Vote>>,
    pub delegates: HashMap<String, Delegate>,
    pub voter_power: HashMap<String, String>,
}

impl GovernanceService {
    pub async fn new(config: Config) -> Result<Self> {
        info!("Initializing Governance Service");
        
        let rpc = Provider::<Http>::try_from(config.rpc_url.clone())?;
        
        let service = Self {
            config: config.clone(),
            rpc,
            state: Arc::new(RwLock::new(GovernanceState {
                proposals: HashMap::new(),
                votes: HashMap::new(),
                delegates: HashMap::new(),
                voter_power: HashMap::new(),
            })),
        };
        
        info!("Governance Service initialized");
        Ok(service)
    }

    /// Get all proposals
    pub fn get_proposals(&self, status: Option<ProposalStatus>) -> Vec<Proposal> {
        let state = self.state.read();
        
        let mut proposals: Vec<_> = state.proposals.values()
            .cloned()
            .collect();
        
        if let Some(s) = status {
            proposals.retain(|p| p.status == s);
        }
        
        proposals.sort_by(|a, b| b.id.cmp(&a.id));
        proposals
    }

    /// Get proposal by ID
    pub fn get_proposal(&self, id: u64) -> Option<Proposal> {
        let state = self.state.read();
        state.proposals.get(&id).cloned()
    }

    /// Get votes for proposal
    pub fn get_proposal_votes(&self, id: u64) -> Option<ProposalVotes> {
        let state = self.state.read();
        
        let proposal = state.proposals.get(&id)?;
        let votes_map = state.votes.get(&id)?;
        
        let votes: Vec<_> = votes_map.values().cloned().collect();
        
        let for_votes: U256 = votes.iter()
            .filter(|v| v.support == VoteChoice::For)
            .map(|v| U256::from_dec_str(&v.votes).unwrap_or_default())
            .sum();
        
        let against_votes: U256 = votes.iter()
            .filter(|v| v.support == VoteChoice::Against)
            .map(|v| U256::from_dec_str(&v.votes).unwrap_or_default())
            .sum();
        
        let abstain_votes: U256 = votes.iter()
            .filter(|v| v.support == VoteChoice::Abstain)
            .map(|v| U256::from_dec_str(&v.votes).unwrap_or_default())
            .sum();
        
        Some(ProposalVotes {
            proposal_id: id,
            votes,
            for_votes: for_votes.to_string(),
            against_votes: against_votes.to_string(),
            abstain_votes: abstain_votes.to_string(),
            voter_count: votes.len() as u64,
        })
    }

    /// Cast a vote
    pub fn cast_vote(&self, proposal_id: u64, voter: &str, support: VoteChoice, votes: &str, reason: Option<String>) -> Result<()> {
        let mut state = self.state.write();
        
        // Check proposal exists and is active
        let proposal = state.proposals.get(&proposal_id)
            .ok_or_else(|| GovernanceError::NotFound(proposal_id.to_string()))?;
        
        if proposal.status != ProposalStatus::Active {
            return Err(GovernanceError::Vote("Proposal not active".to_string()).into());
        }
        
        // Record vote
        let votes_map = state.votes
            .entry(proposal_id)
            .or_insert_with(HashMap::new);
        
        votes_map.insert(voter.to_string(), Vote {
            proposal_id,
            voter: voter.to_string(),
            support,
            votes: votes.to_string(),
            reason,
            timestamp: Utc::now().timestamp(),
        });
        
        Ok(())
    }

    /// Get delegate info
    pub fn get_delegate(&self, address: &str) -> Option<Delegate> {
        let state = self.state.read();
        state.delegates.get(address).cloned()
    }

    /// Get voter power
    pub fn get_voter_power(&self, address: &str) -> String {
        let state = self.state.read();
        state.voter_power.get(address)
            .cloned()
            .unwrap_or_else(|| "0".to_string())
    }

    /// Get governance statistics
    pub fn get_stats(&self) -> GovernanceStats {
        let state = self.state.read();
        
        let total = state.proposals.len() as u64;
        let active = state.proposals.values()
            .filter(|p| p.status == ProposalStatus::Active)
            .count() as u64;
        let passed = state.proposals.values()
            .filter(|p| p.status == ProposalStatus::Executed)
            .count() as u64;
        let failed = state.proposals.values()
            .filter(|p| p.status == ProposalStatus::Defeated)
            .count() as u64;
        
        let total_voters = state.votes.values()
            .map(|v| v.len())
            .sum::<usize>() as u64;
        
        let total_delegates = state.delegates.len() as u64;
        
        let quorum_reached = state.proposals.values()
            .filter(|p| {
                let for_votes = U256::from_dec_str(&p.for_votes).unwrap_or_default();
                for_votes >= U256::from(self.config.quorum)
            })
            .count() as u64;
        
        GovernanceStats {
            total_proposals: total,
            active_proposals: active,
            passed_proposals: passed,
            failed_proposals: failed,
            total_voters,
            total_delegates,
            quorum_reached,
        }
    }

    /// Check if proposal meets quorum
    pub fn check_quorum(&self, proposal_id: u64) -> bool {
        if let Some(votes) = self.get_proposal_votes(proposal_id) {
            let for_votes = U256::from_dec_str(&votes.for_votes).unwrap_or_default();
            for_votes >= U256::from(self.config.quorum)
        } else {
            false
        }
    }
}

// ============================================================================
// API Types
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GovernanceApiRequest {
    pub proposal_id: Option<u64>,
    pub voter: Option<String>,
    pub support: Option<String>,
    pub votes: Option<String>,
    pub reason: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GovernanceApiResponse {
    pub success: bool,
    pub result: Option<serde_json::Value>,
    pub error: Option<String>,
}

// ============================================================================
// Utility Functions
// ============================================================================

/// Format vote weight
pub fn format_votes(votes: &str) -> String {
    let v = U256::from_dec_str(votes).unwrap_or_default();
    format!("{}", v)
}

/// Calculate voting power percentage
pub fn voting_power_percentage(votes: &str, total: &str) -> f64 {
    let v = U256::from_dec_str(votes).unwrap_or_default();
    let t = U256::from_dec_str(total).unwrap_or_default();
    
    if t.is_zero() {
        return 0.0;
    }
    
    (v.as_u128() as f64 / t.as_u128() as f64) * 100.0
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_voting_power() {
        let percentage = voting_power_percentage("500000000000000000000", "1000000000000000000000");
        assert!((percentage - 50.0).abs() < 0.1);
    }
}