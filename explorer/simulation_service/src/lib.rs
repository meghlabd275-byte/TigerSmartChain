//! TigerScan transaction simulation: eth_call plus geth callTracer, with a
//! post-state diff and a safety review of the resulting call tree.

#![forbid(unsafe_code)]

use std::sync::Arc;
use std::time::Duration;

use anyhow::Result;
use ethers::providers::{Http, Middleware, Provider};
use ethers::types::transaction::eip2718::TypedTransaction;
use ethers::core::types::{
    CallFrame, CallLogFrame, GethDebugBuiltInTracerConfig, GethDebugBuiltInTracerType,
    GethDebugTracerConfig, GethDebugTracerType, GethDebugTracingCallOptions,
    GethDebugTracingOptions, GethTrace, GethTraceFrame, PreStateConfig, PreStateFrame,
};
use ethers::types::{Address, Bytes, NameOrAddress, TransactionRequest, H256, U256};
use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use sqlx::postgres::{PgPool, PgPoolOptions};
use thiserror::Error;
use tracing::warn;

// ============================================================================
// Error types
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
    pub rpc_url: String,
    pub database_url: String,
    pub timeout_seconds: u64,
    /// Multiplier applied to the gas estimate to obtain a suggested gas limit.
    pub gas_multiplier: f64,
    pub depth_limit: u32,
    pub trace_enabled: bool,
    /// Chain the RPC endpoint serves; recorded with each simulation.
    pub chain_id: u64,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            rpc_url: std::env::var("RPC_URL")
                .unwrap_or_else(|_| "http://localhost:8545".to_string()),
            database_url: std::env::var("DATABASE_URL")
                .unwrap_or_else(|_| "postgres://localhost:5432/tigerscan".to_string()),
            timeout_seconds: 30,
            gas_multiplier: 1.1,
            depth_limit: 10,
            trace_enabled: true,
            chain_id: std::env::var("CHAIN_ID")
                .ok()
                .and_then(|value| value.parse().ok())
                .unwrap_or(6666),
        }
    }
}

