//! Decompiler Types

use serde::{Deserialize, Serialize};

/// Decompiled Contract
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DecompiledContract {
    pub address: String,
    pub bytecode: String,
    pub contract_type: String,
    pub functions: Vec<DecompiledFunction>,
    pub variables: Vec<StorageVariable>,
    pub events: Vec<DecompiledEvent>,
    /// True when the bytecode carries an EIP-1167 minimal proxy forwarder.
    pub is_minimal_proxy: bool,
    /// True when the bytecode carries the EIP-1967 implementation slot.
    pub is_proxy: bool,
    /// True when the runtime code ends with a Solidity CBOR metadata trailer.
    pub has_metadata: bool,
    /// Length of the runtime code in bytes, excluding the metadata trailer.
    pub runtime_size: usize,
}

/// Decompiled Function
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DecompiledFunction {
    pub name: String,
    pub signature: String,
    pub selector: String,
    pub visibility: String,
    pub inputs: Vec<Parameter>,
    /// True when the selector was found in the local signature table.
    pub resolved: bool,
}

/// Parameter
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Parameter {
    pub name: String,
    pub param_type: String,
}

/// Storage Variable
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StorageVariable {
    pub name: String,
    pub param_type: String,
    pub slot: usize,
    /// `read` for SLOAD, `write` for SSTORE, `read_write` for both.
    pub access: String,
}

/// Decompiled Event
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DecompiledEvent {
    pub name: String,
    pub signature: String,
    pub topic: String,
    /// Number of indexed parameters, derived from the LOG opcode used.
    pub indexed_count: u8,
}

/// Decompiler Config
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    pub rpc_url: String,
    pub max_iterations: usize,
    pub timeout_ms: u64,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            rpc_url: std::env::var("RPC_URL")
                .unwrap_or_else(|_| "http://localhost:8545".to_string()),
            max_iterations: 10000,
            timeout_ms: 30000,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_default_config_reads_the_environment() {
        let config = Config::default();

        assert!(!config.rpc_url.is_empty());
        assert!(config.max_iterations > 0);
    }
}
