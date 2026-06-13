//! Transaction Simulation Service Implementation

use crate::types::*;
use chrono::Utc;
use std::sync::Arc;
use thiserror::Error;
use reqwest::Client;
use serde_json::{json, Value};
use std::time::Duration;

// =============================================================================
// ERRORS
// =============================================================================

#[derive(Error, Debug)]
pub enum SimulationError {
    #[error("RPC error: {0}")]
    RPCError(String),
    #[error("Parse error: {0}")]
    ParseError(String),
    #[error("Simulation failed: {0}")]
    SimulationFailed(String),
}

// =============================================================================
// SERVICE
// =============================================================================

/// Transaction Simulation Service
pub struct SimulationService {
    config: SimulationConfig,
    client: Client,
}

impl SimulationService {
    /// Create new simulation service
    pub fn new(rpc_url: &str) -> Self {
        let config = SimulationConfig {
            rpc_url: rpc_url.to_string(),
            ..Default::default()
        };
        
        let client = Client::builder()
            .timeout(Duration::from_secs(config.timeout_secs))
            .build()
            .unwrap_or_else(|_| Client::new());
        
        Self { config, client }
    }

    /// Create with custom config
    pub fn with_config(config: SimulationConfig) -> Self {
        let client = Client::builder()
            .timeout(Duration::from_secs(config.timeout_secs))
            .build()
            .unwrap_or_else(|_| Client::new());
        
        Self { config, client }
    }

    /// Simulate a call
    pub async fn simulate_call(&self, request: &SimulationRequest) -> Result<SimulationResult, SimulationError> {
        self.simulate_with_overrides(request, None).await
    }

    /// Simulate a call with state overrides
    pub async fn simulate_with_overrides(
        &self,
        request: &SimulationRequest,
        state_overrides: Option<StateOverrideMap>,
    ) -> Result<SimulationResult, SimulationError> {
        let block = request.block_number.as_deref().unwrap_or("latest");
        
        // Build call object
        let call_obj = json!({
            "from": request.from,
            "to": request.to,
            "gas": request.gas,
            "gasPrice": request.gas_price,
            "value": request.value,
            "data": request.data,
        });

        // Build trace options
        let trace_options = json!({
            "disableStorage": false,
            "disableStack": false,
            "enableMemory": true,
            "enableReturnData": true,
            "tracer": self.config.default_tracer,
        });

        // Make the request
        let params = if let Some(overrides) = state_overrides {
            json!([
                call_obj,
                block,
                trace_options,
                overrides.overrides
            ])
        } else {
            json!([
                call_obj,
                block,
                trace_options
            ])
        };

        let request_body = json!({
            "jsonrpc": "2.0",
            "method": "debug_traceCall",
            "params": params,
            "id": 1
        });

        let response = self.client
            .post(&self.config.rpc_url)
            .json(&request_body)
            .send()
            .await
            .map_err(|e| SimulationError::RPCError(e.to_string()))?
            .json::<Value>()
            .await
            .map_err(|e| SimulationError::ParseError(e.to_string()))?;

        if let Some(error) = response.get("error") {
            return Err(SimulationError::SimulationFailed(
                error.get("message")
                    .and_then(|m| m.as_str())
                    .unwrap_or("Unknown error")
                    .to_string()
            ));
        }

        let result = response.get("result")
            .ok_or_else(|| SimulationError::RPCError("No result".to_string()))?;

        // Parse result
        let success = result.get("failed").and_then(|v| v.as_bool()).unwrap_or(false);
        let gas_used = result.get("gasUsed")
            .and_then(|v| v.as_str())
            .map(|s| u64::from_str_radix(s.trim_start_matches("0x"), 16).unwrap_or(0))
            .unwrap_or(0);
        
        let return_value = result.get("returnValue")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();

        let error = result.get("structLogs")
            .and_then(|logs| logs.as_array())
            .and_then(|arr| arr.iter().find_map(|log| {
                log.get("error").and_then(|e| e.as_str()).map(|s| s.to_string())
            }));

        Ok(SimulationResult {
            success: !success,
            gas_used,
            return_value,
            logs: Vec::new(),
            error,
            traces: Vec::new(),
            state_changes: Vec::new(),
        })
    }

