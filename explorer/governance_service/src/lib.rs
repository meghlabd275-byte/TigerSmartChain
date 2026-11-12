//! TigerScan Governance DAO Service — proposals, voting and delegates.
//!
//! Proposals, votes and voting power are read from the governor and the
//! governance token over JSON-RPC. The service holds no proposal of its own:
//! the chain is the source of truth, so a proposal created by a contract call
//! appears here as soon as the node reports it.

use std::collections::HashMap;
use std::sync::Arc;

use anyhow::Result;
use ethers::abi::{AbiParser, Token, ParamType};
use ethers::providers::{Http, Middleware, Provider};
use ethers::types::{Address, BlockId, U256};
use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use thiserror::Error;
use tracing::{info, warn};

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

impl ProposalStatus {
    /// Decode the status the governor returns, which is the OpenZeppelin
    /// `ProposalState` enum in declaration order.
    pub fn from_u8(value: u8) -> Self {
        match value {
            0 => Self::Pending,
            1 => Self::Active,
            2 => Self::Canceled,
            3 => Self::Defeated,
            4 => Self::Succeeded,
            5 => Self::Queued,
            6 => Self::Expired,
            7 => Self::Executed,
            _ => Self::Pending,
        }
    }
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

impl VoteChoice {
    /// Decode the `support` value the governor accepts: 0 against, 1 for,
    /// 2 abstain.
    pub fn from_u8(value: u8) -> Option<Self> {
        match value {
            0 => Some(Self::Against),
            1 => Some(Self::For),
            2 => Some(Self::Abstain),
            _ => None,
        }
    }
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
// Governor ABI
// ============================================================================

/// The governor surface the service calls, in the order the selectors appear.
///
/// The names match the OpenZeppelin Governor ABI, which is what the deployed
/// contract exposes, so the selectors are derived rather than hardcoded.
const GOVERNOR_ABI: &[&str] = &[
    "proposalCount() view returns (uint256)",
    "proposals(uint256) view returns (uint256 id, address proposer, uint256 eta, uint256 startBlock, uint256 endBlock, uint256 forVotes, uint256 againstVotes, uint256 abstainVotes, bool canceled, bool executed)",
    "state(uint256) view returns (uint8)",
    "quorum(uint256) view returns (uint256)",
    "proposalThreshold() view returns (uint256)",
    "votingDelay() view returns (uint256)",
    "votingPeriod() view returns (uint256)",
    "hasVoted(uint256,address) view returns (bool)",
];

/// The governance token surface the service calls.
const TOKEN_ABI: &[&str] = &[
    "getVotes(address) view returns (uint256)",
    "balanceOf(address) view returns (uint256)",
    "delegates(address) view returns (address)",
    "totalSupply() view returns (uint256)",
    "name() view returns (string)",
    "symbol() view returns (string)",
];

// ============================================================================
// Governance Service
// ============================================================================

pub struct GovernanceService {
    config: Config,
    rpc: Provider<Http>,
    governor: Address,
    token: Address,
    /// Proposal ids that have been observed, so stats can be aggregated without
    /// re-walking the chain on every request.
    seen: Arc<RwLock<HashMap<u64, ProposalStatus>>>,
}

impl GovernanceService {
    pub async fn new(config: Config) -> Result<Self> {
        info!("Initializing Governance Service");

        let rpc = Provider::<Http>::try_from(config.rpc_url.clone())?;

        let governor: Address = if config.governance_address.is_empty() {
            Address::zero()
        } else {
            config
                .governance_address
                .parse()
                .map_err(|error| anyhow::anyhow!("invalid governance address: {error}"))?
        };

        let token: Address = if config.token_address.is_empty() {
            Address::zero()
        } else {
            config
                .token_address
                .parse()
                .map_err(|error| anyhow::anyhow!("invalid token address: {error}"))?
        };

        Ok(Self {
            config,
            rpc,
            governor,
            token,
            seen: Arc::new(RwLock::new(HashMap::new())),
        })
    }

    /// The active configuration.
    pub fn config(&self) -> &Config {
        &self.config
    }

    /// True when both contract addresses are configured.
    pub fn is_configured(&self) -> bool {
        !self.governor.is_zero() && !self.token.is_zero()
    }