// ============================================================================
// Types
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SimulationRequest {
    pub from: String,
    pub to: Option<String>,
    pub value: Option<String>,
    pub gas_price: Option<String>,
    pub gas_limit: Option<u64>,
    pub data: Option<String>,
    pub chain_id: Option<u64>,
    pub nonce: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SimulationResult {
    pub success: bool,
    pub error: Option<String>,
    pub gas_used: u64,
    pub gas_limit: u64,
    pub return_value: Option<String>,
    pub logs: Vec<SimulatedLog>,
    pub state_changes: Vec<StateChange>,
    pub calls: Vec<SimulatedCall>,
    pub warnings: Vec<String>,
    pub safety_score: u32,
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
pub struct SafetyFlag {
    pub flag: String,
    pub severity: String,
    pub description: String,
}

// ============================================================================
// Simulation service
// ============================================================================

pub struct SimulationService {
    config: Config,
    db: PgPool,
    rpc: Provider<Http>,
    state: Arc<RwLock<SimulationState>>,
}

#[derive(Debug, Clone, Default)]
pub struct SimulationState {
    pub current_block: u64,
    pub simulations_count: u64,
    pub failures_count: u64,
    pub warnings_count: u64,
    pub gas_used_total: u64,
}

impl SimulationService {
    pub async fn new(config: Config) -> Result<Self, SimulationError> {
        let db = PgPoolOptions::new()
            .max_connections(10)
            .connect(&config.database_url)
            .await?;

        let rpc = Provider::<Http>::try_from(config.rpc_url.clone())
            .map_err(|error| SimulationError::Rpc(error.to_string()))?
            .interval(Duration::from_secs(5));

        Ok(Self {
            config,
            db,
            rpc,
            state: Arc::new(RwLock::new(SimulationState::default())),
        })
    }

    pub async fn simulate(
        &self,
        request: SimulationRequest,
    ) -> Result<SimulationResult, SimulationError> {
        let transaction = self.build_transaction(&request)?;

        let mut result = if self.config.trace_enabled {
            self.simulate_with_trace(&transaction).await?
        } else {
            self.simulate_simple(&transaction).await?
        };

        analyze_safety(&mut result, &transaction);

        apply_simulation_state(&mut self.state.write(), &result);
        self.persist(&request, &result).await;

        Ok(result)
    }

    /// The request is the single source of truth for the transaction that is
    /// sent to the node; every field is validated before it is used.
    fn build_transaction(
        &self,
        request: &SimulationRequest,
    ) -> Result<TypedTransaction, SimulationError> {
        let from = request.from.parse::<Address>().map_err(|error| {
            SimulationError::Validation(format!("invalid from address: {error}"))
        })?;

        let to: Option<NameOrAddress> = match &request.to {
            Some(value) => Some(
                value
                    .parse::<Address>()
                    .map_err(|error| {
                        SimulationError::Validation(format!("invalid to address: {error}"))
                    })?
                    .into(),
            ),
            None => None,
        };

        let value = match &request.value {
            Some(raw) => Some(parse_quantity(raw, "value")?),
            None => None,
        };

        let data = match &request.data {
            Some(raw) => Some(Bytes::from(
                hex::decode(raw.trim_start_matches("0x")).map_err(|error| {
                    SimulationError::Validation(format!("invalid calldata: {error}"))
                })?,
            )),
            None => None,
        };

        let mut transaction = TransactionRequest::new().from(from);

        if let Some(to) = to {
            transaction = transaction.to(to);
        }


        if let Some(value) = value {
            transaction = transaction.value(value);
        }

        if let Some(data) = data {
            transaction = transaction.data(data);
        }

        if let Some(gas_limit) = request.gas_limit {
            transaction = transaction.gas(gas_limit);
        }

        if let Some(raw) = &request.gas_price {
            transaction = transaction.gas_price(parse_quantity(raw, "gas price")?);
        }

        if let Some(nonce) = request.nonce {
            transaction = transaction.nonce(nonce);
        }

        if let Some(chain_id) = request.chain_id {
            transaction = transaction.chain_id(chain_id);
        }

        Ok(transaction.into())
    }

    async fn simulate_with_trace(
        &self,
        transaction: &TypedTransaction,
    ) -> Result<SimulationResult, SimulationError> {
        let block_number = self
            .rpc
            .get_block_number()
            .await
            .map_err(|error| SimulationError::Rpc(error.to_string()))?
            .as_u64();

        self.state.write().current_block = block_number;

        let gas_used = self.estimate_gas(transaction).await;
        let gas_limit = suggested_gas_limit(gas_used, self.config.gas_multiplier);

        let mut result = SimulationResult {
            success: true,
            error: None,
            gas_used,
            gas_limit,
            return_value: None,
            logs: Vec::new(),
            state_changes: Vec::new(),
            calls: Vec::new(),
            warnings: Vec::new(),
            safety_score: 100,
            safety_flags: Vec::new(),
        };

        let options = GethDebugTracingCallOptions {
            tracing_options: GethDebugTracingOptions {
                tracer: Some(GethDebugTracerType::BuiltInTracer(
                    GethDebugBuiltInTracerType::CallTracer,
                )),
                ..Default::default()
            },
            ..Default::default()
        };

        match self
            .rpc
            .debug_trace_call(transaction.clone(), None, options)
            .await
        {
            Ok(GethTrace::Known(GethTraceFrame::CallTracer(frame))) => {
                flatten_call_frame(&frame, 0, self.config.depth_limit, &mut result);

                if let Some(error) = &frame.error {
                    result.success = false;
                    result.error = Some(error.clone());
                }

                result.return_value = frame
                    .output
                    .as_ref()
                    .map(|output| format!("0x{}", hex::encode(output)));
            }
            Ok(GethTrace::Known(_)) | Ok(GethTrace::Unknown(_)) => {
                warn!("node returned an unexpected trace shape; falling back to eth_call");
                self.apply_call_result(transaction, &mut result).await?;
            }
            Err(error) => {
                warn!(%error, "debug_traceCall unavailable; falling back to eth_call");
                self.apply_call_result(transaction, &mut result).await?;
            }
        }

        self.collect_state_changes(transaction, &mut result).await;

        Ok(result)
    }

    async fn simulate_simple(
        &self,
        transaction: &TypedTransaction,
    ) -> Result<SimulationResult, SimulationError> {
        let gas_used = self.estimate_gas(transaction).await;

        let mut result = SimulationResult {
            success: true,
            error: None,
            gas_used,
            gas_limit: suggested_gas_limit(gas_used, self.config.gas_multiplier),
            return_value: None,
            logs: Vec::new(),
            state_changes: Vec::new(),
            calls: Vec::new(),
            warnings: Vec::new(),
            safety_score: 100,
            safety_flags: Vec::new(),
        };

        self.apply_call_result(transaction, &mut result).await?;
        self.collect_state_changes(transaction, &mut result).await;

        Ok(result)
    }

    /// `eth_call` against the latest block, recording the return data or the
    /// revert reason.
    async fn apply_call_result(
        &self,
        transaction: &TypedTransaction,
        result: &mut SimulationResult,
    ) -> Result<(), SimulationError> {
        match self.rpc.call(transaction, None).await {
            Ok(returned) => {
                result.return_value = Some(format!("0x{}", hex::encode(returned)));
                result.success = true;
            }
            Err(error) => {
                let message = error.to_string();

                result.success = false;
                result.error = Some(message.clone());
                result.safety_score = 0;
                result.safety_flags.push(SafetyFlag {
                    flag: "CALL_FAILED".to_string(),
                    severity: "critical".to_string(),
                    description: format!("transaction would revert: {message}"),
                });
            }
        }

        Ok(())
    }

    /// The gas estimate is advisory: a transaction that reverts during
    /// estimation still has to be reported, so the failure becomes zero gas
    /// rather than an error.
    async fn estimate_gas(&self, transaction: &TypedTransaction) -> u64 {
        match self.rpc.estimate_gas(transaction, None).await {
            Ok(estimate) => estimate.as_u64(),
            Err(error) => {
                warn!(%error, "gas estimation failed");
                0
            }
        }
    }

    /// `prestateTracer` with diff mode returns only the accounts the call
    /// touched, which is exactly the set that belongs in a state diff.
    async fn collect_state_changes(
        &self,
        transaction: &TypedTransaction,
        result: &mut SimulationResult,
    ) {
        let options = GethDebugTracingCallOptions {
            tracing_options: GethDebugTracingOptions {
                tracer: Some(GethDebugTracerType::BuiltInTracer(
                    GethDebugBuiltInTracerType::PreStateTracer,
                )),
                tracer_config: Some(GethDebugTracerConfig::BuiltInTracer(
                    GethDebugBuiltInTracerConfig::PreStateTracer(PreStateConfig {
                        diff_mode: Some(true),
                    }),
                )),
                ..Default::default()
            },
            ..Default::default()
        };

        let trace = match self
            .rpc
            .debug_trace_call(transaction.clone(), None, options)
            .await
        {
            Ok(GethTrace::Known(GethTraceFrame::PreStateTracer(frame))) => frame,
            Ok(_) => return,
            Err(error) => {
                warn!(%error, "prestate trace unavailable");
                return;
            }
        };

        let PreStateFrame::Diff(diff) = trace else {
            return;
        };

        for (address, post) in &diff.post {
            let pre_storage = diff
                .pre
                .get(address)
                .and_then(|pre| pre.storage.as_ref());

            let Some(post_storage) = &post.storage else {
                continue;
            };

            for (slot, new_value) in post_storage {
                let old_value = pre_storage
                    .and_then(|storage| storage.get(slot))
                    .copied()
                    .unwrap_or_default();

                if old_value == *new_value {
                    continue;
                }

                result.state_changes.push(StateChange {
                    address: address_hex(*address),
                    slot: slot.to_string(),
                    old_value: old_value.to_string(),
                    new_value: new_value.to_string(),
                });
            }
        }
    }

    /// Simulations are recorded so the explorer can show what was executed and
    /// when. A database failure never fails the simulation itself.
    async fn persist(&self, request: &SimulationRequest, result: &SimulationResult) {
        let details = serde_json::json!({
            "chain_id": request.chain_id.unwrap_or(self.config.chain_id),
            "from": request.from,
            "to": request.to,
            "value": request.value,
            "data": request.data,
            "success": result.success,
            "gas_used": result.gas_used,
            "safety_score": result.safety_score,
            "flags": result.safety_flags,
        });

        let outcome = sqlx::query(
            "INSERT INTO audit_logs (action, resource_type, resource_id, details) \
             VALUES ('simulate', 'transaction', $1, $2)",
        )
        .bind(&request.from)
        .bind(details)
        .execute(&self.db)
        .await;

        if let Err(error) = outcome {
            warn!(%error, "could not record simulation");
        }
    }

    pub fn get_state(&self) -> SimulationState {
        self.state.read().clone()
    }

    pub fn get_metrics(&self) -> SimulationMetrics {
        metrics_from(&self.state.read())
    }
}

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
// Helpers
// ============================================================================

/// `Address`'s `Display` is abbreviated (`0x0000…0001`), so addresses that are
/// reported or stored are rendered through `LowerHex` instead.
fn address_hex(address: Address) -> String {
    format!("{address:#x}")
}

/// Accepts a decimal integer or a `0x`-prefixed quantity.
fn parse_quantity(raw: &str, field: &str) -> Result<U256, SimulationError> {
    let trimmed = raw.trim();

    let parsed = match trimmed
        .strip_prefix("0x")
        .or_else(|| trimmed.strip_prefix("0X"))
    {
        Some(hex) => U256::from_str_radix(hex, 16).map_err(|error| {
            SimulationError::Validation(format!("invalid hex {field} '{raw}': {error}"))
        })?,
        None => U256::from_dec_str(trimmed).map_err(|error| {
            SimulationError::Validation(format!("invalid decimal {field} '{raw}': {error}"))
        })?,
    };

    Ok(parsed)
}

fn suggested_gas_limit(gas_used: u64, multiplier: f64) -> u64 {
    let scaled = gas_used as f64 * multiplier;

    if !scaled.is_finite() || scaled <= 0.0 {
        return gas_used;
    }

    // A multiplier such as 1.1 leaves the product a hair above the exact
    // integer; snapping avoids inflating the limit by a whole gas unit.
    let nearest = scaled.round();
    let ceiling = if (scaled - nearest).abs() < 1e-9 {
        nearest
    } else {
        scaled.ceil()
    };

    ceiling.min(u64::MAX as f64) as u64
}

/// Static review of the transaction and of the call tree it produces.
fn analyze_safety(result: &mut SimulationResult, transaction: &TypedTransaction) {
    let entry_point = transaction.to().and_then(|to| match to {
        NameOrAddress::Address(address) => Some(*address),
        NameOrAddress::Name(_) => None,
    });

    if result.gas_used > 1_000_000 {
        result
            .warnings
            .push(format!("high gas usage: {}", result.gas_used));
        result.safety_score = result.safety_score.saturating_sub(10);
        result.safety_flags.push(SafetyFlag {
            flag: "HIGH_GAS".to_string(),
            severity: "medium".to_string(),
            description: format!("transaction uses {} gas", result.gas_used),
        });
    }

    // Only the calls below the entry point count; the entry point itself is the
    // root frame of the trace.
    for call in result.calls.iter().skip(1) {
        let call_type = call.call_type.to_ascii_lowercase();

        if call_type == "delegatecall" {
            result.safety_score = result.safety_score.saturating_sub(20);
            result
                .warnings
                .push(format!("delegatecall to {} via {}", call.to, call.from));
            result.safety_flags.push(SafetyFlag {
                flag: "DELEGATE_CALL".to_string(),
                severity: "high".to_string(),
                description: format!(
                    "{} delegates execution to {} - the callee can modify this contract's storage",
                    call.from, call.to
                ),
            });
            continue;
        }

        if entry_point
            .map(|entry| address_hex(entry).eq_ignore_ascii_case(&call.to))
            .unwrap_or(false)
        {
            continue;
        }

        result.warnings.push(format!("{call_type} to {}", call.to));
        result.safety_score = result.safety_score.saturating_sub(5);
        result.safety_flags.push(SafetyFlag {
            flag: "EXTERNAL_CALL".to_string(),
            severity: "medium".to_string(),
            description: format!("{call_type} into {}", call.to),
        });
    }

    if !result.state_changes.is_empty() {
        result
            .warnings
            .push(format!("{} storage slots modified", result.state_changes.len()));
        result.safety_score = result.safety_score.saturating_sub(5);
        result.safety_flags.push(SafetyFlag {
            flag: "STORAGE_WRITE".to_string(),
            severity: "low".to_string(),
            description: format!("{} storage slots change", result.state_changes.len()),
        });
    }

    // A call that reports neither an error nor a return value while spending
    // gas has no visible effect and is worth flagging.
    if result.success && result.return_value.is_none() && result.gas_used > 0 {
        result.warnings.push("no return data".to_string());
    }
}

fn apply_simulation_state(state: &mut SimulationState, result: &SimulationResult) {
    state.simulations_count += 1;
    state.gas_used_total = state.gas_used_total.saturating_add(result.gas_used);

    if !result.success {
        state.failures_count += 1;
    }

    state.warnings_count += result.warnings.len() as u64;
}

fn metrics_from(state: &SimulationState) -> SimulationMetrics {
    let (avg_gas_used, success_rate) = if state.simulations_count > 0 {
        (
            state.gas_used_total as f64 / state.simulations_count as f64,
            (state.simulations_count - state.failures_count) as f64
                / state.simulations_count as f64
                * 100.0,
        )
    } else {
        (0.0, 0.0)
    };

    SimulationMetrics {
        current_block: state.current_block,
        simulations_count: state.simulations_count,
        failures_count: state.failures_count,
        warnings_count: state.warnings_count,
        avg_gas_used,
        success_rate,
    }
}

/// Depth-first walk of the call tree, recording one entry per frame.
fn flatten_call_frame(
    frame: &CallFrame,
    depth: u32,
    depth_limit: u32,
    result: &mut SimulationResult,
) {
    if depth > depth_limit {
        return;
    }

    let to = frame
        .to
        .as_ref()
        .map(|to| match to {
            NameOrAddress::Address(address) => address_hex(*address),
            NameOrAddress::Name(name) => name.clone(),
        })
        .unwrap_or_default();

    result.calls.push(SimulatedCall {
        call_type: frame.typ.clone(),
        from: address_hex(frame.from),
        to,
        value: frame.value.unwrap_or_default().to_string(),
        input: format!("0x{}", hex::encode(&frame.input)),
        output: frame
            .output
            .as_ref()
            .map(|output| format!("0x{}", hex::encode(output)))
            .unwrap_or_default(),
        gas_used: frame.gas_used.as_u64(),
        depth,
        error: frame.error.clone(),
    });

    if let Some(logs) = &frame.logs {
        for log in logs {
            result.logs.push(SimulatedLog {
                address: log.address.map(address_hex).unwrap_or_default(),
                topics: log
                    .topics
                    .clone()
                    .unwrap_or_default()
                    .into_iter()
                    .map(|topic| topic.to_string())
                    .collect(),
                data: log
                    .data
                    .as_ref()
                    .map(|data| format!("0x{}", hex::encode(data)))
                    .unwrap_or_default(),
            });
        }
    }

    if let Some(calls) = &frame.calls {
        for child in calls {
            flatten_call_frame(child, depth + 1, depth_limit, result);
        }
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    fn frame(typ: &str, calls: Vec<CallFrame>) -> CallFrame {
        CallFrame {
            typ: typ.to_string(),
            from: Address::from_low_u64_be(1),
            to: Some(NameOrAddress::Address(Address::from_low_u64_be(2))),
            value: Some(U256::from(5)),
            gas: U256::from(100_000),
            gas_used: U256::from(21_000),
            input: Bytes::from(vec![0xaa, 0xbb]),
            output: Some(Bytes::from(vec![0xcc])),
            error: None,
            calls: if calls.is_empty() { None } else { Some(calls) },
            logs: None,
        }
    }

    fn empty_result() -> SimulationResult {
        SimulationResult {
            success: true,
            error: None,
            gas_used: 21_000,
            gas_limit: 23_100,
            return_value: None,
            logs: Vec::new(),
            state_changes: Vec::new(),
            calls: Vec::new(),
            warnings: Vec::new(),
            safety_score: 100,
            safety_flags: Vec::new(),
        }
    }

    fn call(call_type: &str, to: u64, depth: u32) -> SimulatedCall {
        SimulatedCall {
            call_type: call_type.to_string(),
            from: address_hex(Address::from_low_u64_be(1)),
            to: address_hex(Address::from_low_u64_be(to)),
            value: "0".to_string(),
            input: "0x".to_string(),
            output: "0x".to_string(),
            gas_used: 1_000,
            depth,
            error: None,
        }
    }

    fn transaction_to(address: u64) -> TypedTransaction {
        TransactionRequest::new()
            .to(Address::from_low_u64_be(address))
            .from(Address::from_low_u64_be(1))
            .into()
    }

    #[test]
    fn quantities_accept_decimal_and_hex() {
        assert_eq!(parse_quantity("1000", "value").unwrap(), U256::from(1000));
        assert_eq!(parse_quantity("0x10", "value").unwrap(), U256::from(16));
        assert_eq!(parse_quantity("0X10", "value").unwrap(), U256::from(16));
    }

    #[test]
    fn a_non_numeric_quantity_is_rejected() {
        assert!(parse_quantity("0xzz", "value").is_err());
        assert!(parse_quantity("not-a-number", "value").is_err());
    }

    #[test]
    fn the_gas_limit_applies_the_multiplier() {
        assert_eq!(suggested_gas_limit(21_000, 1.1), 23_100);
        assert_eq!(suggested_gas_limit(1_000_000, 1.5), 1_500_000);
        assert_eq!(suggested_gas_limit(0, 1.1), 0);
    }

    #[test]
    fn a_nested_call_tree_is_flattened_in_order() {
        let tree = frame("CALL", vec![frame("STATICCALL", vec![])]);

        let mut result = empty_result();
        flatten_call_frame(&tree, 0, 10, &mut result);

        assert_eq!(result.calls.len(), 2);
        assert_eq!(result.calls[0].depth, 0);
        assert_eq!(result.calls[0].input, "0xaabb");
        assert_eq!(result.calls[0].output, "0xcc");
        assert_eq!(result.calls[1].call_type, "STATICCALL");
        assert_eq!(result.calls[1].depth, 1);
    }

    #[test]
    fn the_depth_limit_stops_the_walk() {
        let tree = frame("CALL", vec![frame("CALL", vec![frame("CALL", vec![])])]);

        let mut result = empty_result();
        flatten_call_frame(&tree, 0, 1, &mut result);

        assert_eq!(result.calls.len(), 2);
    }

    #[test]
    fn logs_inside_a_frame_are_collected() {
        let mut tree = frame("CALL", vec![]);
        tree.logs = Some(vec![CallLogFrame {
            address: Some(Address::from_low_u64_be(9)),
            topics: Some(vec![H256::from_low_u64_be(7)]),
            data: Some(Bytes::from(vec![0x01])),
        }]);

        let mut result = empty_result();
        flatten_call_frame(&tree, 0, 10, &mut result);

        assert_eq!(result.logs.len(), 1);
        assert_eq!(result.logs[0].address, address_hex(Address::from_low_u64_be(9)));
        assert_eq!(result.logs[0].data, "0x01");
    }

    #[test]
    fn a_delegatecall_is_flagged_and_costs_twenty_points() {
        let mut result = empty_result();
        result.calls = vec![call("CALL", 2, 0), call("DELEGATECALL", 3, 1)];

        analyze_safety(&mut result, &transaction_to(2));

        assert_eq!(result.safety_score, 80);
        assert!(result
            .safety_flags
            .iter()
            .any(|flag| flag.flag == "DELEGATE_CALL"));
    }

    #[test]
    fn a_successful_call_into_the_entry_point_is_not_an_external_call() {
        let mut result = empty_result();
        result.calls = vec![call("CALL", 2, 0)];

        analyze_safety(&mut result, &transaction_to(2));

        assert_eq!(result.safety_score, 100);
        assert!(result.safety_flags.is_empty());
    }

    #[test]
    fn a_call_into_another_contract_is_an_external_call() {
        let mut result = empty_result();
        result.calls = vec![call("CALL", 2, 0), call("STATICCALL", 4, 1)];

        analyze_safety(&mut result, &transaction_to(2));

        assert_eq!(result.safety_score, 95);
        assert!(result
            .safety_flags
            .iter()
            .any(|flag| flag.flag == "EXTERNAL_CALL"));
    }

    #[test]
    fn storage_writes_are_reported() {
        let mut result = empty_result();
        result.state_changes = vec![StateChange {
            address: address_hex(Address::from_low_u64_be(2)),
            slot: "0x0".to_string(),
            old_value: "0x0".to_string(),
            new_value: "0x1".to_string(),
        }];

        analyze_safety(&mut result, &transaction_to(2));

        assert_eq!(result.safety_score, 95);
        assert!(result
            .safety_flags
            .iter()
            .any(|flag| flag.flag == "STORAGE_WRITE"));
    }

    #[test]
    fn metrics_average_the_recorded_gas() {
        let mut state = SimulationState::default();

        let mut successful = empty_result();
        successful.gas_used = 30_000;
        apply_simulation_state(&mut state, &successful);

        let mut failed = empty_result();
        failed.success = false;
        failed.gas_used = 10_000;
        apply_simulation_state(&mut state, &failed);

        let metrics = metrics_from(&state);

        assert_eq!(metrics.simulations_count, 2);
        assert_eq!(metrics.failures_count, 1);
        assert_eq!(metrics.avg_gas_used, 20_000.0);
        assert_eq!(metrics.success_rate, 50.0);
    }

    #[test]
    fn metrics_of_an_unused_service_are_zero() {
        let metrics = metrics_from(&SimulationState::default());

        assert_eq!(metrics.simulations_count, 0);
        assert_eq!(metrics.avg_gas_used, 0.0);
        assert_eq!(metrics.success_rate, 0.0);
    }

    #[test]
    fn an_address_is_rendered_in_full() {
        let rendered = address_hex(Address::from_low_u64_be(1));

        assert_eq!(rendered.len(), 42);
        assert_eq!(rendered, "0x0000000000000000000000000000000000000001");
        assert!(rendered.parse::<Address>().is_ok());
    }

    #[tokio::test]
    async fn a_transaction_is_built_from_every_request_field() {
        let service = SimulationService {
            config: Config::default(),
            db: PgPoolOptions::new()
                .connect_lazy("postgres://localhost/none")
                .expect("lazy pools do not connect"),
            rpc: Provider::<Http>::try_from("http://localhost:8545").unwrap(),
            state: Arc::new(RwLock::new(SimulationState::default())),
        };

        let request = SimulationRequest {
            from: address_hex(Address::from_low_u64_be(1)),
            to: Some(address_hex(Address::from_low_u64_be(2))),
            value: Some("0x2a".to_string()),
            gas_price: Some("1000000000".to_string()),
            gas_limit: Some(50_000),
            data: Some("0xdeadbeef".to_string()),
            chain_id: Some(6666),
            nonce: Some(3),
        };

        let transaction = service.build_transaction(&request).unwrap();

        assert_eq!(transaction.value(), Some(&U256::from(42)));
        assert_eq!(transaction.gas(), Some(&U256::from(50_000)));
        assert_eq!(transaction.nonce(), Some(&U256::from(3)));
        assert_eq!(transaction.chain_id(), Some(6666.into()));
        assert_eq!(
            transaction.data(),
            Some(&Bytes::from(vec![0xde, 0xad, 0xbe, 0xef]))
        );
    }

    #[tokio::test]
    async fn a_bad_address_is_rejected() {
        let service = SimulationService {
            config: Config::default(),
            db: PgPoolOptions::new()
                .connect_lazy("postgres://localhost/none")
                .expect("lazy pools do not connect"),
            rpc: Provider::<Http>::try_from("http://localhost:8545").unwrap(),
            state: Arc::new(RwLock::new(SimulationState::default())),
        };

        let request = SimulationRequest {
            from: "not-an-address".to_string(),
            to: None,
            value: None,
            gas_price: None,
            gas_limit: None,
            data: None,
            chain_id: None,
            nonce: None,
        };

        assert!(matches!(
            service.build_transaction(&request),
            Err(SimulationError::Validation(_))
        ));
    }

    #[tokio::test]
    async fn odd_length_calldata_is_rejected() {
        let service = SimulationService {
            config: Config::default(),
            db: PgPoolOptions::new()
                .connect_lazy("postgres://localhost/none")
                .expect("lazy pools do not connect"),
            rpc: Provider::<Http>::try_from("http://localhost:8545").unwrap(),
            state: Arc::new(RwLock::new(SimulationState::default())),
        };

        let request = SimulationRequest {
            from: address_hex(Address::from_low_u64_be(1)),
            to: None,
            value: None,
            gas_price: None,
            gas_limit: None,
            data: Some("0xabc".to_string()),
            chain_id: None,
            nonce: None,
        };

        assert!(matches!(
            service.build_transaction(&request),
            Err(SimulationError::Validation(_))
        ));
    }
}
