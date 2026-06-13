//! WebSocket Subscription Management
//! Secure subscription handling with filters

use std::collections::HashSet;

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::error::{Error, Result};
use crate::events::EventType;

/// Subscription channel
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Channel {
    /// Blocks channel
    Blocks,
    /// Transactions channel
    Transactions,
    /// Pending transactions channel
    PendingTransactions,
    /// Token transfers channel
    TokenTransfers,
    /// Token approvals channel
    TokenApprovals,
    /// NFT transfers channel
    NftTransfers,
    /// Contracts channel
    Contracts,
    /// Validators channel
    Validators,
    /// Custom channel
    Custom,
}

impl Channel {
    /// Get channel name
    pub fn name(&self) -> &'static str {
        match self {
            Self::Blocks => "blocks",
            Self::Transactions => "transactions",
            Self::PendingTransactions => "pending_transactions",
            Self::TokenTransfers => "token_transfers",
            Self::TokenApprovals => "token_approvals",
            Self::NftTransfers => "nft_transfers",
            Self::Contracts => "contracts",
            Self::Validators => "validators",
            Self::Custom => "custom",
        }
    }
    
    /// Parse from string
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "blocks" => Some(Self::Blocks),
            "transactions" => Some(Self::Transactions),
            "pending_transactions" => Some(Self::PendingTransactions),
            "token_transfers" => Some(Self::TokenTransfers),
            "token_approvals" => Some(Self::TokenApprovals),
            "nft_transfers" => Some(Self::NftTransfers),
            "contracts" => Some(Self::Contracts),
            "validators" => Some(Self::Validators),
            "custom" => Some(Self::Custom),
            _ => None,
        }
    }
    
    /// Get event type
    pub fn event_type(&self) -> EventType {
        match self {
            Self::Blocks => EventType::Block,
            Self::Transactions => EventType::Tx,
            Self::PendingTransactions => EventType::PendingTx,
            Self::TokenTransfers => EventType::TokenTransfer,
            Self::TokenApprovals => EventType::TokenApproval,
            Self::NftTransfers => EventType::NftTransfer,
            Self::Contracts => EventType::Contract,
            Self::Validators => EventType::Validator,
            Self::Custom => EventType::Custom,
        }
    }
}

/// Subscription filter
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Filter {
    /// Address filter (for contracts/tokens)
    pub address: Option<String>,
    /// From address filter
    pub from: Option<String>,
    /// To address filter
    pub to: Option<String>,
    /// Block range start
    pub from_block: Option<u64>,
    /// Block range end
    pub to_block: Option<u64>,
    /// Topic filters
    pub topics: Vec<Option<String>>,
    /// Value range
    pub value_min: Option<String>,
    pub value_max: Option<String>,
}

impl Filter {
    /// Create new filter
    pub fn new() -> Self {
        Self::default()
    }
    
    /// Set address filter
    pub fn with_address(mut self, address: String) -> Self {
        self.address = Some(address);
        self
    }
    
    /// Set from address filter
    pub fn with_from(mut self, from: String) -> Self {
        self.from = Some(from);
        self
    }
    
    /// Set to address filter
    pub fn with_to(mut self, to: String) -> Self {
        self.to = Some(to);
        self
    }
    
    /// Set block range
    pub fn with_block_range(mut self, from: u64, to: u64) -> Self {
        self.from_block = Some(from);
        self.to_block = Some(to);
        self
    }
    
    /// Add topic filter
    pub fn with_topic(mut self, topic: String) -> Self {
        if self.topics.len() < 4 {
            self.topics.push(Some(topic));
        }
        self
    }
    
    /// Check if event matches filter
    pub fn matches(&self, _event: &crate::events::Event) -> bool {
        // Simplified matching - in production would check all filter fields
        true
    }
}

/// Subscription entity
#[derive(Debug, Clone)]
pub struct Subscription {
    id: String,
    channel: Channel,
    filter: Filter,
    created_at: u64,
}

impl Subscription {
    /// Create new subscription
    pub fn new(channel: Channel, filter: Filter) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            channel,
            filter,
            created_at: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs(),
        }
    }
    
    /// Get subscription ID
    pub fn id(&self) -> &str {
        &self.id
    }
    
    /// Get channel
    pub fn channel(&self) -> Channel {
        self.channel
    }
    
    /// Get filter
    pub fn filter(&self) -> &Filter {
        &self.filter
    }
    
    /// Check if event matches subscription
    pub fn matches(&self, event: &crate::events::Event) -> bool {
        // Check channel matches event type
        if self.channel.event_type() != event.event_type {
            return false;
        }
        // Check filter
        self.filter.matches(event)
    }
}

/// Subscription manager
#[derive(Debug, Clone, Default)]
pub struct SubscriptionManager {
    subscriptions: parking_lot::RwLock<HashSet<Subscription>>,
}

impl SubscriptionManager {
    /// Create new subscription manager
    pub fn new() -> Self {
        Self::default()
    }
    
    /// Add subscription
    pub fn add(&self, subscription: Subscription) {
        self.subscriptions.write().insert(subscription);
    }
    
    /// Remove subscription
    pub fn remove(&self, id: &str) {
        self.subscriptions.write().retain(|s| s.id() != id);
    }
    
    /// Get subscriptions for event
    pub fn get_subscriptions(&self, event: &crate::events::Event) -> Vec<Subscription> {
        self.subscriptions
            .read()
            .iter()
            .filter(|s| s.matches(event))
            .cloned()
            .collect()
    }
    
    /// Get subscription count
    pub fn count(&self) -> usize {
        self.subscriptions.read().len()
    }
}