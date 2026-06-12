//! EVM Opcodes

use crate::types::*;

// =============================================================================
// OPCODE HELPERS
// =============================================================================

/// Gas costs
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
    pub const SSTORE: u64 = 2900;
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
    pub const REVERT: u64 = 0;
    pub const INVALID: u64 = 0;
    pub const SELFDESTRUCT: u64 = 0;
}

/// Get gas cost for opcode
pub fn gas_cost(opcode: Opcode) -> u64 {
    match opcode {
        Opcode::STOP => GasCost::STOP,
        Opcode::ADD => GasCost::ADD,
        Opcode::MUL => GasCost::MUL,
        Opcode::SUB => GasCost::SUB,
        Opcode::DIV => GasCost::DIV,
        Opcode::SDIV => GasCost::SDIV,
        Opcode::MOD => GasCost::MOD,
        Opcode::SMOD => GasCost::SMOD,
        Opcode::ADDMOD => GasCost::ADDMOD,
        Opcode::MULMOD => GasCost::MULMOD,
        Opcode::EXP => GasCost::EXP,
        Opcode::SHA3 => GasCost::SHA3,
        Opcode::ADDRESS => GasCost::ADDRESS,
        Opcode::BALANCE => GasCost::BALANCE,
        Opcode::SLOAD => GasCost::SLOAD,
        Opcode::SSTORE => GasCost::SSTORE,
        Opcode::CALL => GasCost::CALL,
        Opcode::CREATE => GasCost::CREATE,
        Opcode::CREATE2 => GasCost::CREATE2,
        Opcode::SELFDESTRUCT => GasCost::SELFDESTRUCT,
        _ => 0,
    }
}