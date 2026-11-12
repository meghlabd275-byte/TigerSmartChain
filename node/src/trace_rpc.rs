//! The `trace_` JSON-RPC namespace.
//!
//! `trace_transaction` and `trace_block` return Parity's flat trace list, which
//! is the format BSC, Polygon and Base serve and the one a block explorer
//! consumes to draw an internal-transaction table. `trace_replayTransaction`
//! returns the same frames alongside the state writes when asked.
//!
//! The frames come from the same replay `debug_traceTransaction` uses, so the
//! two namespaces cannot disagree about what a transaction did.

use crate::debug_rpc::{build_tracer, replay_transaction, state_diff_to_json, Replay, Tracer};
use crate::node::TigerNode;
use serde_json::{json, Value};
use std::sync::Arc;
use tiger_evm::trace::CallTraceFrame;

/// Maximum transactions traced in one `trace_block` request.
///
/// Every transaction in a block is replayed from the block's starting state,
/// so the work is linear in the block's size. The cap keeps a request for a
/// very large block from occupying a worker indefinitely; a caller can trace
/// the transactions it needs individually.
const MAX_BLOCK_TRACES: usize = 2_000;

/// Render a frame's `value` field, which the call tracer carries as a decimal
/// string, as the hex quantity the `trace_` namespace uses.
fn hex_value(decimal: &str) -> String {
    // The value is at most 256 bits. Rendering it from its decimal digits keeps
    // the conversion exact without pulling in a bignum type.
    let digits = decimal.trim_start_matches('0');
    if digits.is_empty() {
        return "0x0".to_string();
    }
    // Repeatedly divide by 16 to read off hex digits, least significant first.
    // The digits are carried as values rather than ASCII so each pass can feed
    // the next without a conversion.
    let mut buffer: Vec<u8> = digits.bytes().map(|b| b - b'0').collect();
    let mut out = Vec::new();
    while !buffer.is_empty() {
        let mut carry = 0u32;
        let mut quotient = Vec::with_capacity(buffer.len());
        for digit in &buffer {
            let current = carry * 10 + u32::from(*digit);
            let next = current / 16;
            carry = current % 16;
            if !quotient.is_empty() || next != 0 {
                quotient.push(next as u8);
            }
        }
        out.push(std::char::from_digit(carry, 16).expect("carry is a nibble"));
        buffer = quotient;
    }
    out.reverse();
    format!("0x{}", out.into_iter().collect::<String>())
}

/// Render a byte string as `0x`-prefixed hex, or `0x` when empty.
fn hex_bytes(bytes: &[u8]) -> String {
    format!("0x{}", hex::encode(bytes))
}

/// The lowercase `callType` Parity reports inside `action`.
fn call_type_lower(kind: &str) -> String {
    kind.to_ascii_lowercase()
}

/// Whether a frame is a contract creation rather than a call.
fn is_creation(frame: &CallTraceFrame) -> bool {
    matches!(frame.call_type.as_str(), "CREATE" | "CREATE2")
}

/// Render one frame as a Parity `LocalizedTrace`.
///
/// `trace_address` is the path of child indices from the root, which is how
/// that format expresses nesting in a flat list: the transaction's own frame is
/// `[]`, its first child is `[0]`, that child's second child is `[0][1]`.
fn frame_to_trace(
    frame: &CallTraceFrame,
    trace_address: &[usize],
    replay: &Replay,
) -> Value {
    let mut action = serde_json::Map::new();
    action.insert("from".into(), json!(frame.from.to_hex()));
    action.insert("gas".into(), json!(format!("0x{:x}", frame.gas)));
    action.insert("value".into(), json!(hex_value(&frame.value)));

    if is_creation(frame) {
        // A creation has no `to` and no `input`: the init code is the action.
        action.insert("init".into(), json!(hex_bytes(&frame.input)));
        action.insert(
            "creationMethod".into(),
            json!(if frame.call_type == "CREATE2" {
                "create2"
            } else {
                "create"
            }),
        );
    } else {
        action.insert(
            "callType".into(),
            json!(call_type_lower(&frame.call_type)),
        );
        if let Some(to) = &frame.to {
            action.insert("to".into(), json!(to.to_hex()));
        }
        action.insert("input".into(), json!(hex_bytes(&frame.input)));
    }

    let mut result = serde_json::Map::new();
    result.insert(
        "gasUsed".into(),
        json!(format!("0x{:x}", frame.gas_used)),
    );
    if is_creation(frame) {
        if let Some(to) = &frame.to {
            result.insert("address".into(), json!(to.to_hex()));
        }
        // The deployed runtime code is what a creation returns.
        result.insert("code".into(), json!(hex_bytes(&frame.output)));
    } else {
        result.insert("output".into(), json!(hex_bytes(&frame.output)));
    }

    let mut entry = serde_json::Map::new();
    entry.insert("action".into(), Value::Object(action));
    entry.insert("blockHash".into(), json!(replay.block_hash));
    entry.insert("blockNumber".into(), json!(replay.block_number));
    entry.insert("result".into(), Value::Object(result));
    entry.insert("subtraces".into(), json!(frame.calls.len()));
    entry.insert(
        "traceAddress".into(),
        json!(trace_address.iter().map(|i| json!(i)).collect::<Vec<_>>()),
    );
    entry.insert("transactionHash".into(), json!(replay.tx_hash));
    entry.insert("transactionPosition".into(), json!(replay.tx_index));
    entry.insert("type".into(), json!(call_type_lower(&frame.call_type)));
    if frame.error.is_some() {
        // The format reports a short code here and leaves the revert payload in
        // `result.output`, so the reason is already carried there.
        entry.insert("error".into(), json!("Reverted"));
    }

    Value::Object(entry)
}

