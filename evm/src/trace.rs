//! Structured, step level tracing of interpreter execution.
//!
//! [`StepTracer`] is passed to [`crate::Executor::execute_traced`] and receives
//! one callback per executed opcode plus a final summary. This is the data
//! source for the `debug_traceTransaction` and `debug_traceCall` APIs, so the
//! trace a client receives is the trace the interpreter actually performed.

use crate::opcodes::gas_cost;
pub use crate::types::FrameKind;
use crate::types::{Address, EvmWord, ExecutionContext, Opcode};

/// A single executed opcode.
#[derive(Debug, Clone)]
pub struct Step {
    /// Program counter of the opcode, before execution.
    pub pc: usize,
    /// Raw opcode byte.
    pub opcode: u8,
    /// Mnemonic, e.g. `PUSH1`.
    pub op: &'static str,
    /// Gas remaining before the opcode ran.
    pub gas: u64,
    /// Gas charged for the opcode.
    pub gas_cost: u64,
    /// Call depth, starting at one.
    pub depth: u32,
    /// Stack contents after execution, bottom first.
    pub stack: Vec<EvmWord>,
    /// Memory contents after execution, or `None` when the tracer did not ask
    /// for them.
    pub memory: Option<Vec<u8>>,
    /// Error raised by the opcode, if any.
    pub error: Option<String>,
}

/// Summary handed to a tracer when execution finishes.
#[derive(Debug, Clone)]
pub struct TraceSummary {
    /// Whether execution completed without a fault.
    pub success: bool,
    /// Total gas charged.
    pub gas_used: u64,
    /// Return data.
    pub output: Vec<u8>,
    /// Contract address being executed.
    pub address: Address,
    /// Number of opcodes executed.
    pub steps: usize,
}

/// A frame that is about to execute.
///
/// This carries what a `callTracer` needs to describe the call and what an
/// opcode tracer needs to attribute steps to a frame. It is passed to
/// [`StepTracer::enter_frame`] before the frame's first opcode runs.
#[derive(Debug, Clone)]
pub struct FrameEnter<'a> {
    /// Which call family opened this frame.
    pub kind: FrameKind,
    /// Account whose code is executing. For `DELEGATECALL` this is the
    /// caller's own address, because the callee runs in the caller's context.
    pub code_address: Address,
    /// Account whose storage is being read and written. For `CALLCODE` and
    /// `DELEGATECALL` this is the caller's address, not the callee's.
    pub storage_address: Address,
    /// Account that initiated the call.
    pub caller: Address,
    /// Value attached to the call, zero when there is none.
    pub value: EvmWord,
    /// Calldata, or init code for a creation.
    pub input: &'a [u8],
    /// Gas made available to the frame.
    pub gas: u64,
    /// Whether this is the transaction's own frame rather than a call.
    pub is_top_level: bool,
}

/// Receives execution events. Implementations decide what to retain, which
/// keeps the interpreter free of allocation when tracing is not requested.
pub trait StepTracer {
    /// Called before an opcode executes.
    fn step(&mut self, step: &Step);
    /// Called once when execution finishes, whether or not it succeeded.
    fn finish(&mut self, summary: &TraceSummary);

    /// Called as a frame begins executing, outermost first.
    ///
    /// A frame is one contract's execution: the top-level call plus each
    /// `CALL`, `CALLCODE`, `DELEGATECALL`, `STATICCALL`, `CREATE` and
    /// `CREATE2` it performs. This is what lets a tracer attribute steps to the
    /// frame they belong to and derive the call tree.
    fn enter_frame(&mut self, _frame: &FrameEnter) {}

    /// Called as a frame finishes, innermost first.
    fn exit_frame(&mut self, _summary: &TraceSummary) {}

    /// Number of frames currently open, one-based.
    ///
    /// The default of one suits a tracer that does not track frames. A tracer
    /// that does should report how many `enter_frame` calls are unmatched.
    fn depth(&self) -> u32 {
        1
    }

    /// Whether a frame is already executing.
    ///
    /// The executor asks this before entering a frame to decide whether the
    /// frame it is about to run is the outermost one. Only the outermost frame
    /// reports `finish`, so a nested call cannot overwrite the top-level
    /// summary with its own.
    fn in_frame(&self) -> bool {
        false
    }

    /// Whether the tracer wants the memory image attached to every step.
    ///
    /// Copying memory on every step is quadratic in the number of steps, so it
    /// is opt-in and defaults to off.
    fn wants_memory(&self) -> bool {
        false
    }
}

/// A tracer that keeps every step, bounded by `capacity` to keep memory use
/// predictable on pathological contracts.
#[derive(Debug)]
pub struct StepTracerBuffer {
    steps: Vec<Step>,
    capacity: usize,
    truncated: bool,
    capture_memory: bool,
    summary: Option<TraceSummary>,
    /// Address of each frame currently executing, outermost first.
    frames: Vec<Address>,
}

impl StepTracerBuffer {
    /// Create a buffer that retains at most `capacity` steps.
    pub fn new(capacity: usize) -> Self {
        Self {
            steps: Vec::new(),
            capacity,
            truncated: false,
            capture_memory: false,
            summary: None,
            frames: Vec::new(),
        }
    }

    /// Create a buffer that also records the memory image of every step.
    pub fn with_memory(capacity: usize) -> Self {
        Self {
            capture_memory: true,
            ..Self::new(capacity)
        }
    }

    /// Retained steps.
    pub fn steps(&self) -> &[Step] {
        &self.steps
    }

    /// Whether steps were dropped because `capacity` was reached.
    pub fn truncated(&self) -> bool {
        self.truncated
    }

    /// Address of each frame currently executing, outermost first.
    pub fn frames(&self) -> &[Address] {
        &self.frames
    }

    /// The final summary, once execution has finished.
    pub fn summary(&self) -> Option<&TraceSummary> {
        self.summary.as_ref()
    }
}

impl StepTracer for StepTracerBuffer {
    fn step(&mut self, step: &Step) {
        if self.steps.len() < self.capacity {
            self.steps.push(step.clone());
        } else {
            self.truncated = true;
        }
    }

    fn enter_frame(&mut self, frame: &FrameEnter) {
        self.frames.push(frame.code_address);
    }

    fn exit_frame(&mut self, _summary: &TraceSummary) {
        self.frames.pop();
    }

    fn depth(&self) -> u32 {
        // One-based, so the outermost frame is depth 1.
        self.frames.len().max(1) as u32
    }

    fn in_frame(&self) -> bool {
        !self.frames.is_empty()
    }

    fn finish(&mut self, summary: &TraceSummary) {
        self.summary = Some(summary.clone());
    }

    fn wants_memory(&self) -> bool {
        self.capture_memory
    }
}

/// Mnemonic for an opcode byte. Unknown bytes are reported as `INVALID`.
pub fn mnemonic(byte: u8) -> &'static str {
    match Opcode::from_u8(byte) {
        Some(op) => opcode_name(op),
        None => "INVALID",
    }
}

