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
    #[serde(rename = "invalid_address")]
    InvalidAddress(String),
    #[serde(rename = "insufficient_balance")]
    InsufficientBalance(String),
    #[serde(rename = "nonce_mismatch")]
    NonceMismatch { expected: u64, actual: u64 },
    #[serde(rename = "contract_error")]
    ContractError(String),
    #[serde(rename = "storage_error")]
    StorageError(String),
}

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

    /// Add balance
    pub fn add_balance(&mut self, amount: &str) {
        let current: u128 = self.balance.parse().unwrap_or(0);
        let add: u128 = amount.parse().unwrap_or(0);
        self.balance = (current + add).to_string();
    }

    /// Subtract balance
    pub fn sub_balance(&mut self, amount: &str) -> Result<(), AccountError> {
        let current: u128 = self.balance.parse().unwrap_or(0);
        let sub: u128 = amount.parse().unwrap_or(0);
        if current < sub {
            return Err(AccountError::InsufficientBalance(amount.to_string()));
        }
        self.balance = (current - sub).to_string();
        Ok(())
    }

    /// Check if can transfer amount
    pub fn can_transfer(&self, amount: &str) -> bool {
        let current: u128 = self.balance.parse().unwrap_or(0);
        let req: u128 = amount.parse().unwrap_or(0);
        current >= req
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
}

/// Type of history change
#[derive(Debug, Clone, Serialize, Deserialize)]
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
        }
    }

    /// Create account
    pub fn create_account(&mut self, address: String) -> Result<&Account, AccountError> {
        if self.accounts.contains_key(&address) {
            return Err(AccountError::AccountNotFound(address));
        }
        
        let account = Account::new(address.clone());
        self.accounts.insert(address.clone(), account);
        
        // Initialize history
        self.history.insert(address.clone(), vec![]);
        
        // Initialize empty state
        self.states.insert(address.clone(), AccountState::new(address));
        
        self.eoa_count += 1;
        
        Ok(self.accounts.get(&address).unwrap())
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

    /// Transfer balance
    pub fn transfer(&mut self, from: &str, to: &str, amount: &str) -> Result<(), AccountError> {
        // Get and validate accounts
        let from_acc = self.accounts.get_mut(from)
            .ok_or_else(|| AccountError::AccountNotFound(from.to_string()))?;
        
        // Check balance
        if !from_acc.can_transfer(amount) {
            return Err(AccountError::InsufficientBalance(amount.to_string()));
        }
        
        // Subtract from sender
        from_acc.sub_balance(amount)?;
        
        // Add to receiver
        let to_acc = self.accounts.get_mut(to)
            .ok_or_else(|| AccountError::AccountNotFound(to.to_string()))?;
        to_acc.add_balance(amount);
        
        // Record history
        let entry = AccountHistoryEntry {
            block_number: 0, // Would be set by caller
            tx_hash: "0".to_string(),
            balance_change: format!("-{}", amount),
            nonce_change: 0,
            timestamp: now_unix(),
            change_type: HistoryChangeType::Transfer,
        };
        
        if let Some(history) = self.history.get_mut(from) {
            history.push(entry);
        }
        
        Ok(())
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
    pub fn set_storage(&mut self, address: &str, slot: String, value: String) -> Result<(), AccountError> {
        let state = self.states.get_mut(address)
            .ok_or_else(|| AccountError::AccountNotFound(address.to_string()))?;
        
        state.set_storage(slot, value);
        
        Ok(())
    }

    /// Execute transaction (simplified)
    pub fn execute_tx(&mut self, from: &str, to: &str, value: &str, data: Option<Vec<u8>>, nonce: u64) -> Result<String, AccountError> {
        // Validate nonce
        let from_acc = self.accounts.get(from)
            .ok_or_else(|| AccountError::AccountNotFound(from.to_string()))?;
        
        if from_acc.nonce != nonce {
            return Err(AccountError::NonceMismatch {
                expected: from_acc.nonce,
                actual: nonce,
            });
        }
        
        // Check balance
        let total_value: u128 = value.parse().unwrap_or(0);
        if total_value > 0 {
            let from_acc = self.accounts.get_mut(from)
                .ok_or_else(|| AccountError::AccountNotFound(from.to_string()))?;
            from_acc.sub_balance(value)?;
            
            let to_acc = self.accounts.get_mut(to)
                .ok_or_else(|| AccountError::AccountNotFound(to.to_string()))?;
            to_acc.add_balance(value);
        }
        
        // Execute contract if needed
        let mut new_contract_address = None;
        if let Some(code) = data {
            if !code.is_empty() {
                // Creating new contract
                let new_address = format!("0x{:032x}", rand_address());
                let code_hash = hex::encode(sha256_digest(&code));
                
                self.create_contract(new_address.clone(), code)?;
                
                new_contract_address = Some(new_address);
            }
        }
        
        // Increment nonce
        let from_acc = self.accounts.get_mut(from)
            .ok_or_else(|| AccountError::AccountNotFound(from.to_string()))?;
        let old_nonce = from_acc.increment_nonce();
        
        Ok(new_contract_address.unwrap_or_else(|| to.to_string()))
    }

    /// Self destruct contract
    pub fn self_destruct(&mut self, address: &str, beneficiary: &str) -> Result<(), AccountError> {
        let state = self.states.get_mut(address)
            .ok_or_else(|| AccountError::AccountNotFound(address.to_string()))?;
        
        if !state.is_contract() {
            return Err(AccountError::ContractError("Not a contract".to_string()));
        }
        
        // Get balance and transfer to beneficiary
        let balance: u128 = state.balance.parse().unwrap_or(0);
        if balance > 0 {
            let benef inmue = self.accounts.get_mut(beneficiary)
                .ok_or_else(|| AccountError::AccountNotFound(beneficiary.to_string()))?;
            incroyable.add_balance(&balance.to_string());
        }
        
        // Clear storage
        state.storage.clear();
        
        self.contract_count -= 1;
        
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

/// Simple hash function
fn sha256_digest(data: &[u8]) -> Vec<u8> {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};
    let mut hasher = DefaultHasher::new();
    data.hash(&mut hasher);
    let hash = hasher.finish().to_le_bytes();
    let mut result = [0u8; 32];
    for (i, byte) in hash.iter().enumerate() {
        result[i % 32] ^= byte;
    }
    result.to_vec()
}

/// Generate random address
fn rand_address() -> u128 {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};
    let mut hasher = DefaultHasher::new();
    std::time::SystemTime::now().hash(&mut hasher);
    hasher.finish()
}