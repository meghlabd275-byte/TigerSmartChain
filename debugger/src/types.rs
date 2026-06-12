//! Debugger Types

use serde::{Deserialize, Serialize};

// =============================================================================
// DEBUGGER
// =============================================================================

/// Breakpoint
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Breakpoint {
    pub line: u32,
    pub condition: Option<String>,
    pub enabled: bool,
}

/// Stack Frame
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StackFrame {
    pub contract: String,
    pub function: String,
    pub pc: u32,
    pub stack: Vec<String>,
    pub memory: Vec<u8>,
}

/// Debug Session
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DebugSession {
    pub id: String,
    pub tx_hash: String,
    pub breakpoints: Vec<Breakpoint>,
    pub stack: Vec<StackFrame>,
    pub variables: std::collections::HashMap<String, String>,
    pub status: String,
}

impl DebugSession {
    pub fn new(tx_hash: String) -> Self {
        Self {
            id: format!("debug_{}", tx_hash),
            tx_hash,
            breakpoints: vec![],
            stack: vec![],
            variables: std::collections::HashMap::new(),
            status: "running".to_string(),
        }
    }

    /// Add breakpoint
    pub fn add_breakpoint(&mut self, bp: Breakpoint) {
        self.breakpoints.push(bp);
    }

    /// Step
    pub fn step(&mut self) {
        if let Some(frame) = self.stack.first_mut() {
            frame.pc += 1;
        }
    }
}