//! Structured, step level tracing of interpreter execution.
//!
//! [`StepTracer`] is passed to [`crate::Executor::execute_traced`] and receives
//! one callback per executed opcode plus a final summary. This is the data
//! source for the `debug_traceTransaction` and `debug_traceCall` APIs, so the
//! trace a client receives is the trace the interpreter actually performed.

use crate::opcodes::gas_cost;
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

/// Receives execution events. Implementations decide what to retain, which
/// keeps the interpreter free of allocation when tracing is not requested.
pub trait StepTracer {
    /// Called before an opcode executes.
    fn step(&mut self, step: &Step);
    /// Called once when execution finishes, whether or not it succeeded.
    fn finish(&mut self, summary: &TraceSummary);

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
