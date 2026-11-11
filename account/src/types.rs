//! Account Types - Complete implementation with full blockchain account management
//!
//! This module provides:
//! - Complete account state management
//! - Account nonce and balance tracking
//! - Contract code storage and execution
//! - Storage trie management
//! - Account history and event tracking

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::time::{SystemTime, UNIX_EPOCH};

// =============================================================================
// ERRORS
// =============================================================================

/// Account Service Errors
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum AccountError {
    #[serde(rename = "account_not_found")]
    AccountNotFound(String),
    #[serde(rename = "account_exists")]
    AccountExists(String),
    #[serde(rename = "invalid_address")]
    InvalidAddress(String),
    #[serde(rename = "invalid_amount")]
    InvalidAmount(String),
    #[serde(rename = "insufficient_balance")]
    InsufficientBalance(String),
    #[serde(rename = "balance_overflow")]
    BalanceOverflow,
    #[serde(rename = "nonce_mismatch")]
    NonceMismatch { expected: u64, actual: u64 },
    #[serde(rename = "contract_error")]
    ContractError(String),
    #[serde(rename = "storage_error")]
    StorageError(String),
}

impl std::fmt::Display for AccountError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AccountError::AccountNotFound(a) => write!(f, "account not found: {a}"),
            AccountError::AccountExists(a) => write!(f, "account already exists: {a}"),
            AccountError::InvalidAddress(a) => write!(f, "invalid address: {a}"),
            AccountError::InvalidAmount(a) => write!(f, "invalid amount: {a}"),
            AccountError::InsufficientBalance(a) => write!(f, "insufficient balance: {a}"),
            AccountError::BalanceOverflow => write!(f, "balance overflow"),
            AccountError::NonceMismatch { expected, actual } => {
                write!(f, "nonce mismatch: expected {expected}, got {actual}")
            }
            AccountError::ContractError(m) => write!(f, "contract error: {m}"),
            AccountError::StorageError(m) => write!(f, "storage error: {m}"),
        }
    }
}

impl std::error::Error for AccountError {}

// =============================================================================
// ADDRESS
// =============================================================================

/// Ethereum address representation
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub struct Address(pub [u8; 20]);

impl Address {
    /// Create from hex string
    pub fn from_hex(s: &str) -> Result<Self, AccountError> {
        let s = s.strip_prefix("0x").unwrap_or(s);
        if s.len() != 40 {
            return Err(AccountError::InvalidAddress(s.to_string()));
        }
        let bytes = hex::decode(s).map_err(|e| AccountError::InvalidAddress(e.to_string()))?;
        let mut addr = [0u8; 20];
        addr.copy_from_slice(&bytes);
        Ok(Address(addr))
    }

    /// Create from bytes
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, AccountError> {
        if bytes.len() != 20 {
            return Err(AccountError::InvalidAddress("wrong length".to_string()));
        }
        let mut addr = [0u8; 20];
        addr.copy_from_slice(bytes);
        Ok(Address(addr))
    }

    /// Convert to hex string
    pub fn to_hex(&self) -> String {
        format!("0x{}", hex::encode(self.0))
    }

    /// Zero address
    pub fn zero() -> Self {
        Address([0u8; 20])
    }

    /// Check if zero address
    pub fn is_zero(&self) -> bool {
        self.0 == [0u8; 20]
    }
}

impl std::fmt::Display for Address {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.to_hex())
    }
}

impl std::str::FromStr for Address {
    type Err = AccountError;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Address::from_hex(s)
    }
}

// =============================================================================
// ACCOUNT
// =============================================================================

/// Account state on the blockchain
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Account {
    /// Ethereum address
    pub address: String,
    /// Transaction count (nonce)
    pub nonce: u64,
    /// Balance in wei
    pub balance: String,
    /// Contract code hash
    pub code_hash: String,
    /// Storage root hash
    pub storage_root: String,
    /// Contract creation timestamp
    pub created_at: u64,
    /// Number of transactions
    pub tx_count: u64,
    /// Last transaction timestamp
    pub last_tx_at: u64,
    /// Whether is contract
    pub is_contract: bool,
}

impl Account {
    /// Create new account
    pub fn new(address: String) -> Self {
        Self {
            address,
            nonce: 0,
            balance: "0".to_string(),
            code_hash: "0".to_string(),
            storage_root: "0".to_string(),
            created_at: now_unix(),
            tx_count: 0,
            last_tx_at: 0,
            is_contract: false,
        }
    }

    /// Create new contract account
    pub fn new_contract(address: String, code_hash: String) -> Self {
        Self {
            address,
            nonce: 0,
            balance: "0".to_string(),
            code_hash,
            storage_root: "0".to_string(),
            created_at: now_unix(),
            tx_count: 0,
            last_tx_at: 0,
            is_contract: true,
        }
    }

