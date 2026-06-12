//! Bytecode Analysis Types

use serde::{Deserialize, Serialize};

// =============================================================================
// BYTECODE ANALYSIS
// =============================================================================

/// Bytecode Analysis
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BytecodeAnalysis {
    pub bytecode: String,
    pub opcodes: Vec<String>,
    pub functions: Vec<String>,
    pub libraries: Vec<String>,
    pub compiler: String,
    pub version: String,
}

/// Control Flow Graph
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ControlFlowGraph {
    pub nodes: Vec<String>,
    pub edges: Vec<(String, String)>,
}

/// Bytecode Analyzer
pub struct Analyzer {
    opcodes: std::collections::HashMap<String, String>,
}

impl Analyzer {
    pub fn new() -> Self {
        let mut opcodes = std::collections::HashMap::new();
        opcodes.insert("00".to_string(), "STOP".to_string());
        opcodes.insert("01".to_string(), "ADD".to_string());
        opcodes.insert("02".to_string(), "MUL".to_string());
        opcodes.insert("60".to_string(), "PUSH1".to_string());
        Self { opcodes }
    }

    /// Analyze bytecode
    pub fn analyze(&self, bytecode: &str) -> BytecodeAnalysis {
        BytecodeAnalysis {
            bytecode: bytecode.to_string(),
            opcodes: vec![],
            functions: vec![],
            libraries: vec![],
            compiler: "solc".to_string(),
            version: "0.8.0".to_string(),
        }
    }
}

impl Default for Analyzer {
    fn default() -> Self {
        Self::new()
    }
}