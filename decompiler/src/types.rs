//! Decompiler Types

use serde::{Deserialize, Serialize};

// =============================================================================
// DECOMPILED
// =============================================================================

/// Decompiled Contract
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DecompiledContract {
    pub address: String,
    pub abi: Vec<DecompiledFunction>,
    pub sources: Vec<DecompiledSource>,
}

/// Decompiled Function
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DecompiledFunction {
    pub name: String,
    pub visibility: String,
    pub inputs: Vec<String>,
    pub outputs: Vec<String>,
    pub body: Vec<DecompiledInstruction>,
}

/// Decompiled Source
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DecompiledSource {
    pub line: usize,
    pub code: String,
    pub instruction: String,
}

/// Decompiled Instruction
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DecompiledInstruction {
    pub offset: usize,
    pub opcode: String,
    pub arguments: Vec<String>,
}