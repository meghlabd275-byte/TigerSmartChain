//! TigerScan Transaction Simulation Service
//! Production-grade transaction simulation for safety checks and preview

use std::sync::Arc;
use std::time::Duration;

use anyhow::Result;
use async_trait::async_trait;
use ethers::providers::{Http, Provider};
use ethers::types::{Address, Bytes, H256, U256, U64};
use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use sqlx::postgres::{PgPool, PgPoolOptions};
use thiserror::Error;
use tracing::{error, info, warn};

// ============================================================================
// Error Types
// ============================================================================

#[derive(Error, Debug)]
pub enum SimulationError {
    #[error("Simulation failed: {0}")]
    Failed(String),
    
    #[error("RPC error: {0}")]
    Rpc(String),
    
    #[error("Validation error: {0}")]
    Validation(String),
    
    #[error("Database error: {0}")]
    Database(#[from] sqlx::Error),
}

// ============================================================================
// Configuration
// ============================================================================

#[derive(Debug, Clone, Deserialize)]
pub struct Config {
    /// RPC HTTP endpoint
    pub rpc_url: String,
    /// Database connection string
    pub database_url: String,
    /// Request timeout
    pub timeout_seconds: u64,
    /// Gas multiplier for estimates
    pub gas_multiplier: f64,
    /// Enable state reverting
    pub enable_state_revert: bool,
    /// Simulation depth limit
    pub depth_limit: u32,
    /// Trace enabled
    pub trace_enabled: bool,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            rpc_url: std::env::var("RPC_URL").unwrap_or_else(|_| "http://localhost:8545".to_string()),
            database_url: std::env::var("DATABASE_URL").unwrap_or_else(|_| "postgres://localhost:5432/tigerscan".to_string()),
            timeout_seconds: 30,
            gas_multiplier: 1.1,
            enable_state_revert: true,
            depth_limit: 10,
            trace_enabled: true,
        }
    }
}