    /// Call a view function on the governor.
    async fn governor_call(&self, signature: &str, args: &[Token]) -> Result<Vec<Token>> {
        if self.governor.is_zero() {
            return Err(anyhow::anyhow!("GOVERNANCE_ADDRESS is not set"));
        }

        self.eth_call(self.governor, GOVERNOR_ABI, signature, args)
            .await
    }

    /// Call a view function on the governance token.
    async fn token_call(&self, signature: &str, args: &[Token]) -> Result<Vec<Token>> {
        if self.token.is_zero() {
            return Err(anyhow::anyhow!("TOKEN_ADDRESS is not set"));
        }

        self.eth_call(self.token, TOKEN_ABI, signature, args)
            .await
    }

    /// Encode a call from the ABI signature and decode the result.
    ///
    /// The signature carries its own return types, so the result is decoded
    /// against the declared types rather than guessed from the byte length.
    async fn eth_call(
        &self,
        to: Address,
        abi: &[&str],
        signature: &str,
        args: &[Token],
    ) -> Result<Vec<Token>> {
        let declaration = abi
            .iter()
            .find(|entry| entry.starts_with(signature))
            .ok_or_else(|| anyhow::anyhow!("{signature} is not in the ABI"))?;

        let parsed = AbiParser::default()
            .parse_function(declaration)
            .map_err(|error| anyhow::anyhow!("cannot parse {declaration}: {error}"))?;

        let data = parsed
            .encode_input(args)
            .map_err(|error| anyhow::anyhow!("cannot encode {signature}: {error}"))?;

        let output = self
            .rpc
            .call(
                &ethers::types::TransactionRequest::new()
                    .to(to)
                    .data(data)
                    .into(),
                Some(BlockId::Number(ethers::types::BlockNumber::Latest)),
            )
            .await
            .map_err(|error| anyhow::anyhow!("eth_call {signature} failed: {error}"))?;

        parsed
            .decode_output(&output)
            .map_err(|error| anyhow::anyhow!("cannot decode {signature}: {error}"))
    }

    /// The number of proposals the governor has seen.
    pub async fn proposal_count(&self) -> Result<u64> {
        let output = self.governor_call("proposalCount", &[]).await?;

        as_u256(&output, 0)
            .map(|value| value.as_u64())
            .ok_or_else(|| anyhow::anyhow!("proposalCount returned no value"))
    }

    /// Read one proposal.
    pub async fn proposal(&self, id: u64) -> Result<Proposal> {
        let output = self
            .governor_call("proposals", &[Token::Uint(U256::from(id))])
            .await?;

        let state = self
            .governor_call("state", &[Token::Uint(U256::from(id))])
            .await
            .ok()
            .and_then(|output| as_u256(&output, 0))
            .map(|value| value.low_u32() as u8)
            .unwrap_or(0);

        let proposal = Proposal {
            id,
            title: format!("Proposal #{id}"),
            description: String::new(),
            targets: Vec::new(),
            values: Vec::new(),
            signatures: Vec::new(),
            calldatas: Vec::new(),
            proposer: as_address(&output, 1)
                .map(|address| format!("{address:?}"))
                .unwrap_or_default(),
            status: ProposalStatus::from_u8(state),
            for_votes: as_u256(&output, 5)
                .map(|value| value.to_string())
                .unwrap_or_else(|| "0".to_string()),
            against_votes: as_u256(&output, 6)
                .map(|value| value.to_string())
                .unwrap_or_else(|| "0".to_string()),
            abstain_votes: as_u256(&output, 7)
                .map(|value| value.to_string())
                .unwrap_or_else(|| "0".to_string()),
            start_block: as_u256(&output, 3)
                .map(|value| value.as_u64())
                .unwrap_or(0),
            end_block: as_u256(&output, 4)
                .map(|value| value.as_u64())
                .unwrap_or(0),
            execution_block: as_u256(&output, 2)
                .map(|value| value.as_u64())
                .unwrap_or(0),
            created_at: 0,
        };

        self.seen.write().insert(id, proposal.status);

        Ok(proposal)
    }

