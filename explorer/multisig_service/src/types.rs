//! Multisig Types

use serde::{Deserialize, Serialize};

/// Multisig Info
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MultisigInfo {
    pub address: String,
    pub threshold: u32,
    pub owners: Vec<String>,
    pub implementation: String,
    pub is_gnosis_safe: bool,
    pub is_multisig: bool,
}

/// Multisig Transaction
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MultisigTransaction {
    pub tx_hash: String,
    pub to: String,
    pub value: String,
    pub data: String,
    pub executed: bool,
    pub confirmations: Vec<String>,
}

/// Multisig Config
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    pub rpc_url: String,
    pub safe_master_copy: String,
    pub safe_proxy_factory: String,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            rpc_url: "http://localhost:8545".to_string(),
            safe_master_copy: "0xd9db270c1b5e3bd161e8c8503c55ceabee209425".to_string(),
            safe_proxy_factory: "0xa6b71e26c5e0845f74c812102ca7114b6a896ab2".to_string(),
        }
    }
}