//! EVM Types

use serde::{Deserialize, Serialize};

// =============================================================================
// EVM TYPES
// =============================================================================

/// EVM Word (256-bit)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvmWord(pub [u8; 32]);

impl EvmWord {
    pub fn zero() -> Self {
        Self([0u8; 32])
    }

    pub fn from_u64(v: u64) -> Self {
        let mut bytes = [0u8; 32];
        bytes[24..].copy_from_slice(&v.to_be_bytes());
        Self(bytes)
    }

    pub fn as_u64(&self) -> u64 {
        let mut bytes = [0u8; 8];
        bytes.copy_from_slice(&self.0[24..]);
        u64::from_be_bytes(bytes)
    }

    pub fn add(&self, other: &Self) -> Self {
        let a = self.as_u64();
        let b = other.as_u64();
        Self::from_u64(a.wrapping_add(b))
    }

    pub fn mul(&self, other: &Self) -> Self {
        let a = self.as_u64();
        let b = other.as_u64();
        Self::from_u64(a.wrapping_mul(b))
    }

    pub fn sub(&self, other: &Self) -> Self {
        let a = self.as_u64();
        let b = other.as_u64();
        Self::from_u64(a.wrapping_sub(b))
    }

    pub fn div(&self, other: &Self) -> Self {
        let a = self.as_u64();
        let b = other.as_u64();
        if b == 0 {
            Self::zero()
        } else {
            Self::from_u64(a / b)
        }
    }

    pub fn is_zero(&self) -> bool {
        self.0.iter().all(|&b| b == 0)
    }
}

impl std::hash::Hash for EvmWord {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.0.hash(state);
    }
}

impl PartialEq for EvmWord {
    fn eq(&self, other: &Self) -> bool {
        self.0 == other.0
    }
}

impl Eq for EvmWord {}

/// Address (160-bit)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Address(pub [u8; 20]);

impl Address {
    pub fn zero() -> Self {
        Self([0u8; 20])
    }

    pub fn from_hex(s: &str) -> Option<Self> {
        let bytes = hex::decode(s.strip_prefix("0x").unwrap_or(s)).ok()?;
        let mut addr = [0u8; 20];
        addr.copy_from_slice(&bytes[..20]);
        Some(Self(addr))
    }
}

/// Stack
#[derive(Debug, Clone)]
pub struct Stack {
    data: Vec<EvmWord>,
}

impl Stack {
    pub fn new() -> Self {
        Self { data: vec![] }
    }

    pub fn push(&mut self, value: EvmWord) {
        self.data.push(value);
    }

    pub fn pop(&mut self) -> Option<EvmWord> {
        self.data.pop()
    }

    pub fn dup(&mut self, n: usize) {
        if n <= self.data.len() {
            let val = self.data[self.data.len() - n].clone();
            self.data.push(val);
        }
    }

    pub fn swap(&mut self, n: usize) {
        if n > 0 && n < self.data.len() {
            let len = self.data.len();
            self.data.swap(len - 1, len - 1 - n);
        }
    }
}

/// Memory
#[derive(Debug, Clone)]
pub struct Memory {
    data: Vec<u8>,
}

impl Memory {
    pub fn new() -> Self {
        Self { data: vec![] }
    }

    pub fn store(&mut self, offset: usize, value: &[u8]) {
        if offset + value.len() > self.data.len() {
            self.data.resize(offset + value.len(), 0);
        }
        self.data[offset..offset + value.len()].copy_from_slice(value);
    }

    pub fn load(&self, offset: usize, length: usize) -> Vec<u8> {
        if offset + length > self.data.len() {
            return vec![];
        }
        self.data[offset..offset + length].to_vec()
    }
}

/// Storage
#[derive(Debug, Clone)]
pub struct Storage {
    data: std::collections::HashMap<EvmWord, EvmWord>,
}

impl Storage {
    pub fn new() -> Self {
        Self { data: std::collections::HashMap::new() }
    }

    pub fn store(&mut self, key: EvmWord, value: EvmWord) {
        self.data.insert(key, value);
    }

    pub fn load(&self, key: &EvmWord) -> EvmWord {
        self.data.get(key).cloned().unwrap_or(EvmWord::zero())
    }
}

// =============================================================================
// EXECUTION
// =============================================================================

/// Execution Context
#[derive(Debug, Clone)]
pub struct ExecutionContext {
    pub caller: Address,
    pub origin: Address,
    pub gas: u64,
    pub gas_price: u64,
    pub value: u64,
    pub data: Vec<u8>,
    pub stack: Stack,
    pub memory: Memory,
    pub storage: Storage,
}

impl ExecutionContext {
    pub fn new() -> Self {
        Self {
            caller: Address::zero(),
            origin: Address::zero(),
            gas: 0,
            gas_price: 0,
            value: 0,
            data: vec![],
            stack: Stack::new(),
            memory: Memory::new(),
            storage: Storage::new(),
        }
    }
}

/// Execution Result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutionResult {
    pub success: bool,
    pub gas_used: u64,
    pub output: Vec<u8>,
    pub logs: Vec<Log>,
}

/// Log
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Log {
    pub address: Address,
    pub topics: Vec<EvmWord>,
    pub data: Vec<u8>,
}

// =============================================================================
// OPCODES
// =============================================================================

