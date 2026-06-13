//! Decompiler Types

use serde::{Deserialize, Serialize};

/// Decompiled Contract
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DecompiledContract {
    pub address: String,
    pub bytecode: String,
    pub functions: Vec<DecompiledFunction>,
    pub variables: Vec<StorageVariable>,
    pub events: Vec<DecompiledEvent>,
}

/// Decompiled Function
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DecompiledFunction {
    pub name: String,
    pub signature: String,
    pub selector: String,
    pub visibility: String,
    pub inputs: Vec<Parameter>,
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
}

/// Decompiled Event
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DecompiledEvent {
    pub name: String,
    pub signature: String,
    pub topic: String,
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
            rpc_url: "http://localhost:8545".to_string(),
            max_iterations: 10000,
            timeout_ms: 30000,
        }
    }
}