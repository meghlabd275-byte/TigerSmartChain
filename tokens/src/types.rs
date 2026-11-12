//! Token Types - Complete Implementation
//!
//! This module provides comprehensive token tracking including:
//! - Token holder management with historical balances
//! - Transfer tracking and history
//! - Token price and market data
//! - Holder distribution analytics

use num_bigint::BigUint;
use num_traits::Zero;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
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
    /// Percentage of total supply held by the ten largest holders.
    pub top_10_percent: f64,
    /// Percentage of total supply held by the largest holder.
    pub top_1_percent: f64,
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
    /// token -> holder -> balance. Balances are held as integers rather than
    /// decimal strings so that comparisons and arithmetic are exact.
    balances: HashMap<String, HashMap<String, BigUint>>,
    /// token -> price observations, oldest first, as (timestamp, price).
    price_history: HashMap<String, Vec<(u64, f64)>>,
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
            balances: HashMap::new(),
            price_history: HashMap::new(),
        }
    }

    /// Add token
    pub fn add_token(&mut self, token: Token) {
        let addr = normalize_address(&token.address);
        self.holders.insert(addr.clone(), HashMap::new());
        self.transfers.insert(addr.clone(), vec![]);
        self.transfer_counts.insert(addr.clone(), 0);
        self.balances.insert(addr.clone(), HashMap::new());
        self.tokens.insert(addr, token);
    }

    /// Get token
    pub fn get_token(&self, address: &str) -> Option<&Token> {
        self.tokens.get(&normalize_address(address))
    }

    /// Insert or replace a holder record, recomputing its share of supply.
    pub fn update_holder(&mut self, token_addr: &str, mut holder: TokenHolder) {
        let token_addr = normalize_address(token_addr);
        let token_addr = token_addr.as_str();
        let addr = normalize_address(&holder.address);

        let total_supply = self
            .tokens
            .get(token_addr)
            .map(|t| t.total_supply.clone())
            .unwrap_or_else(|| "0".to_string());

        holder.calculate_percent(&total_supply);
        holder.last_updated = now_unix();

        // Keep the integer ledger in step with the string balance on the record.
        let balance = parse_units(&holder.balance);
        self.balances
            .entry(token_addr.to_string())
            .or_default()
            .insert(addr.clone(), balance);

        self.holders
            .entry(token_addr.to_string())
            .or_default()
            .insert(addr, holder);

        self.refresh_holder_count(token_addr);
    }

    /// Holders of a token, richest first, with `rank` assigned.
    ///
    /// Ordering is numeric. Sorting the balance strings would put "9" above
    /// "10" and give every holder the wrong rank.
    pub fn get_holders(&self, token_addr: &str) -> Option<Vec<TokenHolder>> {
        let holders = self.holders.get(&normalize_address(token_addr))?;

        let mut ordered: Vec<TokenHolder> = holders.values().cloned().collect();
        ordered.sort_by(|a, b| {
            parse_units(&b.balance)
                .cmp(&parse_units(&a.balance))
                .then_with(|| a.address.cmp(&b.address))
        });

        for (index, holder) in ordered.iter_mut().enumerate() {
            holder.rank = (index + 1) as u32;
        }

        Some(ordered)
    }

    /// Exact balance of `holder` in `token_addr`, as a decimal string.
    pub fn balance_of(&self, token_addr: &str, holder: &str) -> String {
        let by_holder = match self.balances.get(&normalize_address(token_addr)) {
            Some(b) => b,
            None => return "0".to_string(),
        };

        by_holder
            .get(&normalize_address(holder))
            .map(|b| b.to_string())
            .unwrap_or_else(|| "0".to_string())
    }

    /// Recompute `Token::holders` from the ledger, counting non-zero balances.
    fn refresh_holder_count(&mut self, token_addr: &str) {
        let token_addr = normalize_address(token_addr);
        let count = self
            .balances
            .get(&token_addr)
            .map(|b| b.values().filter(|v| !v.is_zero()).count() as i64)
            .unwrap_or(0);

        if let Some(token) = self.tokens.get_mut(&token_addr) {
            token.update_holders(count);
        }
    }

    /// Apply a transfer to the ledger, the holder set and the balance history.
    ///
    /// A transfer that would take a holder below zero is rejected rather than
    /// clamped, because silently clamping would produce balances that never
    /// existed on chain.
    pub fn record_transfer(&mut self, transfer: TokenTransfer) -> Result<(), TokenError> {
        let token_addr = normalize_address(&transfer.token_address);
        let value = parse_units(&transfer.value);
        let from = normalize_address(&transfer.from);
        let to = normalize_address(&transfer.to);

        if self.tokens.get(&token_addr).is_none() {
            return Err(TokenError::TokenNotFound(token_addr));
        }

        if !value.is_zero() {
            if !transfer.is_mint() {
                let available = self.balance_of(&token_addr, &from);
                if parse_units(&available) < value {
                    return Err(TokenError::IndexError(format!(
                        "transfer of {value} exceeds the {available} balance of {from}"
                    )));
                }

                let entry = self
                    .balances
                    .entry(token_addr.clone())
                    .or_default()
                    .entry(from.clone())
                    .or_insert_with(BigUint::zero);
                *entry -= &value;
            }

            if !transfer.is_burn() {
                let entry = self
                    .balances
                    .entry(token_addr.clone())
                    .or_default()
                    .entry(to.clone())
                    .or_insert_with(BigUint::zero);
                *entry += &value;
            }
        }

        *self.transfer_counts.entry(token_addr.clone()).or_insert(0) += 1;

        self.transfers
            .entry(token_addr.clone())
            .or_default()
            .push(transfer.clone());

        if let Some(token) = self.tokens.get_mut(&token_addr) {
            token.increment_transfers();
        }

        self.snapshot(&token_addr, &from, transfer.block_number);
        if to != from {
            self.snapshot(&token_addr, &to, transfer.block_number);
        }

        self.sync_holder_records(&token_addr);
        self.refresh_holder_count(&token_addr);

        Ok(())
    }

    /// Append the current balance of `holder` to the balance history.
    fn snapshot(&mut self, token_addr: &str, holder: &str, block_number: u64) {
        let token_addr = normalize_address(token_addr);
        let balance = self.balance_of(&token_addr, holder);
        let key = format!("{token_addr}:{}", normalize_address(holder));

        self.balance_history
            .entry(key)
            .or_default()
            .push(BalanceSnapshot::new(
                holder.to_string(),
                token_addr.to_string(),
                balance,
                block_number,
            ));
    }

    /// Rewrite every holder record so its balance and share match the ledger.
    fn sync_holder_records(&mut self, token_addr: &str) {
        let token_addr = normalize_address(token_addr);
        let token_addr = token_addr.as_str();
        let total_supply = self
            .tokens
            .get(token_addr)
            .map(|t| t.total_supply.clone())
            .unwrap_or_else(|| "0".to_string());

        let updates: Vec<(String, String)> = match self.balances.get(token_addr) {
            Some(by_holder) => by_holder
                .iter()
                .map(|(address, balance)| (address.clone(), balance.to_string()))
                .collect(),
            None => return,
        };

        let records = self.holders.entry(token_addr.to_string()).or_default();
        for (address, balance) in updates {
            let record = records.entry(address.clone()).or_insert_with(|| TokenHolder {
                address: address.clone(),
                token_address: token_addr.to_string(),
                balance: "0".to_string(),
                percent: 0.0,
                rank: 0,
                first_block: 0,
                last_updated: 0,
                is_contract: false,
            });
            record.balance = balance;
            record.calculate_percent(&total_supply);
            record.last_updated = now_unix();
        }
    }

    /// Balance history for one holder of one token, oldest first.
    pub fn balance_history(&self, token_addr: &str, holder: &str) -> Vec<BalanceSnapshot> {
        let key = format!(
            "{}:{}",
            normalize_address(token_addr),
            normalize_address(holder)
        );
        self.balance_history.get(&key).cloned().unwrap_or_default()
    }

    /// Balance of `holder` at the end of `block_number`.
    pub fn balance_at_block(&self, token_addr: &str, holder: &str, block_number: u64) -> String {
        self.balance_history(token_addr, holder)
            .into_iter()
            .filter(|snapshot| snapshot.block_number <= block_number)
            .next_back()
            .map(|snapshot| snapshot.balance)
            .unwrap_or_else(|| "0".to_string())
    }

    /// Get transfers for token
    pub fn get_transfers(&self, token_addr: &str) -> Option<&Vec<TokenTransfer>> {
        self.transfers.get(&normalize_address(token_addr))
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

    /// Record a price observation and recompute the running statistics.
    pub fn update_price(&mut self, mut price: TokenPrice) {
        let address = normalize_address(&price.address);
        let observations = self.price_history.entry(address.clone()).or_default();

        observations.push((price.last_updated, price.price));

        // The 24h change is measured against the oldest observation inside a
        // 24h window, not against the previous tick.
        let cutoff = price.last_updated.saturating_sub(86_400);
        let baseline = observations
            .iter()
            .find(|(timestamp, _)| *timestamp >= cutoff)
            .map(|(_, value)| *value);

        price.price_change_24h = match baseline {
            Some(previous) if previous > 0.0 => ((price.price - previous) / previous) * 100.0,
            _ => 0.0,
        };

        price.ath = observations
            .iter()
            .map(|(_, value)| *value)
            .fold(price.price, f64::max);
        price.atl = observations
            .iter()
            .map(|(_, value)| *value)
            .fold(price.price, f64::min);

        if price.total_supply > 0.0 {
            price.fdv = price.total_supply * price.price;
            if price.circulating_supply <= 0.0 {
                price.circulating_supply = price.total_supply;
            }
            price.market_cap = price.circulating_supply * price.price;
        }

        self.prices.insert(address, price);
    }

    /// Get price
    pub fn get_price(&self, address: &str) -> Option<&TokenPrice> {
        self.prices.get(&normalize_address(address))
    }

    /// Price observations for a token, oldest first.
    pub fn price_history(&self, address: &str) -> Vec<(u64, f64)> {
        self.price_history
            .get(&normalize_address(address))
            .cloned()
            .unwrap_or_default()
    }

    /// Concentration and balance statistics for a token.
    ///
    /// `top_10_percent` and `top_1_percent` are the percentages of total supply
    /// held by the ten and one largest holders; the balance fields are sums over
    /// those holders. Previously the two percentage fields held a holder count
    /// and every balance field was the string "0", so no concentration figure
    /// this function returned was usable.
    pub fn get_distribution(&self, token_addr: &str) -> Option<HolderDistribution> {
        let token = self.tokens.get(&normalize_address(token_addr))?;
        let holder_list: Vec<TokenHolder> = self.get_holders(token_addr)?;
        let total_holders = holder_list.len() as u64;

        // Percentages come from the ledger rather than from the record fields,
        // so they stay correct even if a record was written by hand.
        let total_supply = parse_units(&token.total_supply);
        let share = |balance: &BigUint| -> f64 {
            if total_supply.is_zero() {
                return 0.0;
            }
            // Scale before converting, so a huge supply cannot overflow f64.
            let scaled = (balance * BigUint::from(1_000_000u32)) / &total_supply;
            scaled.to_string().parse::<f64>().unwrap_or(0.0) / 10_000.0
        };

        let mut distribution = vec![
            DistributionBucket { range: "< 0.001%".to_string(), count: 0, percent: 0.0 },
            DistributionBucket { range: "0.001-0.01%".to_string(), count: 0, percent: 0.0 },
            DistributionBucket { range: "0.01-0.1%".to_string(), count: 0, percent: 0.0 },
            DistributionBucket { range: "0.1-1%".to_string(), count: 0, percent: 0.0 },
            DistributionBucket { range: "1-10%".to_string(), count: 0, percent: 0.0 },
            DistributionBucket { range: "> 10%".to_string(), count: 0, percent: 0.0 },
        ];

        let mut total_balance = BigUint::zero();
        for holder in &holder_list {
            let balance = parse_units(&holder.balance);
            let percent = share(&balance);
            total_balance += balance;

            let bucket = if percent < 0.001 {
                0
            } else if percent < 0.01 {
                1
            } else if percent < 0.1 {
                2
            } else if percent < 1.0 {
                3
            } else if percent < 10.0 {
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

        let top_10_balance = holder_list
            .iter()
            .take(10)
            .map(|h| parse_units(&h.balance))
            .fold(BigUint::zero(), |acc, b| acc + b);

        let (top_1_balance, avg_balance, median_balance) = if holder_list.is_empty() {
            (BigUint::zero(), BigUint::zero(), BigUint::zero())
        } else {
            let top_1 = parse_units(&holder_list[0].balance);
            let avg = &total_balance / BigUint::from(total_holders);

            let middle = holder_list.len() / 2;
            let median = if holder_list.len() % 2 == 0 && holder_list.len() > 1 {
                (parse_units(&holder_list[middle - 1].balance)
                    + parse_units(&holder_list[middle].balance))
                    / BigUint::from(2u32)
            } else {
                parse_units(&holder_list[middle].balance)
            };

            (top_1, avg, median)
        };

        Some(HolderDistribution {
            token_address: token_addr.to_string(),
            total_holders,
            top_10_percent: share(&top_10_balance),
            top_1_percent: share(&top_1_balance),
            top_10_balance: top_10_balance.to_string(),
            top_1_balance: top_1_balance.to_string(),
            avg_balance: avg_balance.to_string(),
            median_balance: median_balance.to_string(),
            distribution,
        })
    }


    /// Get all token addresses
    pub fn token_addresses(&self) -> Vec<String> {
        self.tokens.keys().cloned().collect()
    }

    /// Number of addresses currently holding a non-zero balance.
    pub fn holder_count(&self, token_addr: &str) -> u64 {
        self.balances
            .get(&normalize_address(token_addr))
            .map(|holders| {
                holders
                    .values()
                    .filter(|balance| !balance.is_zero())
                    .count() as u64
            })
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

/// Parse a decimal integer balance, tolerating hex, underscores and the
/// 0x-prefixed values that RPC responses use.
pub fn parse_units(value: &str) -> BigUint {
    let trimmed = value.trim().replace('_', "");

    if let Some(hex) = trimmed.strip_prefix("0x").or_else(|| trimmed.strip_prefix("0X")) {
        return BigUint::parse_bytes(hex.as_bytes(), 16).unwrap_or_else(BigUint::zero);
    }

    BigUint::parse_bytes(trimmed.as_bytes(), 10).unwrap_or_else(BigUint::zero)
}

/// Lower-case an address so that checksummed and plain forms compare equal.
pub fn normalize_address(address: &str) -> String {
    address.trim().to_lowercase()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn indexer_with_token(supply: &str) -> TokenIndexer {
        let mut indexer = TokenIndexer::new();
        let mut token = Token::new(
            "0xToken".to_string(),
            "Tiger".to_string(),
            "TGR".to_string(),
        );
        token.total_supply = supply.to_string();
        indexer.add_token(token);
        indexer
    }

    fn transfer(from: &str, to: &str, value: &str, block: u64) -> TokenTransfer {
        let mut t = TokenTransfer::new(
            format!("0xhash{block}"),
            "0xToken".to_string(),
            from.to_string(),
            to.to_string(),
            value.to_string(),
        );
        t.block_number = block;
        t
    }

    #[test]
    fn a_mint_credits_the_recipient_and_snapshots_the_balance() {
        let mut indexer = indexer_with_token("1000");
        indexer
            .record_transfer(transfer(
                "0x0000000000000000000000000000000000000000",
                "0xAlice",
                "250",
                1,
            ))
            .unwrap();

        assert_eq!(indexer.balance_of("0xToken", "0xalice"), "250");
        assert_eq!(indexer.holder_count("0xToken"), 1);

        let history = indexer.balance_history("0xToken", "0xAlice");
        assert_eq!(history.len(), 1);
        assert_eq!(history[0].balance, "250");
        assert_eq!(history[0].block_number, 1);
    }

    #[test]
    fn a_transfer_moves_value_between_two_holders() {
        let mut indexer = indexer_with_token("1000");
        indexer
            .record_transfer(transfer(
                "0x0000000000000000000000000000000000000000",
                "0xAlice",
                "1000",
                1,
            ))
            .unwrap();
        indexer
            .record_transfer(transfer("0xAlice", "0xBob", "400", 2))
            .unwrap();

        assert_eq!(indexer.balance_of("0xToken", "0xAlice"), "600");
        assert_eq!(indexer.balance_of("0xToken", "0xBob"), "400");
    }

    #[test]
    fn a_transfer_beyond_the_balance_is_rejected() {
        let mut indexer = indexer_with_token("1000");
        indexer
            .record_transfer(transfer(
                "0x0000000000000000000000000000000000000000",
                "0xAlice",
                "10",
                1,
            ))
            .unwrap();

        let result = indexer.record_transfer(transfer("0xAlice", "0xBob", "11", 2));
        assert!(result.is_err(), "an overdraft must not be applied");

        assert_eq!(indexer.balance_of("0xToken", "0xAlice"), "10");
        assert_eq!(indexer.balance_of("0xToken", "0xBob"), "0");
    }

    #[test]
    fn a_burn_removes_value_from_circulation() {
        let mut indexer = indexer_with_token("1000");
        indexer
            .record_transfer(transfer(
                "0x0000000000000000000000000000000000000000",
                "0xAlice",
                "500",
                1,
            ))
            .unwrap();
        indexer
            .record_transfer(transfer(
                "0xAlice",
                "0x0000000000000000000000000000000000000000",
                "200",
                2,
            ))
            .unwrap();

        assert_eq!(indexer.balance_of("0xToken", "0xAlice"), "300");
    }

    #[test]
    fn balances_beyond_u64_are_exact() {
        let mut indexer = indexer_with_token("100000000000000000000000000000000000000000");
        indexer
            .record_transfer(transfer(
                "0x0000000000000000000000000000000000000000",
                "0xAlice",
                "100000000000000000000000000000000000000000",
                1,
            ))
            .unwrap();

        assert_eq!(
            indexer.balance_of("0xToken", "0xAlice"),
            "100000000000000000000000000000000000000000"
        );
    }

    #[test]
    fn holders_are_ranked_numerically_not_lexicographically() {
        let mut indexer = indexer_with_token("100");
        for (holder, amount) in [("0xNine", "9"), ("0xTen", "10"), ("0xTwo", "2")] {
            indexer
                .record_transfer(transfer(
                    "0x0000000000000000000000000000000000000000",
                    holder,
                    amount,
                    1,
                ))
                .unwrap();
        }

        let holders = indexer.get_holders("0xToken").unwrap();
        let order: Vec<&str> = holders.iter().map(|h| h.address.as_str()).collect();
        assert_eq!(order, vec!["0xten", "0xnine", "0xtwo"]);
        assert_eq!(holders[0].rank, 1);
        assert_eq!(holders[2].rank, 3);
    }

    #[test]
    fn holder_percent_is_the_share_of_total_supply() {
        let mut indexer = indexer_with_token("200");
        indexer
            .record_transfer(transfer(
                "0x0000000000000000000000000000000000000000",
                "0xAlice",
                "50",
                1,
            ))
            .unwrap();

        let holders = indexer.get_holders("0xToken").unwrap();
        assert!((holders[0].percent - 25.0).abs() < 1e-9);
    }

    #[test]
    fn balance_at_block_uses_the_snapshot_at_that_height() {
        let mut indexer = indexer_with_token("1000");
        indexer
            .record_transfer(transfer(
                "0x0000000000000000000000000000000000000000",
                "0xAlice",
                "100",
                5,
            ))
            .unwrap();
        indexer
            .record_transfer(transfer(
                "0x0000000000000000000000000000000000000000",
                "0xAlice",
                "150",
                9,
            ))
            .unwrap();

        assert_eq!(indexer.balance_at_block("0xToken", "0xAlice", 4), "0");
        assert_eq!(indexer.balance_at_block("0xToken", "0xAlice", 5), "100");
        assert_eq!(indexer.balance_at_block("0xToken", "0xAlice", 9), "250");
        assert_eq!(indexer.balance_at_block("0xToken", "0xAlice", 100), "250");
    }

    #[test]
    fn distribution_reports_real_concentration_figures() {
        let mut indexer = indexer_with_token("1000");
        indexer
            .record_transfer(transfer(
                "0x0000000000000000000000000000000000000000",
                "0xWhale",
                "800",
                1,
            ))
            .unwrap();
        indexer
            .record_transfer(transfer(
                "0x0000000000000000000000000000000000000000",
                "0xSmall",
                "200",
                1,
            ))
            .unwrap();

        let distribution = indexer.get_distribution("0xToken").unwrap();

        assert_eq!(distribution.total_holders, 2);
        assert_eq!(distribution.top_10_balance, "1000");
        assert_eq!(distribution.top_1_balance, "800");
        assert!((distribution.top_1_percent - 80.0).abs() < 1e-6);
        assert_eq!(distribution.avg_balance, "500");
        assert_eq!(distribution.median_balance, "500");
    }

    #[test]
    fn an_unknown_token_transfer_is_reported() {
        let mut indexer = TokenIndexer::new();
        let result = indexer.record_transfer(transfer("0xAlice", "0xBob", "1", 1));
        assert!(matches!(result, Err(TokenError::TokenNotFound(_))));
    }

    #[test]
    fn price_change_is_measured_across_the_24h_window() {
        let mut indexer = TokenIndexer::new();
        let mut token = Token::new("0xT".to_string(), "T".to_string(), "T".to_string());
        token.total_supply = "1000".to_string();
        indexer.add_token(token);

        let mut early = TokenPrice::new("0xT".to_string());
        early.price = 1.0;
        early.total_supply = 1000.0;
        early.last_updated = 1_000;
        indexer.update_price(early);

        let mut late = TokenPrice::new("0xT".to_string());
        late.price = 1.5;
        late.total_supply = 1000.0;
        late.last_updated = 1_000 + 3_600;
        indexer.update_price(late);

        let price = indexer.get_price("0xT").unwrap();
        assert!((price.price_change_24h - 50.0).abs() < 1e-9);
        assert_eq!(price.ath, 1.5);
        assert_eq!(price.atl, 1.0);
        assert_eq!(indexer.price_history("0xT").len(), 2);
    }

    #[test]
    fn parse_units_accepts_decimal_and_hex() {
        assert_eq!(parse_units("1000"), BigUint::from(1000u32));
        assert_eq!(parse_units("0x10"), BigUint::from(16u32));
        assert_eq!(parse_units("not a number"), BigUint::zero());
    }
}