/// Opcode
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Opcode {
    STOP,
    ADD,
    MUL,
    SUB,
    DIV,
    SDIV,
    MOD,
    SMOD,
    ADDMOD,
    MULMOD,
    EXP,
    SIGNEXTEND,
    LT,
    GT,
    SLT,
    SGT,
    EQ,
    ISZERO,
    AND,
    OR,
    XOR,
    NOT,
    BYTE,
    SHL,
    SHR,
    SAR,
    SHA3,
    ADDRESS,
    BALANCE,
    ORIGIN,
    CALLER,
    CALLVALUE,
    CALLDATASIZE,
    CALLDATACOPY,
    CODESIZE,
    CODECOPY,
    GASPRICE,
    EXTCODESIZE,
    EXTCODECOPY,
    RETURNDATASIZE,
    RETURNDATACOPY,
    EXTCODEHASH,
    BLOCKHASH,
    COINBASE,
    TIMESTAMP,
    NUMBER,
    DIFFICULTY,
    GASLIMIT,
    CHAINID,
    BASEFEE,
    POP,
    MLOAD,
    MSTORE,
    MSTORE8,
    SLOAD,
    SSTORE,
    JUMP,
    JUMPI,
    PC,
    MSIZE,
    GAS,
    JUMPDEST,
    PUSH1,
    DUP1,
    DUP2,
    DUP3,
    DUP4,
    DUP5,
    DUP6,
    DUP7,
    DUP8,
    DUP9,
    DUP10,
    DUP11,
    DUP12,
    DUP13,
    DUP14,
    DUP15,
    DUP16,
    SWAP1,
    SWAP2,
    SWAP3,
    SWAP4,
    SWAP5,
    SWAP6,
    SWAP7,
    SWAP8,
    SWAP9,
    SWAP10,
    SWAP11,
    SWAP12,
    SWAP13,
    SWAP14,
    SWAP15,
    SWAP16,
    LOG0,
    LOG1,
    LOG2,
    LOG3,
    LOG4,
    CREATE,
    CALL,
    CALLCODE,
    DELEGATECALL,
    CREATE2,
    STATICCALL,
    REVERT,
    INVALID,
    SELFDESTRUCT,
}

impl Opcode {
    pub fn from_u8(v: u8) -> Option<Self> {
        match v {
            0x00 => Some(Self::STOP),
            0x01 => Some(Self::ADD),
            0x02 => Some(Self::MUL),
            0x03 => Some(Self::SUB),
            0x04 => Some(Self::DIV),
            0x05 => Some(Self::SDIV),
            0x06 => Some(Self::MOD),
            0x07 => Some(Self::SMOD),
            0x08 => Some(Self::ADDMOD),
            0x09 => Some(Self::MULMOD),
            0x0a => Some(Self::EXP),
            0x0b => Some(Self::SIGNEXTEND),
            0x10 => Some(Self::LT),
            0x11 => Some(Self::GT),
            0x12 => Some(Self::SLT),
            0x13 => Some(Self::SGT),
            0x14 => Some(Self::EQ),
            0x15 => Some(Self::ISZERO),
            0x16 => Some(Self::AND),
            0x17 => Some(Self::OR),
            0x18 => Some(Self::XOR),
            0x19 => Some(Self::NOT),
            0x1a => Some(Self::BYTE),
            0x1b => Some(Self::SHL),
            0x1c => Some(Self::SHR),
            0x1d => Some(Self::SAR),
            0x20 => Some(Self::SHA3),
            0x30 => Some(Self::ADDRESS),
            0x31 => Some(Self::BALANCE),
            0x32 => Some(Self::ORIGIN),
            0x33 => Some(Self::CALLER),
            0x34 => Some(Self::CALLVALUE),
            0x35 => Some(Self::CALLDATASIZE),
            0x36 => Some(Self::CALLDATACOPY),
            0x37 => Some(Self::CODESIZE),
            0x38 => Some(Self::CODECOPY),
            0x3a => Some(Self::GASPRICE),
            0x3b => Some(Self::EXTCODESIZE),
            0x3c => Some(Self::EXTCODECOPY),
            0x3d => Some(Self::RETURNDATASIZE),
            0x3e => Some(Self::RETURNDATACOPY),
            0x3f => Some(Self::EXTCODEHASH),
            0x40 => Some(Self::BLOCKHASH),
            0x41 => Some(Self::COINBASE),
            0x42 => Some(Self::TIMESTAMP),
            0x43 => Some(Self::NUMBER),
            0x44 => Some(Self::DIFFICULTY),
            0x45 => Some(Self::GASLIMIT),
            0x46 => Some(Self::CHAINID),
            0x47 => Some(Self::BASEFEE),
            0x50 => Some(Self::POP),
            0x51 => Some(Self::MLOAD),
            0x52 => Some(Self::MSTORE),
            0x53 => Some(Self::MSTORE8),
            0x54 => Some(Self::SLOAD),
            0x55 => Some(Self::SSTORE),
            0x56 => Some(Self::JUMP),
            0x57 => Some(Self::JUMPI),
            0x58 => Some(Self::PC),
            0x59 => Some(Self::MSIZE),
            0x5a => Some(Self::GAS),
            0x5b => Some(Self::JUMPDEST),
            0x60 => Some(Self::PUSH1),
            0x80 => Some(Self::DUP1),
            0x90 => Some(Self::SWAP1),
            0xa0 => Some(Self::LOG0),
            0xf0 => Some(Self::CREATE),
            0xf1 => Some(Self::CALL),
            0xf2 => Some(Self::CALLCODE),
            0xf3 => Some(Self::DELEGATECALL),
            0xf4 => Some(Self::CREATE2),
            0xfa => Some(Self::STATICCALL),
            0xfd => Some(Self::REVERT),
            0xfe => Some(Self::INVALID),
            0xff => Some(Self::SELFDESTRUCT),
            _ => None,
        }
    }
}