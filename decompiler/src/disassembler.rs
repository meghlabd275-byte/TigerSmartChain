//! Disassembler

use crate::types::*;

// =============================================================================
// DISASSEMBLER
// =============================================================================

/// Disassembler
pub struct Disassembler;

impl Disassembler {
    pub fn new() -> Self {
        Self
    }

    /// Disassemble bytecode
    pub fn disassemble(&self, bytecode: &[u8]) -> Vec<DecompiledInstruction> {
        let mut instructions = vec![];
        let mut pc = 0;
        
        while pc < bytecode.len() {
            let opcode = bytecode[pc];
            
            let inst = DecompiledInstruction {
                offset: pc,
                opcode: format!("0x{:02x}", opcode),
                arguments: vec![],
            };
            instructions.push(inst);
            pc += 1;
        }
        
        instructions
    }
}

impl Default for Disassembler {
    fn default() -> Self {
        Self::new()
    }
}

// =============================================================================
// DECOMPILE
// =============================================================================

/// Decompiler
pub struct Decompiler;

impl Decompiler {
    pub fn new() -> Self {
        Self
    }

    /// Decompile contract
    pub fn decompile(&self, address: &str, bytecode: &[u8]) -> DecompiledContract {
        let dis = Disassembler::new();
        let instructions = dis.disassemble(bytecode);
        
        DecompiledContract {
            address: address.to_string(),
            abi: vec![],
            sources: vec![],
        }
    }
}

impl Default for Decompiler {
    fn default() -> Self {
        Self::new()
    }
}