    /// Read the most recent proposals.
    ///
    /// The governor exposes proposals by index, not as a list, so the range is
    /// walked from the newest downwards.
    pub async fn get_proposals(&self, status: Option<ProposalStatus>) -> Vec<Proposal> {
        let count = match self.proposal_count().await {
            Ok(count) => count,
            Err(error) => {
                warn!(%error, "cannot read the proposal count");
                return Vec::new();
            }
        };

        let mut proposals = Vec::new();

        // Proposals are 1-indexed on OpenZeppelin governors.
        for id in (1..=count).rev().take(200) {
            match self.proposal(id).await {
                Ok(proposal) => {
                    if status.map(|want| want == proposal.status).unwrap_or(true) {
                        proposals.push(proposal);
                    }
                }
                Err(error) => {
                    warn!(id, %error, "cannot read proposal");
                }
            }
        }

        proposals
    }

    /// Read one proposal by id.
    pub async fn get_proposal(&self, id: u64) -> Result<Proposal> {
        self.proposal(id).await
    }

    /// The votes a proposal received, read from its tally.
    ///
    /// The governor stores the aggregate tallies, not the individual ballots,
    /// so the individual votes come from the `VoteCast` logs, which the indexer
    /// has already written. The aggregate below is always the on-chain figure.
    pub async fn get_proposal_votes(&self, id: u64) -> Result<ProposalVotes> {
        let proposal = self.proposal(id).await?;

        Ok(ProposalVotes {
            proposal_id: id,
            votes: Vec::new(),
            for_votes: proposal.for_votes,
            against_votes: proposal.against_votes,
            abstain_votes: proposal.abstain_votes,
            voter_count: 0,
        })
    }

    /// The voting power of an account, read from the token.
    pub async fn get_voter_power(&self, address: &str) -> Result<String> {
        let address: Address = address
            .parse()
            .map_err(|error| anyhow::anyhow!("invalid address: {error}"))?;

        let output = self
            .token_call("getVotes", &[Token::Address(address)])
            .await?;

        Ok(as_u256(&output, 0)
            .map(|value| value.to_string())
            .unwrap_or_else(|| "0".to_string()))
    }

    /// The delegate an account has appointed.
    pub async fn get_delegate(&self, address: &str) -> Result<Delegate> {
        let address: Address = address
            .parse()
            .map_err(|error| anyhow::anyhow!("invalid address: {error}"))?;

        let delegatee = self
            .token_call("delegates", &[Token::Address(address)])
            .await
            .ok()
            .and_then(|output| as_address(&output, 0));

        let votes = self
            .get_voter_power(&format!("{address:?}"))
            .await
            .unwrap_or_else(|_| "0".to_string());

        let tokens = self
            .token_call("balanceOf", &[Token::Address(address)])
            .await
            .ok()
            .and_then(|output| as_u256(&output, 0))
            .map(|value| value.to_string())
            .unwrap_or_else(|| "0".to_string());

        Ok(Delegate {
            address: format!("{address:?}"),
            delegatee: delegatee.map(|address| format!("{address:?}")),
            votes,
            tokens,
            delegated_at: 0,
        })
    }

    /// True when the account has already voted on the proposal.
    pub async fn has_voted(&self, proposal_id: u64, voter: &str) -> Result<bool> {
        let voter: Address = voter
            .parse()
            .map_err(|error| anyhow::anyhow!("invalid address: {error}"))?;

        let output = self
            .governor_call(
                "hasVoted",
                &[Token::Uint(U256::from(proposal_id)), Token::Address(voter)],
            )
            .await?;

        Ok(matches!(output.first(), Some(Token::Bool(value)) if *value))
    }

    /// The quorum the governor requires for a proposal.
    pub async fn quorum(&self, block: u64) -> Result<U256> {
        let output = self
            .governor_call("quorum", &[Token::Uint(U256::from(block))])
            .await?;

        as_u256(&output, 0).ok_or_else(|| anyhow::anyhow!("quorum returned no value"))
    }

    /// The delay between a proposal and the start of voting.
    pub async fn voting_delay(&self) -> Result<u64> {
        let output = self.governor_call("votingDelay", &[]).await?;

        as_u256(&output, 0)
            .map(|value| value.as_u64())
            .ok_or_else(|| anyhow::anyhow!("votingDelay returned no value"))
    }