/// Mnemonic for a recognised opcode.
pub fn opcode_name(op: Opcode) -> &'static str {
    use Opcode::*;
    match op {
        STOP => "STOP",
        ADD => "ADD",
        MUL => "MUL",
        SUB => "SUB",
        DIV => "DIV",
        SDIV => "SDIV",
        MOD => "MOD",
        SMOD => "SMOD",
        ADDMOD => "ADDMOD",
        MULMOD => "MULMOD",
        EXP => "EXP",
        SIGNEXTEND => "SIGNEXTEND",
        LT => "LT",
        GT => "GT",
        SLT => "SLT",
        SGT => "SGT",
        EQ => "EQ",
        ISZERO => "ISZERO",
        AND => "AND",
        OR => "OR",
        XOR => "XOR",
        NOT => "NOT",
        BYTE => "BYTE",
        SHL => "SHL",
        SHR => "SHR",
        SAR => "SAR",
        SHA3 => "SHA3",
        ADDRESS => "ADDRESS",
        BALANCE => "BALANCE",
        ORIGIN => "ORIGIN",
        CALLER => "CALLER",
        CALLVALUE => "CALLVALUE",
        CALLDATALOAD => "CALLDATALOAD",
        CALLDATASIZE => "CALLDATASIZE",
        CALLDATACOPY => "CALLDATACOPY",
        CODESIZE => "CODESIZE",
        CODECOPY => "CODECOPY",
        GASPRICE => "GASPRICE",
        EXTCODESIZE => "EXTCODESIZE",
        EXTCODECOPY => "EXTCODECOPY",
        RETURNDATASIZE => "RETURNDATASIZE",
        RETURNDATACOPY => "RETURNDATACOPY",
        EXTCODEHASH => "EXTCODEHASH",
        BLOCKHASH => "BLOCKHASH",
        COINBASE => "COINBASE",
        TIMESTAMP => "TIMESTAMP",
        NUMBER => "NUMBER",
        DIFFICULTY => "DIFFICULTY",
        GASLIMIT => "GASLIMIT",
        CHAINID => "CHAINID",
        SELFBALANCE => "SELFBALANCE",
        BASEFEE => "BASEFEE",
        BLOBHASH => "BLOBHASH",
        BLOBBASEFEE => "BLOBBASEFEE",
        POP => "POP",
        MLOAD => "MLOAD",
        MSTORE => "MSTORE",
        MSTORE8 => "MSTORE8",
        SLOAD => "SLOAD",
        SSTORE => "SSTORE",
        JUMP => "JUMP",
        JUMPI => "JUMPI",
        PC => "PC",
        MSIZE => "MSIZE",
        GAS => "GAS",
        JUMPDEST => "JUMPDEST",
        TLOAD => "TLOAD",
        TSTORE => "TSTORE",
        MCOPY => "MCOPY",
        PUSH0 => "PUSH0",
        PUSH1 => "PUSH1",
        PUSH2 => "PUSH2",
        PUSH3 => "PUSH3",
        PUSH4 => "PUSH4",
        PUSH5 => "PUSH5",
        PUSH6 => "PUSH6",
        PUSH7 => "PUSH7",
        PUSH8 => "PUSH8",
        PUSH9 => "PUSH9",
        PUSH10 => "PUSH10",
        PUSH11 => "PUSH11",
        PUSH12 => "PUSH12",
        PUSH13 => "PUSH13",
        PUSH14 => "PUSH14",
        PUSH15 => "PUSH15",
        PUSH16 => "PUSH16",
        PUSH17 => "PUSH17",
        PUSH18 => "PUSH18",
        PUSH19 => "PUSH19",
        PUSH20 => "PUSH20",
        PUSH21 => "PUSH21",
        PUSH22 => "PUSH22",
        PUSH23 => "PUSH23",
        PUSH24 => "PUSH24",
        PUSH25 => "PUSH25",
        PUSH26 => "PUSH26",
        PUSH27 => "PUSH27",
        PUSH28 => "PUSH28",
        PUSH29 => "PUSH29",
        PUSH30 => "PUSH30",
        PUSH31 => "PUSH31",
        PUSH32 => "PUSH32",
        DUP1 => "DUP1",
        DUP2 => "DUP2",
        DUP3 => "DUP3",
        DUP4 => "DUP4",
        DUP5 => "DUP5",
        DUP6 => "DUP6",
        DUP7 => "DUP7",
        DUP8 => "DUP8",
        DUP9 => "DUP9",
        DUP10 => "DUP10",
        DUP11 => "DUP11",
        DUP12 => "DUP12",
        DUP13 => "DUP13",
        DUP14 => "DUP14",
        DUP15 => "DUP15",
        DUP16 => "DUP16",
        SWAP1 => "SWAP1",
        SWAP2 => "SWAP2",
        SWAP3 => "SWAP3",
        SWAP4 => "SWAP4",
        SWAP5 => "SWAP5",
        SWAP6 => "SWAP6",
        SWAP7 => "SWAP7",
        SWAP8 => "SWAP8",
        SWAP9 => "SWAP9",
        SWAP10 => "SWAP10",
        SWAP11 => "SWAP11",
        SWAP12 => "SWAP12",
        SWAP13 => "SWAP13",
        SWAP14 => "SWAP14",
        SWAP15 => "SWAP15",
        SWAP16 => "SWAP16",
        LOG0 => "LOG0",
        LOG1 => "LOG1",
        LOG2 => "LOG2",
        LOG3 => "LOG3",
        LOG4 => "LOG4",
        CREATE => "CREATE",
        CALL => "CALL",
        CALLCODE => "CALLCODE",
        RETURN => "RETURN",
        DELEGATECALL => "DELEGATECALL",
        CREATE2 => "CREATE2",
        STATICCALL => "STATICCALL",
        REVERT => "REVERT",
        INVALID => "INVALID",
        SELFDESTRUCT => "SELFDESTRUCT",
    }
}

/// Static gas cost of an opcode byte, exposing [`gas_cost`] without requiring
/// callers to convert to [`Opcode`] first.
pub fn gas_cost_of(byte: u8) -> u64 {
    match Opcode::from_u8(byte) {
        Some(op) => gas_cost(op),
        None => crate::opcodes::INVALID_OPCODE_GAS,
    }
}

/// Render a stack for a JSON trace response.
pub fn stack_to_hex(stack: &[EvmWord]) -> Vec<String> {
    stack
        .iter()
        .map(|word| format!("0x{}", hex::encode(word.0)))
        .collect()
}

/// Whether the given opcode is a `PUSH1`..=`PUSH32`.
pub fn push_width(byte: u8) -> Option<usize> {
    (0x60..=0x7f).contains(&byte).then(|| (byte - 0x5f) as usize)
}

/// Address of the account a step is executing, for trace framing.
pub fn context_address(ctx: &ExecutionContext) -> Address {
    ctx.address
}

// =============================================================================
// CALL TRACER
// =============================================================================