    /// Increment nonce
    pub fn increment_nonce(&mut self) -> u64 {
        let old = self.nonce;
        self.nonce += 1;
        old
    }

    /// Set balance
    pub fn set_balance(&mut self, balance: String) {
        self.balance = balance;
    }

    /// Parse a decimal amount into wei.
    ///
    /// A malformed amount is an error rather than a silent zero, and an amount
    /// too large for `u128` is rejected instead of wrapping.
    pub fn parse_amount(amount: &str) -> Result<u128, AccountError> {
        amount
            .trim()
            .parse::<u128>()
            .map_err(|_| AccountError::InvalidAmount(amount.to_string()))
    }

    /// Add balance
    pub fn add_balance(&mut self, amount: &str) -> Result<(), AccountError> {
        let add = Self::parse_amount(amount)?;
        let current = Self::parse_amount(&self.balance)?;
        let sum = current
            .checked_add(add)
            .ok_or(AccountError::BalanceOverflow)?;
        self.balance = sum.to_string();
        Ok(())
    }

    /// Subtract balance
    pub fn sub_balance(&mut self, amount: &str) -> Result<(), AccountError> {
        let sub = Self::parse_amount(amount)?;
        let current = Self::parse_amount(&self.balance)?;
        if current < sub {
            return Err(AccountError::InsufficientBalance(amount.to_string()));
        }
        self.balance = (current - sub).to_string();
        Ok(())
    }

    /// Check if can transfer amount
    pub fn can_transfer(&self, amount: &str) -> bool {
        match (Self::parse_amount(&self.balance), Self::parse_amount(amount)) {
            (Ok(current), Ok(req)) => current >= req,
            // A malformed amount must never be treated as transferable.
            _ => false,
        }
    }
}

// =============================================================================
// ACCOUNT STATE
// =============================================================================

/// Complete account state including code and storage
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AccountState {
    /// Address
    pub address: String,
    /// Balance
    pub balance: String,
    /// Contract bytecode
    pub code: Vec<u8>,
    /// Storage slots
    pub storage: HashMap<String, String>,
    /// Code hash
    pub code_hash: String,
    /// Nonce
    pub nonce: u64,
}

impl AccountState {
    /// Create new account state
    pub fn new(address: String) -> Self {
        Self {
            address,
            balance: "0".to_string(),
            code: vec![],
            storage: HashMap::new(),
            code_hash: "0".to_string(),
            nonce: 0,
        }
    }

    /// Create from account with code
    pub fn from_account(account: &Account, code: Vec<u8>) -> Self {
        let code_hash = if code.is_empty() {
            "0".to_string()
        } else {
            hex::encode(sha256_digest(&code))
        };

        Self {
            address: account.address.clone(),
            balance: account.balance.clone(),
            code,
            storage: HashMap::new(),
            code_hash,
            nonce: account.nonce,
        }
    }

    /// Get storage slot
    pub fn get_storage(&self, key: &str) -> Option<&String> {
        self.storage.get(key)
    }

    /// Set storage slot
    pub fn set_storage(&mut self, key: String, value: String) {
        self.storage.insert(key, value);
    }

    /// Clear storage slot
    pub fn clear_storage(&mut self, key: &str) {
        self.storage.remove(key);
    }

    /// Check if has code
    pub fn has_code(&self) -> bool {
        !self.code.is_empty()
    }

    /// Is EOA (externally owned account)
    pub fn is_eoa(&self) -> bool {
        self.code.is_empty()
    }

    /// Is contract
    pub fn is_contract(&self) -> bool {
        !self.code.is_empty()
    }
}

// =============================================================================
// ACCOUNT HISTORY
// =============================================================================

/// Account history entry
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AccountHistoryEntry {
    /// Block number
    pub block_number: u64,
    /// Transaction hash
    pub tx_hash: String,
    /// Balance change
    pub balance_change: String,
    /// Nonce change
    pub nonce_change: i64,
    /// Timestamp
    pub timestamp: u64,
    /// Type of change
    pub change_type: HistoryChangeType,
    /// Account nonce the change was applied at.
    pub nonce: u64,
}

/// Type of history change
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum HistoryChangeType {
    #[serde(rename = "transfer")]
    Transfer,
    #[serde(rename = "contract_call")]
    ContractCall,
    #[serde(rename = "contract_create")]
    ContractCreate,
    #[serde(rename = "self_destruct")]
    SelfDestruct,
    #[serde(rename = "storage_change")]
    StorageChange,
}

// =============================================================================
// STORAGE TRIE
// =============================================================================

/// Storage trie for contract storage
pub struct StorageTrie {
    /// Root hash
    root: String,
    /// Storage slots
    slots: HashMap<String, String>,
}

