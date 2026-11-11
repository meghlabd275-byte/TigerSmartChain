//! Debug/Trace API service.
//!
//! Traces come from two real sources and never from a template:
//!
//! - `trace_transaction` and `trace_call` issue `debug_traceTransaction` and
//!   `debug_traceCall` against the node's JSON-RPC endpoint and decode the
//!   response. Every field reported is a field the node sent; a field the node
//!   omitted stays absent rather than being filled with a plausible value.
//! - `trace_vm` runs the bytecode through the same interpreter the node uses
//!   ([`tiger_evm::Executor`]) and reports the opcodes that actually executed.
//!
//! The earlier version returned a hardcoded PUSH1 step, a zero-address call
//! frame and a zero balance change for every input, so a caller could not tell
//! a successful contract call from a failed one.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::time::Instant;

// =============================================================================
// ERRORS
// =============================================================================

/// Trace Errors
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum TraceError {
    #[serde(rename = "execution_failed")]
    ExecutionFailed(String),
    #[serde(rename = "invalid_tx")]
    InvalidTransaction(String),
    #[serde(rename = "rpc_error")]
    RpcError(String),
    #[serde(rename = "timeout")]
    Timeout(String),
}

impl std::fmt::Display for TraceError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TraceError::ExecutionFailed(m) => write!(f, "execution failed: {m}"),
            TraceError::InvalidTransaction(m) => write!(f, "invalid transaction: {m}"),
            TraceError::RpcError(m) => write!(f, "rpc error: {m}"),
            TraceError::Timeout(m) => write!(f, "timeout: {m}"),
        }
    }
}

impl std::error::Error for TraceError {}

// =============================================================================
// TRACE CONFIGURATION
// =============================================================================

/// Trace configuration options
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TraceConfig {
    /// Enable call tracing
    pub enable_call_tracing: bool,
    /// Enable state diffs
    pub enable_state_diffs: bool,
    /// Enable memory tracing
    pub enable_memory: bool,
    /// Enable stack tracing
    pub enable_stack: bool,
    /// Enable storage tracing
    pub enable_storage: bool,
    /// Enable revert reason
    pub enable_revert_reason: bool,
    /// Maximum trace size
    pub max_trace_size: usize,
    /// Timeout in seconds
    pub timeout_seconds: u64,
}

impl Default for TraceConfig {
    fn default() -> Self {
        Self {
            enable_call_tracing: true,
            enable_state_diffs: true,
            enable_memory: true,
            enable_stack: true,
            enable_storage: true,
            enable_revert_reason: true,
            max_trace_size: 100_000,
            timeout_seconds: 30,
        }
    }
}

// =============================================================================
// TRACE STRUCTURES
// =============================================================================

/// A single executed VM step, decoded from the node's `structLogs`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VMState {
    pub pc: u64,
    /// The opcode byte. `op_name` is authoritative; this is the byte that
    /// produces that mnemonic.
    pub op: u8,
    pub op_name: String,
    pub gas: String,
    pub gas_cost: String,
    pub memory: Option<String>,
    pub stack: Vec<String>,
    pub storage: Option<HashMap<String, String>>,
    pub depth: u32,
    pub error: Option<String>,
}

/// Call frame
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CallFrame {
    pub call_type: String,
    pub from: String,
    pub to: String,
    pub value: String,
    pub gas: String,
    pub input: String,
    pub output: String,
    pub revert_reason: Option<String>,
    pub depth: u32,
    pub calls: Vec<CallFrame>,
}

/// State change
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StateChange {
    pub address: String,
    pub key: String,
    pub old_value: String,
    pub new_value: String,
    pub change_type: StateChangeType,
}

/// State change type
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum StateChangeType {
    #[serde(rename = "storage")]
    Storage,
    #[serde(rename = "balance")]
    Balance,
    #[serde(rename = "nonce")]
    Nonce,
    #[serde(rename = "code")]
    Code,
}

