//! EVM Opcodes

use crate::types::*;

// =============================================================================
// OPCODE HELPERS
// =============================================================================

/// Gas cost charged for an opcode that is invalid, unknown, or unimplemented.
/// Per the EVM specification 0xfe (INVALID) consumes *all* remaining gas; we
/// model that with a single prohibitive constant so any transaction reaching
/// such an opcode can never run for free (free-work DoS surface). The value is
/// deliberately larger than any realistic per-opcode block gas limit, so a
/// `checked_add` against the transaction gas budget will always trip an
/// out-of-gas halt before the opcode can do any work.
pub const INVALID_OPCODE_GAS: u64 = 1_000_000;

/// Gas costs (Shanghai-era static base costs).
pub struct GasCost;

impl GasCost {
    pub const STOP: u64 = 0;
    pub const ADD: u64 = 3;
    pub const MUL: u64 = 5;
    pub const SUB: u64 = 3;
    pub const DIV: u64 = 5;
    pub const SDIV: u64 = 5;
    pub const MOD: u64 = 5;
    pub const SMOD: u64 = 5;
    pub const ADDMOD: u64 = 8;
    pub const MULMOD: u64 = 8;
    pub const EXP: u64 = 10;
    pub const SIGNEXTEND: u64 = 5;
    pub const LT: u64 = 3;
    pub const GT: u64 = 3;
    pub const SLT: u64 = 3;
    pub const SGT: u64 = 3;
    pub const EQ: u64 = 3;
    pub const ISZERO: u64 = 3;
    pub const AND: u64 = 3;
    pub const OR: u64 = 3;
    pub const XOR: u64 = 3;
    pub const NOT: u64 = 3;
    pub const BYTE: u64 = 3;
    pub const SHL: u64 = 3;
    pub const SHR: u64 = 3;
    pub const SAR: u64 = 3;
    pub const SHA3: u64 = 30;
    pub const ADDRESS: u64 = 2;
    pub const BALANCE: u64 = 700;
    pub const ORIGIN: u64 = 2;
    pub const CALLER: u64 = 2;
    pub const CALLVALUE: u64 = 2;
    pub const CALLDATASIZE: u64 = 2;
    pub const CALLDATACOPY: u64 = 3;
    pub const CODESIZE: u64 = 2;
    pub const CODECOPY: u64 = 3;
    pub const GASPRICE: u64 = 2;
    pub const EXTCODESIZE: u64 = 700;
    pub const EXTCODECOPY: u64 = 700;
    pub const RETURNDATASIZE: u64 = 2;
    pub const RETURNDATACOPY: u64 = 3;
    pub const EXTCODEHASH: u64 = 700;
    pub const BLOCKHASH: u64 = 20;
    pub const COINBASE: u64 = 2;
    pub const TIMESTAMP: u64 = 2;
    pub const NUMBER: u64 = 2;
    pub const DIFFICULTY: u64 = 2;
    pub const GASLIMIT: u64 = 2;
    pub const CHAINID: u64 = 2;
    pub const BASEFEE: u64 = 2;
    pub const POP: u64 = 2;
    pub const MLOAD: u64 = 3;
    pub const MSTORE: u64 = 3;
    pub const MSTORE8: u64 = 3;
    pub const SLOAD: u64 = 100;
    pub const SSTORE: u64 = 20_000;
    pub const JUMP: u64 = 8;
    pub const JUMPI: u64 = 10;
    pub const PC: u64 = 2;
    pub const MSIZE: u64 = 2;
    pub const GAS: u64 = 2;
    pub const JUMPDEST: u64 = 1;
    pub const PUSH1: u64 = 3;
    pub const DUP1: u64 = 3;
    pub const SWAP1: u64 = 3;
    pub const LOG0: u64 = 375;
    pub const LOG1: u64 = 750;
    pub const LOG2: u64 = 1125;
    pub const LOG3: u64 = 1500;
    pub const LOG4: u64 = 1875;
    pub const CREATE: u64 = 32000;
    pub const CALL: u64 = 700;
    pub const CALLCODE: u64 = 700;
    pub const DELEGATECALL: u64 = 700;
    pub const CREATE2: u64 = 32000;
    pub const STATICCALL: u64 = 700;
    pub const RETURN: u64 = 0;
    pub const REVERT: u64 = 0;
    /// 0xfe INVALID: per spec consumes *all* remaining gas. We model that with
    /// the prohibitive invalid-opcode cost so a transaction can never execute
    /// an invalid opcode for free.
    pub const INVALID: u64 = INVALID_OPCODE_GAS;
    /// 0xff SELFDESTRUCT: base cost 5000 (EIP-150). An additional 25_000 applies
    /// when sending to a previously-empty account; that dynamic surcharge is
    /// not represented by this static base constant.
    pub const SELFDESTRUCT: u64 = 5_000;
}