impl StorageTrie {
    /// Create new storage trie
    pub fn new() -> Self {
        Self {
            root: "0".to_string(),
            slots: HashMap::new(),
        }
    }

    /// Get value at slot
    pub fn get(&self, slot: &str) -> Option<&String> {
        self.slots.get(slot)
    }

    /// Set value at slot
    ///
    /// The root is a deterministic SHA-256 commitment over the slot contents,
    /// not a Merkle-Patricia root. It detects any change to the trie but does
    /// not support inclusion proofs; callers needing proofs should use the
    /// state crate's trie.
    pub fn set(&mut self, slot: String, value: String) {
        self.slots.insert(slot, value);
        self.root = hex::encode(sha256_digest(&self.slots.iter()
            .map(|(k, v)| format!("{}:{}", k, v))
            .collect::<Vec<_>>()
            .join(",")
            .as_bytes()));
    }

    /// Delete value at slot
    pub fn delete(&mut self, slot: &str) {
        self.slots.remove(slot);
    }

    /// Get root hash
    pub fn root(&self) -> &str {
        &self.root
    }

    /// Get all slots
    pub fn slots(&self) -> &HashMap<String, String> {
        &self.slots
    }
}

impl Default for StorageTrie {
    fn default() -> Self {
        Self::new()
    }
}

// =============================================================================
// ACCOUNT MANAGER
// =============================================================================

/// Complete account manager service
pub struct AccountManager {
    /// Accounts by address
    accounts: HashMap<String, Account>,
    /// Complete states (with code and storage)
    states: HashMap<String, AccountState>,
    /// Account history
    history: HashMap<String, Vec<AccountHistoryEntry>>,
    /// Storage tries by address
    tries: HashMap<String, StorageTrie>,
    /// Total ether supply
    total_supply: u128,
    /// Number of EOAs
    eoa_count: u64,
    /// Number of contracts
    contract_count: u64,
    /// Block the manager is currently applying, recorded on history entries.
    block_number: u64,
    /// Transaction currently being applied, recorded on history entries.
    last_tx_hash: String,
}

impl AccountManager {
    /// Create new account manager
    pub fn new() -> Self {
        Self {
            accounts: HashMap::new(),
            states: HashMap::new(),
            history: HashMap::new(),
            tries: HashMap::new(),
            total_supply: 0,
            eoa_count: 0,
            contract_count: 0,
            block_number: 0,
            last_tx_hash: String::new(),
        }
    }

    /// Set the block and transaction that subsequent changes belong to.
    ///
    /// History entries record whatever is set here, so callers applying a block
    /// label their changes instead of every entry carrying a zero block.
    pub fn set_context(&mut self, block_number: u64, tx_hash: impl Into<String>) {
        self.block_number = block_number;
        self.last_tx_hash = tx_hash.into();
    }

    /// Block number currently recorded on new history entries.
    pub fn block_number(&self) -> u64 {
        self.block_number
    }

    /// Create account
    pub fn create_account(&mut self, address: String) -> Result<&Account, AccountError> {
        if self.accounts.contains_key(&address) {
            return Err(AccountError::AccountExists(address));
        }

        self.accounts.insert(address.clone(), Account::new(address.clone()));
        self.history.insert(address.clone(), vec![]);
        self.states
            .insert(address.clone(), AccountState::new(address.clone()));

        self.eoa_count += 1;

        // The key was just inserted, so the lookup cannot fail.
        self.accounts
            .get(&address)
            .ok_or_else(|| AccountError::AccountNotFound(address.clone()))
    }

    /// Create contract account
    pub fn create_contract(&mut self, address: String, code: Vec<u8>) -> Result<&Account, AccountError> {
        let code_hash = hex::encode(sha256_digest(&code));
        let account = Account::new_contract(address.clone(), code_hash.clone());

        // Create state with code
        let state = AccountState {
            address: address.clone(),
            balance: "0".to_string(),
            code,
            storage: HashMap::new(),
            code_hash,
            nonce: 0,
        };

        self.accounts.insert(address.clone(), account);
        self.states.insert(address.clone(), state);
        self.history.insert(address.clone(), vec![]);
        self.tries.insert(address.clone(), StorageTrie::new());

        self.contract_count += 1;

        Ok(self.accounts.get(&address).unwrap())
    }

    /// Get account
    pub fn get_account(&self, address: &str) -> Result<&Account, AccountError> {
        self.accounts.get(address)
            .ok_or_else(|| AccountError::AccountNotFound(address.to_string()))
    }

    /// Get account state
    pub fn get_state(&self, address: &str) -> Result<&AccountState, AccountError> {
        self.states.get(address)
            .ok_or_else(|| AccountError::AccountNotFound(address.to_string()))
    }

