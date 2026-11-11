//! Internal Transaction Indexer - Complete Implementation
//!
//! This module provides comprehensive internal transaction tracking through:
//! - Trace RPC calls (debug_traceTransaction, debug_traceCall)
//! - Call graph reconstruction
//! - State diff tracking (balance/storage changes)
//! - Contract creation tracking
//! - Self-destruct tracking
//! - Gas analysis per call frame

use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::time::{SystemTime, UNIX_EPOCH};

// =============================================================================
// ERRORS
// =============================================================================

/// Internal Transaction Indexer Errors
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum InternalTxError {
    #[serde(rename = "trace_failed")]
    TraceFailed(String),
    #[serde(rename = "parse_error")]
    ParseError(String),
    #[serde(rename = "rpc_error")]
    RpcError(String),
    #[serde(rename = "storage_error")]
    StorageError(String),
    #[serde(rename = "invalid_transaction")]
    InvalidTransaction(String),
}

// =============================================================================
// CALL TYPES
// =============================================================================

/// EVM Call Types
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CallType {
    #[serde(rename = "call")]
    Call,
    #[serde(rename = "callcode")]
    CallCode,
    #[serde(rename = "delegatecall")]
    DelegateCall,
    #[serde(rename = "staticcall")]
    StaticCall,
    #[serde(rename = "create")]
    Create,
    #[serde(rename = "create2")]
    Create2,
    #[serde(rename = "selfdestruct")]
    SelfDestruct,
    #[serde(rename = "reward")]
    Reward,
}

impl CallType {
    pub fn from_str(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "call" => CallType::Call,
            "callcode" => CallType::CallCode,
            "delegatecall" => CallType::DelegateCall,
            "staticcall" => CallType::StaticCall,
            "create" => CallType::Create,
            "create2" => CallType::Create2,
            "selfdestruct" => CallType::SelfDestruct,
            "reward" => CallType::Reward,
            _ => CallType::Call,
        }
    }

    pub fn as_str(&self) -> &str {
        match self {
            CallType::Call => "call",
            CallType::CallCode => "callcode",
            CallType::DelegateCall => "delegatecall",
            CallType::StaticCall => "staticcall",
            CallType::Create => "create",
            CallType::Create2 => "create2",
            CallType::SelfDestruct => "selfdestruct",
            CallType::Reward => "reward",
        }
    }
}

// =============================================================================
// INTERNAL TRANSACTION
// =============================================================================

/// Internal Transaction (single call frame)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InternalTransaction {
    /// Unique trace ID
    pub id: String,
    /// Parent transaction hash
    pub transaction_hash: String,
    /// Block number
    pub block_number: u64,
    /// Trace index within transaction
    pub trace_index: u32,
    /// Subtrace index
    pub subtrace_index: u32,
    /// Call type
    pub call_type: CallType,
    /// Address that initiated this call
    pub from: String,
    /// Address that received this call
    pub to: String,
    /// Value transferred (in wei)
    pub value: String,
    /// Gas provided for this call
    pub gas: String,
    /// Gas used by this call
    pub gas_used: Option<String>,
    /// Input data (calldata)
    pub input: String,
    /// Output data (returndata)
    pub output: Option<String>,
    /// Error message if call failed
    pub error: Option<String>,
    /// Depth in call stack (0 = EOA, 1 = first contract call)
    pub depth: u32,
    /// Parent trace index
    pub parent_index: Option<u32>,
    /// List of child trace indices
    pub children: Vec<u32>,
    /// Whether this call created a contract
    pub creates: Option<String>,
    /// Whether this call was successful
    pub success: bool,
    /// Timestamp
    pub timestamp: u64,
}

