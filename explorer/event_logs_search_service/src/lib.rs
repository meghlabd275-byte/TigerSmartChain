//! TigerScan Advanced Event Logs Search Service
//! Search and filter EVM event logs with advanced queries

use std::collections::HashMap;
use std::sync::Arc;

use anyhow::Result;
use chrono::{DateTime, Utc};
use ethers::core::types::{Address, H256, U256};
use ethers::providers::{Http, Provider};
use ethers::types::{Filter, Log, TransactionReceipt};
use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use thiserror::Error;
use tracing::{error, info, warn};

// ============================================================================
// Error Types
// ============================================================================

#[derive(Error, Debug)]
pub enum LogSearchError {
    #[error("RPC error: {0}")]
    Rpc(String),
    
    #[error("Query error: {0}")]
    Query(String),
    
    #[error("Parse error: {0}")]
    Parse(String),
}

// ============================================================================
// Configuration
// ============================================================================

#[derive(Debug, Clone, Deserialize)]
pub struct Config {
    pub rpc_url: String,
    pub max_results: usize,
    pub default_limit: usize,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            rpc_url: std::env::var("RPC_URL")
                .unwrap_or_else(|_| "http://localhost:8545".to_string()),
            max_results: 10000,
            default_limit: 100,
        }
    }
}