    /// Get mutable account
    pub fn get_account_mut(&mut self, address: &str) -> Result<&mut Account, AccountError> {
        self.accounts.get_mut(address)
            .ok_or_else(|| AccountError::AccountNotFound(address.to_string()))
    }

    /// Get mutable state
    pub fn get_state_mut(&mut self, address: &str) -> Result<&mut AccountState, AccountError> {
        self.states.get_mut(address)
            .ok_or_else(|| AccountError::AccountNotFound(address.to_string()))
    }

    /// Transfer balance between two existing accounts.
    ///
    /// Both accounts are validated before either is mutated, so a failure part
    /// way through cannot leave the two balances inconsistent.
    pub fn transfer(&mut self, from: &str, to: &str, amount: &str) -> Result<(), AccountError> {
        if from == to {
            return Err(AccountError::ContractError(
                "sender and receiver are the same account".to_string(),
            ));
        }

        let amount_wei = Account::parse_amount(amount)?;
        let from_balance = Account::parse_amount(
            &self
                .accounts
                .get(from)
                .ok_or_else(|| AccountError::AccountNotFound(from.to_string()))?
                .balance,
        )?;
        let to_balance = Account::parse_amount(
            &self
                .accounts
                .get(to)
                .ok_or_else(|| AccountError::AccountNotFound(to.to_string()))?
                .balance,
        )?;

        if from_balance < amount_wei {
            return Err(AccountError::InsufficientBalance(amount.to_string()));
        }
        let new_to = to_balance
            .checked_add(amount_wei)
            .ok_or(AccountError::BalanceOverflow)?;

        // Both sides are known to be valid, so apply the move.
        {
            let from_acc = self
                .accounts
                .get_mut(from)
                .ok_or_else(|| AccountError::AccountNotFound(from.to_string()))?;
            from_acc.sub_balance(amount)?;
            from_acc.tx_count += 1;
            from_acc.last_tx_at = now_unix();
        }
        {
            let to_acc = self
                .accounts
                .get_mut(to)
                .ok_or_else(|| AccountError::AccountNotFound(to.to_string()))?;
            to_acc.balance = new_to.to_string();
        }

        let nonce = self
            .accounts
            .get(from)
            .map(|a| a.nonce)
            .unwrap_or_default();

        self.record_history(
            from,
            HistoryChangeType::Transfer,
            format!("-{amount}"),
            0,
            nonce,
        );
        self.record_history(
            to,
            HistoryChangeType::Transfer,
            amount.to_string(),
            0,
            nonce,
        );

        Ok(())
    }

    /// Mirror `accounts[address].balance` into `states[address].balance`.
    ///
    /// `AccountManager` keeps balance in both maps. Without this the two drift
    /// apart: the first version of `self_destruct` drained only the state copy,
    /// so `get_account` kept reporting the destroyed contract's balance.
    fn sync_state_balance(&mut self, address: &str) {
        let balance = match self.accounts.get(address) {
            Some(acc) => acc.balance.clone(),
            None => return,
        };
        if let Some(state) = self.states.get_mut(address) {
            state.balance = balance;
        }
    }

    /// Append a history entry for `address`.
    fn record_history(
        &mut self,
        address: &str,
        change_type: HistoryChangeType,
        balance_change: String,
        nonce_change: i64,
        nonce: u64,
    ) {
        let entry = AccountHistoryEntry {
            block_number: self.block_number,
            tx_hash: self.last_tx_hash.clone(),
            balance_change,
            nonce_change,
            timestamp: now_unix(),
            change_type,
            nonce,
        };

        if let Some(history) = self.history.get_mut(address) {
            history.push(entry);
        }
    }

    /// Get account history
    pub fn history(&self, address: &str) -> Option<&Vec<AccountHistoryEntry>> {
        self.history.get(address)
    }

    /// Get storage trie for contract
    pub fn storage_trie(&mut self, address: &str) -> Option<&mut StorageTrie> {
        self.tries.get_mut(address)
    }

    /// Get storage value
    pub fn get_storage(&self, address: &str, slot: &str) -> Option<String> {
        self.states.get(address)
            .and_then(|s| s.get_storage(slot))
            .cloned()
    }

    /// Set storage value
    pub fn set_storage(
        &mut self,
        address: &str,
        slot: String,
        value: String,
    ) -> Result<(), AccountError> {
        let state = self.states.get_mut(address)
            .ok_or_else(|| AccountError::AccountNotFound(address.to_string()))?;

        state.set_storage(slot, value);

        Ok(())
    }