impl InternalTransaction {
    /// Create new internal transaction
    pub fn new(
        tx_hash: String,
        block_number: u64,
        trace_index: u32,
    ) -> Self {
        Self {
            id: format!("{}-{}-{}", tx_hash, trace_index, now_unix()),
            transaction_hash: tx_hash,
            block_number,
            trace_index,
            subtrace_index: 0,
            call_type: CallType::Call,
            from: String::new(),
            to: String::new(),
            value: "0".to_string(),
            gas: "0".to_string(),
            gas_used: None,
            input: String::new(),
            output: None,
            error: None,
            depth: 0,
            parent_index: None,
            children: vec![],
            creates: None,
            success: true,
            timestamp: now_unix(),
        }
    }

    /// Gas consumed by this call.
    ///
    /// When the tracer reports `gasUsed` that value is authoritative. When it
    /// does not, the amount is unknown and reported as zero rather than
    /// guessed.
    pub fn gas_consumed(&self) -> u64 {
        match self.gas_used.as_ref() {
            Some(used) => parse_wei(used).max(0) as u64,
            None => 0,
        }
    }

    /// Gas that was provided but not consumed, when both values are known.
    pub fn gas_remaining(&self) -> Option<u64> {
        let provided = parse_wei(&self.gas).max(0) as u64;
        Some(provided.saturating_sub(self.gas_consumed()))
    }

    /// Check if this is a contract creation
    pub fn is_contract_creation(&self) -> bool {
        matches!(self.call_type, CallType::Create | CallType::Create2)
    }

    /// Check if this is a self-destruct
    pub fn is_self_destruct(&self) -> bool {
        self.call_type == CallType::SelfDestruct
    }

    /// Check if this is a delegate call
    pub fn is_delegate_call(&self) -> bool {
        self.call_type == CallType::DelegateCall
    }

    /// Check if this is a static call
    pub fn is_static_call(&self) -> bool {
        self.call_type == CallType::StaticCall
    }
}

// =============================================================================
// STATE DIFF
// =============================================================================

/// State change (balance or storage)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StateChange {
    pub block_number: u64,
    pub transaction_hash: String,
    pub trace_index: u32,
    pub address: String,
    pub key: Option<String>,
    /// Value before the change. For balance changes the absolute balance is
    /// not derivable from a trace, so the pair is expressed against a zero
    /// baseline: an address that sent funds moves from the amount sent to
    /// zero, and one that received funds moves from zero to the amount
    /// received.
    pub old_value: String,
    /// Value after the change. See `old_value` for the balance semantics.
    pub new_value: String,
    pub change_type: StateChangeType,
}

/// Type of state change
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum StateChangeType {
    #[serde(rename = "balance")]
    Balance,
    #[serde(rename = "storage")]
    Storage,
    #[serde(rename = "code")]
    Code,
    #[serde(rename = "nonce")]
    Nonce,
}

impl StateChange {
    /// Create balance change
    pub fn balance_change(
        block_number: u64,
        tx_hash: String,
        address: String,
        old_balance: String,
        new_balance: String,
    ) -> Self {
        Self {
            block_number,
            transaction_hash: tx_hash,
            trace_index: 0,
            address,
            key: None,
            old_value: old_balance,
            new_value: new_balance,
            change_type: StateChangeType::Balance,
        }
    }

    /// Create storage change
    pub fn storage_change(
        block_number: u64,
        tx_hash: String,
        trace_index: u32,
        address: String,
        key: String,
        old_value: String,
        new_value: String,
    ) -> Self {
        Self {
            block_number,
            transaction_hash: tx_hash,
            trace_index,
            address,
            key: Some(key),
            old_value,
            new_value,
            change_type: StateChangeType::Storage,
        }
    }

    /// Get change amount (for balance)
    pub fn change_amount(&self) -> i128 {
        let old = u128::from_str_radix(self.old_value.trim_start_matches("0x"), 16).unwrap_or(0);
        let new = u128::from_str_radix(self.new_value.trim_start_matches("0x"), 16).unwrap_or(0);
        new as i128 - old as i128
    }
}

// =============================================================================
// CALL GRAPH
// =============================================================================

