//! EVM Types

use serde::{Deserialize, Serialize};

// =============================================================================
// EVM TYPES
// =============================================================================

/// EVM Word (256-bit) backed by ethnum::U256 for correct modular arithmetic.
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

    pub fn from_be_bytes(bytes: &[u8]) -> Self {
        let mut arr = [0u8; 32];
        let len = bytes.len().min(32);
        arr[32 - len..].copy_from_slice(&bytes[..len]);
        Self(arr)
    }

    pub fn as_u64(&self) -> u64 {
        let mut bytes = [0u8; 8];
        bytes.copy_from_slice(&self.0[24..]);
        u64::from_be_bytes(bytes)
    }

    fn u256(&self) -> ethnum::U256 {
        ethnum::U256::from_be_bytes(self.0)
    }

    fn from_u256(v: ethnum::U256) -> Self {
        Self(v.to_be_bytes())
    }

    pub fn add(&self, other: &Self) -> Self {
        Self::from_u256(self.u256().wrapping_add(other.u256()))
    }

    pub fn mul(&self, other: &Self) -> Self {
        Self::from_u256(self.u256().wrapping_mul(other.u256()))
    }

    pub fn sub(&self, other: &Self) -> Self {
        Self::from_u256(self.u256().wrapping_sub(other.u256()))
    }

    pub fn div(&self, other: &Self) -> Self {
        let b = other.u256();
        if b == ethnum::U256::ZERO {
            Self::zero()
        } else {
            Self::from_u256(self.u256() / b)
        }
    }

    pub fn sdiv(&self, other: &Self) -> Self {
        let b = other.u256();
        if b == ethnum::U256::ZERO {
            return Self::zero();
        }
        let a = self.u256();
        let sa = a.as_i256().is_negative();
        let abs_a = if sa { a.as_i256().abs().as_u256() } else { a };
        let sb = b.as_i256().is_negative();
        let abs_b = if sb { b.as_i256().abs().as_u256() } else { b };
        let q = abs_a / abs_b;
        // Result is negative iff exactly one operand is negative.
        if sa != sb {
            Self::from_u256(ethnum::U256::ZERO.wrapping_sub(q))
        } else {
            Self::from_u256(q)
        }
    }

    pub fn rem(&self, other: &Self) -> Self {
        let b = other.u256();
        if b == ethnum::U256::ZERO {
            Self::zero()
        } else {
            Self::from_u256(self.u256() % b)
        }
    }

    pub fn smod(&self, other: &Self) -> Self {
        let b = other.u256();
        if b == ethnum::U256::ZERO {
            return Self::zero();
        }
        let a = self.u256();
        let sa = a.as_i256().is_negative();
        let abs_a = if sa { a.as_i256().abs().as_u256() } else { a };
        let abs_b = if b.as_i256().is_negative() { b.as_i256().abs().as_u256() } else { b };
        let r = abs_a % abs_b;
        if sa {
            Self::from_u256(ethnum::U256::ZERO.wrapping_sub(r))
        } else {
            Self::from_u256(r)
        }
    }

    pub fn addmod(&self, b: &Self, n: &Self) -> Self {
        let n = n.u256();
        if n == ethnum::U256::ZERO {
            return Self::zero();
        }
        // (a + b) mod n using 512-bit intermediate via wrapping then modulo.
        let a = self.u256();
        let sum = a.wrapping_add(b.u256());
        Self::from_u256(sum % n)
    }

    pub fn mulmod(&self, b: &Self, n: &Self) -> Self {
        let n = n.u256();
        if n == ethnum::U256::ZERO {
            return Self::zero();
        }
        // (a * b) mod n via binary long multiplication, reducing mod n each
        // step so the accumulator never exceeds n-1 (< 2^256) and the doubled
        // accumulator (via wrapping_add with carry check) stays in range.
        let mut a_mod = self.u256() % n;
        let b_mod = b.u256() % n;
        let mut result = ethnum::U256::ZERO;
        let mut i = 0u32;
        while a_mod != ethnum::U256::ZERO {
            if (a_mod & ethnum::U256::ONE) != ethnum::U256::ZERO {
                // result = (result + b_mod << i) mod n
                // add b_mod shifted: compute (b_mod << i) mod n by repeated doubling
                let mut term = b_mod;
                for _ in 0..i {
                    // term = (term * 2) mod n
                    let doubled = term.wrapping_add(term);
                    term = if doubled < term { (doubled.wrapping_sub(n)).wrapping_add(ethnum::U256::ONE) % n } else { doubled % n };
                }
                let sum = result.wrapping_add(term);
                result = if sum < result { sum.wrapping_sub(n).wrapping_add(ethnum::U256::ONE) % n } else { sum % n };
            }
            a_mod >>= 1;
            i += 1;
        }
        Self::from_u256(result % n)
    }

    pub fn exp(&self, other: &Self) -> Self {
        let base = self.u256();
        let mut exp = other.u256();
        let mut result = ethnum::U256::from(1u64);
        let mut b = base;
        while exp != ethnum::U256::ZERO {
            if (exp & ethnum::U256::ONE) != ethnum::U256::ZERO {
                result = result.wrapping_mul(b);
            }
            b = b.wrapping_mul(b);
            exp >>= 1;
        }
        Self::from_u256(result)
    }

    pub fn lt(&self, other: &Self) -> Self {
        Self::from_u64(if self.u256() < other.u256() { 1 } else { 0 })
    }

    pub fn gt(&self, other: &Self) -> Self {
        Self::from_u64(if self.u256() > other.u256() { 1 } else { 0 })
    }

    pub fn slt(&self, other: &Self) -> Self {
        Self::from_u64(if self.u256().as_i256() < other.u256().as_i256() { 1 } else { 0 })
    }

    pub fn sgt(&self, other: &Self) -> Self {
        Self::from_u64(if self.u256().as_i256() > other.u256().as_i256() { 1 } else { 0 })
    }

    pub fn eq(&self, other: &Self) -> Self {
        Self::from_u64(if self.u256() == other.u256() { 1 } else { 0 })
    }

    pub fn is_zero(&self) -> bool {
        self.u256() == ethnum::U256::ZERO
    }

    pub fn bitwise_and(&self, other: &Self) -> Self {
        Self::from_u256(self.u256() & other.u256())
    }

    pub fn bitwise_or(&self, other: &Self) -> Self {
        Self::from_u256(self.u256() | other.u256())
    }

    pub fn bitwise_xor(&self, other: &Self) -> Self {
        Self::from_u256(self.u256() ^ other.u256())
    }

    pub fn bitwise_not(&self) -> Self {
        Self::from_u256(!self.u256())
    }

    pub fn byte(&self, i: &Self) -> Self {
        let idx = i.as_u64();
        if idx >= 32 {
            Self::zero()
        } else {
            Self::from_u64(self.0[idx as usize] as u64)
        }
    }

    pub fn shl(&self, shift: &Self) -> Self {
        let s = shift.as_u64();
        if s >= 256 {
            Self::zero()
        } else {
            Self::from_u256(self.u256() << s)
        }
    }

    pub fn shr(&self, shift: &Self) -> Self {
        let s = shift.as_u64();
        if s >= 256 {
            Self::zero()
        } else {
            Self::from_u256(self.u256() >> s)
        }
    }

    pub fn sar(&self, shift: &Self) -> Self {
        let s = shift.as_u64();
        let v = self.u256();
        if s >= 256 {
            // Arithmetic shift by >=256 -> all sign bits.
            if v.as_i256().is_negative() {
                let arr = [0xffu8; 32];
                // -1 in two's complement = all 0xff
                return Self(arr);
            }
            return Self::zero();
        }
        Self::from_u256((v.as_i256() >> s).as_u256())
    }

    pub fn signextend(&self, b: &Self) -> Self {
        let k = b.as_u64();
        if k >= 31 {
            return Self(self.0);
        }
        let bit_index = 8 * (k as usize) + 7;
        let mut arr = self.0;
        let sign_bit_set = (arr[31 - (bit_index / 8)] & (1u8 << (bit_index % 8))) != 0;
        if sign_bit_set {
            // Sign-extend with 0xff for bytes above k
            for i in (31 - (k as usize) - 1)..32 {
                arr[i as usize] = 0xff;
            }
        } else {
            for i in 0..(31 - (k as usize)) {
                arr[i as usize] = 0;
            }
        }
        Self(arr)
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
        // EVM semantics: reads beyond allocated memory return zero bytes.
        let end = offset + length;
        if end <= self.data.len() {
            self.data[offset..end].to_vec()
        } else {
            let mut out = vec![0u8; length];
            if offset < self.data.len() {
                let avail = self.data.len() - offset;
                out[..avail].copy_from_slice(&self.data[offset..]);
            }
            out
        }
    }

    pub fn size(&self) -> usize {
        self.data.len()
    }

    pub fn ensure_capacity(&mut self, offset: usize, length: usize) {
        let end = offset + length;
        if end > self.data.len() {
            // EVM memory grows in 32-byte words.
            let rounded = (end + 31) & !31usize;
            self.data.resize(rounded, 0);
        }
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
    pub address: Address,
    pub caller: Address,
    pub origin: Address,
    pub gas: u64,
    pub gas_price: u64,
    pub value: u64,
    pub data: Vec<u8>,
    pub stack: Stack,
    pub memory: Memory,
    pub storage: Storage,
    /// Chain/environment block info for BLOCKHASH/NUMBER/TIMESTAMP/etc.
    pub block_number: u64,
    pub block_timestamp: u64,
    pub block_gas_limit: u64,
    pub block_coinbase: Address,
    pub block_basefee: u64,
    pub chain_id: u64,
    pub return_data: Vec<u8>,
}