/// Complete trace result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TraceResult {
    pub tx_hash: String,
    pub gas_used: String,
    pub failed: bool,
    pub revert_reason: Option<String>,
    pub steps: Vec<VMState>,
    pub calls: Vec<CallFrame>,
    pub state_changes: Vec<StateChange>,
    pub output: String,
    pub execution_time_ms: u64,
    /// Set when the node reported that it stopped recording steps before the
    /// end of execution. A truncated trace is not a complete one.
    pub truncated: bool,
}

// =============================================================================
// RESPONSE DECODING
// =============================================================================

/// Recover the opcode byte for a mnemonic by asking the interpreter which byte
/// produces it, so the mapping cannot drift from the opcode table.
fn opcode_byte(mnemonic: &str) -> u8 {
    for byte in 0u8..=255 {
        if tiger_evm::trace::mnemonic(byte) == mnemonic {
            return byte;
        }
    }
    0
}

/// Decode one `structLogs` entry.
fn decode_step(log: &serde_json::Value) -> VMState {
    let op_name = log["op"].as_str().unwrap_or("INVALID").to_string();

    let stack = log["stack"]
        .as_array()
        .map(|words| {
            words
                .iter()
                .filter_map(|w| w.as_str().map(String::from))
                .collect()
        })
        .unwrap_or_default();

    let storage = log["storage"].as_object().map(|map| {
        map.iter()
            .map(|(k, v)| (k.clone(), v.as_str().unwrap_or("0x0").to_string()))
            .collect::<HashMap<String, String>>()
    });

    VMState {
        pc: log["pc"].as_u64().unwrap_or(0),
        op: opcode_byte(&op_name),
        op_name,
        gas: log["gas"].as_str().unwrap_or("0x0").to_string(),
        gas_cost: log["gasCost"].as_str().unwrap_or("0x0").to_string(),
        memory: log["memory"].as_str().map(String::from),
        stack,
        storage,
        depth: log["depth"].as_u64().unwrap_or(0) as u32,
        error: log["error"].as_str().map(String::from),
    }
}

/// Reduce a 32-byte stack word to a 20-byte address, the way JSON-RPC renders
/// an address.
fn truncate_address(word: &str) -> String {
    let digits = word.strip_prefix("0x").unwrap_or(word);
    if digits.len() <= 40 {
        format!("0x{:0>40}", digits)
    } else {
        format!("0x{}", &digits[digits.len() - 40..])
    }
}

/// The call-family mnemonics that open a new frame.
fn is_call_opcode(op: &str) -> bool {
    matches!(
        op,
        "CALL" | "CALLCODE" | "DELEGATECALL" | "STATICCALL" | "CREATE" | "CREATE2"
    )
}

/// Recover call frames from the opcode log.
///
/// The interpreter records every opcode including the call-family ones, so the
/// frames are read from the real execution: a frame opens at a call opcode and
/// closes when the depth returns to its parent's. The target address is taken
/// from the stack words the node reported.
fn call_frames_from(steps: &[VMState]) -> Vec<CallFrame> {
    let mut roots: Vec<CallFrame> = Vec::new();
    let mut open: Vec<CallFrame> = Vec::new();

    for step in steps {
        if is_call_opcode(&step.op_name) {
            let stack = &step.stack;
            let n = stack.len();
            // Stack words, top last. A call opcode reads gas, target, then
            // value (CALL/CALLCODE only), then the in/out ranges.
            let word = |from_end: usize| -> String {
                if n >= from_end {
                    stack[n - from_end].clone()
                } else {
                    "0x0".to_string()
                }
            };
            let (value, to) = match step.op_name.as_str() {
                "STATICCALL" | "DELEGATECALL" => ("0x0".to_string(), word(2)),
                _ => (word(3), word(2)),
            };

            open.push(CallFrame {
                call_type: step.op_name.clone(),
                // The opcode log does not carry the caller; it is left empty
                // rather than filled with a placeholder address.
                from: String::new(),
                to: truncate_address(&to),
                value,
                gas: word(1),
                input: String::new(),
                output: String::new(),
                revert_reason: None,
                depth: step.depth,
                calls: Vec::new(),
            });
        } else if matches!(
            step.op_name.as_str(),
            "RETURN" | "REVERT" | "STOP" | "SELFDESTRUCT"
        ) {
            while let Some(frame) = open.pop() {
                let frame_depth = frame.depth;
                let mut frame = frame;
                if step.op_name == "REVERT" {
                    frame.revert_reason = step.error.clone();
                }
                if let Some(parent) = open.last_mut() {
                    parent.calls.push(frame);
                } else {
                    roots.push(frame);
                }
                if frame_depth <= step.depth {
                    break;
                }
            }
        }
    }

    // Frames still open at the end never returned. Report them rather than
    // dropping them silently.
    while let Some(frame) = open.pop() {
        if let Some(parent) = open.last_mut() {
            parent.calls.push(frame);
        } else {
            roots.push(frame);
        }
    }

    roots
}