/// One frame of a call trace, as `debug_traceTransaction` with the
/// `callTracer` reports it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CallTraceFrame {
    /// `CALL`, `DELEGATECALL`, `STATICCALL`, `CREATE` and so on.
    pub call_type: String,
    /// Account the call came from.
    pub from: Address,
    /// Account the call went to. A creation that failed has none, because the
    /// address was never used.
    pub to: Option<Address>,
    /// Value attached to the call, in wei, as a decimal string.
    pub value: String,
    /// Calldata, or init code for a creation.
    pub input: Vec<u8>,
    /// Return data, or the revert payload.
    pub output: Vec<u8>,
    /// Gas made available to the frame.
    pub gas: u64,
    /// Gas the frame consumed.
    pub gas_used: u64,
    /// Why the frame stopped, when it did not complete normally.
    pub error: Option<String>,
    /// Frames this one called, in the order they started.
    pub calls: Vec<CallTraceFrame>,
}

impl CallTraceFrame {
    /// Render the frame as the JSON a `callTracer` returns.
    pub fn to_json(&self) -> serde_json::Value {
        let mut obj = serde_json::Map::new();
        obj.insert("type".into(), serde_json::json!(self.call_type));
        obj.insert("from".into(), serde_json::json!(self.from.to_hex()));
        if let Some(to) = &self.to {
            obj.insert("to".into(), serde_json::json!(to.to_hex()));
        }
        obj.insert("value".into(), serde_json::json!(self.value));
        obj.insert("gas".into(), serde_json::json!(format!("0x{:x}", self.gas)));
        obj.insert(
            "gasUsed".into(),
            serde_json::json!(format!("0x{:x}", self.gas_used)),
        );
        obj.insert(
            "input".into(),
            serde_json::json!(format!("0x{}", hex::encode(&self.input))),
        );
        obj.insert(
            "output".into(),
            serde_json::json!(format!("0x{}", hex::encode(&self.output))),
        );
        if let Some(error) = &self.error {
            obj.insert("error".into(), serde_json::json!(error));
        }
        if !self.calls.is_empty() {
            let calls: Vec<serde_json::Value> =
                self.calls.iter().map(|c| c.to_json()).collect();
            obj.insert("calls".into(), serde_json::json!(calls));
        }
        serde_json::Value::Object(obj)
    }
}

/// Builds a call tree from the frames the interpreter reports.
///
/// The tree is the same structure `debug_traceTransaction` returns with the
/// `callTracer`, and it is derived from the execution this node performed
/// rather than from a second replay: every frame comes from the interpreter
/// entering it, and every field comes from the frame's own context.
///
/// Frames are reported as they are entered and exited, which nests naturally:
/// a frame that is open when another opens is its parent.
#[derive(Debug, Default)]
pub struct CallTracer {
    /// Frames that have not yet finished, outermost first.
    open: Vec<PartialFrame>,
    /// Finished top-level frames, in the order they completed. Normally one.
    roots: Vec<CallTraceFrame>,
    /// Set once a frame has been dropped because the cap was reached.
    truncated: bool,
    /// Maximum frames to retain. Deeper frames are dropped rather than
    /// aborting the trace, so a contract that recurses cannot exhaust memory.
    max_frames: usize,
    /// One entry per frame entered, in entry order, saying whether that frame
    /// was retained. Frames nest strictly, so this pairs each `exit_frame` with
    /// the `enter_frame` that opened it.
    entered: Vec<bool>,
}

/// A frame that has been entered but has not exited yet.
#[derive(Debug)]
struct PartialFrame {
    frame: FrameEnterOwned,
    calls: Vec<CallTraceFrame>,
}

/// [`FrameEnter`] with its borrowed input copied out.
#[derive(Debug)]
struct FrameEnterOwned {
    kind: FrameKind,
    code_address: Address,
    caller: Address,
    value: EvmWord,
    input: Vec<u8>,
    gas: u64,
}

impl CallTracer {
    /// A tracer that retains up to `max_frames` frames.
    pub fn new(max_frames: usize) -> Self {
        Self {
            open: Vec::new(),
            roots: Vec::new(),
            truncated: false,
            max_frames,
            entered: Vec::new(),
        }
    }

    /// The completed trace.
    ///
    /// A trace normally has exactly one root, the transaction's own frame. More
    /// than one is possible only if a frame never reported its exit, and the
    /// roots are then returned in completion order.
    pub fn roots(&self) -> &[CallTraceFrame] {
        &self.roots
    }

    /// The single root, or `None` if no frame completed.
    pub fn root(&self) -> Option<&CallTraceFrame> {
        self.roots.first()
    }

    /// Whether any frame was dropped because the cap was reached.
    pub fn truncated(&self) -> bool {
        self.truncated
    }

    /// The trace as the JSON a `callTracer` returns.
    pub fn to_json(&self) -> serde_json::Value {
        match self.root() {
            Some(root) => root.to_json(),
            None => serde_json::Value::Null,
        }
    }

    /// Decide whether a newly entered frame fits under the cap.
    fn retain(&mut self) -> bool {
        if self.entered.len() >= self.max_frames {
            self.truncated = true;
            self.entered.push(false);
            return false;
        }
        self.entered.push(true);
        true
    }

    /// Convert a finished frame, given what the interpreter reported.
    fn close(&mut self, summary: &TraceSummary) {
        let partial = match self.open.pop() {
            Some(p) => p,
            None => return,
        };
        let info = partial.frame;

        // The interpreter reports success and the return data. A frame that
        // did not succeed either reverted, in which case the payload is the
        // revert reason, or faulted and produced nothing.
        let error = if summary.success {
            None
        } else {
            Some(revert_reason(&summary.output))
        };

        let frame = CallTraceFrame {
            call_type: call_type_name(info.kind).to_string(),
            from: info.caller,
            to: if info.kind == FrameKind::Create || info.kind == FrameKind::Create2 {
                // A creation that succeeded deployed code at the address it
                // ran at; one that failed left nothing there.
                if summary.success {
                    Some(info.code_address)
                } else {
                    None
                }
            } else {
                Some(info.code_address)
            },
            value: info.value.to_decimal_string(),
            input: info.input,
            output: summary.output.clone(),
            gas: info.gas,
            gas_used: summary.gas_used,
            error,
            calls: partial.calls,
        };

        match self.open.last_mut() {
            Some(parent) => parent.calls.push(frame),
            None => self.roots.push(frame),
        }
    }
}

