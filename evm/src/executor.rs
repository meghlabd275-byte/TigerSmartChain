//! EVM Executor with Gas Metering and Opcode implementation
//!
//! High-performance EVM implementation in Rust for TigerSmartChain.

use crate::types::*;
use crate::opcodes::*;

/// EVM Executor
pub struct Executor;

impl Executor {
    pub fn new() -> Self {
        Self
    }

    /// Execute smart contract code
    pub fn execute(&self, code: &[u8], ctx: &mut ExecutionContext) -> ExecutionResult {
        let mut pc = 0;
        let mut gas_used = 0;
        let logs = vec![];
        let mut success = true;

        while pc < code.len() {
            let op_byte = code[pc];
            let opcode = match Opcode::from_u8(op_byte) {
                Some(op) => op,
                None => {
                    success = false;
                    break;
                }
            };

            // Calculate and check gas
            let cost = gas_cost(opcode);
            if gas_used + cost > ctx.gas {
                success = false;
                break;
            }
            gas_used += cost;

            match opcode {
                Opcode::STOP => break,
                Opcode::ADD => {
                    if let (Some(a), Some(b)) = (ctx.stack.pop(), ctx.stack.pop()) {
                        ctx.stack.push(a.add(&b));
                    } else { success = false; break; }
                }
                Opcode::MUL => {
                    if let (Some(a), Some(b)) = (ctx.stack.pop(), ctx.stack.pop()) {
                        ctx.stack.push(a.mul(&b));
                    } else { success = false; break; }
                }
                Opcode::SUB => {
                    if let (Some(a), Some(b)) = (ctx.stack.pop(), ctx.stack.pop()) {
                        ctx.stack.push(a.sub(&b));
                    } else { success = false; break; }
                }
                Opcode::DIV => {
                    if let (Some(a), Some(b)) = (ctx.stack.pop(), ctx.stack.pop()) {
                        ctx.stack.push(a.div(&b));
                    } else { success = false; break; }
                }
                Opcode::ISZERO => {
                    if let Some(a) = ctx.stack.pop() {
                        ctx.stack.push(EvmWord::from_u64(if a.is_zero() { 1 } else { 0 }));
                    } else { success = false; break; }
                }
                Opcode::POP => {
                    if ctx.stack.pop().is_none() { success = false; break; }
                }
                Opcode::MLOAD => {
                    if let Some(offset) = ctx.stack.pop() {
                        let data = ctx.memory.load(offset.as_u64() as usize, 32);
                        let mut word = [0u8; 32];
                        let len = data.len().min(32);
                        word[..len].copy_from_slice(&data[..len]);
                        ctx.stack.push(EvmWord(word));
                    } else { success = false; break; }
                }
                Opcode::MSTORE => {
                    if let (Some(offset), Some(value)) = (ctx.stack.pop(), ctx.stack.pop()) {
                        ctx.memory.store(offset.as_u64() as usize, &value.0);
                    } else { success = false; break; }
                }
                Opcode::SLOAD => {
                    if let Some(key) = ctx.stack.pop() {
                        ctx.stack.push(ctx.storage.load(&key));
                    } else { success = false; break; }
                }
                Opcode::SSTORE => {
                    if let (Some(key), Some(value)) = (ctx.stack.pop(), ctx.stack.pop()) {
                        ctx.storage.store(key, value);
                    } else { success = false; break; }
                }
                Opcode::PUSH1 => {
                    pc += 1;
                    if pc < code.len() {
                        ctx.stack.push(EvmWord::from_u64(code[pc] as u64));
                    } else { success = false; break; }
                }
                Opcode::CALLVALUE => {
                    ctx.stack.push(EvmWord::from_u64(ctx.value));
                }
                Opcode::GAS => {
                    ctx.stack.push(EvmWord::from_u64(ctx.gas - gas_used));
                }
                _ => {
                    // Other opcodes not yet implemented in this version
                }
            }

            pc += 1;
        }

        ExecutionResult {
            success,
            gas_used,
            output: vec![],
            logs,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_add_op() {
        let mut ctx = ExecutionContext::new();
        ctx.gas = 1000;
        let code = vec![0x60, 0x0A, 0x60, 0x05, 0x01, 0x00]; // PUSH1 10, PUSH1 5, ADD, STOP
        let executor = Executor::new();
        let result = executor.execute(&code, &mut ctx);

        assert!(result.success);
        assert_eq!(ctx.stack.pop().unwrap().as_u64(), 15);
    }

    #[test]
    fn test_out_of_gas() {
        let mut ctx = ExecutionContext::new();
        ctx.gas = 2; // Not enough gas for PUSH1 (3)
        let code = vec![0x60, 0x0A, 0x60, 0x05, 0x01, 0x00];
        let executor = Executor::new();
        let result = executor.execute(&code, &mut ctx);

        assert!(!result.success);
    }
}