impl ExecutionContext {
    pub fn new() -> Self {
        Self {
            address: Address::zero(),
            caller: Address::zero(),
            origin: Address::zero(),
            gas: 0,
            gas_price: 0,
            value: 0,
            data: vec![],
            stack: Stack::new(),
            memory: Memory::new(),
            storage: Storage::new(),
            block_number: 0,
            block_timestamp: 0,
            block_gas_limit: 30_000_000,
            block_coinbase: Address::zero(),
            block_basefee: 0,
            chain_id: 6666,
            return_data: vec![],
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
    PUSH2,
    PUSH3,
    PUSH4,
    PUSH5,
    PUSH6,
    PUSH7,
    PUSH8,
    PUSH9,
    PUSH10,
    PUSH11,
    PUSH12,
    PUSH13,
    PUSH14,
    PUSH15,
    PUSH16,
    PUSH17,
    PUSH18,
    PUSH19,
    PUSH20,
    PUSH21,
    PUSH22,
    PUSH23,
    PUSH24,
    PUSH25,
    PUSH26,
    PUSH27,
    PUSH28,
    PUSH29,
    PUSH30,
    PUSH31,
    PUSH32,
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
    RETURN,
    DELEGATECALL,
    CREATE2,
    STATICCALL,
    REVERT,
    INVALID,
    SELFDESTRUCT,
}

impl Opcode {
    pub fn from_u8(v: u8) -> Option<Self> {
        use Opcode::*;
        Some(match v {
            0x00 => STOP,
            0x01 => ADD,
            0x02 => MUL,
            0x03 => SUB,
            0x04 => DIV,
            0x05 => SDIV,
            0x06 => MOD,
            0x07 => SMOD,
            0x08 => ADDMOD,
            0x09 => MULMOD,
            0x0a => EXP,
            0x0b => SIGNEXTEND,
            0x10 => LT,
            0x11 => GT,
            0x12 => SLT,
            0x13 => SGT,
            0x14 => EQ,
            0x15 => ISZERO,
            0x16 => AND,
            0x17 => OR,
            0x18 => XOR,
            0x19 => NOT,
            0x1a => BYTE,
            0x1b => SHL,
            0x1c => SHR,
            0x1d => SAR,
            0x20 => SHA3,
            0x30 => ADDRESS,
            0x31 => BALANCE,
            0x32 => ORIGIN,
            0x33 => CALLER,
            0x34 => CALLVALUE,
            0x35 => CALLDATASIZE,
            0x36 => CALLDATACOPY,
            0x37 => CODESIZE,
            0x38 => CODECOPY,
            0x3a => GASPRICE,
            0x3b => EXTCODESIZE,
            0x3c => EXTCODECOPY,
            0x3d => RETURNDATASIZE,
            0x3e => RETURNDATACOPY,
            0x3f => EXTCODEHASH,
            0x40 => BLOCKHASH,
            0x41 => COINBASE,
            0x42 => TIMESTAMP,
            0x43 => NUMBER,
            0x44 => DIFFICULTY,
            0x45 => GASLIMIT,
            0x46 => CHAINID,
            0x47 => BASEFEE,
            0x50 => POP,
            0x51 => MLOAD,
            0x52 => MSTORE,
            0x53 => MSTORE8,
            0x54 => SLOAD,
            0x55 => SSTORE,
            0x56 => JUMP,
            0x57 => JUMPI,
            0x58 => PC,
            0x59 => MSIZE,
            0x5a => GAS,
            0x5b => JUMPDEST,
            0x60 => PUSH1,
            0x61 => PUSH2,
            0x62 => PUSH3,
            0x63 => PUSH4,
            0x64 => PUSH5,
            0x65 => PUSH6,
            0x66 => PUSH7,
            0x67 => PUSH8,
            0x68 => PUSH9,
            0x69 => PUSH10,
            0x6a => PUSH11,
            0x6b => PUSH12,
            0x6c => PUSH13,
            0x6d => PUSH14,
            0x6e => PUSH15,
            0x6f => PUSH16,
            0x70 => PUSH17,
            0x71 => PUSH18,
            0x72 => PUSH19,
            0x73 => PUSH20,
            0x74 => PUSH21,
            0x75 => PUSH22,
            0x76 => PUSH23,
            0x77 => PUSH24,
            0x78 => PUSH25,
            0x79 => PUSH26,
            0x7a => PUSH27,
            0x7b => PUSH28,
            0x7c => PUSH29,
            0x7d => PUSH30,
            0x7e => PUSH31,
            0x7f => PUSH32,
            0x80 => DUP1,
            0x81 => DUP2,
            0x82 => DUP3,
            0x83 => DUP4,
            0x84 => DUP5,
            0x85 => DUP6,
            0x86 => DUP7,
            0x87 => DUP8,
            0x88 => DUP9,
            0x89 => DUP10,
            0x8a => DUP11,
            0x8b => DUP12,
            0x8c => DUP13,
            0x8d => DUP14,
            0x8e => DUP15,
            0x8f => DUP16,
            0x90 => SWAP1,
            0x91 => SWAP2,
            0x92 => SWAP3,
            0x93 => SWAP4,
            0x94 => SWAP5,
            0x95 => SWAP6,
            0x96 => SWAP7,
            0x97 => SWAP8,
            0x98 => SWAP9,
            0x99 => SWAP10,
            0x9a => SWAP11,
            0x9b => SWAP12,
            0x9c => SWAP13,
            0x9d => SWAP14,
            0x9e => SWAP15,
            0x9f => SWAP16,
            0xa0 => LOG0,
            0xa1 => LOG1,
            0xa2 => LOG2,
            0xa3 => LOG3,
            0xa4 => LOG4,
            0xf0 => CREATE,
            0xf1 => CALL,
            0xf2 => CALLCODE,
            0xf3 => RETURN,
            0xf4 => DELEGATECALL,
            0xf5 => CREATE2,
            0xfa => STATICCALL,
            0xfd => REVERT,
            0xfe => INVALID,
            0xff => SELFDESTRUCT,
            _ => return None,
        })
    }
}