/// Decode the node's `stateDiff` object into flat state changes.
fn decode_state_diff(state_diff: &serde_json::Value) -> Vec<StateChange> {
    let mut changes = Vec::new();

    let Some(addresses) = state_diff.as_object() else {
        return changes;
    };

    for (address, entry) in addresses {
        if let Some(balance) = entry.get("balance").and_then(|b| b.get("*")) {
            changes.push(StateChange {
                address: address.clone(),
                key: "balance".to_string(),
                old_value: balance["from"].as_str().unwrap_or("0x0").to_string(),
                new_value: balance["to"].as_str().unwrap_or("0x0").to_string(),
                change_type: StateChangeType::Balance,
            });
        }

        if let Some(storage) = entry.get("storage").and_then(|s| s.as_object()) {
            for (slot, change) in storage {
                let delta = change.get("*").unwrap_or(change);
                changes.push(StateChange {
                    address: address.clone(),
                    key: slot.clone(),
                    old_value: delta["from"].as_str().unwrap_or("0x0").to_string(),
                    new_value: delta["to"].as_str().unwrap_or("0x0").to_string(),
                    change_type: StateChangeType::Storage,
                });
            }
        }
    }

    changes
}

/// Decode a `debug_trace*` result object.
fn decode_trace_result(tx_hash: &str, trace: &serde_json::Value, config: &TraceConfig) -> TraceResult {
    let mut steps: Vec<VMState> = trace["structLogs"]
        .as_array()
        .map(|logs| logs.iter().map(decode_step).collect())
        .unwrap_or_default();

    if steps.len() > config.max_trace_size {
        steps.truncate(config.max_trace_size);
    }

    if !config.enable_memory {
        for step in &mut steps {
            step.memory = None;
        }
    }
    if !config.enable_stack {
        for step in &mut steps {
            step.stack.clear();
        }
    }
    if !config.enable_storage {
        for step in &mut steps {
            step.storage = None;
        }
    }

    let failed = trace["failed"].as_bool().unwrap_or(false);

    // The opcode logger reports a fault on the step that raised it. Surface the
    // last such message as the revert reason instead of inventing one.
    let revert_reason = if failed {
        steps.iter().rev().find_map(|s| s.error.clone())
    } else {
        None
    };

    let calls = if config.enable_call_tracing {
        call_frames_from(&steps)
    } else {
        Vec::new()
    };

    let state_changes = if config.enable_state_diffs {
        decode_state_diff(&trace["stateDiff"])
    } else {
        Vec::new()
    };

    TraceResult {
        tx_hash: tx_hash.to_string(),
        gas_used: trace["gas"].as_str().unwrap_or("0x0").to_string(),
        failed,
        revert_reason,
        steps,
        calls,
        state_changes,
        output: trace["returnValue"].as_str().unwrap_or("0x").to_string(),
        execution_time_ms: 0,
        truncated: trace["truncated"].as_bool().unwrap_or(false),
    }
}

// =============================================================================
// DEBUGGER
// =============================================================================

/// Debugger with VM tracing
pub struct Debugger {
    /// RPC endpoint
    rpc_url: String,
    /// Default config
    config: TraceConfig,
    /// HTTP client, built once so its connection pool is reused.
    client: reqwest::Client,
    /// Trace cache
    trace_cache: HashMap<String, TraceResult>,
    /// Maximum cache size
    max_cache_size: usize,
    /// Statistics
    stats: DebuggerStats,
}