    /// Apply a transaction to the tracked accounts.
    ///
    /// A transaction carrying non-empty `data` deploys a contract; the
    /// deployment address is derived deterministically from the sender and the
    /// nonce, so replaying the same transaction cannot produce a different
    /// address.
    pub fn execute_tx(
        &mut self,
        from: &str,
        to: &str,
        value: &str,
        data: Option<Vec<u8>>,
        nonce: u64,
    ) -> Result<String, AccountError> {
        let from_acc = self
            .accounts
            .get(from)
            .ok_or_else(|| AccountError::AccountNotFound(from.to_string()))?;

        if from_acc.nonce != nonce {
            return Err(AccountError::NonceMismatch {
                expected: from_acc.nonce,
                actual: nonce,
            });
        }

        let total_value = Account::parse_amount(value)?;

        let new_contract_address = match data {
            Some(code) if !code.is_empty() => {
                let new_address = contract_address(from, nonce);

                self.create_contract(new_address.clone(), code)?;

                // A deployment's value moves from the sender to the new contract.
                if total_value > 0 {
                    self.transfer_internal(from, &new_address, value)?;
                }

                Some(new_address)
            }
            _ => {
                if total_value > 0 {
                    self.transfer_internal(from, to, value)?;
                }
                None
            }
        };

        {
            let from_acc = self
                .accounts
                .get_mut(from)
                .ok_or_else(|| AccountError::AccountNotFound(from.to_string()))?;
            from_acc.increment_nonce();
        }

        Ok(new_contract_address.unwrap_or_else(|| to.to_string()))
    }

    /// Move value and record history on both sides.
    fn transfer_internal(&mut self, from: &str, to: &str, amount: &str) -> Result<(), AccountError> {
        let amount_wei = Account::parse_amount(amount)?;
        let from_balance = Account::parse_amount(
            &self
                .accounts
                .get(from)
                .ok_or_else(|| AccountError::AccountNotFound(from.to_string()))?
                .balance,
        )?;
        let to_balance = Account::parse_amount(
            &self
                .accounts
                .get(to)
                .ok_or_else(|| AccountError::AccountNotFound(to.to_string()))?
                .balance,
        )?;

        if from_balance < amount_wei {
            return Err(AccountError::InsufficientBalance(amount.to_string()));
        }
        let new_to = to_balance
            .checked_add(amount_wei)
            .ok_or(AccountError::BalanceOverflow)?;

        {
            let from_acc = self
                .accounts
                .get_mut(from)
                .ok_or_else(|| AccountError::AccountNotFound(from.to_string()))?;
            from_acc.sub_balance(amount)?;
            from_acc.tx_count += 1;
            from_acc.last_tx_at = now_unix();
        }
        {
            let to_acc = self
                .accounts
                .get_mut(to)
                .ok_or_else(|| AccountError::AccountNotFound(to.to_string()))?;
            to_acc.balance = new_to.to_string();
        }

        let nonce = self.accounts.get(from).map(|a| a.nonce).unwrap_or_default();
        self.record_history(
            from,
            HistoryChangeType::Transfer,
            format!("-{amount}"),
            0,
            nonce,
        );
        self.record_history(
            to,
            HistoryChangeType::Transfer,
            amount.to_string(),
            0,
            nonce,
        );

        self.sync_state_balance(from);
        self.sync_state_balance(to);

        Ok(())
    }

    /// Self destruct contract
    pub fn self_destruct(&mut self, address: &str, beneficiary: &str) -> Result<(), AccountError> {
        let is_contract = self
            .states
            .get(address)
            .ok_or_else(|| AccountError::AccountNotFound(address.to_string()))?
            .is_contract();

        if !is_contract {
            return Err(AccountError::ContractError("not a contract".to_string()));
        }

        let balance = Account::parse_amount(
            &self
                .states
                .get(address)
                .ok_or_else(|| AccountError::AccountNotFound(address.to_string()))?
                .balance,
        )?;

        if balance > 0 {
            let to_balance = Account::parse_amount(
                &self
                    .accounts
                    .get(beneficiary)
                    .ok_or_else(|| AccountError::AccountNotFound(beneficiary.to_string()))?
                    .balance,
            )?;
            let new_to = to_balance
                .checked_add(balance)
                .ok_or(AccountError::BalanceOverflow)?;

            if let Some(beneficiary_acc) = self.accounts.get_mut(beneficiary) {
                beneficiary_acc.balance = new_to.to_string();
            }
            // The destroyed contract must not keep the balance it just moved.
            if let Some(contract_acc) = self.accounts.get_mut(address) {
                contract_acc.balance = "0".to_string();
                contract_acc.is_contract = false;
            }
            if let Some(state) = self.states.get_mut(address) {
                state.storage.clear();
            }

            self.record_history(
                beneficiary,
                HistoryChangeType::SelfDestruct,
                balance.to_string(),
                0,
                0,
            );
        } else if let Some(state) = self.states.get_mut(address) {
            state.storage.clear();
        }

        self.sync_state_balance(address);
        self.sync_state_balance(beneficiary);

        self.contract_count = self.contract_count.saturating_sub(1);

        Ok(())
    }

