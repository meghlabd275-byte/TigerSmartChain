//! Transaction Simulator - Simulate contract calls and transactions
//!
//! Both entry points talk to the node over JSON-RPC. Nothing is guessed: a
//! revert from the node is reported as an error, and the gas figure is whatever
//! `eth_estimateGas` returned.

use crate::types::*;
use serde_json::{json, Value};
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
    /// RPC URL for connecting to the node.
    rpc_url: String,
    /// Reused across calls so connections are pooled.
    client: reqwest::Client,
}

/// One JSON-RPC reply: either its result or its error message.
struct RpcOutcome {
    result: Option<Value>,
    error: Option<String>,
}

impl Simulator {
    /// Create new simulator
    pub fn new(rpc_url: &str) -> Self {
        Self {
            rpc_url: rpc_url.to_string(),
            client: reqwest::Client::new(),
        }
    }

    /// Send one JSON-RPC request.
    async fn rpc(&self, method: &str, params: Value) -> Result<RpcOutcome, SimulationError> {
        let request = json!({
            "jsonrpc": "2.0",
            "method": method,
            "params": params,
            "id": 1,
        });

        let response: Value = self
            .client
            .post(&self.rpc_url)
            .json(&request)
            .send()
            .await
            .map_err(|e| SimulationError::RPCError(e.to_string()))?
            .json()
            .await
            .map_err(|e| SimulationError::RPCError(e.to_string()))?;

        let error = response.get("error").map(|error| {
            error
                .get("message")
                .and_then(Value::as_str)
                .unwrap_or("unknown error")
                .to_string()
        });

        Ok(RpcOutcome {
            result: response.get("result").cloned(),
            error,
        })
    }

    /// Read a JSON-RPC quantity as a `u64`.
    fn quantity(outcome: &RpcOutcome, field: &str) -> Result<u64, SimulationError> {
        outcome
            .result
            .as_ref()
            .and_then(Value::as_str)
            .and_then(|hex| u64::from_str_radix(hex.trim_start_matches("0x"), 16).ok())
            .ok_or_else(|| SimulationError::SimulationFailed(format!("{field} returned no value")))
    }

    /// Simulate a read-only call.
    ///
    /// Runs `eth_call` at the requested block and then `eth_estimateGas` for the
    /// same call, so the reported gas figure is the node's own estimate rather
    /// than the caller's limit.
    pub async fn simulate_call(
        &self,
        call: &SimulateCallRequest,
    ) -> Result<SimulateResponse, SimulationError> {
        let block = call.block.as_deref().unwrap_or("latest");

        let mut params = json!({ "to": call.to, "data": call.data });
        if let Some(gas_limit) = call.gas_limit {
            params["gas"] = json!(format!("0x{gas_limit:x}"));
        }

        let outcome = self.rpc("eth_call", json!([params, block])).await?;

        if let Some(message) = outcome.error {
            return Err(SimulationError::SimulationFailed(message));
        }

        let data = outcome
            .result
            .as_ref()
            .and_then(Value::as_str)
            .ok_or_else(|| {
                SimulationError::SimulationFailed("eth_call returned no data".to_string())
            })?
            .to_string();

        let gas_used = match self
            .rpc("eth_estimateGas", json!([{ "to": call.to, "data": call.data }]))
            .await
        {
            Ok(estimate) if estimate.error.is_none() => Self::quantity(&estimate, "eth_estimateGas")?,
            Ok(estimate) => return Err(SimulationError::SimulationFailed(
                estimate.error.unwrap_or_else(|| "eth_estimateGas failed".to_string()),
            )),
            Err(error) => return Err(error),
        };

        Ok(SimulateResponse {
            data,
            gas_used,
            success: true,
            logs: vec![],
        })
    }

    /// Simulate a transaction.
    ///
    /// Runs `eth_estimateGas` first so that a revert is reported as a failure,
    /// then replays the call at the head to capture the return data.
    pub async fn simulate_transaction(
        &self,
        tx: &SimulateTransactionRequest,
    ) -> Result<SimulateResponse, SimulationError> {
        let mut params = json!({ "from": tx.from, "to": tx.to });

        if let Some(value) = tx.value.as_deref() {
            params["value"] = json!(value);
        }
        if let Some(data) = tx.data.as_deref() {
            params["data"] = json!(data);
        }

        let estimate = self.rpc("eth_estimateGas", json!([params])).await?;

        if let Some(message) = estimate.error {
            return Err(SimulationError::SimulationFailed(message));
        }

        let gas_used = Self::quantity(&estimate, "eth_estimateGas")?;

        params["gas"] = json!(format!("0x{gas_used:x}"));

        let outcome = self.rpc("eth_call", json!([params, "latest"])).await?;

        if let Some(message) = outcome.error {
            return Err(SimulationError::SimulationFailed(message));
        }

        let data = outcome
            .result
            .as_ref()
            .and_then(Value::as_str)
            .ok_or_else(|| {
                SimulationError::SimulationFailed("eth_call returned no data".to_string())
            })?
            .to_string();

        Ok(SimulateResponse {
            data,
            gas_used,
            success: true,
            logs: vec![],
        })
    }

    /// Simulate multiple calls in a batch
    pub async fn simulate_batch(
        &self,
        calls: &[SimulateCallRequest],
    ) -> Vec<Result<SimulateResponse, SimulationError>> {
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

    #[test]
    fn a_missing_quantity_is_an_error() {
        let outcome = RpcOutcome {
            result: None,
            error: None,
        };
        assert!(Simulator::quantity(&outcome, "eth_estimateGas").is_err());

        let outcome = RpcOutcome {
            result: Some(json!("0x5208")),
            error: None,
        };
        assert_eq!(Simulator::quantity(&outcome, "eth_estimateGas").unwrap(), 21000);
    }
}