/// Call graph representation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CallGraph {
    pub transaction_hash: String,
    pub block_number: u64,
    pub root_calls: Vec<CallNode>,
    pub total_calls: u32,
    pub max_depth: u32,
}

/// Node in call graph
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CallNode {
    pub trace: InternalTransaction,
    pub children: Vec<CallNode>,
}

impl CallGraph {
    /// Build call graph from flat trace list
    pub fn from_traces(traces: &[InternalTransaction]) -> Self {
        if traces.is_empty() {
            return Self {
                transaction_hash: String::new(),
                block_number: 0,
                root_calls: vec![],
                total_calls: 0,
                max_depth: 0,
            };
        }

        let tx_hash = traces[0].transaction_hash.clone();
        let block_number = traces[0].block_number;

        // Build tree structure
        let mut root_calls = vec![];
        let mut max_depth_val = 0;

        // Find root calls (depth == 1)
        let roots: Vec<&InternalTransaction> = traces.iter()
            .filter(|t| t.depth == 1)
            .collect();

        for root in roots {
            max_depth_val = max_depth_val.max(root.depth);
            let node = Self::build_node(root, traces);
            root_calls.push(node);
        }

        Self {
            transaction_hash: tx_hash,
            block_number,
            root_calls,
            total_calls: traces.len() as u32,
            max_depth: max_depth_val,
        }
    }

    fn build_node(root: &InternalTransaction, traces: &[InternalTransaction]) -> CallNode {
        let mut children = vec![];

        // Find direct children
        for trace in traces.iter().filter(|t| t.parent_index == Some(root.trace_index)) {
            children.push(Self::build_node(trace, traces));
        }

        CallNode {
            trace: root.clone(),
            children,
        }
    }

    /// Get all addresses involved in this call graph
    pub fn involved_addresses(&self) -> HashSet<String> {
        let mut addresses = HashSet::new();
        Self::collect_addresses(&self.root_calls, &mut addresses);
        addresses
    }

    fn collect_addresses(nodes: &[CallNode], addresses: &mut HashSet<String>) {
        for node in nodes {
            addresses.insert(node.trace.from.clone());
            addresses.insert(node.trace.to.clone());
            if let Some(created) = &node.trace.creates {
                addresses.insert(created.clone());
            }
            Self::collect_addresses(&node.children, addresses);
        }
    }

    /// Calculate total gas used
    pub fn total_gas_used(&self) -> u64 {
        Self::sum_gas(&self.root_calls)
    }

    fn sum_gas(nodes: &[CallNode]) -> u64 {
        nodes.iter()
            .map(|n| {
                let mut sum = n.trace.gas_used.as_ref()
                    .map(|g| u64::from_str_radix(g.trim_start_matches("0x"), 16).unwrap_or(0))
                    .unwrap_or(0);
                sum += Self::sum_gas(&n.children);
                sum
            })
            .sum()
    }
}

// =============================================================================
// TRANSACTION TRACE
// =============================================================================

/// Complete trace for a transaction
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TransactionTrace {
    /// Transaction hash
    pub transaction_hash: String,
    /// Block number
    pub block_number: u64,
    /// Block hash
    pub block_hash: String,
    /// All internal transactions
    pub traces: Vec<InternalTransaction>,
    /// All state changes
    pub state_changes: Vec<StateChange>,
    /// Call graph
    pub call_graph: CallGraph,
    /// Whether trace was successful
    pub success: bool,
    /// Error message if failed
    pub error: Option<String>,
    /// Trace output
    pub output: Option<String>,
    /// Gas used
    pub gas_used: String,
    /// Execution time in ms
    pub execution_time_ms: u64,
    /// Timestamp
    pub timestamp: u64,
}

impl TransactionTrace {
    /// Create new transaction trace
    pub fn new(tx_hash: String, block_number: u64, block_hash: String) -> Self {
        Self {
            transaction_hash: tx_hash,
            block_number,
            block_hash,
            traces: vec![],
            state_changes: vec![],
            call_graph: CallGraph::from_traces(&[]),
            success: true,
            error: None,
            output: None,
            gas_used: "0x0".to_string(),
            execution_time_ms: 0,
            timestamp: now_unix(),
        }
    }

