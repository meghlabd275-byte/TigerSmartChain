//! TigerScan Decompiler Service — runtime bytecode analysis.
//!
//! The service disassembles EVM runtime code the way the interpreter does,
//! then reads function selectors out of the dispatcher, constant storage slots
//! out of SLOAD/SSTORE sequences, and event topics out of LOG instructions.
//! Bytecode is fetched over `eth_getCode` when the caller does not supply it.

#![forbid(unsafe_code)]

mod analysis;
mod opcodes;
mod patterns;
mod server;
mod types;

pub use analysis::{
    decode_bytecode, metadata_trailer_len, Analyzer, KNOWN_EVENTS, KNOWN_SELECTORS,
    EIP1167_PREFIX, EIP1967_ADMIN_SLOT, EIP1967_BEACON_SLOT, EIP1967_IMPLEMENTATION_SLOT,
};
pub use opcodes::{disassemble, mnemonic, name as opcode_name, push_size, Instruction};
pub use patterns::{interface_id, ContractType, PatternDetector};
pub use server::{ApiRequest, ApiResponse, Server};
pub use types::{
    Config, DecompiledContract, DecompiledEvent, DecompiledFunction, Parameter, StorageVariable,
};
