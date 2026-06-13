//! Simulation Types

use serde::{Deserialize, Serialize};

// =============================================================================
// SIMULATION
// =============================================================================

/// Call request for simulation (read-only)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SimulateCallRequest {
    /// Contract address to call
    pub to: String,
    /// Call data (encoded function selector + parameters)
    pub data: String,
    /// Block to execute at (default: latest)
    pub block: Option<String>,
    /// Gas limit
    pub gas_limit: Option<u64>,
}

/// Transaction request for simulation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SimulateTransactionRequest {
    /// From address
    pub from: String,
    /// To address
    pub to: String,
    /// Value in wei
    pub value: Option<String>,
    /// Call data
    pub data: Option<String>,
    /// Gas limit
    pub gas_limit: Option<u64>,
}

/// Simulation response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SimulateResponse {
    /// Return data from the call
    pub data: String,
    /// Gas used
    pub gas_used: u64,
    /// Whether the call succeeded
    pub success: bool,
    /// Event logs
    pub logs: Vec<SimulateLog>,
}

/// Log entry from simulation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SimulateLog {
    pub address: String,
    pub topics: Vec<String>,
    pub data: String,
}