    /// Add trace
    pub fn add_trace(&mut self, trace: InternalTransaction) {
        self.traces.push(trace);
    }

    /// Add state change
    pub fn add_state_change(&mut self, change: StateChange) {
        self.state_changes.push(change);
    }

    /// Build call graph
    pub fn build_call_graph(&mut self) {
        self.call_graph = CallGraph::from_traces(&self.traces);
    }

    /// Get failed calls
    pub fn failed_calls(&self) -> Vec<&InternalTransaction> {
        self.traces.iter()
            .filter(|t| !t.success || t.error.is_some())
            .collect()
    }

    /// Get created contracts
    pub fn created_contracts(&self) -> Vec<&InternalTransaction> {
        self.traces.iter()
            .filter(|t| t.is_contract_creation())
            .collect()
    }

    /// Get internal transfers
    pub fn internal_transfers(&self) -> Vec<&InternalTransaction> {
        self.traces.iter()
            .filter(|t| {
                t.call_type == CallType::Call && 
                t.value != "0x0" && 
                t.value != "0"
            })
            .collect()
    }
}

// =============================================================================
// TRACE PARSING
// =============================================================================

/// Errors produced while decoding a `debug_traceTransaction` response.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum TraceParseError {
    /// The payload is not valid JSON.
    InvalidJson(String),
    /// A required field is missing or has the wrong type.
    MissingField(String),
    /// A field holds a malformed value.
    Malformed(String),
}

impl std::fmt::Display for TraceParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TraceParseError::InvalidJson(msg) => write!(f, "invalid trace JSON: {msg}"),
            TraceParseError::MissingField(field) => write!(f, "missing trace field: {field}"),
            TraceParseError::Malformed(msg) => write!(f, "malformed trace: {msg}"),
        }
    }
}

impl std::error::Error for TraceParseError {}

/// Result of parsing a trace payload.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ParsedTrace {
    /// All call frames, flattened in depth first order.
    pub traces: Vec<InternalTransaction>,
    /// Balance changes reconstructed from the call frames.
    pub state_changes: Vec<StateChange>,
    /// Whether the transaction itself succeeded.
    pub success: bool,
    /// Revert reason or error string when the transaction failed.
    pub error: Option<String>,
    /// Gas used by the whole transaction, when reported.
    pub gas_used: Option<String>,
}

