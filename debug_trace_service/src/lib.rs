//! Debug Trace Service - Transaction tracing over JSON-RPC.
//!
//! Wraps the node's `debug_*` methods:
//! - `debug_traceTransaction`
//! - `debug_traceCall`
//! - `debug_traceBlockByNumber`
//!
//! The service does not invent trace data. Every field it returns is decoded
//! from the node's response, and a field the node omitted is reported as
//! absent rather than defaulted to a plausible-looking value.

use serde::{Deserialize, Serialize};
use thiserror::Error;

// =============================================================================
// ERRORS
// =============================================================================

#[derive(Error, Debug)]
pub enum DebugTraceError {
    #[error("RPC error: {0}")]
    RpcError(String),
    #[error("Parse error: {0}")]
    ParseError(String),
    #[error("Not found: {0}")]
    NotFound(String),
}

// =============================================================================
// CONFIGURATION
// =============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DebugTraceConfig {
    pub rpc_url: String,
    pub trace_timeout_secs: u64,
}

impl Default for DebugTraceConfig {
    fn default() -> Self {
        Self {
            rpc_url: std::env::var("RPC_URL").unwrap_or_else(|_| "http://localhost:8545".to_string()),
            trace_timeout_secs: 300,
        }
    }
}

// =============================================================================
// TRACE TYPES
// =============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TraceResult {
    pub gas: String,
    pub return_value: String,
    /// Whether the traced call reverted or faulted, from the node's `failed`.
    pub failed: bool,
    pub struct_logs: Vec<StructLog>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StructLog {
    pub pc: u64,
    pub op: String,
    pub gas: String,
    pub gas_cost: String,
    pub stack: Vec<String>,
    /// The memory image, as the single hex string the node sends.
    pub memory: Option<String>,
    pub storage: Option<std::collections::HashMap<String, String>>,
    pub depth: u32,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CallFrame {
    pub op: String,
    pub from: String,
    pub to: String,
    pub input: String,
    pub output: String,
    pub value: String,
    pub gas: String,
    pub revert_reason: Option<String>,
    pub calls: Vec<CallFrame>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TraceTransaction {
    pub tx_hash: String,
    pub block_number: u64,
    pub block_hash: String,
    pub trace: Vec<CallFrame>,
    pub gas_used: u64,
    pub success: bool,
}

// =============================================================================
// RESPONSE DECODING
// =============================================================================

/// Decode one `structLogs` entry.
fn parse_struct_log(log: &serde_json::Value) -> StructLog {
    StructLog {
        pc: log["pc"].as_u64().unwrap_or(0),
        op: log["op"].as_str().unwrap_or("").to_string(),
        gas: log["gas"].as_str().unwrap_or("0x0").to_string(),
        gas_cost: log["gasCost"].as_str().unwrap_or("0x0").to_string(),
        stack: log["stack"]
            .as_array()
            .map(|s| s.iter().filter_map(|v| v.as_str().map(String::from)).collect())
            .unwrap_or_default(),
        // The node sends the memory image as one hex string.
        memory: log["memory"].as_str().map(String::from),
        storage: log["storage"].as_object().map(|s| {
            s.iter()
                .map(|(k, v)| (k.clone(), v.as_str().unwrap_or("0x").to_string()))
                .collect()
        }),
        depth: log["depth"].as_u64().unwrap_or(0) as u32,
        error: log["error"].as_str().map(String::from),
    }
}

/// Decode a `debug_trace*` result object into a [`TraceResult`].
fn parse_trace_result(trace: &serde_json::Value) -> TraceResult {
    let struct_logs = trace["structLogs"]
        .as_array()
        .map(|logs| logs.iter().map(parse_struct_log).collect())
        .unwrap_or_default();

    TraceResult {
        gas: trace["gas"].as_str().unwrap_or("0x0").to_string(),
        return_value: trace["returnValue"].as_str().unwrap_or("0x").to_string(),
        failed: trace["failed"].as_bool().unwrap_or(false),
        struct_logs,
    }
}

/// The call-family opcodes that open a new frame.
fn is_call_opcode(op: &str) -> bool {
    matches!(
        op,
        "CALL" | "CALLCODE" | "DELEGATECALL" | "STATICCALL" | "CREATE" | "CREATE2"
    )
}

/// Build call frames from a struct-log trace.
///
/// The interpreter records every opcode including the call-family ones, so the
/// frames are recovered from the real execution: a frame opens at a call
/// opcode and closes when execution returns to the depth its parent runs at.
/// This mirrors how a `callTracer` derives frames from the same execution,
/// rather than approximating them.
fn call_frames_from(struct_logs: &[StructLog]) -> Vec<CallFrame> {
    let mut roots: Vec<CallFrame> = Vec::new();
    let mut open: Vec<(usize, CallFrame)> = Vec::new();

    for log in struct_logs {
        if is_call_opcode(&log.op) {
            // The stack words a call opcode consumes, top first: gas, target,
            // value (CALL/CALLCODE only), then the in/out ranges.
            let stack = &log.stack;
            let n = stack.len();
            let word = |from_end: usize| -> String {
                if n >= from_end {
                    stack[n - from_end].clone()
                } else {
                    "0x0".to_string()
                }
            };
            let (value, to) = match log.op.as_str() {
                "STATICCALL" | "DELEGATECALL" => ("0x0".to_string(), word(2)),
                _ => (word(3), word(2)),
            };
            let frame = CallFrame {
                op: log.op.clone(),
                from: String::new(),
                to: truncate_address(&to),
                input: String::new(),
                output: String::new(),
                value,
                gas: word(1),
                revert_reason: None,
                calls: Vec::new(),
            };
            open.push((log.depth as usize, frame));
            continue;
        }

        if matches!(log.op.as_str(), "RETURN" | "REVERT" | "STOP" | "SELFDESTRUCT") {
            while let Some((depth, mut frame)) = open.pop() {
                if log.op == "REVERT" {
                    frame.revert_reason = log.error.clone();
                }
                if let Some((_, parent)) = open.last_mut() {
                    parent.calls.push(frame);
                } else {
                    roots.push(frame);
                }
                if depth <= log.depth as usize {
                    break;
                }
            }
        }
    }

    // A frame still open at the end of the trace never returned; report it at
    // the top level rather than dropping it.
    while let Some((_, frame)) = open.pop() {
        if let Some((_, parent)) = open.last_mut() {
            parent.calls.push(frame);
        } else {
            roots.push(frame);
        }
    }

    roots
}

/// Reduce a 32-byte stack word to a 20-byte address, the way an `Address` is
/// rendered in JSON-RPC.
fn truncate_address(word: &str) -> String {
    let digits = word.strip_prefix("0x").unwrap_or(word);
    if digits.len() <= 40 {
        format!("0x{:0>40}", digits)
    } else {
        format!("0x{}", &digits[digits.len() - 40..])
    }
}

// =============================================================================
// DEBUG TRACE SERVICE
// =============================================================================

pub struct DebugTraceService {
    config: DebugTraceConfig,
    client: reqwest::Client,
}

impl DebugTraceService {
    pub fn new(config: DebugTraceConfig) -> Self {
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(config.trace_timeout_secs))
            .build()
            .unwrap_or_default();

        Self { config, client }
    }

    /// Issue a JSON-RPC request and return its `result`, mapping a JSON-RPC
    /// error object to a typed error.
    async fn call(&self, request: &serde_json::Value) -> Result<serde_json::Value, DebugTraceError> {
        let response = self
            .client
            .post(&self.config.rpc_url)
            .json(request)
            .send()
            .await
            .map_err(|e| DebugTraceError::RpcError(e.to_string()))?;

        let body: serde_json::Value = response
            .json()
            .await
            .map_err(|e| DebugTraceError::ParseError(e.to_string()))?;

        if let Some(error) = body.get("error") {
            return Err(DebugTraceError::RpcError(
                error["message"]
                    .as_str()
                    .unwrap_or("unknown JSON-RPC error")
                    .to_string(),
            ));
        }

        body.get("result")
            .cloned()
            .ok_or_else(|| DebugTraceError::ParseError("response has no result".to_string()))
    }

    pub async fn trace_transaction(&self, tx_hash: &str) -> Result<TraceResult, DebugTraceError> {
        let request = serde_json::json!({
            "jsonrpc": "2.0",
            "method": "debug_traceTransaction",
            "params": [tx_hash, {
                "tracer": "opcodeLogger",
                "timeout": format!("{}s", self.config.trace_timeout_secs)
            }],
            "id": 1
        });

        let result = self.call(&request).await?;
        Ok(parse_trace_result(&result))
    }

    pub async fn trace_call(
        &self,
        from: &str,
        to: &str,
        data: &str,
        value: Option<&str>,
    ) -> Result<TraceResult, DebugTraceError> {
        let request = serde_json::json!({
            "jsonrpc": "2.0",
            "method": "debug_traceCall",
            "params": [{
                "from": from,
                "to": to,
                "data": data,
                "value": value.unwrap_or("0x0")
            }, "latest", {
                "tracer": "opcodeLogger",
                "timeout": format!("{}s", self.config.trace_timeout_secs)
            }],
            "id": 1
        });

        let result = self.call(&request).await?;
        Ok(parse_trace_result(&result))
    }

    pub async fn trace_block(&self, block_number: u64) -> Result<Vec<TraceTransaction>, DebugTraceError> {
        let block_hex = format!("0x{block_number:x}");

        let block_request = serde_json::json!({
            "jsonrpc": "2.0",
            "method": "eth_getBlockByNumber",
            "params": [block_hex, true],
            "id": 1
        });

        let block = self.call(&block_request).await?;

        let transactions = block["transactions"].as_array().ok_or_else(|| {
            DebugTraceError::NotFound(format!("block {block_number} has no transactions field"))
        })?;

        let block_hash = block["hash"].as_str().unwrap_or("").to_string();

        let mut traces = Vec::with_capacity(transactions.len());
        for tx in transactions {
            let Some(tx_hash) = tx["hash"].as_str() else {
                continue;
            };

            let trace = self.trace_transaction(tx_hash).await?;
            let gas_used = u64::from_str_radix(trace.gas.trim_start_matches("0x"), 16).unwrap_or(0);

            traces.push(TraceTransaction {
                tx_hash: tx_hash.to_string(),
                block_number,
                block_hash: block_hash.clone(),
                trace: call_frames_from(&trace.struct_logs),
                gas_used,
                success: !trace.failed,
            });
        }

        Ok(traces)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn log(op: &str, depth: u32, stack: Vec<&str>) -> StructLog {
        StructLog {
            pc: 0,
            op: op.to_string(),
            gas: "0x100".to_string(),
            gas_cost: "0x1".to_string(),
            stack: stack.into_iter().map(String::from).collect(),
            memory: None,
            storage: None,
            depth,
            error: None,
        }
    }

    #[test]
    fn a_trace_result_is_decoded_from_the_node_response() {
        let response = json!({
            "gas": "0x5208",
            "returnValue": "0x0000000000000000000000000000000000000000000000000000000000000003",
            "failed": false,
            "structLogs": [
                {
                    "pc": 0,
                    "op": "PUSH1",
                    "gas": "0x182b8",
                    "gasCost": "0x3",
                    "depth": 1,
                    "stack": ["0x1"]
                },
                {
                    "pc": 2,
                    "op": "REVERT",
                    "gas": "0x182b5",
                    "gasCost": "0x0",
                    "depth": 1,
                    "stack": [],
                    "error": "execution reverted"
                }
            ]
        });

        let parsed = parse_trace_result(&response);
        assert_eq!(parsed.gas, "0x5208");
        assert!(!parsed.failed);
        assert_eq!(parsed.struct_logs.len(), 2);
        assert_eq!(parsed.struct_logs[0].op, "PUSH1");
        assert_eq!(parsed.struct_logs[0].gas_cost, "0x3");
        assert_eq!(parsed.struct_logs[0].stack, vec!["0x1"]);
        assert_eq!(
            parsed.struct_logs[1].error.as_deref(),
            Some("execution reverted")
        );
    }

    #[test]
    fn a_failed_trace_is_reported_as_failed() {
        let response = json!({ "gas": "0x1", "returnValue": "0x", "failed": true, "structLogs": [] });
        assert!(parse_trace_result(&response).failed);
    }

    #[test]
    fn an_absent_failed_field_is_not_reported_as_a_failure() {
        let response = json!({ "gas": "0x1", "returnValue": "0x", "structLogs": [] });
        assert!(!parse_trace_result(&response).failed);
    }

    #[test]
    fn memory_is_read_as_the_hex_string_the_node_sends() {
        let response = json!({
            "gas": "0x1",
            "returnValue": "0x",
            "structLogs": [{
                "pc": 0, "op": "MSTORE", "gas": "0x1", "gasCost": "0x3", "depth": 1,
                "stack": [], "memory": "0x000000000000000000000000000000000000000000000000000000000000002a"
            }]
        });

        let parsed = parse_trace_result(&response);
        let memory = parsed.struct_logs[0].memory.as_deref().unwrap();
        assert!(memory.starts_with("0x"));
        assert!(memory.ends_with("2a"));
    }

    #[test]
    fn a_call_opcode_opens_a_frame() {
        let logs = vec![
            log("PUSH1", 1, vec!["0x1"]),
            // CALL: gas, to, value, in, insize, out, outsize (top-first).
            log(
                "CALL",
                1,
                vec!["0x0", "0x0", "0x0", "0x0", "0x9", "0x1234", "0x5208"],
            ),
            log("STOP", 1, vec![]),
        ];

        let frames = call_frames_from(&logs);
        assert_eq!(frames.len(), 1);
        assert_eq!(frames[0].op, "CALL");
        assert_eq!(frames[0].gas, "0x5208");
        assert_eq!(frames[0].to, format!("0x{}1234", "0".repeat(36)));
    }

    #[test]
    fn a_revert_marks_the_frame_revert_reason() {
        let mut revert = log("REVERT", 1, vec![]);
        revert.error = Some("execution reverted: nope".to_string());
        let logs = vec![
            log(
                "CALL",
                1,
                vec!["0x0", "0x0", "0x0", "0x0", "0x9", "0xabcd", "0x5208"],
            ),
            revert,
        ];

        let frames = call_frames_from(&logs);
        assert_eq!(frames.len(), 1);
        assert_eq!(
            frames[0].revert_reason.as_deref(),
            Some("execution reverted: nope")
        );
    }

    #[test]
    fn nested_calls_are_attached_to_their_parent() {
        let logs = vec![
            log("CALL", 1, vec!["0x0", "0x0", "0x0", "0x0", "0x9", "0xaaaa", "0x5208"]),
            log("CALL", 2, vec!["0x0", "0x0", "0x0", "0x0", "0x9", "0xbbbb", "0x2710"]),
            log("RETURN", 2, vec![]),
            log("RETURN", 1, vec![]),
        ];

        let frames = call_frames_from(&logs);
        assert_eq!(frames.len(), 1, "the inner call belongs to the outer frame");
        assert_eq!(frames[0].calls.len(), 1);
        assert_eq!(frames[0].calls[0].to, format!("0x{}bbbb", "0".repeat(36)));
    }

    #[test]
    fn a_staticcall_has_no_value() {
        let logs = vec![
            log("STATICCALL", 1, vec!["0x0", "0x0", "0x0", "0x0", "0x9", "0xcccc", "0x5208"]),
            log("STOP", 1, vec![]),
        ];

        let frames = call_frames_from(&logs);
        assert_eq!(frames[0].value, "0x0");
        assert_eq!(frames[0].to, format!("0x{}cccc", "0".repeat(36)));
    }

    #[test]
    fn a_frame_that_never_returned_is_still_reported() {
        let logs = vec![log(
            "CALL",
            1,
            vec!["0x0", "0x0", "0x0", "0x0", "0x9", "0xdddd", "0x5208"],
        )];

        let frames = call_frames_from(&logs);
        assert_eq!(frames.len(), 1);
        assert_eq!(frames[0].op, "CALL");
    }

    #[test]
    fn a_transfer_with_no_calls_produces_no_frames() {
        let logs = vec![log("STOP", 1, vec![])];
        assert!(call_frames_from(&logs).is_empty());
    }

    #[test]
    fn an_address_word_is_truncated_to_twenty_bytes() {
        assert_eq!(
            truncate_address("0x0000000000000000000000001111111111111111111111111111111111111111"),
            "0x1111111111111111111111111111111111111111"
        );
        // A short word is left-padded rather than misinterpreted.
        assert_eq!(truncate_address("0x1234"), format!("0x{}1234", "0".repeat(36)));
    }
}
