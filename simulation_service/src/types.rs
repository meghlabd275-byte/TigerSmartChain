//! Simulation Types for TigerScan

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

// =============================================================================
// SIMULATION REQUEST
// =============================================================================

/// Simulation Request
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SimulationRequest {
    /// From address
    pub from: Option<String>,
    /// To address
    pub to: String,
    /// Gas limit
    pub gas: Option<String>,
    /// Gas price
    pub gas_price: Option<String>,
    /// Value
    pub value: Option<String>,
    /// Input data
    pub data: Option<String>,
    /// Nonce
    pub nonce: Option<String>,
    /// Block number (for historical simulation)
    pub block_number: Option<String>,
}

/// State Override
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StateOverride {
    /// Override balance
    pub balance: Option<String>,
    /// Override nonce
    pub nonce: Option<String>,
    /// Override code
    pub code: Option<String>,
    /// Override storage
    pub storage: HashMap<String, String>,
}

/// State Override Map
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StateOverrideMap {
    #[serde(flatten)]
    pub overrides: HashMap<String, StateOverride>,
}

// =============================================================================
// SIMULATION RESULT
// =============================================================================

/// Simulation Result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SimulationResult {
    /// Whether simulation was successful
    pub success: bool,
    /// Gas used
    pub gas_used: u64,
    /// Return value
    pub return_value: String,
    /// Logs
    pub logs: Vec<SimulationLog>,
    /// Error message (if any)
    pub error: Option<String>,
    /// Call traces
    pub traces: Vec<CallFrame>,
    /// State changes
    pub state_changes: Vec<StateChange>,
}

/// Simulation Log
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SimulationLog {
    pub address: String,
    pub topics: Vec<String>,
    pub data: String,
}

/// Call Frame
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CallFrame {
    pub call_type: String,
    pub from: String,
    pub to: String,
    pub value: String,
    pub gas: String,
    pub gas_used: String,
    pub input: String,
    pub output: String,
    pub calls: Vec<CallFrame>,
    pub error: Option<String>,
}

/// State Change
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StateChange {
    pub address: String,
    pub key: Option<String>,
    pub previous: String,
    pub current: String,
}

// =============================================================================
// GAS ESTIMATION
// =============================================================================

/// Gas Estimation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GasEstimation {
    /// Low estimate
    pub low: u64,
    /// Standard estimate
    pub standard: u64,
    /// Fast estimate
    pub fast: u64,
    /// Estimated gas used by call
    pub estimated_gas: u64,
    /// Error (if estimation failed)
    pub error: Option<String>,
}

// =============================================================================
// TOKEN SIMULATION
// =============================================================================

/// Token Balance Simulation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenBalanceSimulation {
    pub token_address: String,
    pub owner: String,
    pub balance: String,
    pub block_number: u64,
}

/// Token Transfer Simulation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenTransferSimulation {
    pub token_address: String,
    pub from: String,
    pub to: String,
    pub amount: String,
    pub success: bool,
    pub balance_before_from: String,
    pub balance_after_from: String,
    pub balance_before_to: String,
    pub balance_after_to: String,
}

// =============================================================================
// MULTI-CALL SIMULATION
// =============================================================================

/// Multi-Call Request
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MultiCallRequest {
    /// Calls to execute
    pub calls: Vec<SimulationRequest>,
    /// State overrides
    pub state_overrides: Option<StateOverrideMap>,
    /// Block number
    pub block_number: Option<String>,
}

/// Multi-Call Result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MultiCallResult {
    /// Results for each call
    pub results: Vec<SimulationResult>,
    /// Block number used
    pub block_number: u64,
    /// Timestamp
    pub timestamp: i64,
}

// =============================================================================
// CONFIG
// =============================================================================

/// Simulation Service Configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SimulationConfig {
    /// RPC URL
    pub rpc_url: String,
    /// Request timeout
    pub timeout_secs: u64,
    /// Max gas limit
    pub max_gas_limit: u64,
    /// Enable trace
    pub enable_trace: bool,
    /// Enable state diffs
    pub enable_state_diffs: bool,
    /// Default tracer
    pub default_tracer: String,
}

impl Default for SimulationConfig {
    fn default() -> Self {
        Self {
            rpc_url: "http://localhost:8545".to_string(),
            timeout_secs: 30,
            max_gas_limit: 30_000_000,
            enable_trace: true,
            enable_state_diffs: true,
            default_tracer: "callTracer".to_string(),
        }
    }
}