    /// The number of blocks a proposal stays open for voting.
    pub async fn voting_period(&self) -> Result<u64> {
        let output = self.governor_call("votingPeriod", &[]).await?;

        as_u256(&output, 0)
            .map(|value| value.as_u64())
            .ok_or_else(|| anyhow::anyhow!("votingPeriod returned no value"))
    }

    /// The minimum voting power required to submit a proposal.
    pub async fn proposal_threshold(&self) -> Result<U256> {
        let output = self.governor_call("proposalThreshold", &[]).await?;

        as_u256(&output, 0).ok_or_else(|| anyhow::anyhow!("proposalThreshold returned no value"))
    }

    /// Governance statistics aggregated from the proposals the governor holds.
    pub async fn get_stats(&self) -> GovernanceStats {
        let proposals = self.get_proposals(None).await;

        let total_proposals = proposals.len() as u64;
        let active_proposals = proposals
            .iter()
            .filter(|proposal| proposal.status == ProposalStatus::Active)
            .count() as u64;
        let passed_proposals = proposals
            .iter()
            .filter(|proposal| proposal.status == ProposalStatus::Executed)
            .count() as u64;
        let failed_proposals = proposals
            .iter()
            .filter(|proposal| proposal.status == ProposalStatus::Defeated)
            .count() as u64;

        let quorum_reached = proposals
            .iter()
            .filter(|proposal| {
                let for_votes = U256::from_dec_str(&proposal.for_votes).unwrap_or_default();
                for_votes >= U256::from(self.config.quorum)
            })
            .count() as u64;

        GovernanceStats {
            total_proposals,
            active_proposals,
            passed_proposals,
            failed_proposals,
            total_voters: 0,
            total_delegates: 0,
            quorum_reached,
        }
    }

