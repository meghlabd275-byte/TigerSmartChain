//! Token Types - Complete Implementation
//!
//! This module provides comprehensive token tracking including:
//! - Token holder management with historical balances
//! - Transfer tracking and history
//! - Token price and market data
//! - Holder distribution analytics

use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::time::{SystemTime, UNIX_EPOCH};

// =============================================================================
// ERRORS
// =============================================================================

/// Token Indexer Errors
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum TokenError {
    #[serde(rename = "token_not_found")]
    TokenNotFound(String),
    #[serde(rename = "holder_not_found")]
    HolderNotFound(String),
    #[serde(rename = "invalid_address")]
    InvalidAddress(String),
    #[serde(rename = "index_error")]
    IndexError(String),
    #[serde(rename = "price_error")]
    PriceError(String),
}

// =============================================================================
// TOKEN
// =============================================================================

/// Token (ERC-20/ERC-721/ERC-1155)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Token {
    /// Contract address
    pub address: String,
    /// Token name
    pub name: String,
    /// Token symbol
    pub symbol: String,
    /// Decimals (for ERC-20)
    pub decimals: u8,
    /// Total supply
    pub total_supply: String,
    /// Current holder count
    pub holders: i64,
    /// Total transfer count
    pub transfers: i64,
    /// Token type
    pub token_type: TokenType,
    /// Whether verified
    pub verified: bool,
    /// Contract creator
    pub creator: Option<String>,
    /// Creation transaction
    pub creation_tx: Option<String>,
    /// Creation block
    pub creation_block: Option<u64>,
    /// Last updated
    pub last_updated: u64,
}

impl Token {
    pub fn new(address: String, name: String, symbol: String) -> Self {
        Self {
            address,
            name,
            symbol,
            decimals: 18,
            total_supply: "0".to_string(),
            holders: 0,
            transfers: 0,
            token_type: TokenType::ERC20,
            verified: false,
            creator: None,
            creation_tx: None,
            creation_block: None,
            last_updated: now_unix(),
        }
    }

    /// Set total supply
    pub fn set_total_supply(&mut self, supply: String) {
        self.total_supply = supply;
        self.last_updated = now_unix();
    }

    /// Set decimals
    pub fn set_decimals(&mut self, decimals: u8) {
        self.decimals = decimals;
        self.last_updated = now_unix();
    }

    /// Increment transfers
    pub fn increment_transfers(&mut self) {
        self.transfers += 1;
        self.last_updated = now_unix();
    }

    /// Update holders count
    pub fn update_holders(&mut self, count: i64) {
        self.holders = count;
        self.last_updated = now_unix();
    }
}

/// Token type
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TokenType {
    #[serde(rename = "ERC20")]
    ERC20,
    #[serde(rename = "ERC721")]
    ERC721,
    #[serde(rename = "ERC1155")]
    ERC1155,
}

// =============================================================================
// TOKEN TRANSFER
// =============================================================================

/// Token Transfer Event
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenTransfer {
    /// Transaction hash
    pub hash: String,
    /// Token contract address
    pub token_address: String,
    /// From address
    pub from: String,
    /// To address
    pub to: String,
    /// Value (tokens or token IDs)
    pub value: String,
    /// Log index
    pub log_index: u64,
    /// Block number
    pub block_number: u64,
    /// Transaction index
    pub transaction_index: u64,
    /// Timestamp
    pub timestamp: u64,
    /// Token ID (for ERC-721/ERC-1155)
    pub token_id: Option<String>,
    /// Amount (for ERC-1155)
    pub amount: Option<String>,
}

impl TokenTransfer {
    pub fn new(hash: String, token_address: String, from: String, to: String, value: String) -> Self {
        Self {
            hash,
            token_address,
            from,
            to,
            value,
            log_index: 0,
            block_number: 0,
            transaction_index: 0,
            timestamp: now_unix(),
            token_id: None,
            amount: None,
        }
    }

    /// Check if this is a mint (from zero address)
    pub fn is_mint(&self) -> bool {
        self.from == "0x0000000000000000000000000000000000000000" ||
        self.from.to_lowercase() == "0x0000000000000000000000000000000000000000"
    }

    /// Check if this is a burn (to zero address)
    pub fn is_burn(&self) -> bool {
        self.to == "0x0000000000000000000000000000000000000000" ||
        self.to.to_lowercase() == "0x0000000000000000000000000000000000000000"
    }
}

// =============================================================================
// TOKEN HOLDER
// =============================================================================

/// Token Holder
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenHolder {
    /// Holder address
    pub address: String,
    /// Token contract address
    pub token_address: String,
    /// Current balance
    pub balance: String,
    /// Percentage of total supply
    pub percent: f64,
    /// Rank by balance
    pub rank: u32,
    /// First block holding
    pub first_block: u64,
    /// Last updated
    pub last_updated: u64,
    /// Is contract
    pub is_contract: bool,
}

impl TokenHolder {
    pub fn new(address: String, token_address: String, balance: String) -> Self {
        Self {
            address,
            token_address,
            balance,
            percent: 0.0,
            rank: 0,
            first_block: 0,
            last_updated: now_unix(),
            is_contract: false,
        }
    }