    /// Estimate gas for a call
    pub async fn estimate_gas(&self, request: &SimulationRequest) -> Result<GasEstimation, SimulationError> {
        let call_obj = json!({
            "from": request.from,
            "to": request.to,
            "gas": request.gas,
            "gasPrice": request.gas_price,
            "value": request.value,
            "data": request.data,
        });

        let request_body = json!({
            "jsonrpc": "2.0",
            "method": "eth_estimateGas",
            "params": [call_obj, "latest"],
            "id": 1
        });

        let response = self.client
            .post(&self.config.rpc_url)
            .json(&request_body)
            .send()
            .await
            .map_err(|e| SimulationError::RPCError(e.to_string()))?
            .json::<Value>()
            .await
            .map_err(|e| SimulationError::ParseError(e.to_string()))?;

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
            .map(|s| u64::from_str_radix(s.trim_start_matches("0x"), 16).unwrap_or(0))
            .unwrap_or(21000);

        // Estimate different gas levels
        let low = (result as f64 * 1.1) as u64;
        let standard = (result as f64 * 1.2) as u64;
        let fast = (result as f64 * 1.3) as u64;

        Ok(GasEstimation {
            low,
            standard,
            fast,
            estimated_gas: result,
            error: None,
        })
    }

    /// Execute multiple calls
    pub async fn simulate_multi(&self, request: &MultiCallRequest) -> Result<MultiCallResult, SimulationError> {
        let mut results = Vec::new();
        
        for call in &request.calls {
            let result = self.simulate_with_overrides(call, request.state_overrides.clone()).await?;
            results.push(result);
        }

        let block_number = request.block_number.as_ref()
            .and_then(|b| b.strip_prefix("0x"))
            .and_then(|b| u64::from_str_radix(b, 16).ok())
            .unwrap_or(0);

        Ok(MultiCallResult {
            results,
            block_number,
            timestamp: Utc::now().timestamp(),
        })
    }

    /// Simulate token balance check
    pub async fn simulate_token_balance(
        &self,
        token: &str,
        owner: &str,
        block: Option<&str>,
    ) -> Result<TokenBalanceSimulation, SimulationError> {
        // ERC20 balanceOf selector: 0x70a08231
        let data = format!("0x70a08231{:0>64}", hex::encode(owner.trim_start_matches("0x")));
        
        let request = SimulationRequest {
            from: None,
            to: token.to_string(),
            gas: None,
            gas_price: None,
            value: None,
            data: Some(data),
            nonce: None,
            block_number: block.map(|b| b.to_string()),
        };

        let result = self.simulate_call(&request).await?;
        
        let balance = if result.success {
            // Parse balance from return value
            let hex_val = result.return_value.trim_start_matches("0x");
            u128::from_str_radix(hex_val, 16).unwrap_or(0).to_string()
        } else {
            "0".to_string()
        };

        Ok(TokenBalanceSimulation {
            token_address: token.to_string(),
            owner: owner.to_string(),
            balance,
            block_number: 0,
        })
    }

    /// Simulate token transfer
    pub async fn simulate_token_transfer(
        &self,
        token: &str,
        from: &str,
        to: &str,
        amount: &str,
    ) -> Result<TokenTransferSimulation, SimulationError> {
        // Get balances before
        let balance_before_from = self.simulate_token_balance(token, from, None).await?;
        let balance_before_to = self.simulate_token_balance(token, to, None).await?;

        // ERC20 transfer selector: 0xa9059cbb + padded address + padded amount
        let amount_hex = format!("{:0>64}", hex::encode(amount.trim_start_matches("0x")));
        let to_padded = format!("{:0>64}", to.trim_start_matches("0x").trim_start_matches("0x"));
        let data = format!("0xa9059cbb{}{}", to_padded, amount_hex);
        
        let request = SimulationRequest {
            from: Some(from.to_string()),
            to: token.to_string(),
            gas: None,
            gas_price: None,
            value: None,
            data: Some(data),
            nonce: None,
            block_number: None,
        };

        let result = self.simulate_call(&request).await?;

        // Get balances after
        let balance_after_from = self.simulate_token_balance(token, from, None).await?;
        let balance_after_to = self.simulate_token_balance(token, to, None).await?;

        Ok(TokenTransferSimulation {
            token_address: token.to_string(),
            from: from.to_string(),
            to: to.to_string(),
            amount: amount.to_string(),
            success: result.success,
            balance_before_from: balance_before_from.balance,
            balance_after_from: balance_after_from.balance,
            balance_before_to: balance_before_to.balance,
            balance_after_to: balance_after_to.balance,
        })
    }
}