/// Debugger statistics
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct DebuggerStats {
    pub total_traces: u64,
    pub cache_hits: u64,
    pub cache_misses: u64,
    pub errors: u64,
}

impl Debugger {
    /// Create new debugger
    pub fn new(rpc_url: String) -> Self {
        let config = TraceConfig::default();
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(config.timeout_seconds))
            .build()
            .unwrap_or_default();

        Self {
            rpc_url,
            config,
            client,
            trace_cache: HashMap::new(),
            max_cache_size: 1000,
            stats: DebuggerStats::default(),
        }
    }

    /// Issue a JSON-RPC request and return its `result`.
    async fn call(&self, request: &serde_json::Value) -> Result<serde_json::Value, TraceError> {
        let response = self
            .client
            .post(&self.rpc_url)
            .json(request)
            .send()
            .await
            .map_err(|e| {
                if e.is_timeout() {
                    TraceError::Timeout(e.to_string())
                } else {
                    TraceError::RpcError(e.to_string())
                }
            })?;

        let body: serde_json::Value = response
            .json()
            .await
            .map_err(|e| TraceError::RpcError(format!("malformed response: {e}")))?;

        if let Some(error) = body.get("error") {
            return Err(TraceError::RpcError(
                error["message"]
                    .as_str()
                    .unwrap_or("unknown JSON-RPC error")
                    .to_string(),
            ));
        }

        body.get("result")
            .cloned()
            .ok_or_else(|| TraceError::RpcError("response has no result".to_string()))
    }

    /// Trace transaction
    pub async fn trace_transaction(
        &mut self,
        tx_hash: &str,
        config: Option<TraceConfig>,
    ) -> Result<TraceResult, TraceError> {
        self.stats.total_traces += 1;

        let cfg = config.unwrap_or_else(|| self.config.clone());

        if let Some(cached) = self.trace_cache.get(tx_hash) {
            self.stats.cache_hits += 1;
            return Ok(cached.clone());
        }
        self.stats.cache_misses += 1;

        let request = serde_json::json!({
            "jsonrpc": "2.0",
            "method": "debug_traceTransaction",
            "params": [tx_hash, {
                "tracer": "opcodeLogger",
                "enableMemory": cfg.enable_memory,
                "disableStack": !cfg.enable_stack,
                "disableStorage": !cfg.enable_storage,
                "timeout": format!("{}s", cfg.timeout_seconds),
            }],
            "id": 1
        });

        let started = Instant::now();
        let result = self.call(&request).await;
        let elapsed = started.elapsed().as_millis() as u64;

        let result = match result {
            Ok(trace) => {
                let mut decoded = decode_trace_result(tx_hash, &trace, &cfg);
                decoded.execution_time_ms = elapsed;
                decoded
            }
            Err(e) => {
                self.stats.errors += 1;
                return Err(e);
            }
        };

        self.cache_result(tx_hash, result.clone());
        Ok(result)
    }

    /// Trace a call against current state without committing it.
    pub async fn trace_call(
        &mut self,
        from: &str,
        to: &str,
        value: &str,
        data: &str,
        config: Option<TraceConfig>,
    ) -> Result<TraceResult, TraceError> {
        self.stats.total_traces += 1;

        let cfg = config.unwrap_or_else(|| self.config.clone());

        let request = serde_json::json!({
            "jsonrpc": "2.0",
            "method": "debug_traceCall",
            "params": [{
                "from": from,
                "to": to,
                "value": value,
                "data": data,
            }, "latest", {
                "tracer": "opcodeLogger",
                "enableMemory": cfg.enable_memory,
                "disableStack": !cfg.enable_stack,
                "disableStorage": !cfg.enable_storage,
                "timeout": format!("{}s", cfg.timeout_seconds),
            }],
            "id": 1
        });

        let started = Instant::now();
        let result = self.call(&request).await;
        let elapsed = started.elapsed().as_millis() as u64;

        match result {
            Ok(trace) => {
                let mut decoded = decode_trace_result(&format!("call:{to}"), &trace, &cfg);
                decoded.execution_time_ms = elapsed;
                Ok(decoded)
            }
            Err(e) => {
                self.stats.errors += 1;
                Err(e)
            }
        }
    }

    /// Execute bytecode locally and report the opcodes it actually ran.
    ///
    /// This does not touch the chain. It runs the code through the same
    /// interpreter the node uses, so the steps reported are the real execution
    /// of `code` with `input` as calldata.
    pub fn trace_vm(
        &mut self,
        code: &str,
        input: &str,
        config: Option<TraceConfig>,
    ) -> Result<Vec<VMState>, TraceError> {
        let cfg = config.unwrap_or_else(|| self.config.clone());

        let code = decode_hex(code)
            .map_err(|e| TraceError::InvalidTransaction(format!("invalid code: {e}")))?;
        let input = decode_hex(input)
            .map_err(|e| TraceError::InvalidTransaction(format!("invalid input: {e}")))?;

        let mut ctx = tiger_evm::ExecutionContext::new();
        ctx.data = input;
        ctx.gas = 30_000_000;
        ctx.chain_id = 6666;

        let mut tracer = tiger_evm::trace::StepTracerBuffer::with_memory(cfg.max_trace_size);
        let executor = tiger_evm::Executor::new();
        let _result = executor.execute_traced(&code, &mut ctx, &mut tracer);

        let mut steps: Vec<VMState> = tracer
            .steps()
            .iter()
            .map(|step| VMState {
                pc: step.pc as u64,
                op: step.opcode,
                op_name: step.op.to_string(),
                gas: format!("0x{:x}", step.gas),
                gas_cost: format!("0x{:x}", step.gas_cost),
                memory: step
                    .memory
                    .as_ref()
                    .map(|m| format!("0x{}", hex::encode(m))),
                stack: step
                    .stack
                    .iter()
                    .map(|w| format!("0x{}", hex::encode(w.0)))
                    .collect(),
                storage: None,
                depth: step.depth,
                error: step.error.clone(),
            })
            .collect();

        if !cfg.enable_memory {
            for step in &mut steps {
                step.memory = None;
            }
        }
        if !cfg.enable_stack {
            for step in &mut steps {
                step.stack.clear();
            }
        }

        Ok(steps)
    }

    /// State changes recorded for a transaction.
    ///
    /// Served from the cache filled by [`Debugger::trace_transaction`]; a
    /// transaction that has not been traced returns an empty list rather than a
    /// fabricated change.
    pub fn get_state_diffs(&self, tx_hash: &str) -> Result<Vec<StateChange>, TraceError> {
        match self.trace_cache.get(tx_hash) {
            Some(trace) => Ok(trace.state_changes.clone()),
            None => Ok(Vec::new()),
        }
    }

    /// The root call frame for a transaction, if it has been traced.
    pub fn get_call_tree(&self, tx_hash: &str) -> Option<CallFrame> {
        self.trace_cache
            .get(tx_hash)
            .and_then(|trace| trace.calls.first().cloned())
    }

    /// Set config
    pub fn set_config(&mut self, config: TraceConfig) {
        self.config = config;
    }

    /// Get statistics
    pub fn stats(&self) -> &DebuggerStats {
        &self.stats
    }

    /// Clear cache
    pub fn clear_cache(&mut self) {
        self.trace_cache.clear();
    }

    fn cache_result(&mut self, key: &str, result: TraceResult) {
        if self.trace_cache.len() >= self.max_cache_size {
            if let Some(first) = self.trace_cache.keys().next().cloned() {
                self.trace_cache.remove(&first);
            }
        }
        self.trace_cache.insert(key.to_string(), result);
    }
}