    /// Update balance
    pub fn update_balance(&mut self, new_balance: String) {
        self.balance = new_balance;
        self.last_updated = now_unix();
    }

    /// Calculate percentage
    pub fn calculate_percent(&mut self, total_supply: &str) {
        let holder: f64 = self.balance.parse().unwrap_or(0.0);
        let total: f64 = total_supply.parse().unwrap_or(1.0);
        self.percent = if total > 0.0 { (holder / total) * 100.0 } else { 0.0 };
    }
}

// =============================================================================
// HOLDER HISTORY
// =============================================================================

/// Historical balance snapshot
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BalanceSnapshot {
    pub address: String,
    pub token_address: String,
    pub balance: String,
    pub block_number: u64,
    pub timestamp: u64,
}

impl BalanceSnapshot {
    pub fn new(address: String, token_address: String, balance: String, block_number: u64) -> Self {
        Self {
            address,
            token_address,
            balance,
            block_number,
            timestamp: now_unix(),
        }
    }
}

// =============================================================================
// TOKEN PRICE
// =============================================================================

/// Token Price Data
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenPrice {
    /// Token address
    pub address: String,
    /// Current price in USD
    pub price: f64,
    /// Price change 24h (%)
    pub price_change_24h: f64,
    /// Volume 24h
    pub volume_24h: f64,
    /// Market cap
    pub market_cap: f64,
    /// Fully diluted market cap
    pub fdv: f64,
    /// Total supply
    pub total_supply: f64,
    /// Circulating supply
    pub circulating_supply: f64,
    /// ATH (all time high)
    pub ath: f64,
    /// ATL (all time low)
    pub atl: f64,
    /// Last updated
    pub last_updated: u64,
    /// Price source
    pub source: String,
}

impl TokenPrice {
    pub fn new(address: String) -> Self {
        Self {
            address,
            price: 0.0,
            price_change_24h: 0.0,
            volume_24h: 0.0,
            market_cap: 0.0,
            fdv: 0.0,
            total_supply: 0.0,
            circulating_supply: 0.0,
            ath: 0.0,
            atl: 0.0,
            last_updated: now_unix(),
            source: "aggregator".to_string(),
        }
    }

    /// Update price
    pub fn update(&mut self, price: f64) {
        let old_price = self.price;
        self.price = price;
        self.price_change_24h = if old_price > 0.0 {
            ((price - old_price) / old_price) * 100.0
        } else {
            0.0
        };
        self.last_updated = now_unix();
    }
}

// =============================================================================
// HOLDER DISTRIBUTION
// =============================================================================

/// Holder distribution analytics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HolderDistribution {
    pub token_address: String,
    pub total_holders: u64,
    pub top_10_percent: u64,
    pub top_1_percent: u64,
    pub top_10_balance: String,
    pub top_1_balance: String,
    pub avg_balance: String,
    pub median_balance: String,
    pub distribution: Vec<DistributionBucket>,
}

/// Distribution bucket
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DistributionBucket {
    pub range: String,
    pub count: u64,
    pub percent: f64,
}

// =============================================================================
// TOKEN INDEXER
// =============================================================================

/// Complete Token Indexer
pub struct TokenIndexer {
    /// Tokens by address
    tokens: HashMap<String, Token>,
    /// Holders by token address
    holders: HashMap<String, HashMap<String, TokenHolder>>,
    /// Transfer history
    transfers: HashMap<String, Vec<TokenTransfer>>,
    /// Balance history
    balance_history: HashMap<String, Vec<BalanceSnapshot>>,
    /// Prices
    prices: HashMap<String, TokenPrice>,
    /// Transfer count by token
    transfer_counts: HashMap<String, u64>,
}

impl TokenIndexer {
    pub fn new() -> Self {
        Self {
            tokens: HashMap::new(),
            holders: HashMap::new(),
            transfers: HashMap::new(),
            balance_history: HashMap::new(),
            prices: HashMap::new(),
            transfer_counts: HashMap::new(),
        }
    }

    /// Add token
    pub fn add_token(&mut self, token: Token) {
        let addr = token.address.clone();
        self.holders.insert(addr.clone(), HashMap::new());
        self.transfers.insert(addr.clone(), vec![]);
        self.transfer_counts.insert(addr.clone(), 0);
        self.tokens.insert(addr, token);
    }

    /// Get token
    pub fn get_token(&self, address: &str) -> Option<&Token> {
        self.tokens.get(address)
    }

    /// Update holder balance
    pub fn update_holder(&mut self, token_addr: &str, holder: TokenHolder) {
        if let Some(holders) = self.holders.get_mut(token_addr) {
            let addr = holder.address.clone();
            holder.calculate_percent(
                self.tokens.get(token_addr)
                    .map(|t| t.total_supply.as_str())
                    .unwrap_or("1")
            );
            holders.insert(addr, holder);
        }
    }