/// Every frame of the trace as a Parity trace list.
///
/// The order is depth first, which is the order the format requires: a parent
/// always precedes its children, and `traceAddress` carries the nesting.
fn traces_of(replay: &Replay) -> Vec<Value> {
    let root = match replay.tracer_root() {
        Some(root) => root,
        None => return Vec::new(),
    };

    let mut out = Vec::new();
    let mut path = Vec::new();
    walk(root, &mut path, &mut out, replay);
    out
}

fn walk(
    frame: &CallTraceFrame,
    path: &mut Vec<usize>,
    out: &mut Vec<Value>,
    replay: &Replay,
) {
    out.push(frame_to_trace(frame, path, replay));
    for (index, child) in frame.calls.iter().enumerate() {
        path.push(index);
        walk(child, path, out, replay);
        path.pop();
    }
}

impl Replay {
    /// The root call frame, when the replay was traced with a call tracer.
    ///
    /// A `trace_` request that reached this point always used one; the
    /// `Option` keeps the accessor honest rather than panicking if that
    /// invariant is ever broken.
    fn tracer_root(&self) -> Option<&CallTraceFrame> {
        match &self.tracer {
            Tracer::Calls(t) => t.root(),
            Tracer::Opcodes(_) => None,
        }
    }
}

/// Replay a transaction with a call tracer, whatever the caller asked for.
///
/// The `trace_` namespace reports frames, so the opcode logger would produce
/// nothing to report. A request that explicitly names a tracer is refused
/// rather than quietly reinterpreted.
async fn replay_for_traces(
    node: &Arc<TigerNode>,
    tx_hash: &str,
) -> Result<Replay, String> {
    let tracer = build_tracer(Some(&json!({ "tracer": "callTracer" })))?;
    replay_transaction(node, tx_hash, tracer).await
}

/// `trace_transaction`: the frames of one transaction.
pub async fn trace_transaction(
    node: &Arc<TigerNode>,
    tx_hash: &str,
) -> Result<Value, String> {
    let replay = replay_for_traces(node, tx_hash).await?;
    Ok(json!(traces_of(&replay)))
}

/// `trace_block`: the frames of every transaction in a block, in order.
pub async fn trace_block(
    node: &Arc<TigerNode>,
    block_tag: &str,
) -> Result<Value, String> {
    let height = node.get_chain_manager().await.chain().height();
    let number = match block_tag {
        "latest" | "pending" | "safe" | "finalized" => height,
        "earliest" => 0,
        tag => tag
            .strip_prefix("0x")
            .ok_or_else(|| format!("invalid block tag {tag:?}"))?
            .parse::<u64>()
            .map_err(|e| format!("invalid block tag {tag:?}: {e}"))
            .or_else(|_| {
                u64::from_str_radix(tag.trim_start_matches("0x"), 16)
                    .map_err(|e| format!("invalid block tag {tag:?}: {e}"))
            })?,
    };

    let hashes: Vec<String> = {
        let manager = node.get_chain_manager().await;
        let chain = manager.chain();
        chain
            .get_block(number)
            .ok_or_else(|| format!("block {number} not found"))?
            .transactions
            .iter()
            .map(|tx| tx.hash.clone())
            .collect()
    };

    if hashes.len() > MAX_BLOCK_TRACES {
        return Err(format!(
            "block {number} has {} transactions, more than the {MAX_BLOCK_TRACES} this \
             node will trace in one request",
            hashes.len()
        ));
    }

    let mut traces = Vec::new();
    for hash in &hashes {
        let replay = replay_for_traces(node, hash).await?;
        traces.extend(traces_of(&replay));
    }
    Ok(json!(traces))
}