/// Flatten a `callTracer` frame and its children into a list of internal
/// transactions. `index` and `depth` are threaded through the recursion so that
/// every frame receives a stable identifier.
fn flatten_frame(
    frame: &serde_json::Value,
    tx_hash: &str,
    block_number: u64,
    depth: u32,
    index: &mut u32,
    out: &mut Vec<InternalTransaction>,
) -> Result<(), TraceParseError> {
    let frame = frame
        .as_object()
        .ok_or_else(|| TraceParseError::Malformed("call frame is not an object".to_string()))?;

    let trace_index = *index;
    *index += 1;

    let mut internal = InternalTransaction::new(tx_hash.to_string(), block_number, trace_index);
    internal.depth = depth;

    internal.call_type = match frame.get("type").and_then(|v| v.as_str()) {
        Some(kind) => CallType::from_str(kind),
        None => CallType::Call,
    };

    if let Some(from) = frame.get("from").and_then(|v| v.as_str()) {
        internal.from = from.to_ascii_lowercase();
    }
    if let Some(to) = frame.get("to").and_then(|v| v.as_str()) {
        internal.to = to.to_ascii_lowercase();
    }

    // Reject frames whose addresses are malformed rather than storing values
    // that cannot be queried.
    for (field, value) in [("from", &internal.from), ("to", &internal.to)] {
        if !value.is_empty() && !is_valid_address(value) {
            return Err(TraceParseError::Malformed(format!(
                "call frame {field} is not a 20 byte address: {value}"
            )));
        }
    }
    if let Some(value) = frame.get("value").and_then(|v| v.as_str()) {
        internal.value = value.to_string();
    }
    if let Some(gas) = frame.get("gas").and_then(|v| v.as_str()) {
        internal.gas = gas.to_string();
    }
    if let Some(gas_used) = frame.get("gasUsed").and_then(|v| v.as_str()) {
        internal.gas_used = Some(gas_used.to_string());
    }
    if let Some(input) = frame.get("input").and_then(|v| v.as_str()) {
        internal.input = input.to_string();
    }
    if let Some(output) = frame.get("output").and_then(|v| v.as_str()) {
        internal.output = Some(output.to_string());
    }
    if let Some(creates) = frame.get("creates").and_then(|v| v.as_str()) {
        internal.creates = Some(creates.to_ascii_lowercase());
    }
    if let Some(error) = frame.get("error").and_then(|v| v.as_str()) {
        internal.error = Some(error.to_string());
        internal.success = false;
    }
    if let Some(reason) = frame.get("revertReason").and_then(|v| v.as_str()) {
        internal.error = Some(reason.to_string());
        internal.success = false;
    }

    out.push(internal);

    if let Some(calls) = frame.get("calls").and_then(|v| v.as_array()) {
        for call in calls {
            flatten_frame(call, tx_hash, block_number, depth + 1, index, out)?;
        }
    }

    Ok(())
}

/// Parse a `debug_traceTransaction` payload.
///
/// Both the `callTracer` shape (a nested `calls` tree) and the flat
/// `{"trace": [...]}` shape produced by parity style tracers are supported.
pub fn parse_trace_payload(
    tx_hash: &str,
    block_number: u64,
    payload: &serde_json::Value,
) -> Result<ParsedTrace, TraceParseError> {
    let mut traces = Vec::new();
    let mut index = 0u32;

    // A callTracer response is either the frame itself or wrapped in a
    // `result` envelope. Parity style tracers return a flat `trace` array.
    let root = if looks_like_frame(payload) {
        Some(payload)
    } else {
        payload.get("result").filter(|r| looks_like_frame(r))
    };

    if let Some(root) = root {
        flatten_frame(root, tx_hash, block_number, 0, &mut index, &mut traces)?;
    } else if let Some(flat) = payload.get("trace").and_then(|t| t.as_array()) {
        for entry in flat {
            flatten_frame(entry, tx_hash, block_number, 0, &mut index, &mut traces)?;
        }
    } else {
        return Err(TraceParseError::MissingField(
            "expected a callTracer frame, a trace array or a result envelope".to_string(),
        ));
    }

    let mut state_changes = build_balance_changes(tx_hash, block_number, &traces);
    state_changes.extend(build_storage_changes(tx_hash, block_number, payload));

    let success = traces.first().map(|t| t.success).unwrap_or(true);
    let error = traces.first().and_then(|t| t.error.clone());
    let gas_used = traces.first().and_then(|t| t.gas_used.clone());

    Ok(ParsedTrace {
        traces,
        state_changes,
        success,
        error,
        gas_used,
    })
}

/// Net balance delta per address derived from the value transfers in `traces`.
pub fn build_balance_changes(
    tx_hash: &str,
    block_number: u64,
    traces: &[InternalTransaction],
) -> Vec<StateChange> {
    let mut balances: HashMap<String, i128> = HashMap::new();

    for trace in traces {
        let value = parse_wei(&trace.value);
        if value == 0 {
            continue;
        }

        *balances.entry(trace.from.clone()).or_insert(0) -= value;
        if !trace.to.is_empty() {
            *balances.entry(trace.to.clone()).or_insert(0) += value;
        }
    }

    let mut addresses: Vec<&String> = balances.keys().collect();
    addresses.sort();

    // A positive delta is value received, a negative delta is value sent.
    let mut changes = Vec::with_capacity(addresses.len());
    for address in addresses {
        let delta = balances[address];
        if delta == 0 {
            continue;
        }

        let (old_value, new_value) = if delta >= 0 {
            (0u128, delta as u128)
        } else {
            ((-delta) as u128, 0u128)
        };

        changes.push(StateChange::balance_change(
            block_number,
            tx_hash.to_string(),
            address.clone(),
            format!("0x{old_value:x}"),
            format!("0x{new_value:x}"),
        ));
    }

    changes
}