/// The `error` a `callTracer` reports for a frame that did not succeed.
///
/// A revert with a standard payload carries a human-readable reason, and that
/// reason is what a client displays. Anything else is reported the way
/// go-ethereum reports it: `execution reverted` for a revert with no payload or
/// an unrecognised one, and `out of gas` when the frame consumed all its gas.
///
/// The payload is `Error(string)` when its first four bytes are the selector
/// `0x08c379a0`, followed by an ABI-encoded string.
pub fn revert_reason(output: &[u8]) -> String {
    /// `keccak256("Error(string)")[0..4]`.
    const ERROR_STRING_SELECTOR: [u8; 4] = [0x08, 0xc3, 0x79, 0xa0];

    if output.len() < 4 {
        return "execution reverted".to_string();
    }
    if output[..4] != ERROR_STRING_SELECTOR {
        return "execution reverted".to_string();
    }

    // ABI: a 32-byte offset, then a 32-byte length, then the bytes, padded.
    let body = &output[4..];
    if body.len() < 64 {
        return "execution reverted".to_string();
    }
    let offset = match read_u256(&body[..32]) {
        Some(o) => o,
        None => return "execution reverted".to_string(),
    };
    let offset = match usize::try_from(offset) {
        Ok(o) => o,
        Err(_) => return "execution reverted".to_string(),
    };
    if body.len() < offset + 32 {
        return "execution reverted".to_string();
    }
    let len = match read_u256(&body[offset..offset + 32]) {
        Some(l) => l,
        None => return "execution reverted".to_string(),
    };
    let len = match usize::try_from(len) {
        Ok(l) => l,
        Err(_) => return "execution reverted".to_string(),
    };
    let start = offset + 32;
    if body.len() < start + len {
        return "execution reverted".to_string();
    }
    match std::str::from_utf8(&body[start..start + len]) {
        Ok(text) => format!("execution reverted: {text}"),
        Err(_) => "execution reverted".to_string(),
    }
}

/// Read a big-endian 256-bit integer, refusing one that cannot be a length or
/// an offset, which also rules out a hostile payload that would overflow.
fn read_u256(bytes: &[u8]) -> Option<u128> {
    if bytes.len() < 32 {
        return None;
    }
    // A length or offset above u128 is not a real ABI encoding, and refusing it
    // avoids a cast that would silently truncate.
    if bytes[..16].iter().any(|b| *b != 0) {
        return None;
    }
    let mut buf = [0u8; 16];
    buf.copy_from_slice(&bytes[16..32]);
    Some(u128::from_be_bytes(buf))
}

/// The `type` field a `callTracer` reports for a frame.
///
/// A transaction's own frame is reported as `CALL`, the same as an explicit
/// `CALL` opcode, because the tracer has no separate name for it.
fn call_type_name(kind: FrameKind) -> &'static str {
    match kind {
        FrameKind::Call | FrameKind::CallOpcode => "CALL",
        FrameKind::CallCode => "CALLCODE",
        FrameKind::DelegateCall => "DELEGATECALL",
        FrameKind::StaticCall => "STATICCALL",
        FrameKind::Create => "CREATE",
        FrameKind::Create2 => "CREATE2",
    }
}

impl StepTracer for CallTracer {
    fn step(&mut self, _step: &Step) {
        // A call tracer reports structure, not opcodes, so steps are ignored.
        // This is what makes it cheap enough to leave on for every
        // transaction.
    }

    /// The number of frames currently open, one-based.
    fn depth(&self) -> u32 {
        self.open.len().max(1) as u32
    }

    /// Whether a frame is already executing.
    ///
    /// The executor reads this to decide whether the frame it is about to run
    /// is the outermost one. Without it a nested frame would be described as
    /// top-level and would report its own `finish`, so the tree would lose the
    /// distinction between the transaction's frame and the calls it makes.
    fn in_frame(&self) -> bool {
        !self.open.is_empty()
    }

    fn enter_frame(&mut self, frame: &FrameEnter) {
        if !self.retain() {
            return;
        }
        self.open.push(PartialFrame {
            frame: FrameEnterOwned {
                kind: frame.kind,
                code_address: frame.code_address.clone(),
                caller: frame.caller.clone(),
                value: frame.value.clone(),
                input: frame.input.to_vec(),
                gas: frame.gas,
            },
            calls: Vec::new(),
        });
    }

    fn exit_frame(&mut self, summary: &TraceSummary) {
        // A frame the cap refused still reports its exit, so the flag recorded
        // at entry decides whether there is anything to close.
        match self.entered.pop() {
            Some(true) => self.close(summary),
            Some(false) => {}
            None => {}
        }
    }