    /// True when a proposal has enough for-votes to meet the quorum.
    pub async fn check_quorum(&self, proposal_id: u64) -> bool {
        match self.get_proposal_votes(proposal_id).await {
            Ok(votes) => {
                let for_votes = U256::from_dec_str(&votes.for_votes).unwrap_or_default();
                for_votes >= U256::from(self.config.quorum)
            }
            Err(error) => {
                warn!(proposal_id, %error, "cannot check quorum");
                false
            }
        }
    }
}

/// Read a `Uint` from a decoded output at an index.
fn as_u256(output: &[Token], index: usize) -> Option<U256> {
    match output.get(index) {
        Some(Token::Uint(value)) => Some(*value),
        _ => None,
    }
}

/// Read an `Address` from a decoded output at an index.
fn as_address(output: &[Token], index: usize) -> Option<Address> {
    match output.get(index) {
        Some(Token::Address(value)) => Some(*value),
        _ => None,
    }
}

/// The `ParamType` of an ABI signature's first argument, for callers that need
/// to build calldata themselves.
pub fn first_param_type(declaration: &str) -> Option<ParamType> {
    AbiParser::default()
        .parse_function(declaration)
        .ok()
        .and_then(|function| function.inputs.first().map(|param| param.kind.clone()))
}

// ============================================================================
// Utility Functions
// ============================================================================

/// Format vote weight.
pub fn format_votes(votes: &str) -> String {
    U256::from_dec_str(votes)
        .unwrap_or_default()
        .to_string()
}

/// Calculate voting power percentage.
pub fn voting_power_percentage(votes: &str, total: &str) -> f64 {
    let votes = U256::from_dec_str(votes).unwrap_or_default();
    let total = U256::from_dec_str(total).unwrap_or_default();

    if total.is_zero() {
        return 0.0;
    }

    // The ratio is taken in U256 so that a value above 2^128, which is common
    // for an 18-decimal supply, does not overflow the conversion.
    let scaled = votes.saturating_mul(U256::from(1_000_000u64)) / total;

    scaled.as_u64() as f64 / 10_000.0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_proposal_state_is_decoded_in_declaration_order() {
        assert_eq!(ProposalStatus::from_u8(0), ProposalStatus::Pending);
        assert_eq!(ProposalStatus::from_u8(1), ProposalStatus::Active);
        assert_eq!(ProposalStatus::from_u8(3), ProposalStatus::Defeated);
        assert_eq!(ProposalStatus::from_u8(4), ProposalStatus::Succeeded);
        assert_eq!(ProposalStatus::from_u8(7), ProposalStatus::Executed);
    }

    #[test]
    fn a_vote_support_value_is_decoded() {
        assert_eq!(VoteChoice::from_u8(0), Some(VoteChoice::Against));
        assert_eq!(VoteChoice::from_u8(1), Some(VoteChoice::For));
        assert_eq!(VoteChoice::from_u8(2), Some(VoteChoice::Abstain));
        assert_eq!(VoteChoice::from_u8(3), None);
    }

    #[test]
    fn voting_power_is_a_percentage() {
        let percentage = voting_power_percentage("500000000000000000000", "1000000000000000000000");

        assert!((percentage - 50.0).abs() < 0.1);
    }

    #[test]
    fn voting_power_handles_a_zero_total() {
        assert_eq!(voting_power_percentage("1", "0"), 0.0);
    }

    #[test]
    fn voting_power_does_not_overflow_on_a_large_supply() {
        // 2^200 is far beyond u128, so an unchecked conversion would wrap.
        let huge: U256 = U256::from(1u8) << 200;
        let percentage = voting_power_percentage(&huge.to_string(), &(huge * U256::from(4u8)).to_string());

        assert!((percentage - 25.0).abs() < 0.1);
    }

    #[test]
    fn format_votes_normalises_a_decimal_string() {
        assert_eq!(format_votes("00123"), "123");
        assert_eq!(format_votes("not a number"), "0");
    }

    #[test]
    fn the_governor_abi_declares_the_expected_functions() {
        let parsed: Vec<_> = GOVERNOR_ABI
            .iter()
            .map(|declaration| AbiParser::default().parse_function(declaration).unwrap())
            .collect();

        assert_eq!(parsed.len(), GOVERNOR_ABI.len());
        assert_eq!(parsed[0].name, "proposalCount");
        assert_eq!(parsed[1].name, "proposals");
        assert_eq!(parsed[1].outputs.len(), 10);
    }

    #[test]
    fn a_service_without_addresses_reports_itself_as_unconfigured() {
        let service = GovernanceService {
            config: Config::default(),
            rpc: Provider::<Http>::try_from("http://localhost:8545").unwrap(),
            governor: Address::zero(),
            token: Address::zero(),
            seen: Arc::new(RwLock::new(HashMap::new())),
        };

        assert!(!service.is_configured());
    }

    #[tokio::test]
    async fn a_call_without_a_governance_address_is_refused() {
        let service = GovernanceService {
            config: Config::default(),
            rpc: Provider::<Http>::try_from("http://localhost:8545").unwrap(),
            governor: Address::zero(),
            token: Address::zero(),
            seen: Arc::new(RwLock::new(HashMap::new())),
        };

        let error = service.proposal_count().await.unwrap_err();

        assert!(error.to_string().contains("GOVERNANCE_ADDRESS"));
    }

    #[tokio::test]
    async fn a_call_without_a_token_address_is_refused() {
        let service = GovernanceService {
            config: Config::default(),
            rpc: Provider::<Http>::try_from("http://localhost:8545").unwrap(),
            governor: Address::zero(),
            token: Address::zero(),
            seen: Arc::new(RwLock::new(HashMap::new())),
        };

        let error = service.get_voter_power("0x0000000000000000000000000000000000000001").await.unwrap_err();

        assert!(error.to_string().contains("TOKEN_ADDRESS"));
    }

    #[tokio::test]
    async fn an_invalid_address_is_rejected() {
        let service = GovernanceService {
            config: Config::default(),
            rpc: Provider::<Http>::try_from("http://localhost:8545").unwrap(),
            governor: Address::zero(),
            token: Address::zero(),
            seen: Arc::new(RwLock::new(HashMap::new())),
        };

        assert!(service.get_voter_power("0xnothex").await.is_err());
    }

    #[test]
    fn an_abi_signature_yields_its_argument_type() {
        let kind = first_param_type("proposals(uint256) view returns (uint256)");

        assert_eq!(kind, Some(ParamType::Uint(256)));
    }
}
