//! EVM Executor

use crate::types::*;

// =============================================================================
// EXECUTOR
// =============================================================================

/// EVM Executor
pub struct Executor {
    max_gas: u64,
}

impl Executor {
    pub fn new(max_gas: u64) -> Self {
        Self { max_gas }
    }

    /// Execute smart contract
    pub fn execute(&self, code: &[u8], ctx: &mut ExecutionContext) -> ExecutionResult {
        let mut pc = 0;
        let mut output = vec![];
        let mut logs = vec![];
        
        while pc < code.len() {
            let opcode = Opcode::from_u8(code[pc]);
            
            match opcode {
                Some(Opcode::STOP) => break,
                Some(Opcode::ADD) => {
                    if let (Some(a), Some(b)) = (ctx.stack.pop(), ctx.stack.pop()) {
                        // Add
                    }
                }
                Some(Opcode::MUL) => {
                    if let (Some(a), Some(b)) = (ctx.stack.pop(), ctx.stack.pop()) {
                        // Multiply
                    }
                }
                Some(Opcode::SSTORE) => {
                    if let (Some(key)), Some(value) = (ctx.stack.pop(), ctx.stack.pop()) {
                        ctx.storage.store(key, value);
                    }
                }
                Some(Opcode::SLOAD) => {
                    if let Some(key) = ctx.stack.pop() {
                        let value = ctx.storage.load(&key);
                        ctx.stack.push(value);
                    }
                }
                Some(Opcode::CALLVALUE) => {
                    ctx.stack.push(EvmWord::from_u64(ctx.value));
                }
                _ => {}
            }
            
            pc += 1;
        }
        
        ExecutionResult {
            success: true,
            gas_used: self.max_gas - ctx.gas,
            output,
            logs,
        }
    }
}

// =============================================================================
// INTERPRETER
// =============================================================================

/// Interpreter
pub struct Interpreter;

impl Interpreter {
    pub fn new() -> Self {
        Self
    }

    /// Run contract
    pub fn run(&self, code: &[u8], ctx: &mut ExecutionContext) -> ExecutionResult {
        let executor = Executor::new(ctx.gas);
        executor.execute(code, ctx)
    }
}

impl Default for Interpreter {
    fn default() -> Self {
        Self::new()
    }
}