/// `trace_replayTransaction`: the frames of one transaction, plus the state
/// writes when `stateDiff` is among the requested tracers.
pub async fn trace_replay_transaction(
    node: &Arc<TigerNode>,
    tx_hash: &str,
    tracers: Option<&Value>,
) -> Result<Value, String> {
    let wanted = requested_tracers(tracers)?;

    let replay = replay_for_traces(node, tx_hash).await?;
    let mut response = serde_json::Map::new();
    response.insert("output".into(), json!(hex_bytes(&replay.output)));
    response.insert("trace".into(), json!(traces_of(&replay)));

    if wanted.state_diff {
        response.insert("stateDiff".into(), state_diff_to_json(&replay.diffs));
    }
    if wanted.vm_trace {
        // A VM trace is a per-opcode record with its own stack, memory and
        // storage at every step. This node does not produce that format, and
        // answering with something shaped similarly would be worse than
        // refusing.
        return Err(
            "the vmTrace tracer is not implemented; request \"trace\" and \
             optionally \"stateDiff\", or use debug_traceTransaction with \
             \"opcodeLogger\" for per-opcode output"
                .to_string(),
        );
    }

    Ok(Value::Object(response))
}

/// `trace_replayBlockTransactions`: one entry per transaction in a block.
pub async fn trace_replay_block_transactions(
    node: &Arc<TigerNode>,
    block_tag: &str,
    tracers: Option<&Value>,
) -> Result<Value, String> {
    let height = node.get_chain_manager().await.chain().height();
    let number = match block_tag {
        "latest" | "pending" | "safe" | "finalized" => height,
        "earliest" => 0,
        _ => block_tag
            .strip_prefix("0x")
            .ok_or_else(|| format!("invalid block tag {block_tag:?}"))
            .and_then(|digits| {
                u64::from_str_radix(digits, 16)
                    .map_err(|e| format!("invalid block tag {block_tag:?}: {e}"))
            })?,
    };

    let hashes: Vec<String> = {
        let manager = node.get_chain_manager().await;
        let chain = manager.chain();
        chain
            .get_block(number)
            .ok_or_else(|| format!("block {number} not found"))?
            .transactions
            .iter()
            .map(|tx| tx.hash.clone())
            .collect()
    };

    if hashes.len() > MAX_BLOCK_TRACES {
        return Err(format!(
            "block {number} has {} transactions, more than the {MAX_BLOCK_TRACES} this \
             node will trace in one request",
            hashes.len()
        ));
    }

    let mut entries = Vec::with_capacity(hashes.len());
    for hash in &hashes {
        let value = trace_replay_transaction(node, hash, tracers).await?;
        entries.push(value);
    }
    Ok(json!(entries))
}

/// `trace_get`: one frame of one transaction, addressed by its path.
///
/// The path is the same `traceAddress` the list form reports, so a caller can
/// read a list and then fetch a single frame by the address it saw.
pub async fn trace_get(
    node: &Arc<TigerNode>,
    tx_hash: &str,
    trace_address: &Value,
) -> Result<Value, String> {
    let path: Vec<usize> = trace_address
        .as_array()
        .ok_or_else(|| "traceAddress must be an array of indices".to_string())?
        .iter()
        .map(|v| {
            v.as_u64()
                .and_then(|n| usize::try_from(n).ok())
                .ok_or_else(|| format!("traceAddress contains a non-index {v}"))
        })
        .collect::<Result<_, _>>()?;

    let replay = replay_for_traces(node, tx_hash).await?;
    let root = match replay.tracer_root() {
        Some(root) => root,
        None => return Err(format!("transaction {tx_hash} produced no frames")),
    };

    let mut frame = root;
    for (depth, index) in path.iter().enumerate() {
        frame = frame
            .calls
            .get(*index)
            .ok_or_else(|| {
                format!(
                    "no frame at {}: depth {depth} has {} children",
                    format_path(&path),
                    frame.calls.len()
                )
            })?;
    }

    Ok(frame_to_trace(frame, &path, &replay))
}

