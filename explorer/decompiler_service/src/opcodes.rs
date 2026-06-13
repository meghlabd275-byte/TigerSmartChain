//! EVM Opcodes Reference

/// EVM opcode constants
pub mod opcodes {
    /// Stop and arithmetic operations
    pub const STOP: u8 = 0x00;
    pub const ADD: u8 = 0x01;
    pub const MUL: u8 = 0x02;
    pub const SUB: u8 = 0x03;
    pub const DIV: u8 = 0x04;
    pub const SDIV: u8 = 0x05;
    pub const MOD: u8 = 0x06;
    pub const SMOD: u8 = 0x07;
    pub const ADDMOD: u8 = 0x08;
    pub const MULMOD: u8 = 0x09;
    pub const EXP: u8 = 0x0a;
    pub const SIGNEXTEND: u8 = 0x0b;
    
    /// Comparison and bitwise operations
    pub const LT: u8 = 0x10;
    pub const GT: u8 = 0x11;
    pub const SLT: u8 = 0x12;
    pub const SGT: u8 = 0x13;
    pub const EQ: u8 = 0x14;
    pub const ISZERO: u8 = 0x15;
    pub const AND: u8 = 0x16;
    pub const OR: u8 = 0x17;
    pub const XOR: u8 = 0x18;
    pub const NOT: u8 = 0x19;
    pub const BYTE: u8 = 0x1a;
    pub const SHL: u8 = 0x1b;
    pub const SHR: u8 = 0x1c;
    pub const SAR: u8 = 0x1d;
    
    /// Memory operations
    pub const MLOAD: u8 = 0x51;
    pub const MSTORE: u8 = 0x52;
    pub const MSTORE8: u8 = 0x53;
    pub const SLOAD: u8 = 0x54;
    pub const SSTORE: u8 = 0x55;
    pub const MSize: u8 = 0x59;
    
    /// Control flow
    pub const JUMP: u8 = 0x56;
    pub const JUMPI: u8 = 0x57;
    pub const PC: u8 = 0x58;
    pub const JUMPDEST: u8 = 0x5b;
    
    /// Logging
    pub const LOG0: u8 = 0xa0;
    pub const LOG1: u8 = 0xa1;
    pub const LOG2: u8 = 0xa2;
    pub const LOG3: u8 = 0xa3;
    pub const LOG4: u8 = 0xa4;
    
    /// System operations
    pub const CREATE: u8 = 0xf0;
    pub const CALL: u8 = 0xf1;
    pub const CALLCODE: u8 = 0xf2;
    pub const RETURN: u8 = 0xf3;
    pub const DELEGATECALL: u8 = 0xf4;
    pub const CREATE2: u8 = 0xf5;
    pub const STATICCALL: u8 = 0xfa;
    pub const REVERT: u8 = 0xfd;
    pub const INVALID: u8 = 0xfe;
    pub const SELFDESTRUCT: u8 = 0xff;
    
    /// Get opcode name
    pub fn name(opcode: u8) -> &'static str {
        match opcode {
            STOP => "STOP",
            ADD => "ADD",
            MUL => "MUL",
            SUB => "SUB",
            DIV => "DIV",
            SDIV => "SDIV",
            MOD => "MOD",
            SMOD => "SMOD",
            LT => "LT",
            GT => "GT",
            EQ => "EQ",
            AND => "AND",
            OR => "OR",
            XOR => "XOR",
            NOT => "NOT",
            MLOAD => "MLOAD",
            MSTORE => "MSTORE",
            SLOAD => "SLOAD",
            SSTORE => "SSTORE",
            JUMP => "JUMP",
            JUMPI => "JUMPI",
            JUMPDEST => "JUMPDEST",
            CALL => "CALL",
            DELEGATECALL => "DELEGATECALL",
            STATICCALL => "STATICCALL",
            RETURN => "RETURN",
            REVERT => "REVERT",
            INVALID => "INVALID",
            SELFDESTRUCT => "SELFDESTRUCT",
            _ => "UNKNOWN",
        }
    }
}