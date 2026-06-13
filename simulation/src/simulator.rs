//! Transaction Simulator - Simulate contract calls and transactions

use crate::types::*;
use thiserror::Error;

// =============================================================================
// ERRORS
// =============================================================================

#[derive(Error, Debug)]
pub enum SimulationError {
    #[error("RPC error: {0}")]
    RPCError(String),
    #[error("Simulation failed: {0}")]
    SimulationFailed(String),
    #[error("Invalid request: {0}")]
    InvalidRequest(String),
}

// =============================================================================
// SIMULATOR
// =============================================================================

/// Transaction Simulator
pub struct Simulator {
    /// RPC URL for connecting to Ethereum node
    rpc_url: String,
}

impl Simulator {
    /// Create new simulator
    pub fn new(rpc_url: &str) -> Self {
        Self {
            rpc_url: rpc_url.to_string(),
        }
    }

    /// Simulate a call (read-only)
    pub async fn simulate_call(&self, call: &SimulateCallRequest) -> Result<SimulateResponse, SimulationError> {
        let client = reqwest::Client::new();
        
        let params = serde_json::json!({
            "to": call.to,
            "data": call.data,
        });

        let request = serde_json::json!({
            "jsonrpc": "2.0",
            "method": "eth_call",
            "params": [params, call.block.unwrap_or("latest")],
            "id": 1
        });

        let response = client
            .post(&self.rpc_url)
            .json(&request)
            .send()
            .await
            .map_err(|e| SimulationError::RPCError(e.to_string()))?
            .json::<serde_json::Value>()
            .await
            .map_err(|e| SimulationError::RPCError(e.to_string()))?;

        if let Some(error) = response.get("error") {
            return Err(SimulationError::SimulationFailed(
                error.get("message")
                    .and_then(|m| m.as_str())
                    .unwrap_or("Unknown error")
                    .to_string()
            ));
        }

        let result = response.get("result")
            .and_then(|v| v.as_str())
            .unwrap_or("0x")
            .to_string();

        Ok(SimulateResponse {
            data: result,
            gas_used: call.gas_limit,
            success: true,
            logs: vec![],
        })
    }

    /// Simulate a transaction (estimate gas)
    pub async fn simulate_transaction(&self, tx: &SimulateTransactionRequest) -> Result<SimulateResponse, SimulationError> {
        let client = reqwest::Client::new();
        
        let params = serde_json::json!({
            "from": tx.from,
            "to": tx.to,
            "value": tx.value,
            "data": tx.data,
        });

        let request = serde_json::json!({
            "jsonrpc": "2.0",
            "method": "eth_estimateGas",
            "params": [params],
            "id": 1
        });

        let response = client
            .post(&self.rpc_url)
            .json(&request)
            .send()
            .await
            .map_err(|e| SimulationError::RPCError(e.to_string()))?
            .json::<serde_json::Value>()
            .await
            .map_err(|e| SimulationError::RPCError(e.to_string()))?;

        let gas_hex = response.get("result")
            .and_then(|v| v.as_str())
            .unwrap_or("0x5208");

        let gas_used = u64::from_str_radix(gas_hex.trim_start_matches("0x"), 16)
            .unwrap_or(21000);

        // Also try to call the transaction to see what it returns
        let call_params = serde_json::json!({
            "from": tx.from,
            "to": tx.to,
            "value": tx.value,
            "data": tx.data,
        });

        let call_request = serde_json::json!({
            "jsonrpc": "2.0",
            "method": "eth_call",
            "params": [call_params, "latest"],
            "id": 2
        });

        let call_response = client
            .post(&self.rpc_url)
            .json(&call_request)
            .send()
            .await
            .map_err(|e| SimulationError::RPCError(e.to_string()))?
            .json::<serde_json::Value>()
            .await
            .map_err(|e| SimulationError::RPCError(e.to_string()))?;

        let result_data = call_response.get("result")
            .and_then(|v| v.as_str())
            .unwrap_or("0x")
            .to_string();

        Ok(SimulateResponse {
            data: result_data,
            gas_used,
            success: true,
            logs: vec![],
        })
    }

    /// Simulate multiple calls in a batch
    pub async fn simulate_batch(&self, calls: &[SimulateCallRequest]) -> Vec<Result<SimulateResponse, SimulationError>> {
        let mut results = Vec::new();
        
        for call in calls {
            results.push(self.simulate_call(call).await);
        }
        
        results
    }
}

// =============================================================================
// TESTS
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_simulator_creation() {
        let sim = Simulator::new("http://localhost:8545");
        assert_eq!(sim.rpc_url, "http://localhost:8545");
    }
}