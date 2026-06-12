//! Precompile Types

use serde::{Deserialize, Serialize};

// =============================================================================
// PRECOMPILE
// =============================================================================

/// Precompile Contract
#[derive(Debug, Clone)]
pub struct Precompile {
    pub address: u64,
    pub name: String,
    pub min_gas: u64,
    pub function: fn(&[u8], u64) -> Result<Vec<u8>, String>,
}

/// Precompile Result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PrecompileResult {
    pub success: bool,
    pub output: Vec<u8>,
    pub gas_used: u64,
}

impl PrecompileResult {
    pub fn success(output: Vec<u8>, gas_used: u64) -> Self {
        Self {
            success: true,
            output,
            gas_used,
        }
    }

    pub fn error(message: String, gas_used: u64) -> Self {
        Self {
            success: false,
            output: message.into_bytes(),
            gas_used,
        }
    }
}

// =============================================================================
// GAS COSTS
// =============================================================================

/// Precompile gas costs
pub struct PrecompileGas {
    pub ecrecover: u64,
    pub sha256: u64,
    pub ripemd160: u64,
    pub identity: u64,
    pub modexp: u64,
    pub ecadd: u64,
    pub ecmul: u64,
    pub ecpairing: u64,
    pub blake2f: u64,
}

impl Default for PrecompileGas {
    fn default() -> Self {
        Self {
            ecrecover: 3000,
            sha256: 60,
            ripemd160: 600,
            identity: 15,
            modexp: 0,
            ecadd: 150,
            ecmul: 6000,
            ecpairing: 45000,
            blake2f: 0,
        }
    }
}