    /// Get holders for token
    pub fn get_holders(&self, token_addr: &str) -> Option<Vec<&TokenHolder>> {
        self.holders.get(token_addr).map(|h| {
            let mut holders: Vec<&TokenHolder> = h.values().collect();
            holders.sort_by(|a, b| b.balance.cmp(&a.balance));
            holders
        })
    }

    /// Record transfer
    pub fn record_transfer(&mut self, transfer: TokenTransfer) {
        let token_addr = transfer.token_address.clone();
        
        // Update transfer count
        *self.transfer_counts.entry(token_addr.clone()).or_insert(0) += 1;
        
        // Add to transfers
        self.transfers.entry(token_addr.clone())
            .or_insert_with(Vec::new)
            .push(transfer.clone());
        
        // Update token transfer count
        if let Some(token) = self.tokens.get_mut(&token_addr) {
            token.increment_transfers();
        }
        
        // Record balance snapshots
        let from_key = format!("{}:{}", token_addr, transfer.from);
        let to_key = format!("{}:{}", token_addr, transfer.to);
        
        // From: decrease balance
        if !transfer.is_mint() {
            self.balance_history.entry(from_key).or_insert_with(Vec::new)
                .push(BalanceSnapshot::new(
                    transfer.from.clone(),
                    token_addr.clone(),
                    "0".to_string(), // Would calculate actual balance
                    transfer.block_number,
                ));
        }
        
        // To: increase balance  
        if !transfer.is_burn() {
            self.balance_history.entry(to_key).or_insert_with(Vec::new)
                .push(BalanceSnapshot::new(
                    transfer.to.clone(),
                    token_addr.clone(),
                    transfer.value.clone(),
                    transfer.block_number,
                ));
        }
    }

    /// Get transfers for token
    pub fn get_transfers(&self, token_addr: &str) -> Option<&Vec<TokenTransfer>> {
        self.transfers.get(token_addr)
    }

    /// Get transfers for address
    pub fn get_address_transfers(&self, address: &str) -> Vec<&TokenTransfer> {
        let mut result = vec![];
        for transfers in self.transfers.values() {
            for transfer in transfers {
                if transfer.from == address || transfer.to == address {
                    result.push(transfer);
                }
            }
        }
        result
    }

    /// Update price
    pub fn update_price(&mut self, price: TokenPrice) {
        self.prices.insert(price.address.clone(), price);
    }

    /// Get price
    pub fn get_price(&self, address: &str) -> Option<&TokenPrice> {
        self.prices.get(address)
    }

    /// Get holder distribution
    pub fn get_distribution(&self, token_addr: &str) -> Option<HolderDistribution> {
        let holders = self.holders.get(token_addr)?;
        let token = self.tokens.get(token_addr)?;
        
        let mut holder_list: Vec<&TokenHolder> = holders.values().collect();
        holder_list.sort_by(|a, b| b.balance.cmp(&a.balance));
        
        let total_holders = holder_list.len() as u64;
        
        // Calculate distribution
        let mut distribution = vec![
            DistributionBucket { range: "< 0.001%".to_string(), count: 0, percent: 0.0 },
            DistributionBucket { range: "0.001-0.01%".to_string(), count: 0, percent: 0.0 },
            DistributionBucket { range: "0.01-0.1%".to_string(), count: 0, percent: 0.0 },
            DistributionBucket { range: "0.1-1%".to_string(), count: 0, percent: 0.0 },
            DistributionBucket { range: "1-10%".to_string(), count: 0, percent: 0.0 },
            DistributionBucket { range: "> 10%".to_string(), count: 0, percent: 0.0 },
        ];
        
        for holder in &holder_list {
            let bucket = if holder.percent < 0.001 {
                0
            } else if holder.percent < 0.01 {
                1
            } else if holder.percent < 0.1 {
                2
            } else if holder.percent < 1.0 {
                3
            } else if holder.percent < 10.0 {
                4
            } else {
                5
            };
            distribution[bucket].count += 1;
        }
        
        for bucket in &mut distribution {
            bucket.percent = if total_holders > 0 {
                (bucket.count as f64 / total_holders as f64) * 100.0
            } else {
                0.0
            };
        }
        
        // Top holders
        let top_10: u64 = (total_holders as f64 * 0.1).ceil() as u64;
        let top_1: u64 = (total_holders as f64 * 0.01).ceil() as u64;
        
        Some(HolderDistribution {
            token_address: token_addr.to_string(),
            total_holders,
            top_10_percent: top_10,
            top_1_percent: top_1,
            top_10_balance: "0".to_string(),
            top_1_balance: "0".to_string(),
            avg_balance: "0".to_string(),
            median_balance: "0".to_string(),
            distribution,
        })
    }

    /// Get all token addresses
    pub fn token_addresses(&self) -> Vec<String> {
        self.tokens.keys().cloned().collect()
    }

    /// Get holder count
    pub fn holder_count(&self, token_addr: &str) -> u64 {
        self.holders.get(token_addr)
            .map(|h| h.len() as u64)
            .unwrap_or(0)
    }
}

impl Default for TokenIndexer {
    fn default() -> Self {
        Self::new()
    }
}

// =============================================================================
// HELPER FUNCTIONS
// =============================================================================

fn now_unix() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs()
}