fn format_path(path: &[usize]) -> String {
    let mut out = String::from("[");
    for (i, index) in path.iter().enumerate() {
        if i > 0 {
            out.push_str(", ");
        }
        out.push_str(&index.to_string());
    }
    out.push(']');
    out
}

/// Which optional pieces a `trace_replay*` request asked for.
#[derive(Debug)]
struct RequestedTracers {
    state_diff: bool,
    vm_trace: bool,
}

/// Parse the tracer list of a `trace_replay*` request.
///
/// The list is optional and defaults to the trace alone. An unrecognised name
/// is refused: silently ignoring it would return a response missing something
/// the caller asked for, without saying so.
fn requested_tracers(tracers: Option<&Value>) -> Result<RequestedTracers, String> {
    let mut wanted = RequestedTracers {
        state_diff: false,
        vm_trace: false,
    };
    let list = match tracers {
        None | Some(Value::Null) => return Ok(wanted),
        Some(Value::Array(list)) => list,
        Some(other) => {
            return Err(format!(
                "the tracer list must be an array of names, got {other}"
            ))
        }
    };

    for name in list {
        let name = name
            .as_str()
            .ok_or_else(|| format!("tracer names must be strings, got {name}"))?;
        match name {
            "trace" => {}
            "stateDiff" => wanted.state_diff = true,
            "vmTrace" => wanted.vm_trace = true,
            other => {
                return Err(format!(
                    "unknown tracer {other:?}; this node supports \"trace\", \"stateDiff\" \
                     and \"vmTrace\" (which is refused rather than approximated)"
                ))
            }
        }
    }
    Ok(wanted)
}