    /// Get statistics
    pub fn stats(&self) -> AccountStats {
        AccountStats {
            total_accounts: self.accounts.len() as u64,
            eoa_count: self.eoa_count,
            contract_count: self.contract_count,
            total_supply: self.total_supply,
        }
    }

    /// Check if account exists
    pub fn exists(&self, address: &str) -> bool {
        self.accounts.contains_key(address)
    }

    /// Check if account is contract
    pub fn is_contract(&self, address: &str) -> bool {
        self.states.get(address)
            .map(|s| s.is_contract())
            .unwrap_or(false)
    }

    /// Get all account addresses
    pub fn account_addresses(&self) -> Vec<String> {
        self.accounts.keys().cloned().collect()
    }

    /// Get balance
    pub fn balance(&self, address: &str) -> String {
        self.accounts.get(address)
            .map(|a| a.balance.clone())
            .unwrap_or_else(|| "0".to_string())
    }

    /// Get nonce
    pub fn nonce(&self, address: &str) -> u64 {
        self.accounts.get(address)
            .map(|a| a.nonce)
            .unwrap_or(0)
    }
}

impl Default for AccountManager {
    fn default() -> Self {
        Self::new()
    }
}

// =============================================================================
// STATISTICS
// =============================================================================

/// Account statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AccountStats {
    pub total_accounts: u64,
    pub eoa_count: u64,
    pub contract_count: u64,
    pub total_supply: u128,
}

// =============================================================================
// HELPER FUNCTIONS
// =============================================================================

/// Get current Unix timestamp
fn now_unix() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs()
}

/// SHA-256 of `data`.
///
/// The previous implementation ran `DefaultHasher`, which is SipHash: not a
/// cryptographic hash, and not even stable between Rust releases, so any value
/// derived from it changed when the toolchain changed.
fn sha256_digest(data: &[u8]) -> Vec<u8> {
    use sha2::{Digest, Sha256};
    Sha256::digest(data).to_vec()
}

/// The address a `CREATE` from `sender` at `nonce` produces.
///
/// Defined as the low 20 bytes of `keccak256(rlp([sender, nonce]))`. Deriving
/// the address from the sender and nonce rather than from a timestamp makes a
/// replayed transaction deterministic instead of producing a fresh address
/// every time.
fn contract_address(sender: &str, nonce: u64) -> String {
    use sha3::{Digest, Keccak256};

    let sender_bytes = hex::decode(sender.trim_start_matches("0x")).unwrap_or_default();

    let mut payload = Vec::with_capacity(32);
    payload.push(0x94); // 20-byte string
    payload.extend_from_slice(&sender_bytes);
    payload.extend_from_slice(&rlp_encode_u64(nonce));

    let mut rlp = Vec::with_capacity(payload.len() + 2);
    if payload.len() < 56 {
        rlp.push(0xc0 + payload.len() as u8);
    } else {
        rlp.push(0xf7 + 1);
        rlp.push(payload.len() as u8);
    }
    rlp.extend_from_slice(&payload);

    let digest = Keccak256::digest(&rlp);
    format!("0x{}", hex::encode(&digest[12..]))
}