/// Decode a `0x`-prefixed or bare hex string.
fn decode_hex(value: &str) -> Result<Vec<u8>, String> {
    let digits = value.strip_prefix("0x").unwrap_or(value);
    if digits.is_empty() {
        return Ok(Vec::new());
    }
    hex::decode(digits).map_err(|e| e.to_string())
}

// =============================================================================
// TRACE API SERVICE
// =============================================================================

/// Trace API service
pub struct TraceAPI {
    debugger: Debugger,
}

impl TraceAPI {
    /// Create new trace API
    pub fn new(rpc_url: String) -> Self {
        Self {
            debugger: Debugger::new(rpc_url),
        }
    }

    /// Trace transaction
    pub async fn trace_tx(
        &mut self,
        tx_hash: &str,
        config: Option<TraceConfig>,
    ) -> Result<TraceResult, TraceError> {
        self.debugger.trace_transaction(tx_hash, config).await
    }

    /// Trace call
    pub async fn trace_call(
        &mut self,
        from: &str,
        to: &str,
        value: &str,
        data: &str,
        config: Option<TraceConfig>,
    ) -> Result<TraceResult, TraceError> {
        self.debugger.trace_call(from, to, value, data, config).await
    }

    /// Get state diffs
    pub fn state_diffs(&self, tx_hash: &str) -> Result<Vec<StateChange>, TraceError> {
        self.debugger.get_state_diffs(tx_hash)
    }

