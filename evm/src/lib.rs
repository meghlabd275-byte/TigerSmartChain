//! TigerSmartChain EVM interpreter.
//!
//! Executes EVM bytecode with gas metering and real world-state access via the
//! [`Host`] trait, so balances, code, storage and block hashes come from the
//! node's actual state rather than from placeholders. [`transition`] applies a
//! signed transaction to the world state.

pub mod diff;
pub mod executor;
pub mod host;
pub mod opcodes;
pub mod precompiles;
pub mod trace;
pub mod transition;
pub mod types;

pub use diff::{BalanceDiff, StateDiffRecorder, StorageDiff};
pub use executor::{Executor, NullTracer};
pub use host::{Host, MemoryHost};
pub use opcodes::*;
pub use precompiles::{is_precompile, run as run_precompile, PrecompileError, PrecompileOutput};
pub use trace::{
    mnemonic, opcode_name, revert_reason, CallTraceFrame, CallTracer, FrameEnter, FrameKind,
    Step, StepTracer, StepTracerBuffer, TraceSummary,
};
pub use transition::{
    apply_transaction, apply_transaction_traced, create_address, intrinsic_gas, BlockContext,
    StateHost, TraceDiffs, TransactionRequest, TransitionError, TransitionOutcome,
};
pub use types::*;

// Re-exported so the interpreter can build CREATE addresses without depending
// on the state crate directly.
pub use tiger_state::rlp::RlpItem;