/// Get gas cost for opcode. Every opcode has a real, non-zero cost where the
/// EVM spec mandates one; unknown/unimplemented opcodes cost a prohibitive
/// amount so they can never be used for a free-work DoS.
pub fn gas_cost(opcode: Opcode) -> u64 {
    use Opcode::*;
    match opcode {
        STOP => GasCost::STOP,
        ADD => GasCost::ADD,
        MUL => GasCost::MUL,
        SUB => GasCost::SUB,
        DIV => GasCost::DIV,
        SDIV => GasCost::SDIV,
        MOD => GasCost::MOD,
        SMOD => GasCost::SMOD,
        ADDMOD => GasCost::ADDMOD,
        MULMOD => GasCost::MULMOD,
        EXP => GasCost::EXP,
        SIGNEXTEND => GasCost::SIGNEXTEND,
        LT => GasCost::LT,
        GT => GasCost::GT,
        SLT => GasCost::SLT,
        SGT => GasCost::SGT,
        EQ => GasCost::EQ,
        ISZERO => GasCost::ISZERO,
        AND => GasCost::AND,
        OR => GasCost::OR,
        XOR => GasCost::XOR,
        NOT => GasCost::NOT,
        BYTE => GasCost::BYTE,
        SHL => GasCost::SHL,
        SHR => GasCost::SHR,
        SAR => GasCost::SAR,
        SHA3 => GasCost::SHA3,
        ADDRESS => GasCost::ADDRESS,
        BALANCE => GasCost::BALANCE,
        ORIGIN => GasCost::ORIGIN,
        CALLER => GasCost::CALLER,
        CALLVALUE => GasCost::CALLVALUE,
        CALLDATASIZE => GasCost::CALLDATASIZE,
        CALLDATACOPY => GasCost::CALLDATACOPY,
        CODESIZE => GasCost::CODESIZE,
        CODECOPY => GasCost::CODECOPY,
        GASPRICE => GasCost::GASPRICE,
        EXTCODESIZE => GasCost::EXTCODESIZE,
        EXTCODECOPY => GasCost::EXTCODECOPY,
        RETURNDATASIZE => GasCost::RETURNDATASIZE,
        RETURNDATACOPY => GasCost::RETURNDATACOPY,
        EXTCODEHASH => GasCost::EXTCODEHASH,
        BLOCKHASH => GasCost::BLOCKHASH,
        COINBASE => GasCost::COINBASE,
        TIMESTAMP => GasCost::TIMESTAMP,
        NUMBER => GasCost::NUMBER,
        DIFFICULTY => GasCost::DIFFICULTY,
        GASLIMIT => GasCost::GASLIMIT,
        CHAINID => GasCost::CHAINID,
        BASEFEE => GasCost::BASEFEE,
        POP => GasCost::POP,
        MLOAD => GasCost::MLOAD,
        MSTORE => GasCost::MSTORE,
        MSTORE8 => GasCost::MSTORE8,
        SLOAD => GasCost::SLOAD,
        SSTORE => GasCost::SSTORE,
        JUMP => GasCost::JUMP,
        JUMPI => GasCost::JUMPI,
        PC => GasCost::PC,
        MSIZE => GasCost::MSIZE,
        GAS => GasCost::GAS,
        JUMPDEST => GasCost::JUMPDEST,
        // PUSH1..PUSH32 all cost 3
        PUSH1 | PUSH2 | PUSH3 | PUSH4 | PUSH5 | PUSH6 | PUSH7 | PUSH8
        | PUSH9 | PUSH10 | PUSH11 | PUSH12 | PUSH13 | PUSH14 | PUSH15 | PUSH16
        | PUSH17 | PUSH18 | PUSH19 | PUSH20 | PUSH21 | PUSH22 | PUSH23 | PUSH24
        | PUSH25 | PUSH26 | PUSH27 | PUSH28 | PUSH29 | PUSH30 | PUSH31 | PUSH32 => 3,
        // DUP1..DUP16 all cost 3
        DUP1 | DUP2 | DUP3 | DUP4 | DUP5 | DUP6 | DUP7 | DUP8
        | DUP9 | DUP10 | DUP11 | DUP12 | DUP13 | DUP14 | DUP15 | DUP16 => 3,
        // SWAP1..SWAP16 all cost 3
        SWAP1 | SWAP2 | SWAP3 | SWAP4 | SWAP5 | SWAP6 | SWAP7 | SWAP8
        | SWAP9 | SWAP10 | SWAP11 | SWAP12 | SWAP13 | SWAP14 | SWAP15 | SWAP16 => 3,
        LOG0 => GasCost::LOG0,
        LOG1 => GasCost::LOG1,
        LOG2 => GasCost::LOG2,
        LOG3 => GasCost::LOG3,
        LOG4 => GasCost::LOG4,
        CREATE => GasCost::CREATE,
        CALL => GasCost::CALL,
        CALLCODE => GasCost::CALLCODE,
        DELEGATECALL => GasCost::DELEGATECALL,
        CREATE2 => GasCost::CREATE2,
        STATICCALL => GasCost::STATICCALL,
        RETURN => GasCost::RETURN,
        REVERT => GasCost::REVERT,
        INVALID => GasCost::INVALID,
        SELFDESTRUCT => GasCost::SELFDESTRUCT,
    }
}