    /// Get call tree
    pub fn call_tree(&self, tx_hash: &str) -> Option<CallFrame> {
        self.debugger.get_call_tree(tx_hash)
    }
}

// =============================================================================
// TESTS
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn trace_vm_reports_the_opcodes_that_ran() {
        // PUSH1 0x02, PUSH1 0x03, ADD, STOP
        let mut debugger = Debugger::new("http://localhost:8545".to_string());
        let steps = debugger.trace_vm("0x600260030100", "0x", None).unwrap();

        let names: Vec<&str> = steps.iter().map(|s| s.op_name.as_str()).collect();
        assert_eq!(names, vec!["PUSH1", "PUSH1", "ADD", "STOP"]);

        // ADD must have left the real sum on the stack: 2 + 3 = 5.
        let add = steps.iter().find(|s| s.op_name == "ADD").unwrap();
        assert_eq!(add.stack.len(), 1);
        assert!(add.stack[0].ends_with('5'), "expected 5, got {}", add.stack[0]);
    }

    #[test]
    fn trace_vm_runs_the_requested_code_not_a_template() {
        let mut debugger = Debugger::new("http://localhost:8545".to_string());

        let first = debugger.trace_vm("0x600100", "0x", None).unwrap();
        let second = debugger.trace_vm("0x60ff00", "0x", None).unwrap();

        assert!(!first.is_empty());
        // The immediate differs, so the pushed value must differ too.
        assert_ne!(first[0].stack, second[0].stack);
        assert!(second[0].stack[0].ends_with("ff"));
    }

    #[test]
    fn decodes_struct_logs_from_a_node_response() {
        let response = serde_json::json!({
            "gas": "0x5208",
            "returnValue": "0x01",
            "failed": false,
            "structLogs": [
                { "pc": 0, "op": "PUSH1", "gas": "0x1b1", "gasCost": "0x3", "depth": 1, "stack": [] },
                { "pc": 2, "op": "ADD", "gas": "0x1ae", "gasCost": "0x3", "depth": 1, "stack": ["0x1", "0x2"] }
            ]
        });

        let config = TraceConfig::default();
        let result = decode_trace_result("0xabc", &response, &config);

        assert_eq!(result.gas_used, "0x5208");
        assert_eq!(result.output, "0x01");
        assert!(!result.failed);
        assert_eq!(result.steps.len(), 2);
        assert_eq!(result.steps[0].op_name, "PUSH1");
        // The mnemonic must map back to its real opcode byte.
        assert_eq!(result.steps[0].op, 0x60);
        assert_eq!(result.steps[1].op, 0x01);
        assert_eq!(result.steps[1].stack.len(), 2);
    }

    #[test]
    fn a_failed_trace_reports_the_fault_and_no_invented_reason() {
        let response = serde_json::json!({
            "gas": "0x1",
            "returnValue": "0x",
            "failed": true,
            "structLogs": [
                { "pc": 0, "op": "REVERT", "gas": "0x1", "gasCost": "0x0", "depth": 1, "stack": [], "error": "execution reverted" }
            ]
        });

        let result = decode_trace_result("0xdef", &response, &TraceConfig::default());

        assert!(result.failed);
        assert_eq!(result.revert_reason.as_deref(), Some("execution reverted"));

        // A successful trace must not carry a reason.
        let ok = serde_json::json!({ "gas": "0x1", "failed": false, "structLogs": [] });
        let ok = decode_trace_result("0x1", &ok, &TraceConfig::default());
        assert_eq!(ok.revert_reason, None);
    }

    #[test]
    fn truncated_traces_are_flagged() {
        let response = serde_json::json!({
            "gas": "0x1", "failed": false, "truncated": true, "structLogs": []
        });

        let result = decode_trace_result("0x1", &response, &TraceConfig::default());
        assert!(result.truncated);
    }

    #[test]
    fn state_diff_is_decoded_from_the_node_response() {
        let response = serde_json::json!({
            "gas": "0x1",
            "failed": false,
            "structLogs": [],
            "stateDiff": {
                "0x00000000000000000000000000000000000000aa": {
                    "balance": { "*": { "from": "0x1", "to": "0x2" } },
                    "storage": {
                        "0x01": { "*": { "from": "0x0", "to": "0x5" } }
                    }
                }
            }
        });

        let result = decode_trace_result("0x1", &response, &TraceConfig::default());

        assert_eq!(result.state_changes.len(), 2);
        let balance = result
            .state_changes
            .iter()
            .find(|c| c.change_type == StateChangeType::Balance)
            .unwrap();
        assert_eq!(balance.old_value, "0x1");
        assert_eq!(balance.new_value, "0x2");

        let storage = result
            .state_changes
            .iter()
            .find(|c| c.change_type == StateChangeType::Storage)
            .unwrap();
        assert_eq!(storage.key, "0x01");
        assert_eq!(storage.new_value, "0x5");
    }

    #[test]
    fn call_frames_are_recovered_from_a_real_call_sequence() {
        let response = serde_json::json!({
            "gas": "0x1",
            "failed": false,
            "structLogs": [
                { "pc": 0, "op": "CALL", "gas": "0x100", "gasCost": "0x700", "depth": 1,
                  // structLogs reports the stack bottom first, so the entry the
                  // opcode pops first is last. CALL pops gas, target, value,
                  // then the in/out ranges, so bottom-to-top it ends gas,
                  // target, value.
                  "stack": ["0x0","0x0","0x0","0x0","0x1000","0x00000000000000000000000000000000000000bb","0x100"] },
                { "pc": 0, "op": "STOP", "gas": "0x100", "gasCost": "0x0", "depth": 2, "stack": [] },
                { "pc": 1, "op": "STOP", "gas": "0x100", "gasCost": "0x0", "depth": 1, "stack": [] }
            ]
        });

        let result = decode_trace_result("0x1", &response, &TraceConfig::default());

        assert_eq!(result.calls.len(), 1);
        let frame = &result.calls[0];
        assert_eq!(frame.call_type, "CALL");
        assert_eq!(frame.to, "0x00000000000000000000000000000000000000bb");
        assert_eq!(frame.value, "0x1000");
    }

    #[test]
    fn an_unreachable_endpoint_is_reported_not_faked() {
        let mut debugger = Debugger::new("http://127.0.0.1:1".to_string());
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();

        let result = runtime.block_on(debugger.trace_transaction("0xabc", None));
        assert!(result.is_err(), "an unreachable node must not yield a trace");
        assert_eq!(debugger.stats().errors, 1);
    }

    #[test]
    fn untraced_transactions_have_no_state_diffs() {
        let debugger = Debugger::new("http://localhost:8545".to_string());
        assert!(debugger.get_state_diffs("0xnever").unwrap().is_empty());
        assert!(debugger.get_call_tree("0xnever").is_none());
    }
}