    fn finish(&mut self, _summary: &TraceSummary) {
        // Any frame still open never reported an exit, which means the
        // interpreter stopped mid-frame. It is closed here so the trace does
        // not silently lose it.
        while !self.open.is_empty() {
            let partial = self.open.pop().expect("checked non-empty");
            let info = partial.frame;
            let frame = CallTraceFrame {
                call_type: call_type_name(info.kind).to_string(),
                from: info.caller,
                to: Some(info.code_address),
                value: info.value.to_decimal_string(),
                input: info.input,
                output: Vec::new(),
                gas: info.gas,
                gas_used: 0,
                error: Some("execution stopped before the frame returned".to_string()),
                calls: partial.calls,
            };
            match self.open.last_mut() {
                Some(parent) => parent.calls.push(frame),
                None => self.roots.push(frame),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Executor;

    #[test]
    fn push_width_is_one_for_push1_and_thirty_two_for_push32() {
        assert_eq!(push_width(0x60), Some(1));
        assert_eq!(push_width(0x7f), Some(32));
        assert_eq!(push_width(0x5f), None);
        assert_eq!(push_width(0x80), None);
    }

    #[test]
    fn unknown_bytes_are_reported_as_invalid() {
        // 0x0c has no assigned opcode.
        assert_eq!(mnemonic(0x0c), "INVALID");
        assert_eq!(mnemonic(0x60), "PUSH1");
        assert_eq!(mnemonic(0x01), "ADD");
    }

    #[test]
    fn a_traced_addition_records_both_operands_and_the_result() {
        // PUSH1 0x02, PUSH1 0x03, ADD, STOP
        let code = [0x60, 0x02, 0x60, 0x03, 0x01, 0x00];
        let mut ctx = ExecutionContext::new();
        ctx.gas = 100_000;

        let mut tracer = StepTracerBuffer::new(64);
        let result = Executor::new().execute_traced(&code, &mut ctx, &mut tracer);

        assert!(result.success);

        let steps = tracer.steps();
        assert_eq!(steps.len(), 4);
        assert_eq!(steps[0].op, "PUSH1");
        assert_eq!(steps[0].pc, 0);
        assert_eq!(steps[1].pc, 2);
        assert_eq!(steps[2].op, "ADD");
        assert_eq!(steps[2].stack.len(), 1);
        assert_eq!(steps[2].stack[0].as_u64(), 5);
        assert_eq!(steps[3].op, "STOP");

        let summary = tracer.summary().expect("summary is recorded");
        assert!(summary.success);
        assert_eq!(summary.steps, 4);
        assert_eq!(summary.gas_used, 3 + 3 + 3);
    }

    #[test]
    fn a_failing_run_reports_the_error_and_still_finishes() {
        // PUSH1 0x01, POP, POP -> stack underflow on the second POP.
        let code = [0x60, 0x01, 0x50, 0x50];
        let mut ctx = ExecutionContext::new();
        ctx.gas = 100_000;

        let mut tracer = StepTracerBuffer::new(16);
        let result = Executor::new().execute_traced(&code, &mut ctx, &mut tracer);

        assert!(!result.success);
        let summary = tracer.summary().expect("summary is recorded even on failure");
        assert!(!summary.success);
    }

    /// A contract that calls another contract and ignores the result.
    ///
    ///   PUSH1 0  PUSH1 0  PUSH1 0  PUSH1 0  PUSH1 0
    ///   PUSH20 <callee>  GAS  CALL  POP  STOP
    fn calls_the_callee(callee: Address) -> Vec<u8> {
        let mut code = vec![0x60, 0x00, 0x60, 0x00, 0x60, 0x00, 0x60, 0x00, 0x60, 0x00];
        code.push(0x73); // PUSH20
        code.extend_from_slice(&callee.0);
        code.push(0x5a); // GAS
        code.push(0xf1); // CALL
        code.push(0x50); // POP
        code.push(0x00); // STOP
        code
    }

    #[test]
    fn a_callee_frame_is_traced_and_its_steps_carry_depth_two() {
        let caller_addr = Address::from_hex("0x00000000000000000000000000000000000000aa").unwrap();
        let callee_addr = Address::from_hex("0x00000000000000000000000000000000000000bb").unwrap();

        // The callee pushes a value then stops.
        let callee_code = vec![0x60, 0x2a, 0x00];

        let mut host = crate::host::MemoryHost::new()
            .with_code(callee_addr.clone(), callee_code)
            .with_balance(caller_addr.clone(), EvmWord::from_u64(0));

        let caller_code = calls_the_callee(callee_addr.clone());

        let mut ctx = ExecutionContext::new();
        ctx.address = caller_addr.clone();
        ctx.gas = 1_000_000;

        let mut tracer = StepTracerBuffer::new(1_000);
        let result =
            Executor::new().execute_traced_with_host(&caller_code, &mut ctx, &mut host, &mut tracer);

        assert!(result.success, "the call should succeed");

        let depths: Vec<u32> = tracer.steps().iter().map(|s| s.depth).collect();
        assert!(
            depths.iter().any(|d| *d == 1),
            "the caller's own opcodes are depth one, got {depths:?}"
        );
        assert!(
            depths.iter().any(|d| *d == 2),
            "the callee's opcodes must be depth two, got {depths:?}"
        );

        // The callee's PUSH1 0x2a ran inside the callee's frame. Without the
        // tracer reaching the child frame these steps do not exist at all.
        let callee_steps = tracer.steps().iter().filter(|s| s.depth == 2).count();
        assert_eq!(
            callee_steps, 2,
            "the callee pushes one value and stops, so it executes two opcodes"
        );

        // Frames close again, so nothing is left on the stack.
        assert!(tracer.frames().is_empty(), "every frame must be closed");
    }

    #[test]
    fn a_tracer_that_ignores_frames_reports_depth_one() {
        // The default trait implementation must stay usable: a tracer that does
        // not track frames still gets a coherent depth.
        struct Plain;
        impl StepTracer for Plain {
            fn step(&mut self, _step: &Step) {}
            fn finish(&mut self, _summary: &TraceSummary) {}
        }
        let mut plain = Plain;
        assert_eq!(plain.depth(), 1);
        let frame = FrameEnter {
            kind: FrameKind::Call,
            code_address: Address::zero(),
            storage_address: Address::zero(),
            caller: Address::zero(),
            value: EvmWord::zero(),
            input: &[],
            gas: 0,
            is_top_level: true,
        };
        plain.enter_frame(&frame);
        assert_eq!(plain.depth(), 1, "the default must not depend on frames");
    }

    #[test]
    fn a_bounded_tracer_drops_steps_instead_of_growing_without_limit() {
        // A long run of PUSH1/POP pairs.
        let mut code = Vec::new();
        for _ in 0..50 {
            code.extend_from_slice(&[0x60, 0x01, 0x50]);
        }
        code.push(0x00);

        let mut ctx = ExecutionContext::new();
        ctx.gas = 1_000_000;

        let mut tracer = StepTracerBuffer::new(10);
        let _ = Executor::new().execute_traced(&code, &mut ctx, &mut tracer);

        assert_eq!(tracer.steps().len(), 10);
        assert!(tracer.truncated());
    }

    #[test]
    fn out_of_gas_stops_execution_and_is_reported() {
        let code = [0x60, 0x01, 0x60, 0x02, 0x01, 0x00];
        let mut ctx = ExecutionContext::new();
        ctx.gas = 5;

        let mut tracer = StepTracerBuffer::new(16);
        let result = Executor::new().execute_traced(&code, &mut ctx, &mut tracer);

        assert!(!result.success);
        assert!(tracer.steps().len() < 6);
    }


    // -------------------------------------------------------------------------
    // Call tracer
    // -------------------------------------------------------------------------

    /// A transaction frame, as the interpreter reports it.
    fn top_frame(caller: &Address, to: &Address, value: u64) -> FrameEnter<'static> {
        // `input` is borrowed, so a static empty slice keeps the lifetime
        // simple for frames that carry no calldata.
        FrameEnter {
            kind: FrameKind::Call,
            code_address: to.clone(),
            storage_address: to.clone(),
            caller: caller.clone(),
            value: EvmWord::from_u64(value),
            input: &[],
            gas: 100_000,
            is_top_level: true,
        }
    }

    fn summary(success: bool, gas_used: u64, output: Vec<u8>, address: &Address) -> TraceSummary {
        TraceSummary {
            success,
            gas_used,
            output,
            address: address.clone(),
            steps: 1,
        }
    }

    #[test]
    fn a_call_tracer_builds_one_frame_from_one_call() {
        let from = Address::from_hex("0x00000000000000000000000000000000000000aa").unwrap();
        let to = Address::from_hex("0x00000000000000000000000000000000000000bb").unwrap();

        let mut tracer = CallTracer::new(64);
        tracer.enter_frame(&top_frame(&from, &to, 7));
        tracer.exit_frame(&summary(true, 21_000, vec![0xde, 0xad], &to));

        let root = tracer.root().expect("the call completed, so there is a root");
        assert_eq!(root.call_type, "CALL");
        assert_eq!(root.from, from);
        assert_eq!(root.to, Some(to.clone()));
        assert_eq!(root.value, "7");
        assert_eq!(root.gas_used, 21_000);
        assert_eq!(root.output, vec![0xde, 0xad]);
        assert!(root.error.is_none());
        assert!(root.calls.is_empty());
        assert!(!tracer.truncated());
    }

    #[test]
    fn nested_calls_are_attached_to_the_frame_that_made_them() {
        let eoa = Address::from_hex("0x00000000000000000000000000000000000000aa").unwrap();
        let router = Address::from_hex("0x00000000000000000000000000000000000000bb").unwrap();
        let token = Address::from_hex("0x00000000000000000000000000000000000000cc").unwrap();

        let mut tracer = CallTracer::new(64);
        tracer.enter_frame(&top_frame(&eoa, &router, 0));
        // The router makes a STATICCALL into the token.
        tracer.enter_frame(&FrameEnter {
            kind: FrameKind::StaticCall,
            code_address: token.clone(),
            storage_address: token.clone(),
            caller: router.clone(),
            value: EvmWord::zero(),
            input: &[],
            gas: 5_000,
            is_top_level: false,
        });
        // The token returns first, then the router.
        tracer.exit_frame(&summary(true, 500, vec![0x01], &token));
        tracer.exit_frame(&summary(true, 1_200, vec![], &router));

        let root = tracer.root().expect("a root frame");
        assert_eq!(root.calls.len(), 1, "the token call is the router's child");
        let child = &root.calls[0];
        assert_eq!(child.call_type, "STATICCALL");
        assert_eq!(child.from, router);
        assert_eq!(child.to, Some(token.clone()));
        assert_eq!(child.gas_used, 500);
        // The child is not also a root.
        assert_eq!(tracer.roots().len(), 1);
    }

    #[test]
    fn a_reverted_frame_reports_its_reason() {
        let from = Address::zero();
        let to = Address::from_hex("0x00000000000000000000000000000000000000bb").unwrap();

        let mut tracer = CallTracer::new(16);
        tracer.enter_frame(&top_frame(&from, &to, 0));
        tracer.exit_frame(&summary(false, 900, Vec::new(), &to));

        let root = tracer.root().expect("a root frame");
        assert_eq!(root.error.as_deref(), Some("execution reverted"));
    }

    /// `Error(string)` with `reason` == `"nope"`, ABI-encoded.
    fn error_string_payload(reason: &str) -> Vec<u8> {
        let mut out = vec![0x08, 0xc3, 0x79, 0xa0];
        let mut offset = [0u8; 32];
        offset[31] = 32;
        out.extend_from_slice(&offset);
        let mut len = [0u8; 32];
        len[31] = reason.len() as u8;
        out.extend_from_slice(&len);
        let mut text = reason.as_bytes().to_vec();
        while text.len() % 32 != 0 {
            text.push(0);
        }
        out.extend_from_slice(&text);
        out
    }

    #[test]
    fn a_standard_revert_payload_is_decoded_into_the_reason() {
        let from = Address::zero();
        let to = Address::from_hex("0x00000000000000000000000000000000000000bb").unwrap();

        let mut tracer = CallTracer::new(16);
        tracer.enter_frame(&top_frame(&from, &to, 0));
        tracer.exit_frame(&summary(false, 900, error_string_payload("nope"), &to));

        let root = tracer.root().expect("a root frame");
        assert_eq!(
            root.error.as_deref(),
            Some("execution reverted: nope"),
            "the payload carries a reason and it must be surfaced"
        );
    }

    #[test]
    fn a_revert_payload_that_is_not_a_reason_does_not_panic() {
        // Selector present but the body is truncated: must not read past the
        // end or claim a reason it does not have.
        let truncated = vec![0x08, 0xc3, 0x79, 0xa0, 0x00, 0x01];
        assert_eq!(revert_reason(&truncated), "execution reverted");

        // A length that runs past the payload must be refused too.
        let mut lying = vec![0x08, 0xc3, 0x79, 0xa0];
        lying.extend_from_slice(&[0u8; 31]);
        lying.push(32); // offset
        lying.extend_from_slice(&[0xff; 32]); // length far beyond the data
        assert_eq!(revert_reason(&lying), "execution reverted");

        // A reason whose bytes are not valid UTF-8 is not displayed as text.
        let mut bad = vec![0x08, 0xc3, 0x79, 0xa0];
        let mut offset = [0u8; 32];
        offset[31] = 32;
        bad.extend_from_slice(&offset);
        let mut len = [0u8; 32];
        len[31] = 2;
        bad.extend_from_slice(&len);
        bad.extend_from_slice(&[0xff, 0xfe]);
        bad.extend_from_slice(&[0u8; 30]);
        assert_eq!(revert_reason(&bad), "execution reverted");
    }

    #[test]
    fn a_failed_creation_has_no_address_but_a_successful_one_does() {
        let deployer = Address::from_hex("0x00000000000000000000000000000000000000aa").unwrap();
        let created = Address::from_hex("0x00000000000000000000000000000000000000dd").unwrap();

        let mut ok = CallTracer::new(8);
        ok.enter_frame(&FrameEnter {
            kind: FrameKind::Create,
            code_address: created.clone(),
            storage_address: created.clone(),
            caller: deployer.clone(),
            value: EvmWord::zero(),
            input: &[0x60, 0x00],
            gas: 50_000,
            is_top_level: true,
        });
        ok.exit_frame(&summary(true, 40_000, vec![0x60, 0x00], &created));
        let root = ok.root().unwrap();
        assert_eq!(root.call_type, "CREATE");
        assert_eq!(root.to, Some(created.clone()));
        assert_eq!(root.input, vec![0x60, 0x00], "init code is the frame input");

        let mut failed = CallTracer::new(8);
        failed.enter_frame(&FrameEnter {
            kind: FrameKind::Create,
            code_address: created.clone(),
            storage_address: created.clone(),
            caller: deployer.clone(),
            value: EvmWord::zero(),
            input: &[0xfe],
            gas: 50_000,
            is_top_level: true,
        });
        failed.exit_frame(&summary(false, 50_000, Vec::new(), &created));
        let root = failed.root().unwrap();
        assert_eq!(
            root.to, None,
            "a creation that failed deployed nothing, so it has no address"
        );
    }

    #[test]
    fn the_frame_cap_drops_deeper_frames_and_says_so() {
        let from = Address::zero();
        let to = Address::from_hex("0x00000000000000000000000000000000000000bb").unwrap();

        let mut tracer = CallTracer::new(2);
        // Three frames nest: the third must be dropped.
        tracer.enter_frame(&top_frame(&from, &to, 0));
        tracer.enter_frame(&FrameEnter {
            kind: FrameKind::CallOpcode,
            code_address: to.clone(),
            storage_address: to.clone(),
            caller: to.clone(),
            value: EvmWord::zero(),
            input: &[],
            gas: 1_000,
            is_top_level: false,
        });
        tracer.enter_frame(&FrameEnter {
            kind: FrameKind::CallOpcode,
            code_address: to.clone(),
            storage_address: to.clone(),
            caller: to.clone(),
            value: EvmWord::zero(),
            input: &[],
            gas: 1_000,
            is_top_level: false,
        });
        // All three exit, including the one that was refused.
        tracer.exit_frame(&summary(true, 10, vec![], &to));
        tracer.exit_frame(&summary(true, 10, vec![], &to));
        tracer.exit_frame(&summary(true, 10, vec![], &to));

        assert!(tracer.truncated(), "dropping a frame must be reported");
        let root = tracer.root().expect("the retained frames still close");
        assert_eq!(root.calls.len(), 1, "only the second frame was retained");
        // The refused frame's exit must not have closed the retained parent.
        assert!(root.calls[0].calls.is_empty());
    }

    #[test]
    fn a_frame_that_never_returned_is_still_reported() {
        let from = Address::zero();
        let to = Address::from_hex("0x00000000000000000000000000000000000000bb").unwrap();

        let mut tracer = CallTracer::new(8);
        tracer.enter_frame(&top_frame(&from, &to, 0));
        // No `exit_frame`: the interpreter stopped mid-frame.
        tracer.finish(&summary(false, 0, Vec::new(), &to));

        let root = tracer.root().expect("an unreturned frame is not dropped");
        assert_eq!(
            root.error.as_deref(),
            Some("execution stopped before the frame returned")
        );
    }

    #[test]
    fn the_trace_renders_as_the_json_a_call_tracer_returns() {
        let from = Address::from_hex("0x00000000000000000000000000000000000000aa").unwrap();
        let to = Address::from_hex("0x00000000000000000000000000000000000000bb").unwrap();

        let mut tracer = CallTracer::new(8);
        tracer.enter_frame(&FrameEnter {
            kind: FrameKind::Call,
            code_address: to.clone(),
            storage_address: to.clone(),
            caller: from.clone(),
            value: EvmWord::from_u64(1_000),
            input: &[0xab, 0xcd],
            gas: 0x5208,
            is_top_level: true,
        });
        tracer.exit_frame(&summary(true, 0x5208, vec![0x01], &to));

        let json = tracer.to_json();
        assert_eq!(json["type"], "CALL");
        assert_eq!(json["from"], "0x00000000000000000000000000000000000000aa");
        assert_eq!(json["to"], "0x00000000000000000000000000000000000000bb");
        assert_eq!(json["value"], "1000");
        assert_eq!(json["gas"], "0x5208");
        assert_eq!(json["gasUsed"], "0x5208");
        assert_eq!(json["input"], "0xabcd");
        assert_eq!(json["output"], "0x01");
        // No `calls` key when there are no children, which is what clients
        // expect from a leaf.
        assert!(json.get("calls").is_none());
    }

    #[test]
    fn an_empty_trace_renders_as_null() {
        let tracer = CallTracer::new(8);
        assert_eq!(tracer.to_json(), serde_json::Value::Null);
        assert!(tracer.root().is_none());
    }

    /// The decisive end-to-end test: a real `CALL` through the interpreter must
    /// produce a nested frame with the callee's address and the caller's
    /// address, and the callee's return data must land in the parent's output.
    #[test]
    fn a_real_nested_call_is_reported_with_both_addresses() {
        let caller_addr = Address::from_hex("0x00000000000000000000000000000000000000aa").unwrap();
        let callee_addr = Address::from_hex("0x00000000000000000000000000000000000000bb").unwrap();

        // The callee returns 32 bytes of 0x2a.
        //   PUSH1 0x2a  PUSH1 0  MSTORE  PUSH1 32  PUSH1 0  RETURN
        let callee_code = vec![
            0x60, 0x2a, 0x60, 0x00, 0x52, 0x60, 0x20, 0x60, 0x00, 0xf3,
        ];

        let mut host = crate::host::MemoryHost::new()
            .with_code(callee_addr.clone(), callee_code)
            .with_balance(caller_addr.clone(), EvmWord::from_u64(0));

        let caller_code = calls_the_callee(callee_addr.clone());

        let mut ctx = ExecutionContext::new();
        ctx.address = caller_addr.clone();
        ctx.caller = caller_addr.clone();
        ctx.gas = 1_000_000;

        let mut tracer = CallTracer::new(64);
        let result =
            Executor::new().execute_traced_with_host(&caller_code, &mut ctx, &mut host, &mut tracer);

        assert!(result.success, "the call should succeed");

        let root = tracer.root().expect("a top-level frame");
        assert_eq!(root.from, caller_addr);
        assert_eq!(root.to, Some(caller_addr.clone()));

        assert_eq!(
            root.calls.len(),
            1,
            "the CALL must produce exactly one child frame"
        );
        let child = &root.calls[0];
        assert_eq!(child.call_type, "CALL");
        assert_eq!(
            child.from, caller_addr,
            "the child's caller is the contract that called it"
        );
        assert_eq!(
            child.to,
            Some(callee_addr.clone()),
            "the child's target is the callee"
        );
        assert_eq!(
            child.output,
            vec![
                0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
                0, 0, 0, 0, 0x2a
            ],
            "the callee's return data must be reported on the child frame"
        );
        assert!(child.error.is_none());
    }

    /// A `DELEGATECALL` runs the callee's code in the caller's storage context.
    ///
    /// The trace names the library as the target, because that is the operand
    /// the caller supplied, and the `DELEGATECALL` type is what tells a reader
    /// that storage belongs to `from` rather than `to`. The decisive check is
    /// the storage itself: the library writes a slot, and that write must land
    /// in the caller's account.
    #[test]
    fn a_delegate_call_writes_into_the_callers_storage_not_the_librarys() {
        let caller_addr = Address::from_hex("0x00000000000000000000000000000000000000aa").unwrap();
        let lib_addr = Address::from_hex("0x00000000000000000000000000000000000000cc").unwrap();

        // The library stores 0x2a at slot 0:
        //   PUSH1 0x2a  PUSH1 0  SSTORE  STOP
        let lib_code = vec![0x60, 0x2a, 0x60, 0x00, 0x55, 0x00];

        let mut host = crate::host::MemoryHost::new()
            .with_code(lib_addr.clone(), lib_code)
            .with_balance(caller_addr.clone(), EvmWord::from_u64(0));

        // PUSH1 0 x4, PUSH20 <lib>, GAS, DELEGATECALL, POP, STOP
        let mut caller_code = vec![0x60, 0x00, 0x60, 0x00, 0x60, 0x00, 0x60, 0x00];
        caller_code.push(0x73);
        caller_code.extend_from_slice(&lib_addr.0);
        caller_code.push(0x5a); // GAS
        caller_code.push(0xf4); // DELEGATECALL
        caller_code.push(0x50); // POP
        caller_code.push(0x00); // STOP

        let mut ctx = ExecutionContext::new();
        ctx.address = caller_addr.clone();
        ctx.caller = caller_addr.clone();
        ctx.gas = 1_000_000;

        let mut tracer = CallTracer::new(64);
        let result =
            Executor::new().execute_traced_with_host(&caller_code, &mut ctx, &mut host, &mut tracer);

        assert!(result.success);
        let root = tracer.root().expect("a top-level frame");
        assert_eq!(root.calls.len(), 1);
        let child = &root.calls[0];
        assert_eq!(child.call_type, "DELEGATECALL");
        assert_eq!(
            child.from, caller_addr,
            "the call was issued by the caller's account"
        );
        assert_eq!(
            child.to,
            Some(lib_addr.clone()),
            "the target is the library the caller named"
        );
        assert_eq!(
            child.value, "0",
            "a DELEGATECALL attaches no value, whatever the parent frame holds"
        );

        // The library's write landed in the caller's storage. This is the
        // property the trace's `type` field exists to convey.
        use crate::host::Host as _;
        let written = host
            .storage(&caller_addr, &EvmWord::zero())
            .to_decimal_string();
        assert_eq!(written, "42", "the library's SSTORE must use the caller's storage");
        assert_eq!(
            host.storage(&lib_addr, &EvmWord::zero()).to_decimal_string(),
            "0",
            "the library's own storage must be untouched"
        );
    }
}

#[cfg(test)]
mod conformance {
    use super::*;

    /// The complete EVM opcode assignment this chain executes. Any byte not
    /// listed here is invalid. Keeping the table in the test (rather than
    /// deriving it from the implementation) is what makes it a conformance
    /// check: a shift in the decode table cannot hide behind itself.
    fn expected(byte: u8) -> Option<&'static str> {
        let name = match byte {
            0x00 => "STOP",
            0x01 => "ADD",
            0x02 => "MUL",
            0x03 => "SUB",
            0x04 => "DIV",
            0x05 => "SDIV",
            0x06 => "MOD",
            0x07 => "SMOD",
            0x08 => "ADDMOD",
            0x09 => "MULMOD",
            0x0a => "EXP",
            0x0b => "SIGNEXTEND",
            0x10 => "LT",
            0x11 => "GT",
            0x12 => "SLT",
            0x13 => "SGT",
            0x14 => "EQ",
            0x15 => "ISZERO",
            0x16 => "AND",
            0x17 => "OR",
            0x18 => "XOR",
            0x19 => "NOT",
            0x1a => "BYTE",
            0x1b => "SHL",
            0x1c => "SHR",
            0x1d => "SAR",
            0x20 => "SHA3",
            0x30 => "ADDRESS",
            0x31 => "BALANCE",
            0x32 => "ORIGIN",
            0x33 => "CALLER",
            0x34 => "CALLVALUE",
            0x35 => "CALLDATALOAD",
            0x36 => "CALLDATASIZE",
            0x37 => "CALLDATACOPY",
            0x38 => "CODESIZE",
            0x39 => "CODECOPY",
            0x3a => "GASPRICE",
            0x3b => "EXTCODESIZE",
            0x3c => "EXTCODECOPY",
            0x3d => "RETURNDATASIZE",
            0x3e => "RETURNDATACOPY",
            0x3f => "EXTCODEHASH",
            0x40 => "BLOCKHASH",
            0x41 => "COINBASE",
            0x42 => "TIMESTAMP",
            0x43 => "NUMBER",
            0x44 => "DIFFICULTY",
            0x45 => "GASLIMIT",
            0x46 => "CHAINID",
            0x47 => "SELFBALANCE",
            0x48 => "BASEFEE",
            0x49 => "BLOBHASH",
            0x4a => "BLOBBASEFEE",
            0x50 => "POP",
            0x51 => "MLOAD",
            0x52 => "MSTORE",
            0x53 => "MSTORE8",
            0x54 => "SLOAD",
            0x55 => "SSTORE",
            0x56 => "JUMP",
            0x57 => "JUMPI",
            0x58 => "PC",
            0x59 => "MSIZE",
            0x5a => "GAS",
            0x5b => "JUMPDEST",
            0x5c => "TLOAD",
            0x5d => "TSTORE",
            0x5e => "MCOPY",
            0x5f => "PUSH0",
            0xf0 => "CREATE",
            0xf1 => "CALL",
            0xf2 => "CALLCODE",
            0xf3 => "RETURN",
            0xf4 => "DELEGATECALL",
            0xf5 => "CREATE2",
            0xfa => "STATICCALL",
            0xfd => "REVERT",
            0xfe => "INVALID",
            0xff => "SELFDESTRUCT",
            0x60..=0x7f => return Some(PUSH_NAMES[(byte - 0x60) as usize]),
            0x80..=0x8f => return Some(DUP_NAMES[(byte - 0x80) as usize]),
            0x90..=0x9f => return Some(SWAP_NAMES[(byte - 0x90) as usize]),
            0xa0..=0xa4 => return Some(LOG_NAMES[(byte - 0xa0) as usize]),
            _ => return None,
        };

        Some(name)
    }

    static PUSH_NAMES: [&str; 32] = [
        "PUSH1", "PUSH2", "PUSH3", "PUSH4", "PUSH5", "PUSH6", "PUSH7", "PUSH8", "PUSH9",
        "PUSH10", "PUSH11", "PUSH12", "PUSH13", "PUSH14", "PUSH15", "PUSH16", "PUSH17",
        "PUSH18", "PUSH19", "PUSH20", "PUSH21", "PUSH22", "PUSH23", "PUSH24", "PUSH25",
        "PUSH26", "PUSH27", "PUSH28", "PUSH29", "PUSH30", "PUSH31", "PUSH32",
    ];
    static DUP_NAMES: [&str; 16] = [
        "DUP1", "DUP2", "DUP3", "DUP4", "DUP5", "DUP6", "DUP7", "DUP8", "DUP9", "DUP10",
        "DUP11", "DUP12", "DUP13", "DUP14", "DUP15", "DUP16",
    ];
    static SWAP_NAMES: [&str; 16] = [
        "SWAP1", "SWAP2", "SWAP3", "SWAP4", "SWAP5", "SWAP6", "SWAP7", "SWAP8", "SWAP9",
        "SWAP10", "SWAP11", "SWAP12", "SWAP13", "SWAP14", "SWAP15", "SWAP16",
    ];
    static LOG_NAMES: [&str; 5] = ["LOG0", "LOG1", "LOG2", "LOG3", "LOG4"];

    #[test]
    fn every_assigned_opcode_decodes_to_the_right_mnemonic() {
        let mut mismatches = Vec::new();

        for byte in 0u8..=0xff {
            let want = expected(byte);
            let got = Opcode::from_u8(byte);

            match (want, got) {
                (Some(name), Some(op)) => {
                    if mnemonic(byte) != name {
                        mismatches.push(format!(
                            "{byte:#04x}: expected {name}, mnemonic reported {}",
                            mnemonic(byte)
                        ));
                    }
                    if opcode_name(op) != name {
                        mismatches.push(format!(
                            "{byte:#04x}: expected {name}, opcode_name reported {}",
                            opcode_name(op)
                        ));
                    }
                }
                (None, Some(op)) => mismatches.push(format!(
                    "{byte:#04x}: unassigned byte decoded to {}",
                    opcode_name(op)
                )),
                (Some(name), None) => {
                    mismatches.push(format!("{byte:#04x}: {name} does not decode"))
                }
                (None, None) => {}
            }
        }

        assert!(
            mismatches.is_empty(),
            "opcode table does not match the specification:\n{}",
            mismatches.join("\n")
        );
    }

    #[test]
    fn no_opcode_decodes_to_a_mnemonic_of_another_opcode() {
        // A shifted table makes two bytes report the same mnemonic, so a
        // duplicate is direct evidence of a misalignment.
        let mut seen: Vec<(&'static str, u8)> = Vec::new();
        let mut duplicates = Vec::new();

        for byte in 0u8..=0xff {
            if let Some(op) = Opcode::from_u8(byte) {
                let name = opcode_name(op);
                if let Some((_, first)) = seen.iter().find(|(n, _)| *n == name) {
                    duplicates.push(format!("{name} claimed by {first:#04x} and {byte:#04x}"));
                } else {
                    seen.push((name, byte));
                }
            }
        }

        assert!(
            duplicates.is_empty(),
            "opcodes are misaligned:\n{}",
            duplicates.join("\n")
        );
    }

    #[test]
    fn gas_cost_of_an_unassigned_byte_is_prohibitive() {
        // 0x0c is unassigned; reaching it must never be free.
        assert_eq!(gas_cost_of(0x0c), crate::opcodes::INVALID_OPCODE_GAS);
        assert!(gas_cost_of(0x01) > 0);
        assert_eq!(gas_cost_of(0x00), 0);
    }
}