/// Handle a `trace_` method, or return `None` when the method is not one.
pub async fn handle(
    node: &Arc<TigerNode>,
    method: &str,
    params: &Value,
) -> Option<Result<Value, String>> {
    let string_param = |index: usize| -> Result<String, String> {
        params
            .get(index)
            .and_then(|v| v.as_str())
            .map(String::from)
            .ok_or_else(|| format!("{method} needs a string as argument {}", index + 1))
    };

    let result = match method {
        "trace_transaction" => match string_param(0) {
            Ok(hash) => trace_transaction(node, &hash).await,
            Err(e) => Err(e),
        },
        "trace_block" => match string_param(0) {
            Ok(tag) => trace_block(node, &tag).await,
            Err(e) => Err(e),
        },
        "trace_replayTransaction" => match string_param(0) {
            Ok(hash) => trace_replay_transaction(node, &hash, params.get(1)).await,
            Err(e) => Err(e),
        },
        "trace_replayBlockTransactions" => match string_param(0) {
            Ok(tag) => trace_replay_block_transactions(node, &tag, params.get(1)).await,
            Err(e) => Err(e),
        },
        "trace_get" => match (string_param(0), params.get(1)) {
            (Ok(hash), Some(address)) => trace_get(node, &hash, address).await,
            (Err(e), _) => Err(e),
            (_, None) => Err("trace_get needs a transaction hash and a trace address".to_string()),
        },
        _ => return None,
    };
    Some(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::debug_rpc::is_call_tracer;
    use tiger_evm::trace::FrameKind;

    /// Render a 256-bit word as the hex quantity the `trace_` namespace uses.
    ///
    /// The wire format carries values as decimal strings, so production code
    /// converts from those; this renders from a word, which the tests build
    /// directly.
    fn hex_word(value: &tiger_evm::EvmWord) -> String {
        let bytes = &value.0;
        let first = bytes.iter().position(|b| *b != 0);
        match first {
            None => "0x0".to_string(),
            Some(index) => {
                let digits = hex::encode(&bytes[index..]);
                // The most significant byte was non-zero, so trimming cannot
                // empty the string.
                format!("0x{}", digits.trim_start_matches('0'))
            }
        }
    }

    fn frame(
        call_type: &str,
        from: u8,
        to: Option<u8>,
        calls: Vec<CallTraceFrame>,
    ) -> CallTraceFrame {
        CallTraceFrame {
            call_type: call_type.to_string(),
            from: tiger_evm::Address([from; 20]),
            to: to.map(|b| tiger_evm::Address([b; 20])),
            value: "0".to_string(),
            input: Vec::new(),
            output: Vec::new(),
            gas: 0x1000,
            gas_used: 0x100,
            error: None,
            calls,
        }
    }

    /// A minimal `Replay` around a call tracer holding one frame.
    fn replay_with(root: CallTraceFrame) -> Replay {
        let mut tracer = tiger_evm::CallTracer::new(64);
        enter_and_exit(&mut tracer, &root);
        Replay {
            tracer: Tracer::Calls(tracer),
            gas_used: 0,
            diffs: Default::default(),
            output: Vec::new(),
            tx_hash: "0xabc".to_string(),
            block_number: 5,
            block_hash: "0xdef".to_string(),
            tx_index: 2,
        }
    }

    /// Feed a frame and its children through a tracer so the tree is built the
    /// way the interpreter builds it.
    fn enter_and_exit(tracer: &mut tiger_evm::CallTracer, frame: &CallTraceFrame) {
        use tiger_evm::trace::{FrameEnter, StepTracer, TraceSummary};

        let kind = match frame.call_type.as_str() {
            "CREATE" => FrameKind::Create,
            "CREATE2" => FrameKind::Create2,
            "DELEGATECALL" => FrameKind::DelegateCall,
            "STATICCALL" => FrameKind::StaticCall,
            _ => FrameKind::Call,
        };
        let code_address = frame.to.clone().unwrap_or_else(|| frame.from.clone());
        tracer.enter_frame(&FrameEnter {
            kind,
            code_address: code_address.clone(),
            storage_address: code_address.clone(),
            caller: frame.from.clone(),
            value: tiger_evm::EvmWord::zero(),
            input: &frame.input,
            gas: frame.gas,
            is_top_level: true,
        });
        for child in &frame.calls {
            enter_and_exit(tracer, child);
        }
        tracer.exit_frame(&TraceSummary {
            success: frame.error.is_none(),
            gas_used: frame.gas_used,
            output: frame.output.clone(),
            address: code_address,
            steps: 0,
        });
    }

    #[test]
    fn a_single_frame_renders_as_one_parity_trace() {
        let replay = replay_with(frame("CALL", 0xaa, Some(0xbb), vec![]));
        let traces = traces_of(&replay);
        assert_eq!(traces.len(), 1);
        let t = &traces[0];
        assert_eq!(t["type"], "call");
        assert_eq!(t["action"]["callType"], "call");
        assert_eq!(t["action"]["from"], format!("0x{}", "aa".repeat(20)));
        assert_eq!(t["action"]["to"], format!("0x{}", "bb".repeat(20)));
        assert_eq!(t["traceAddress"], json!([]));
        assert_eq!(t["subtraces"], 0);
        assert_eq!(t["blockNumber"], 5);
        assert_eq!(t["transactionHash"], "0xabc");
        assert_eq!(t["transactionPosition"], 2);
        assert_eq!(t["result"]["gasUsed"], "0x100");
    }

    #[test]
    fn nesting_is_expressed_through_the_trace_address() {
        let child = frame("CALL", 0xbb, Some(0xcc), vec![]);
        let root = frame("CALL", 0xaa, Some(0xbb), vec![child]);
        let replay = replay_with(root);

        let traces = traces_of(&replay);
        assert_eq!(traces.len(), 2, "the parent and its child");
        assert_eq!(traces[0]["traceAddress"], json!([]));
        assert_eq!(traces[0]["subtraces"], 1);
        assert_eq!(
            traces[1]["traceAddress"],
            json!([0]),
            "the child is the parent's first child"
        );
    }

    #[test]
    fn a_grandchild_gets_a_two_element_trace_address() {
        let grandchild = frame("STATICCALL", 0xcc, Some(0xdd), vec![]);
        let child = frame("CALL", 0xbb, Some(0xcc), vec![grandchild]);
        let root = frame("CALL", 0xaa, Some(0xbb), vec![child]);
        let replay = replay_with(root);

        let traces = traces_of(&replay);
        assert_eq!(traces.len(), 3);
        assert_eq!(traces[0]["traceAddress"], json!([]));
        assert_eq!(traces[1]["traceAddress"], json!([0]));
        assert_eq!(traces[2]["traceAddress"], json!([0, 0]));
        assert_eq!(traces[2]["type"], "staticcall");
    }

    #[test]
    fn a_second_child_is_indexed_one() {
        let first = frame("CALL", 0xbb, Some(0x01), vec![]);
        let second = frame("CALL", 0xbb, Some(0x02), vec![]);
        let root = frame("CALL", 0xaa, Some(0xbb), vec![first, second]);
        let replay = replay_with(root);

        let traces = traces_of(&replay);
        assert_eq!(traces[1]["traceAddress"], json!([0]));
        assert_eq!(traces[2]["traceAddress"], json!([1]));
    }

    #[test]
    fn a_creation_renders_with_init_and_code_rather_than_input_and_output() {
        let mut created = frame("CREATE", 0xaa, Some(0xbb), vec![]);
        created.input = vec![0x60, 0x00];
        created.output = vec![0x60, 0x2a];
        let replay = replay_with(created);

        let t = &traces_of(&replay)[0];
        assert_eq!(t["type"], "create");
        assert_eq!(t["action"]["creationMethod"], "create");
        assert_eq!(t["action"]["init"], "0x6000");
        assert_eq!(t["result"]["address"], format!("0x{}", "bb".repeat(20)));
        assert_eq!(t["result"]["code"], "0x602a");
        // A creation has neither of these; reporting them would misdescribe it.
        assert!(t["action"].get("input").is_none());
        assert!(t["action"].get("callType").is_none());
        assert!(t["result"].get("output").is_none());
    }

    #[test]
    fn a_failed_frame_reports_reverted() {
        let mut failed = frame("CALL", 0xaa, Some(0xbb), vec![]);
        failed.error = Some("execution reverted: nope".to_string());
        let replay = replay_with(failed);
        assert_eq!(traces_of(&replay)[0]["error"], "Reverted");
    }

    #[test]
    fn a_zero_value_is_reported_as_a_bare_zero_quantity() {
        // Parity reports quantities without leading zeros, so a zero is `0x0`
        // rather than a full 32-byte word.
        assert_eq!(hex_word(&tiger_evm::EvmWord::zero()), "0x0");
        assert_eq!(hex_word(&tiger_evm::EvmWord::from_u64(255)), "0xff");
    }

    #[test]
    fn a_value_too_large_for_u64_is_rendered_exactly() {
        // 2^128, which a u64 path would silently truncate.
        let mut bytes = [0u8; 32];
        bytes[15] = 1;
        let word = tiger_evm::EvmWord(bytes);
        assert_eq!(hex_word(&word), "0x100000000000000000000000000000000");
        assert_eq!(
            hex_value("340282366920938463463374607431768211456"),
            "0x100000000000000000000000000000000",
            "a decimal value renders to the same quantity"
        );
    }

    #[test]
    fn a_decimal_value_renders_as_a_hex_quantity() {
        assert_eq!(hex_value("0"), "0x0");
        assert_eq!(hex_value("1"), "0x1");
        assert_eq!(hex_value("15"), "0xf");
        assert_eq!(hex_value("16"), "0x10");
        assert_eq!(hex_value("1000"), "0x3e8");
        // 10^18 wei, one ether.
        assert_eq!(hex_value("1000000000000000000"), "0xde0b6b3a7640000");
    }

    #[test]
    fn the_default_tracer_list_asks_for_the_trace_alone() {
        let wanted = requested_tracers(None).unwrap();
        assert!(!wanted.state_diff);
        assert!(!wanted.vm_trace);

        let wanted = requested_tracers(Some(&json!(["trace"]))).unwrap();
        assert!(!wanted.state_diff);
    }

    #[test]
    fn state_diff_is_recognised_and_vm_trace_is_recorded() {
        let wanted = requested_tracers(Some(&json!(["trace", "stateDiff"]))).unwrap();
        assert!(wanted.state_diff);
        assert!(!wanted.vm_trace);

        let wanted = requested_tracers(Some(&json!(["vmTrace"]))).unwrap();
        assert!(wanted.vm_trace);
    }

    #[test]
    fn an_unknown_tracer_name_is_refused() {
        let err = match requested_tracers(Some(&json!(["trace", "nonsense"]))) {
            Ok(_) => panic!("an unknown tracer must be refused"),
            Err(e) => e,
        };
        assert!(err.contains("nonsense"), "the error names it: {err}");
    }

    #[test]
    fn a_tracer_list_that_is_not_an_array_is_refused() {
        let err = match requested_tracers(Some(&json!("trace"))) {
            Ok(_) => panic!("a bare string is not a tracer list"),
            Err(e) => e,
        };
        assert!(err.contains("array"), "{err}");
    }

    // -------------------------------------------------------------------------
    // End to end, against a real node
    // -------------------------------------------------------------------------

    use crate::debug_rpc::tests::{account_with_balance, account_with_code, address_of, node_with_blocks, signed_tx};

    /// A transaction that calls a contract which calls another contract.
    ///
    /// The caller transfers 7 wei to the callee and returns. Both frames must
    /// appear, the child with the callee's address.
    #[tokio::test]
    async fn trace_transaction_reports_a_real_nested_call() {
        let key = [0x71u8; 32];
        let callee = [0x72u8; 20];
        let callee_hex = format!("0x{}", tiger_blockchain::hex_encode(&callee));

        // The callee stops immediately.
        let callee_code = vec![0x00];

        let caller = [0x73u8; 20];
        let caller_hex = format!("0x{}", tiger_blockchain::hex_encode(&caller));
        // CALL takes gas, to, value and four offsets, so the operands are
        // pushed in the reverse of that order: four zeros, the value, the
        // callee, then the gas.
        let mut caller_code = vec![0x60, 0x00, 0x60, 0x00, 0x60, 0x00, 0x60, 0x00];
        caller_code.push(0x60);
        caller_code.push(0x07); // value 7
        caller_code.push(0x73); // PUSH20
        caller_code.extend_from_slice(&callee);
        caller_code.push(0x5a); // GAS
        caller_code.push(0xf1); // CALL
        caller_code.push(0x50); // POP
        caller_code.push(0x00); // STOP

        let tx = signed_tx(&key, 0, &caller_hex, "0x0", 200_000, &[]);

        let node = node_with_blocks(
            vec![
                (address_of(&key).as_str(), account_with_balance(1_000_000)),
                (caller_hex.as_str(), account_with_code(1_000, &caller_code)),
                (callee_hex.as_str(), account_with_code(0, &callee_code)),
            ],
            vec![vec![tx.clone()]],
        )
        .await;

        let traces = trace_transaction(&node, &tx.hash).await.unwrap();
        let traces = traces.as_array().expect("a list of traces");
        assert_eq!(
            traces.len(),
            2,
            "the transaction's own frame and the call it made: {traces:?}"
        );

        assert_eq!(traces[0]["traceAddress"], json!([]));
        assert_eq!(traces[0]["type"], "call");
        assert_eq!(traces[0]["subtraces"], 1);
        assert_eq!(traces[0]["action"]["to"], caller_hex);

        assert_eq!(traces[1]["traceAddress"], json!([0]));
        assert_eq!(
            traces[1]["action"]["to"], callee_hex,
            "the child frame names the callee"
        );
        assert_eq!(
            traces[1]["action"]["value"], "0x7",
            "the seven wei the caller attached"
        );
        assert_eq!(traces[1]["transactionHash"], tx.hash);
        assert_eq!(traces[1]["blockNumber"], 1);
    }

    #[tokio::test]
    async fn trace_block_reports_every_transaction_in_order() {
        let key_a = [0x81u8; 32];
        let key_b = [0x82u8; 32];
        let recipient = format!("0x{}", "33".repeat(20));

        let tx_a = signed_tx(&key_a, 0, &recipient, "0x1", 21_000, &[]);
        let tx_b = signed_tx(&key_b, 0, &recipient, "0x2", 21_000, &[]);

        let node = node_with_blocks(
            vec![
                (address_of(&key_a).as_str(), account_with_balance(1_000_000)),
                (address_of(&key_b).as_str(), account_with_balance(1_000_000)),
            ],
            vec![vec![tx_a.clone(), tx_b.clone()]],
        )
        .await;

        let traces = trace_block(&node, "0x1").await.unwrap();
        let traces = traces.as_array().unwrap();
        assert_eq!(traces.len(), 2, "one frame per transaction");
        assert_eq!(traces[0]["transactionHash"], tx_a.hash);
        assert_eq!(traces[1]["transactionHash"], tx_b.hash);
        assert_eq!(
            traces[1]["transactionPosition"], 1,
            "the second transaction is at position one"
        );
    }

    #[tokio::test]
    async fn trace_replay_transaction_reports_the_state_writes() {
        let key = [0x91u8; 32];
        let contract = [0x92u8; 20];
        let contract_hex = format!("0x{}", tiger_blockchain::hex_encode(&contract));

        // PUSH1 0x2a, PUSH1 0x00, SSTORE, PUSH1 0x2a, PUSH1 0x00, MSTORE,
        // PUSH1 0x20, PUSH1 0x00, RETURN: writes a slot and returns a word, so
        // the response must carry both.
        let code = vec![
            0x60, 0x2a, 0x60, 0x00, 0x55, 0x60, 0x2a, 0x60, 0x00, 0x52, 0x60, 0x20, 0x60, 0x00,
            0xf3,
        ];
        let tx = signed_tx(&key, 0, &contract_hex, "0x0", 100_000, &[]);

        let node = node_with_blocks(
            vec![
                (address_of(&key).as_str(), account_with_balance(1_000_000)),
                (contract_hex.as_str(), account_with_code(0, &code)),
            ],
            vec![vec![tx.clone()]],
        )
        .await;

        let response = trace_replay_transaction(&node, &tx.hash, Some(&json!(["trace", "stateDiff"])))
            .await
            .unwrap();
        assert_eq!(response["trace"].as_array().unwrap().len(), 1);
        // The transaction returned a word; reporting an empty output would
        // discard the one thing a caller cannot reconstruct from the frames.
        assert_eq!(
            response["output"],
            json!(format!("0x{}2a", "00".repeat(31))),
            "the return data must be the transaction's own"
        );
        // The contract wrote slot zero, and the write must be reported.
        assert!(
            response["stateDiff"][&contract_hex].is_object(),
            "the contract's storage write must appear: {}",
            response["stateDiff"]
        );
    }

    #[tokio::test]
    async fn trace_get_fetches_a_frame_by_its_address() {
        let key = [0xa1u8; 32];
        let callee = [0xa2u8; 20];
        let callee_hex = format!("0x{}", tiger_blockchain::hex_encode(&callee));
        let caller = [0xa3u8; 20];
        let caller_hex = format!("0x{}", tiger_blockchain::hex_encode(&caller));

        // Five zeros: the value is zero too, so this is a plain call.
        let mut caller_code = vec![0x60, 0x00, 0x60, 0x00, 0x60, 0x00, 0x60, 0x00, 0x60, 0x00];
        caller_code.push(0x73);
        caller_code.extend_from_slice(&callee);
        caller_code.push(0x5a);
        caller_code.push(0xf1);
        caller_code.push(0x50);
        caller_code.push(0x00);

        let tx = signed_tx(&key, 0, &caller_hex, "0x0", 200_000, &[]);
        let node = node_with_blocks(
            vec![
                (address_of(&key).as_str(), account_with_balance(1_000_000)),
                (caller_hex.as_str(), account_with_code(0, &caller_code)),
                (callee_hex.as_str(), account_with_code(0, &[0x00])),
            ],
            vec![vec![tx.clone()]],
        )
        .await;

        // The root frame.
        let root = trace_get(&node, &tx.hash, &json!([])).await.unwrap();
        assert_eq!(root["traceAddress"], json!([]));
        assert_eq!(root["action"]["to"], caller_hex);

        // Its first child.
        let child = trace_get(&node, &tx.hash, &json!([0])).await.unwrap();
        assert_eq!(child["traceAddress"], json!([0]));
        assert_eq!(child["action"]["to"], callee_hex);

        // A path that does not exist is refused, and the message says why.
        let err = trace_get(&node, &tx.hash, &json!([9]))
            .await
            .expect_err("there is no tenth child");
        assert!(err.contains("no frame"), "{err}");
    }

    #[tokio::test]
    async fn a_creation_is_reported_as_a_create_trace() {
        let key = [0xb1u8; 32];
        // PUSH1 0x00, PUSH1 0x00, RETURN: deploys empty code, which is enough
        // to prove the creation frame is reported.
        let init_code = vec![0x60, 0x00, 0x60, 0x00, 0xf3];
        let tx = signed_tx(&key, 0, "", "0x0", 200_000, &init_code);

        let node = node_with_blocks(
            vec![(address_of(&key).as_str(), account_with_balance(1_000_000))],
            vec![vec![tx.clone()]],
        )
        .await;

        let traces = trace_transaction(&node, &tx.hash).await.unwrap();
        let traces = traces.as_array().unwrap();
        assert_eq!(traces.len(), 1, "a creation makes one frame");
        assert_eq!(traces[0]["type"], "create");
        assert_eq!(traces[0]["action"]["creationMethod"], "create");
        assert_eq!(traces[0]["action"]["init"], "0x60006000f3");
        assert!(
            traces[0]["result"]["address"].is_string(),
            "a successful creation reports the address it deployed"
        );
    }

    #[test]
    fn an_opcode_tracer_has_no_root_frame_to_report() {
        // A `trace_` request always builds a call tracer, so this guards the
        // accessor rather than a reachable path.
        let replay = Replay {
            tracer: build_tracer(None).unwrap(),
            gas_used: 0,
            diffs: Default::default(),
            output: Vec::new(),
            tx_hash: "0x1".to_string(),
            block_number: 0,
            block_hash: "0x0".to_string(),
            tx_index: 0,
        };
        assert!(!is_call_tracer(&replay.tracer));
        assert!(traces_of(&replay).is_empty());
    }
}
