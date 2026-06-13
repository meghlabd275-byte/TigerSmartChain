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
            safe_master_copy: "0xd9dbafc0b7dd8e3f08b1a6a4c5f5d3a8e4f2b1c".to_string(),
            safe_proxy_factory: "0xa6b71e26c5e0845f74c1021c2661f19ec6e3ce6e".to_string(),
        }
    }
}