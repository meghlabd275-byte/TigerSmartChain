//! Simulation Types

use serde::{Deserialize, Serialize};

// =============================================================================
// SIMULATION
// =============================================================================

/// Simulation Request
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SimulationRequest {
    pub from: String,
    pub to: String,
    pub value: u64,
    pub data: Vec<u8>,
    pub gas: u64,
    pub block_number: u64,
}

/// Simulation Result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SimulationResult {
    pub success: bool,
    pub gas_used: u64,
    pub return_value: Vec<u8>,
    pub logs: Vec<Log>,
    pub error: Option<String>,
}

/// Log
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Log {
    pub address: String,
    pub topics: Vec<String>,
    pub data: Vec<u8>,
}

/// Simulator
pub struct Simulator {
    block_number: u64,
}

impl Simulator {
    pub fn new(block_number: u64) -> Self {
        Self { block_number }
    }

    /// Simulate
    pub fn simulate(&self, _req: &SimulationRequest) -> SimulationResult {
        SimulationResult {
            success: true,
            gas_used: 21000,
            return_value: vec![],
            logs: vec![],
            error: None,
        }
    }
}