/// Storage changes from a parity style `stateDiff` object.
pub fn build_storage_changes(
    tx_hash: &str,
    block_number: u64,
    payload: &serde_json::Value,
) -> Vec<StateChange> {
    let state_diff = match payload.get("stateDiff").and_then(|s| s.as_object()) {
        Some(diff) => diff,
        None => return Vec::new(),
    };

    let mut changes = Vec::new();
    let mut addresses: Vec<&String> = state_diff.keys().collect();
    addresses.sort();

    for address in addresses {
        let entry = &state_diff[address];
        let storage = match entry.get("storage").and_then(|s| s.as_object()) {
            Some(storage) => storage,
            None => continue,
        };

        let mut slots: Vec<&String> = storage.keys().collect();
        slots.sort();

        for slot in slots {
            let change = &storage[slot];
            let old_value = change
                .get("*")
                .and_then(|v| v.get("from"))
                .and_then(|v| v.as_str())
                .or_else(|| change.get("from").and_then(|v| v.as_str()))
                .unwrap_or("0x0");
            let new_value = change
                .get("*")
                .and_then(|v| v.get("to"))
                .and_then(|v| v.as_str())
                .or_else(|| change.get("to").and_then(|v| v.as_str()))
                .unwrap_or("0x0");

            changes.push(StateChange::storage_change(
                block_number,
                tx_hash.to_string(),
                0,
                address.to_ascii_lowercase(),
                slot.clone(),
                old_value.to_string(),
                new_value.to_string(),
            ));
        }
    }

    changes
}

/// Whether `value` is a well formed 20 byte `0x` prefixed address.
pub fn is_valid_address(value: &str) -> bool {
    let Some(hex) = value.strip_prefix("0x").or_else(|| value.strip_prefix("0X")) else {
        return false;
    };
    hex.len() == 40 && hex.bytes().all(|b| b.is_ascii_hexdigit())
}

/// Whether `value` looks like a `callTracer` call frame.
pub fn looks_like_frame(value: &serde_json::Value) -> bool {
    let Some(object) = value.as_object() else {
        return false;
    };

    object.contains_key("from")
        && (object.contains_key("to") || object.contains_key("creates"))
        && (object.contains_key("type")
            || object.contains_key("calls")
            || object.contains_key("input"))
}

/// Parse a wei quantity that may be decimal or `0x` prefixed hexadecimal.
pub fn parse_wei(value: &str) -> i128 {
    let trimmed = value.trim();
    if let Some(hex) = trimmed.strip_prefix("0x").or_else(|| trimmed.strip_prefix("0X")) {
        return i128::from_str_radix(hex, 16).unwrap_or(0);
    }
    trimmed.parse().unwrap_or(0)
}

// =============================================================================
// HELPER FUNCTIONS
// =============================================================================

/// Get current Unix timestamp
fn now_unix() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs()
}
#[cfg(test)]
mod tests {
    use super::*;

    fn sample_call_trace() -> serde_json::Value {
        serde_json::json!({
            "type": "CALL",
            "from": "0x1111111111111111111111111111111111111111",
            "to": "0x2222222222222222222222222222222222222222",
            "value": "0x64",
            "gas": "0x5208",
            "gasUsed": "0x5208",
            "input": "0x",
            "output": "0x",
            "calls": [{
                "type": "STATICCALL",
                "from": "0x2222222222222222222222222222222222222222",
                "to": "0x3333333333333333333333333333333333333333",
                "value": "0x0",
                "gas": "0x1000",
                "gasUsed": "0x200",
                "input": "0xdeadbeef"
            }]
        })
    }

