//! AA Types

use serde::{Deserialize, Serialize};

/// Account Abstraction Info
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AccountAbstractionInfo {
    pub address: String,
    pub is_smart_account: bool,
    pub entry_point: Option<String>,
    pub factory: Option<String>,
    pub paymaster: Option<String>,
    pub account_type: String,
}

/// Validation Result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidationResult {
    pub sender: String,
    pub nonce: String,
    pub valid_after: u64,
    pub valid_until: u64,
}

/// AA Config
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    pub entry_point: String,
    pub rpc_url: String,
    pub bundler_url: String,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            entry_point: "0x5FF137D4b0FDCD49DcA30C7CF57E578a026d2789".to_string(),
            rpc_url: "http://localhost:8545".to_string(),
            bundler_url: "http://localhost:3000".to_string(),
        }
    }
}