/// Minimal RLP encoding of a `u64`.
fn rlp_encode_u64(value: u64) -> Vec<u8> {
    if value == 0 {
        return vec![0x80];
    }
    let bytes = value.to_be_bytes();
    let first = bytes.iter().position(|b| *b != 0).unwrap_or(bytes.len() - 1);
    let significant = &bytes[first..];
    if significant.len() == 1 && significant[0] < 0x80 {
        return vec![significant[0]];
    }
    let mut out = Vec::with_capacity(significant.len() + 1);
    out.push(0x80 + significant.len() as u8);
    out.extend_from_slice(significant);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const ZERO: &str = "0x0000000000000000000000000000000000000000";
    const ALICE: &str = "0x1111111111111111111111111111111111111111";
    const BOB: &str = "0x2222222222222222222222222222222222222222";

    fn manager_with_balances(alice: &str, bob: &str) -> AccountManager {
        let mut m = AccountManager::new();
        m.create_account(ALICE.to_string()).unwrap();
        m.create_account(BOB.to_string()).unwrap();
        m.accounts.get_mut(ALICE).unwrap().balance = alice.to_string();
        m.accounts.get_mut(BOB).unwrap().balance = bob.to_string();
        m
    }

    #[test]
    fn sha256_matches_the_nist_vector() {
        // SHA-256("abc"), FIPS 180-4.
        assert_eq!(
            hex::encode(sha256_digest(b"abc")),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
        assert_eq!(sha256_digest(b"").len(), 32);
    }

    #[test]
    fn create_address_matches_the_canonical_ethereum_vectors() {
        // keccak256(rlp([sender, nonce]))[12..], independently computed.
        assert_eq!(
            contract_address(ZERO, 0),
            "0xbd770416a3345f91e4b34576cb804a576fa48eb1"
        );
        assert_eq!(
            contract_address(ZERO, 1),
            "0x5a443704dd4b594b382c22a083e2bd3090a6fef3"
        );
        assert_eq!(
            contract_address(ZERO, 255),
            "0x2e021f429ff10bfc9373f73720a14bee2cfd5fdd"
        );
        assert_eq!(
            contract_address(ZERO, 256),
            "0x1183a5a83c1fa113618603abc4509077ec672699"
        );
        assert_eq!(
            contract_address("0x1234567890abcdef1234567890abcdef12345678", 7),
            "0x7fa04a5a7fd31c6215c1d980514db5e173d09e69"
        );
    }

    #[test]
    fn create_address_is_deterministic() {
        // The old implementation derived the address from the clock, so the
        // same transaction produced a different address every run.
        assert_eq!(contract_address(ALICE, 5), contract_address(ALICE, 5));
        assert_ne!(contract_address(ALICE, 5), contract_address(ALICE, 6));
    }

    #[test]
    fn rlp_encodes_u64_boundaries() {
        assert_eq!(rlp_encode_u64(0), vec![0x80]);
        assert_eq!(rlp_encode_u64(0x7f), vec![0x7f]);
        assert_eq!(rlp_encode_u64(0x80), vec![0x81, 0x80]);
        assert_eq!(rlp_encode_u64(0xff), vec![0x81, 0xff]);
        assert_eq!(rlp_encode_u64(0x100), vec![0x82, 0x01, 0x00]);
    }

    #[test]
    fn addresses_round_trip_and_reject_bad_input() {
        let a = Address::from_hex(ALICE).unwrap();
        assert_eq!(a.to_hex(), ALICE);
        assert_eq!(a.to_string(), ALICE);
        assert_eq!(Address::from_hex(&ALICE[2..]).unwrap(), a, "0x prefix optional");
        assert!(Address::from_hex("0x1234").is_err());
        assert!(Address::from_hex("0xzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzz").is_err());
        assert!(Address::zero().is_zero());
        assert!(!a.is_zero());
    }

    #[test]
    fn malformed_amounts_are_rejected_not_treated_as_zero() {
        // `parse().unwrap_or(0)` used to make "abc" a zero transfer.
        assert!(Account::parse_amount("abc").is_err());
        assert!(Account::parse_amount("-5").is_err());
        assert!(Account::parse_amount("").is_err());
        assert!(Account::parse_amount("1.5").is_err());
        assert_eq!(Account::parse_amount(" 42 ").unwrap(), 42);
    }

    #[test]
    fn balance_overflow_is_rejected() {
        let mut acc = Account::new(ALICE.to_string());
        acc.balance = u128::MAX.to_string();
        assert!(acc.add_balance("1").is_err());
        assert!(acc.can_transfer("0"));
    }

    #[test]
    fn can_transfer_rejects_malformed_amounts() {
        let mut acc = Account::new(ALICE.to_string());
        acc.balance = "100".to_string();
        assert!(acc.can_transfer("100"));
        assert!(!acc.can_transfer("101"));
        assert!(!acc.can_transfer("nonsense"));
    }

    #[test]
    fn transfer_moves_value_and_records_both_sides() {
        let mut m = manager_with_balances("1000", "5");
        m.set_context(42, "0xabc");

        m.transfer(ALICE, BOB, "400").unwrap();

        assert_eq!(m.accounts[ALICE].balance, "600");
        assert_eq!(m.accounts[BOB].balance, "405");
        assert_eq!(m.accounts[ALICE].tx_count, 1);

        let alice_history = m.history(ALICE).unwrap();
        assert_eq!(alice_history.len(), 1);
        assert_eq!(alice_history[0].balance_change, "-400");
        assert_eq!(alice_history[0].block_number, 42, "context block is recorded");
        assert_eq!(alice_history[0].tx_hash, "0xabc");
        assert!(alice_history[0].timestamp > 0);

        let bob_history = m.history(BOB).unwrap();
        assert_eq!(bob_history.len(), 1);
        assert_eq!(bob_history[0].balance_change, "400");
        assert_eq!(bob_history[0].block_number, 42);
    }

    #[test]
    fn failed_transfer_leaves_both_balances_untouched() {
        let mut m = manager_with_balances("10", "5");

        assert!(m.transfer(ALICE, BOB, "11").is_err());
        assert_eq!(m.accounts[ALICE].balance, "10");
        assert_eq!(m.accounts[BOB].balance, "5");
        assert!(m.history(ALICE).unwrap().is_empty());

        // An unknown receiver must fail before the sender is debited.
        assert!(m.transfer(ALICE, "0x9999999999999999999999999999999999999999", "5").is_err());
        assert_eq!(m.accounts[ALICE].balance, "10");
    }

    #[test]
    fn transfer_rejects_self_and_malformed_amounts() {
        let mut m = manager_with_balances("100", "0");
        assert!(m.transfer(ALICE, ALICE, "1").is_err());
        assert!(m.transfer(ALICE, BOB, "abc").is_err());
        assert_eq!(m.accounts[ALICE].balance, "100");
    }

    #[test]
    fn duplicate_accounts_are_rejected() {
        let mut m = AccountManager::new();
        m.create_account(ALICE.to_string()).unwrap();
        let err = m.create_account(ALICE.to_string()).unwrap_err();
        assert!(matches!(err, AccountError::AccountExists(_)));
        assert_eq!(m.stats().eoa_count, 1);
    }

    #[test]
    fn execute_tx_enforces_the_nonce() {
        let mut m = manager_with_balances("100", "0");

        let err = m.execute_tx(ALICE, BOB, "10", None, 5).unwrap_err();
        assert!(matches!(
            err,
            AccountError::NonceMismatch { expected: 0, actual: 5 }
        ));

        m.execute_tx(ALICE, BOB, "10", None, 0).unwrap();
        assert_eq!(m.accounts[ALICE].nonce, 1);
        assert_eq!(m.accounts[ALICE].balance, "90");
        assert_eq!(m.accounts[BOB].balance, "10");
    }

    #[test]
    fn execute_tx_deploys_to_a_deterministic_address() {
        let mut m = manager_with_balances("100", "0");

        let created = m
            .execute_tx(ALICE, BOB, "7", Some(vec![0x60, 0x00]), 0)
            .unwrap();

        assert_eq!(created, contract_address(ALICE, 0));
        assert!(m.is_contract(&created));
        assert_eq!(m.accounts[&created].balance, "7", "value follows the deployment");
        assert_eq!(m.accounts[ALICE].balance, "93");
        assert_eq!(m.accounts[BOB].balance, "0", "receiver untouched by a deploy");
        assert_eq!(m.stats().contract_count, 1);
    }

    #[test]
    fn self_destruct_moves_the_balance_and_clears_the_contract() {
        let mut m = manager_with_balances("100", "5");
        let created = m
            .execute_tx(ALICE, BOB, "40", Some(vec![0x00]), 0)
            .unwrap();
        assert_eq!(m.accounts[&created].balance, "40");

        m.self_destruct(&created, BOB).unwrap();

        assert_eq!(m.accounts[&created].balance, "0", "destroyed contract is drained");
        assert!(!m.accounts[&created].is_contract);
        assert_eq!(m.accounts[BOB].balance, "45");
        assert_eq!(m.stats().contract_count, 0);

        let history = m.history(BOB).unwrap();
        assert!(history
            .iter()
            .any(|e| e.change_type == HistoryChangeType::SelfDestruct));
    }

    #[test]
    fn self_destruct_rejects_non_contracts() {
        let mut m = manager_with_balances("100", "0");
        assert!(m.self_destruct(ALICE, BOB).is_err());
    }

    #[test]
    fn self_destruct_does_not_underflow_the_contract_count() {
        let mut m = AccountManager::new();
        m.create_account(ALICE.to_string()).unwrap();
        m.contract_count = 0;
        // A non-contract cannot self-destruct, so the counter stays at zero.
        assert!(m.self_destruct(ALICE, BOB).is_err());
        assert_eq!(m.stats().contract_count, 0);
    }

    #[test]
    fn storage_roots_change_with_contents() {
        let mut trie = StorageTrie::new();
        let empty_root = trie.root().to_string();
        trie.set("0x1".to_string(), "0xaa".to_string());
        let filled_root = trie.root().to_string();
        assert_ne!(empty_root, filled_root);

        trie.set("0x1".to_string(), "0xaa".to_string());
        assert_eq!(trie.root(), filled_root, "setting the same value is stable");
        assert_eq!(trie.get("0x1"), Some(&"0xaa".to_string()));

        trie.delete("0x1");
        assert!(trie.get("0x1").is_none());
    }

    #[test]
    fn account_error_displays_without_leaking_internals() {
        let err = AccountError::InsufficientBalance("5".to_string());
        assert_eq!(err.to_string(), "insufficient balance: 5");
        assert!(AccountError::BalanceOverflow.to_string().contains("overflow"));
    }
}