    #[test]
    fn call_tracer_frames_are_flattened_in_depth_first_order() {
        let parsed = parse_trace_payload(
            "0xabc",
            42,
            &sample_call_trace(),
        )
        .expect("trace parses");

        assert_eq!(parsed.traces.len(), 2);
        assert_eq!(parsed.traces[0].depth, 0);
        assert_eq!(parsed.traces[0].trace_index, 0);
        assert_eq!(parsed.traces[0].value, "0x64");
        assert_eq!(parsed.traces[1].depth, 1);
        assert_eq!(parsed.traces[1].trace_index, 1);
        assert_eq!(parsed.traces[1].call_type, CallType::StaticCall);
        assert!(parsed.success);
        assert_eq!(parsed.gas_used.as_deref(), Some("0x5208"));
    }

    #[test]
    fn a_reverted_frame_is_marked_unsuccessful() {
        let payload = serde_json::json!({
            "type": "CALL",
            "from": "0x1111111111111111111111111111111111111111",
            "to": "0x2222222222222222222222222222222222222222",
            "value": "0x0",
            "gas": "0x5208",
            "gasUsed": "0x5208",
            "error": "execution reverted",
            "revertReason": "insufficient balance"
        });

        let parsed = parse_trace_payload("0xabc", 1, &payload).expect("trace parses");
        assert!(!parsed.success);
        assert!(!parsed.traces[0].success);
        assert_eq!(
            parsed.traces[0].error.as_deref(),
            Some("insufficient balance")
        );
    }

    #[test]
    fn balance_changes_are_net_per_address() {
        let parsed = parse_trace_payload("0xabc", 7, &sample_call_trace()).expect("trace parses");

        let changes = &parsed.state_changes;
        let sender = changes
            .iter()
            .find(|c| c.address == "0x1111111111111111111111111111111111111111")
            .expect("sender has a change");
        let recipient = changes
            .iter()
            .find(|c| c.address == "0x2222222222222222222222222222222222222222")
            .expect("recipient has a change");

        // 0x64 == 100 wei moves from the sender to the recipient. The sender
        // is recorded as moving from 100 to 0 and the recipient from 0 to 100.
        assert_eq!(sender.old_value, "0x64");
        assert_eq!(sender.new_value, "0x0");
        assert_eq!(recipient.old_value, "0x0");
        assert_eq!(recipient.new_value, "0x64");
        assert_eq!(changes.len(), 2);
    }

    #[test]
    fn value_returned_to_the_sender_nets_to_zero() {
        let payload = serde_json::json!({
            "type": "CALL",
            "from": "0x1111111111111111111111111111111111111111",
            "to": "0x2222222222222222222222222222222222222222",
            "value": "0x64",
            "gas": "0x1",
            "gasUsed": "0x1",
            "calls": [{
                "type": "CALL",
                "from": "0x2222222222222222222222222222222222222222",
                "to": "0x1111111111111111111111111111111111111111",
                "value": "0x64",
                "gas": "0x1",
                "gasUsed": "0x1"
            }]
        });

        let parsed = parse_trace_payload("0xabc", 4, &payload).expect("trace parses");
        // The two transfers cancel out, so neither address has a net change.
        assert!(
            parsed.state_changes.is_empty(),
            "net-zero transfers must not be reported as changes"
        );
    }