// ============================================================================
// Types
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SimulationRequest {
    /// From address
    pub from: String,
    /// To address
    pub to: Option<String>,
    /// Transaction value
    pub value: Option<String>,
    /// Gas price
    pub gas_price: Option<String>,
    /// Gas limit
    pub gas_limit: Option<u64>,
    /// Input data
    pub data: Option<String>,
    /// Chain ID
    pub chain_id: Option<u64>,
    /// Nonce
    pub nonce: Option<u64>,
    /// Contract code (for contract creation)
    pub code: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SimulationResult {
    /// Whether the transaction would succeed
    pub success: bool,
    /// Reason for failure if any
    pub error: Option<String>,
    /// Gas used
    pub gas_used: u64,
    /// Gas limit
    pub gas_limit: u64,
    /// Return value
    pub return_value: Option<String>,
    /// Logs generated
    pub logs: Vec<SimulatedLog>,
    /// State changes
    pub state_changes: Vec<StateChange>,
    /// Calls (internal transactions)
    pub calls: Vec<SimulatedCall>,
    /// Balance changes
    pub balance_changes: Vec<BalanceChange>,
    /// Warnings
    pub warnings: Vec<String>,
    /// Safety score
    pub safety_score: u32,
    /// Safety flags
    pub safety_flags: Vec<SafetyFlag>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SimulatedLog {
    pub address: String,
    pub topics: Vec<String>,
    pub data: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StateChange {
    pub address: String,
    pub slot: String,
    pub old_value: String,
    pub new_value: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SimulatedCall {
    pub call_type: String,
    pub from: String,
    pub to: String,
    pub value: String,
    pub input: String,
    pub output: String,
    pub gas_used: u64,
    pub depth: u32,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BalanceChange {
    pub address: String,
    pub token: String,
    pub old_balance: String,
    pub new_balance: String,
    pub change: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SafetyFlag {
    pub flag: String,
    pub severity: String,
    pub description: String,
}

// ============================================================================
// Simulation Service
// ============================================================================

pub struct SimulationService {
    config: Config,
    db: PgPool,
    rpc: Provider<Http>,
    state: Arc<RwLock<SimulationState>>,
}

#[derive(Debug, Clone)]
pub struct SimulationState {
    pub current_block: u64,
    pub simulations_count: u64,
    pub failures_count: u64,
    pub warnings_count: u64,
}

impl Default for SimulationState {
    fn default() -> Self {
        Self {
            current_block: 0,
            simulations_count: 0,
            failures_count: 0,
            warnings_count: 0,
        }
    }
}

impl SimulationService {
    /// Create a new simulation service
    pub async fn new(config: Config) -> Result<Self, SimulationError> {
        // Initialize database pool
        let db = PgPoolOptions::new()
            .max_connections(10)
            .connect(&config.database_url)
            .await?;

        // Initialize RPC client
        let rpc = Provider::<Http>::try_from(config.rpc_url.clone())
            .map_err(|e| SimulationError::Rpc(e.to_string()))?
            .interval(Duration::from_secs(5));

        Ok(Self {
            config,
            db,
            rpc,
            state: Arc::new(RwLock::new(SimulationState::default())),
        })
    }

    /// Simulate a transaction
    pub async fn simulate(&self, request: SimulationRequest) -> Result<SimulationResult, SimulationError> {
        info!("Simulating transaction from {}", request.from);

        // Parse addresses
        let from = request.from.parse::<Address>()
            .map_err(|e| SimulationError::Validation(format!("Invalid from address: {}", e)))?;

        let to = if let Some(to_str) = &request.to {
            Some(to_str.parse::<Address>()
                .map_err(|e| SimulationError::Validation(format!("Invalid to address: {}", e))?)
        } else {
            None
        };

        // Parse value
        let value = if let Some(v) = &request.value {
            v.parse::<U256>()
                .map_err(|e| SimulationError::Validation(format!("Invalid value: {}", e)))?
        } else {
            U256::zero()
        };

        // Parse data
        let data = if let Some(d) = &request.data {
            hex::decode(d.trim_start_matches("0x"))
                .map_err(|e| SimulationError::Validation(format!("Invalid data: {}", e)))?
                .into()
        } else {
            Bytes::new()
        };

        // Build call message
        let mut msg = ethers::types::CallRequest {
            from: Some(from),
            to: to,
            value: Some(value.into()),
            data: Some(data),
            ..Default::default()
        };

        // Set gas if provided
        if let Some(gas) = request.gas_limit {
            msg.gas = Some(gas.into());
        }

        // Set gas price if provided
        if let Some(gp) = &request.gas_price {
            let gp_val = gp.parse::<U256>()
                .map_err(|e| SimulationError::Validation(format!("Invalid gas price: {}", e)))?;
            msg.gas_price = Some(gp_val.into());
        }

        // Execute simulation with debug_traceCall
        if self.config.trace_enabled {
            self.simulate_with_trace(msg).await
        } else {
            self.simulate_simple(msg).await
        }
    }

    /// Simulate with full tracing
    async fn simulate_with_trace(&self, msg: ethers::types::CallRequest) -> Result<SimulationResult, SimulationError> {
        // Get current block number
        let block_num = self.rpc.get_block_number()
            .await
            .map_err(|e| SimulationError::Rpc(e.to_string()))?
            .as_u64();

        self.state.write().current_block = block_num;

        // Try to estimate gas
        let gas_estimate = self.rpc.estimate_gas(&msg, block_num.into())
            .await
            .map_err(|e| SimulationError::Rpc(e.to_string()))?;

        let gas_used = gas_estimate.as_u64();
        let gas_limit = ((gas_used as f64) * self.config.gas_multiplier) as u64;

        // Try debug trace
        let trace_config = ethers::types::DebugTraceCallConfig {
            tracer: Some(ethers::types::GethDebugTracerType::CallTracer),
            ..Default::default()
        };

        let result = self.rpc.debug_trace_call(msg.clone(), "latest".to_string(), trace_config.clone())
            .await;

        let mut sim_result = SimulationResult {
            success: true,
            error: None,
            gas_used,
            gas_limit,
            return_value: None,
            logs: vec![],
            state_changes: vec![],
            calls: vec![],
            balance_changes: vec![],
            warnings: vec![],
            safety_score: 100,
            safety_flags: vec![],
        };

        // Check result
        match result {
            Ok(traces) => {
                // Process traces
                for trace in traces {
                    if let Some(call) = trace.call {
                        sim_result.calls.push(SimulatedCall {
                            call_type: call.call_type.map(|c| c.to_string()).unwrap_or_default(),
                            from: call.from.to_string(),
                            to: call.to.to_string(),
                            value: call.value.map(|v| v.to_string()).unwrap_or_default(),
                            input: call.input.to_string(),
                            output: call.output.to_string(),
                            gas_used: call.gas_used.as_u64(),
                            depth: call.depth as u32,
                            error: call.error,
                        });
                    }
                }
            }
            Err(e) => {
                warn!("Trace failed: {}", e);
                // Try simple call
                match self.rpc.call(&msg, "latest".to_string()).await {
                    Ok(ret) => {
                        sim_result.return_value = Some(format!("0x{}", hex::encode(ret)));
                    }
                    Err(call_err) => {
                        sim_result.success = false;
                        sim_result.error = Some(call_err.to_string());
                        sim_result.safety_score = 0;
                        sim_result.safety_flags.push(SafetyFlag {
                            flag: "CALL_FAILED".to_string(),
                            severity: "critical".to_string(),
                            description: format!("Transaction would fail: {}", call_err),
                        });
                    }
                }
            }
        }

        // Analyze for safety
        self.analyze_safety(&mut sim_result, &msg).await?;

        // Update stats
        {
            let mut state = self.state.write();
            state.simulations_count += 1;
            if !sim_result.success {
                state.failures_count += 1;
            }
            if !sim_result.warnings.is_empty() {
                state.warnings_count += sim_result.warnings.len() as u64;
            }
        }

        info!("Simulation complete: success={}, gas_used={}", sim_result.success, sim_result.gas_used);

        Ok(sim_result)
    }

    /// Simulate without tracing
    async fn simulate_simple(&self, msg: ethers::types::CallRequest) -> Result<SimulationResult, SimulationError> {
        let gas_estimate = self.rpc.estimate_gas(&msg, "latest".to_string())
            .await
            .map_err(|e| SimulationError::Rpc(e.to_string()))?;

        let gas_used = gas_estimate.as_u64();
        let gas_limit = ((gas_used as f64) * self.config.gas_multiplier) as u64;

        let result = self.rpc.call(&msg, "latest".to_string()).await;

        let mut sim_result = SimulationResult {
            success: true,
            error: None,
            gas_used,
            gas_limit,
            return_value: None,
            logs: vec![],
            state_changes: vec![],
            calls: vec![],
            balance_changes: vec![],
            warnings: vec![],
            safety_score: 100,
            safety_flags: vec![],
        };

        match result {
            Ok(ret) => {
                sim_result.return_value = Some(format!("0x{}", hex::encode(ret)));
            }
            Err(e) => {
                sim_result.success = false;
                sim_result.error = Some(e.to_string());
                sim_result.safety_score = 0;
                sim_result.safety_flags.push(SafetyFlag {
                    flag: "CALL_FAILED".to_string(),
                    severity: "critical".to_string(),
                    description: format!("Transaction would fail: {}", e),
                });
            }
        }

        self.analyze_safety(&mut sim_result, &msg).await?;

        Ok(sim_result)
    }

    /// Analyze transaction for safety issues
    async fn analyze_safety(&self, result: &mut SimulationResult, msg: &ethers::types::CallRequest) -> Result<(), SimulationError> {
        // Check for high gas usage
        if result.gas_used > 1000000 {
            result.warnings.push(format!("High gas usage: {}", result.gas_used));
            result.safety_score = result.safety_score.saturating_sub(10);
            result.safety_flags.push(SafetyFlag {
                flag: "HIGH_GAS".to_string(),
                severity: "medium".to_string(),
                description: format!("Transaction uses {} gas", result.gas_used),
            });
        }

        // Check for external calls
        for call in &result.calls {
            if call.to != msg.to.unwrap_or_default().to_string() {
                result.warnings.push(format!("External call to {}", call.to));
                result.safety_score = result.safety_score.saturating_sub(5);

                // Check for delegatecall
                if call.call_type == "delegatecall" {
                    result.safety_score = result.safety_score.saturating_sub(20);
                    result.safety_flags.push(SafetyFlag {
                        flag: "DELEGATE_CALL".to_string(),
                        severity: "high".to_string(),
                        description: "Transaction uses delegatecall - potential security risk".to_string(),
                    });
                }

                // Check for value transfer
                if let Ok(v) = call.value.parse::<U256>() {
                    if !v.is_zero() {
                        result.safety_score = result.safety_score.saturating_sub(10);
                        result.safety_flags.push(SafetyFlag {
                            flag: "VALUE_TRANSFER".to_string(),
                            severity: "medium".to_string(),
                            description: format!("Sends {} ETH", v),
                        });
                    }
                }
            }
        }

        // Check for state changes
        if !result.state_changes.is_empty() {
            result.warnings.push(format!("{} storage slots modified", result.state_changes.len()));
            result.safety_score = result.safety_score.saturating_sub(5);
        }

        // Check for suspicious patterns in data
        if let Some(data) = &msg.data {
            let data_str = hex::encode(data);
            
            // Self-destruct pattern
            if data_str.contains("ff") {
                result.warnings.push("Self-destruct detected".to_string());
                result.safety_score = result.safety_score.saturating_sub(30);
                result.safety_flags.push(SafetyFlag {
                    flag: "SELF_DESTRUCT".to_string(),
                    severity: "critical".to_string(),
                    description: "Transaction may destroy the contract".to_string(),
                });
            }

            // Delegatecall in constructor
            if data_str.starts_with("3d04e1d3") || data_str.contains("c011a73") {
                result.warnings.push("Potential delegatecall in constructor".to_string());
                result.safety_score = result.safety_score.saturating_sub(20);
            }
        }

        Ok(())
    }

    /// Get service state
    pub fn get_state(&self) -> SimulationState {
        self.state.read().clone()
    }

    /// Get service metrics
    pub async fn get_metrics(&self) -> Result<SimulationMetrics, SimulationError> {
        let state = self.state.read();

        let avg_gas: f64 = if state.simulations_count > 0 {
            (state.simulations_count as f64) / (state.simulations_count as f64)
        } else {
            0.0
        };

        Ok(SimulationMetrics {
            current_block: state.current_block,
            simulations_count: state.simulations_count,
            failures_count: state.failures_count,
            warnings_count: state.warnings_count,
            avg_gas_used: avg_gas,
            success_rate: if state.simulations_count > 0 {
                ((state.simulations_count - state.failures_count) as f64 / state.simulations_count as f64) * 100.0
            } else {
                0.0
            },
        })
    }
}

// ============================================================================
// Metrics
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SimulationMetrics {
    pub current_block: u64,
    pub simulations_count: u64,
    pub failures_count: u64,
    pub warnings_count: u64,
    pub avg_gas_used: f64,
    pub success_rate: f64,
}

// ============================================================================
// Helper Functions
// ============================================================================

/// Parse address from string
pub fn parse_address(s: &str) -> Option<Address> {
    let s = s.strip_prefix("0x").unwrap_or(s);
    if s.len() != 40 {
        return None;
    }
    
    let bytes = hex::decode(s).ok()?;
    if bytes.len() != 20 {
        return None;
    }
    
    Some(Address::from_slice(&bytes))
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_address() {
        let addr = "0x742d35Cc6634C0532925a3b8D3812e09e48F2F0504";
        let parsed = parse_address(addr);
        assert!(parsed.is_some());
    }
}