/// Number of immediate bytes a PUSH opcode consumes from code.
pub fn push_size(opcode: Opcode) -> usize {
    use Opcode::*;
    match opcode {
        PUSH1 => 1, PUSH2 => 2, PUSH3 => 3, PUSH4 => 4, PUSH5 => 5, PUSH6 => 6,
        PUSH7 => 7, PUSH8 => 8, PUSH9 => 9, PUSH10 => 10, PUSH11 => 11, PUSH12 => 12,
        PUSH13 => 13, PUSH14 => 14, PUSH15 => 15, PUSH16 => 16, PUSH17 => 17, PUSH18 => 18,
        PUSH19 => 19, PUSH20 => 20, PUSH21 => 21, PUSH22 => 22, PUSH23 => 23, PUSH24 => 24,
        PUSH25 => 25, PUSH26 => 26, PUSH27 => 27, PUSH28 => 28, PUSH29 => 29, PUSH30 => 30,
        PUSH31 => 31, PUSH32 => 32,
        _ => 0,
    }
}

/// DUP/SWAP index (1..=16) for the opcode, or 0 if not a DUP/SWAP.
pub fn dup_index(opcode: Opcode) -> usize {
    use Opcode::*;
    match opcode {
        DUP1 => 1, DUP2 => 2, DUP3 => 3, DUP4 => 4, DUP5 => 5, DUP6 => 6, DUP7 => 7, DUP8 => 8,
        DUP9 => 9, DUP10 => 10, DUP11 => 11, DUP12 => 12, DUP13 => 13, DUP14 => 14, DUP15 => 15, DUP16 => 16,
        _ => 0,
    }
}

pub fn swap_index(opcode: Opcode) -> usize {
    use Opcode::*;
    match opcode {
        SWAP1 => 1, SWAP2 => 2, SWAP3 => 3, SWAP4 => 4, SWAP5 => 5, SWAP6 => 6, SWAP7 => 7, SWAP8 => 8,
        SWAP9 => 9, SWAP10 => 10, SWAP11 => 11, SWAP12 => 12, SWAP13 => 13, SWAP14 => 14, SWAP15 => 15, SWAP16 => 16,
        _ => 0,
    }
}