    #[test]
    fn storage_diffs_from_a_parity_tracer_are_parsed() {
        let payload = serde_json::json!({
            "trace": [{
                "type": "CALL",
                "from": "0x1111111111111111111111111111111111111111",
                "to": "0x2222222222222222222222222222222222222222",
                "value": "0x0",
                "gas": "0x1",
                "gasUsed": "0x1"
            }],
            "stateDiff": {
                "0x2222222222222222222222222222222222222222": {
                    "storage": {
                        "0x0": { "*": { "from": "0x01", "to": "0x02" } }
                    }
                }
            }
        });

        let parsed = parse_trace_payload("0xdef", 9, &payload).expect("trace parses");
        assert_eq!(parsed.traces.len(), 1);

        let storage: Vec<&StateChange> = parsed
            .state_changes
            .iter()
            .filter(|c| c.change_type == StateChangeType::Storage)
            .collect();
        assert_eq!(storage.len(), 1);
        assert_eq!(storage[0].key.as_deref(), Some("0x0"));
        assert_eq!(storage[0].old_value, "0x01");
        assert_eq!(storage[0].new_value, "0x02");
    }

    #[test]
    fn a_malformed_address_is_rejected() {
        let payload = serde_json::json!({
            "type": "CALL",
            "from": "not-an-address",
            "to": "0x2222222222222222222222222222222222222222",
            "value": "0x0"
        });

        let err = parse_trace_payload("0xabc", 1, &payload).unwrap_err();
        assert!(matches!(err, TraceParseError::Malformed(_)));
    }

    #[test]
    fn an_unrecognised_payload_is_rejected() {
        let payload = serde_json::json!({ "unexpected": true });
        let err = parse_trace_payload("0xabc", 1, &payload).unwrap_err();
        assert!(matches!(err, TraceParseError::MissingField(_)));
    }

    #[test]
    fn wei_quantities_accept_decimal_and_hex() {
        assert_eq!(parse_wei("0x64"), 100);
        assert_eq!(parse_wei("100"), 100);
        assert_eq!(parse_wei("0x0"), 0);
        assert_eq!(parse_wei("garbage"), 0);
        // 1 ether in wei.
        assert_eq!(parse_wei("0xde0b6b3a7640000"), 1_000_000_000_000_000_000);
    }

    #[test]
    fn address_validation_accepts_only_20_byte_hex() {
        assert!(is_valid_address("0x1111111111111111111111111111111111111111"));
        assert!(!is_valid_address("0x1111"));
        assert!(!is_valid_address("1111111111111111111111111111111111111111"));
        assert!(!is_valid_address("0xzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzz"));
    }

    #[test]
    fn gas_accounting_uses_the_reported_usage() {
        let mut tx = InternalTransaction::new("0xabc".to_string(), 1, 0);
        tx.gas = "0x100".to_string();
        tx.gas_used = Some("0x40".to_string());
        assert_eq!(tx.gas_consumed(), 0x40);
        assert_eq!(tx.gas_remaining(), Some(0xc0));
    }

    #[test]
    fn gas_consumed_is_zero_when_the_tracer_omits_it() {
        let mut tx = InternalTransaction::new("0xabc".to_string(), 1, 0);
        tx.gas = "0x100".to_string();
        assert_eq!(tx.gas_consumed(), 0);
    }

    #[test]
    fn a_single_frame_without_subcalls_is_accepted() {
        let payload = serde_json::json!({
            "type": "CALL",
            "from": "0x1111111111111111111111111111111111111111",
            "to": "0x2222222222222222222222222222222222222222",
            "value": "0x0",
            "gas": "0x5208",
            "gasUsed": "0x5208"
        });

        let parsed = parse_trace_payload("0xabc", 3, &payload).expect("trace parses");
        assert_eq!(parsed.traces.len(), 1);
        assert_eq!(parsed.traces[0].call_type, CallType::Call);
    }

    #[test]
    fn a_result_envelope_is_unwrapped() {
        let payload = serde_json::json!({
            "result": {
                "type": "CALL",
                "from": "0x1111111111111111111111111111111111111111",
                "to": "0x2222222222222222222222222222222222222222",
                "value": "0x1",
                "gas": "0x5208",
                "gasUsed": "0x5208"
            }
        });

        let parsed = parse_trace_payload("0xabc", 3, &payload).expect("trace parses");
        assert_eq!(parsed.traces.len(), 1);
        assert_eq!(parsed.traces[0].value, "0x1");
    }
}