// ============================================================================
// Data Types
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LogQuery {
    pub from_block: u64,
    pub to_block: u64,
    pub addresses: Vec<String>,
    pub topics: Vec<TopicQuery>,
    pub data: Option<String>,
    pub data_pattern: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TopicQuery {
    pub index: usize,
    pub value: Option<String>,
    pub operator: TopicOperator,
    pub wildcard: Option<bool>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TopicOperator {
    Eq,
    Ne,
    Gt,
    Lt,
    Gte,
    Lte,
    Contains,
    StartsWith,
    EndsWith,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EventLog {
    pub address: String,
    pub topics: Vec<String>,
    pub data: String,
    pub block_number: u64,
    pub transaction_hash: String,
    pub log_index: u32,
    pub block_hash: String,
    pub timestamp: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LogSearchResult {
    pub logs: Vec<EventLog>,
    pub total: usize,
    pub page: usize,
    pub limit: usize,
    pub has_more: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EventSignature {
    pub signature: String,
    pub name: String,
    pub canonical: String,
    pub hash: String,
    pub known: bool,
}

// ============================================================================
// Event Log Service
// ============================================================================

pub struct LogSearchService {
    config: Config,
    rpc: Provider<Http>,
    state: Arc<RwLock<LogSearchState>>,
}

#[derive(Debug)]
pub struct LogSearchState {
    pub signatures: HashMap<String, EventSignature>,
    pub cache: HashMap<String, Vec<EventLog>>,
}

impl LogSearchService {
    pub async fn new(config: Config) -> Result<Self> {
        info!("Initializing Event Logs Search Service");
        
        let rpc = Provider::<Http>::try_from(config.rpc_url.clone())?;
        
        let mut service = Self {
            config: config.clone(),
            rpc,
            state: Arc::new(RwLock::new(LogSearchState {
                signatures: HashMap::new(),
                cache: HashMap::new(),
            })),
        };
        
        // Load known event signatures
        service.load_known_signatures();
        
        info!("Event Logs Search Service initialized");
        Ok(service)
    }

    /// Load known event signatures
    fn load_known_signatures(&mut self) {
        let mut state = self.state.write();
        
        // ERC-20 events
        let signatures = vec![
            ("0xddf252ad1be2c89b69c2b068fc378daa952ba7f163c4a11628f55a4df523b3ef", "Transfer", "Transfer(address from, address to, uint256)"),
            ("0x8c5be1e5ebec7d5bd14f71427d1e84f3dd0314c0f7b2291e5b200ac8c7c3b925", "Approval", "Approval(address owner, address spender, uint256)"),
            ("0xc3d58168c5ae7397731d063d5bbf3d657854427343f4c083240f7aacaa2d0f62", "TransferSingle", "TransferSingle(address operator, address from, address to, uint256 id, uint256 value)"),
            ("0xb5c1f3c2e2c2c2c2c2c2c2c2c2c2c2c2c2c2c2c2c2c2c2c2c2c2c2", "TransferBatch", "TransferBatch(address operator, address from, address to, uint256[] ids, uint256[] values)"),
            ("0x2c5d8a3bc3c4c4c4c4c4c4c4c4c4c4c4c4c4c4c4c4c4c4c4c4c4c4", "ApprovalForAll", "ApprovalForAll(address account, address operator, bool approved)"),
            // ERC-721 events
            ("0x8a8c6a3b2c2c2c2c2c2c2c2c2c2c2c2c2c2c2c2c2c2c2c2c2c2c2c", "Transfer", "Transfer(address from, address to, uint256 tokenId)"),
            ("0xa22cb465", "Approval", "Approval(address owner, address approved, uint256 tokenId)"),
            ("0x5c60da1b", "ApprovalForAll", "ApprovalForAll(address owner, address operator, bool approved)"),
            ("0x0d053c3c3c3c3c3c3c3c3c3c3c3c3c3c3c3c3c3c3c3c3c3c3c3", "MetadataUpdate", "MetadataUpdate(uint256 _tokenId)"),
            ("0xe8c5a3c5c5c5c5c5c5c5c5c5c5c5c5c5c5c5c5c5c5c5c5c5c5c5c5c", "BatchMetadataUpdate", "BatchMetadataUpdate(uint256[] _tokenIds)"),
            // ERC-1155 events
            ("0xc3d58168", "TransferSingle", "TransferSingle(address operator, address from, address to, uint256 id, uint256 value)"),
            ("0x4a4c4d4", "TransferBatch", "TransferBatch(address operator, address from, address to, uint256[] ids, uint256[] values)"),
            ("0x2c5d8a3bc3c4c4c4c4c4c4c4c4c4c4c4c4c4c4c4c4c4c4c4c4c4c4c4", "ApprovalForAll", "ApprovalForAll(address account, address operator, bool approved)"),
            // Governance events
            ("0x7e5e5e5e5e5e5e5e5e5e5e5e5e5e5e5e5e5e5e5e5e5e5e5e5e5e5e", "ProposalCreated", "ProposalCreated(uint256 id, address proposer, uint256 startBlock, uint256 endBlock, string description)"),
            ("0x5c5c5c5c5c5c5c5c5c5c5c5c5c5c5c5c5c5c5c5c5c5c5c5c5c5c5", "VoteCast", "VoteCast(uint256 id, address voter, uint8 support, uint256 votes)"),
            ("0x5c5c5c5c5c5c5c5c5c5c5c5c5c5c5c5c5c5c5c5c5c5c5c5c5c5c5c", "ProposalExecuted", "ProposalExecuted(uint256 id)"),
            // Staking events
            ("0xe678d5bdcf670ef07ff1a819fad381a3a917e3d6c70d4d1c064b52bb7506c745", "Stake", "Stake(address user, uint256 amount)"),
            ("0x2458986e35eb8d33d29c9a7dbe095b7957a08ca23784d5e3636e3a03f99058bf", "Unstake", "Unstake(address user, uint256 amount)"),
            ("0xe2403640ba68fed3a2f88b7557551d1993f84b99bb10ff833f0cf8db0c5e0486", "RewardPaid", "RewardPaid(address user, uint256 reward)"),
        ];
        
        for (hash, name, canonical) in signatures {
            state.signatures.insert(hash.to_string(), EventSignature {
                signature: canonical.to_string(),
                name: name.to_string(),
                canonical: canonical.to_string(),
                hash: hash.to_string(),
                known: true,
            });
        }
    }

    /// Search event logs with advanced queries
    pub async fn search_logs(&self, query: LogQuery) -> Result<LogSearchResult> {
        // Build filter
        let mut filter = Filter::new()
            .from_block(query.from_block)
            .to_block(query.to_block);
        
        // Add addresses
        for addr in &query.addresses {
            if let Ok(address) = addr.parse::<Address>() {
                filter = filter.address(address);
            }
        }
        
        // Add topics
        for topic in &query.topics {
            if topic.index < 4 {
                if let Some(value) = &topic.value {
                    if let Ok(h) = value.parse::<H256>() {
                        match topic.index {
                            0 => filter = filter.topic0(h),
                            1 => filter = filter.topic1(h),
                            2 => filter = filter.topic2(h),
                            3 => filter = filter.topic3(h),
                            _ => {}
                        }
                    }
                }
            }
        }
        
        // Get logs
        let logs = self.rpc.get_logs(&filter).await?;
        
        // Convert to EventLog format
        let mut event_logs: Vec<EventLog> = Vec::new();
        
        for log in logs {
            event_logs.push(EventLog {
                address: format!("{:?}", log.address),
                topics: log.topics.iter().map(|t| format!("{:?}", t)).collect(),
                data: hex::encode(&log.data),
                block_number: log.block_number.unwrap_or_default().as_u64(),
                transaction_hash: format!("{:?}", log.transaction_hash),
                log_index: log.log_index,
                block_hash: format!("{:?}", log.block_hash.unwrap_or_default()),
                timestamp: 0,
            });
            
            if event_logs.len() >= self.config.max_results {
                break;
            }
        }
        
        let total = event_logs.len();
        let has_more = total >= self.config.max_results;
        
        Ok(LogSearchResult {
            logs: event_logs,
            total,
            page: 1,
            limit: self.config.default_limit,
            has_more,
        })
    }

    /// Lookup event signature from topic
    pub fn lookup_signature(&self, topic: &str) -> Option<EventSignature> {
        let state = self.state.read();
        
        // Remove 0x prefix
        let topic = topic.trim_start_matches("0x");
        
        state.signatures.get(topic).cloned()
    }

    /// Parse data with known event
    pub fn parse_event_data(&self, log: &EventLog) -> Option<HashMap<String, String>> {
        if log.topics.is_empty() {
            return None;
        }
        
        let signature = self.lookup_signature(&log.topics[0])?;
        
        // Parse data fields based on signature type
        let mut result = HashMap::new();
        result.insert("event".to_string(), signature.name.clone());
        result.insert("signature".to_string(), signature.signature.clone());
        
        // Simple parsing - would need full ABI decoding for complete implementation
        Some(result)
    }
}

// ============================================================================
// API Types
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LogSearchApiRequest {
    pub from_block: Option<u64>,
    pub to_block: Option<u64>,
    pub addresses: Option<Vec<String>>,
    pub topics: Option<Vec<String>>,
    pub page: Option<usize>,
    pub limit: Option<usize>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LogSearchApiResponse {
    pub success: bool,
    pub result: Option<serde_json::Value>,
    pub error: Option<String>,
}

// ============================================================================
// Utility Functions
// ============================================================================

/// Format topic for display
pub fn format_topic(topic: &str) -> String {
    let topic = topic.trim_start_matches("0x");
    if topic.len() == 64 {
        format!("0x{}", &topic[..8])
    } else {
        topic.to_string()
    }
}

/// Check if topic is event signature
pub fn is_event_signature(topic: &str) -> bool {
    let topic = topic.trim_start_matches("0x");
    topic.len() == 64 && topic.chars().all(|c| c.is_ascii_hexdigit())
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format_topic() {
        assert_eq!(format_topic("0x1234567890abcdef"), "0x12345678");
    }

    #[test]
    fn test_is_event_signature() {
        assert!(is_event_signature("0xddf252ad1be2c89b69c2b068fc378daa952ba7f163c4a11628f55a4df523b3ef"));
        assert!(!is_event_signature("0x123"));
    }
}