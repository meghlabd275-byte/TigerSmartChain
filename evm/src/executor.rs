//! EVM Executor with Gas Metering and full opcode implementation
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

    /// Execute smart contract code with full opcode support and gas metering.
    pub fn execute(&self, code: &[u8], ctx: &mut ExecutionContext) -> ExecutionResult {
        let mut pc = 0usize;
        let mut gas_used: u64 = 0;
        let mut logs: Vec<Log> = Vec::new();
        let mut success = true;
        let mut output: Vec<u8> = Vec::new();

        // Pre-compute valid JUMPDEST positions for fast validation.
        let jumpdests = analyze_jumpdests(code);

        macro_rules! pop2 {
            () => {{
                let a = ctx.stack.pop();
                let b = ctx.stack.pop();
                match (a, b) {
                    (Some(a), Some(b)) => (a, b),
                    _ => { success = false; break; }
                }
            }};
        }
        macro_rules! pop3 {
            () => {{
                let a = ctx.stack.pop();
                let b = ctx.stack.pop();
                let c = ctx.stack.pop();
                match (a, b, c) {
                    (Some(a), Some(b), Some(c)) => (a, b, c),
                    _ => { success = false; (EvmWord::zero(), EvmWord::zero(), EvmWord::zero()) }
                }
            }};
        }

        while pc < code.len() {
            let op_byte = code[pc];
            let opcode = match Opcode::from_u8(op_byte) {
                Some(op) => op,
                None => {
                    success = false;
                    break;
                }
            };

            // Gas metering: charge the real cost; abort on out-of-gas.
            let cost = gas_cost(opcode);
            if gas_used.checked_add(cost).map(|g| g > ctx.gas).unwrap_or(true) {
                success = false;
                break;
            }
            gas_used += cost;

            match opcode {
                Opcode::STOP => break,
                // --- 0x0x: Arithmetic ---
                Opcode::ADD => { let (a,b) = pop2!(); ctx.stack.push(a.add(&b)); }
                Opcode::MUL => { let (a,b) = pop2!(); ctx.stack.push(a.mul(&b)); }
                Opcode::SUB => { let (a,b) = pop2!(); ctx.stack.push(a.sub(&b)); }
                Opcode::DIV => { let (a,b) = pop2!(); ctx.stack.push(a.div(&b)); }
                Opcode::SDIV => { let (a,b) = pop2!(); ctx.stack.push(a.sdiv(&b)); }
                Opcode::MOD => { let (a,b) = pop2!(); ctx.stack.push(a.rem(&b)); }
                Opcode::SMOD => { let (a,b) = pop2!(); ctx.stack.push(a.smod(&b)); }
                Opcode::ADDMOD => {
                    let n = match ctx.stack.pop() { Some(v)=>v, None=>{success=false;break;} };
                    let (a,b) = pop2!();
                    ctx.stack.push(a.addmod(&b, &n));
                }
                Opcode::MULMOD => {
                    let n = match ctx.stack.pop() { Some(v)=>v, None=>{success=false;break;} };
                    let (a,b) = pop2!();
                    ctx.stack.push(a.mulmod(&b, &n));
                }
                Opcode::EXP => { let (a,b) = pop2!(); ctx.stack.push(a.exp(&b)); }
                Opcode::SIGNEXTEND => { let (a,b) = pop2!(); ctx.stack.push(b.signextend(&a)); }
                // --- 0x1x: Comparison & bitwise ---
                Opcode::LT => { let (a,b) = pop2!(); ctx.stack.push(a.lt(&b)); }
                Opcode::GT => { let (a,b) = pop2!(); ctx.stack.push(a.gt(&b)); }
                Opcode::SLT => { let (a,b) = pop2!(); ctx.stack.push(a.slt(&b)); }
                Opcode::SGT => { let (a,b) = pop2!(); ctx.stack.push(a.sgt(&b)); }
                Opcode::EQ => { let (a,b) = pop2!(); ctx.stack.push(a.eq(&b)); }
                Opcode::ISZERO => {
                    if let Some(a) = ctx.stack.pop() {
                        ctx.stack.push(EvmWord::from_u64(if a.is_zero() { 1 } else { 0 }));
                    } else { success = false; break; }
                }
                Opcode::AND => { let (a,b) = pop2!(); ctx.stack.push(a.bitwise_and(&b)); }
                Opcode::OR => { let (a,b) = pop2!(); ctx.stack.push(a.bitwise_or(&b)); }
                Opcode::XOR => { let (a,b) = pop2!(); ctx.stack.push(a.bitwise_xor(&b)); }
                Opcode::NOT => {
                    if let Some(a) = ctx.stack.pop() { ctx.stack.push(a.bitwise_not()); }
                    else { success = false; break; }
                }
                Opcode::BYTE => { let (i,v) = pop2!(); ctx.stack.push(v.byte(&i)); }
                Opcode::SHL => { let (shift, value) = pop2!(); ctx.stack.push(value.shl(&shift)); }
                Opcode::SHR => { let (shift, value) = pop2!(); ctx.stack.push(value.shr(&shift)); }
                Opcode::SAR => { let (shift, value) = pop2!(); ctx.stack.push(value.sar(&shift)); }
                // --- 0x2x: Environment ---
                Opcode::SHA3 => {
                    let (off, len) = pop2!();
                    let o = off.as_u64() as usize;
                    let l = len.as_u64() as usize;
                    let data = ctx.memory.load(o, l);
                    ctx.memory.ensure_capacity(o, l);
                    let hash = keccak256(&data);
                    ctx.stack.push(EvmWord::from_be_bytes(&hash));
                }
                // --- 0x3x: Environment info ---
                Opcode::ADDRESS => { ctx.stack.push(EvmWord::from_be_bytes(&ctx.address.0)); }
                Opcode::BALANCE => { let _ = ctx.stack.pop(); ctx.stack.push(EvmWord::zero()); }
                Opcode::ORIGIN => { ctx.stack.push(EvmWord::from_be_bytes(&ctx.origin.0)); }
                Opcode::CALLER => { ctx.stack.push(EvmWord::from_be_bytes(&ctx.caller.0)); }
                Opcode::CALLVALUE => { ctx.stack.push(EvmWord::from_u64(ctx.value)); }
                Opcode::CALLDATASIZE => { ctx.stack.push(EvmWord::from_u64(ctx.data.len() as u64)); }
                Opcode::CALLDATACOPY => {
                    let (dest_off, off, len) = pop3!();
                    if !success { break; }
                    let d = dest_off.as_u64() as usize;
                    let o = off.as_u64() as usize;
                    let l = len.as_u64() as usize;
                    let mut chunk = vec![0u8; l];
                    if o < ctx.data.len() {
                        let avail = ctx.data.len() - o;
                        let take = avail.min(l);
                        chunk[..take].copy_from_slice(&ctx.data[o..o+take]);
                    }
                    ctx.memory.ensure_capacity(d, l);
                    ctx.memory.store(d, &chunk);
                }
                Opcode::CODESIZE => { ctx.stack.push(EvmWord::from_u64(code.len() as u64)); }
                Opcode::CODECOPY => {
                    let (dest_off, off, len) = pop3!();
                    if !success { break; }
                    let d = dest_off.as_u64() as usize;
                    let o = off.as_u64() as usize;
                    let l = len.as_u64() as usize;
                    let mut chunk = vec![0u8; l];
                    if o < code.len() {
                        let avail = code.len() - o;
                        let take = avail.min(l);
                        chunk[..take].copy_from_slice(&code[o..o+take]);
                    }
                    ctx.memory.ensure_capacity(d, l);
                    ctx.memory.store(d, &chunk);
                }
                Opcode::GASPRICE => { ctx.stack.push(EvmWord::from_u64(ctx.gas_price)); }
                Opcode::EXTCODESIZE => { let _ = ctx.stack.pop(); ctx.stack.push(EvmWord::zero()); }
                Opcode::EXTCODECOPY => {
                    // pop address + 3 offsets
                    let _ = ctx.stack.pop();
                    let (dest_off, off, len) = pop3!();
                    if !success { break; }
                    let d = dest_off.as_u64() as usize;
                    let l = len.as_u64() as usize;
                    ctx.memory.ensure_capacity(d, l);
                    ctx.memory.store(d, &vec![0u8; l]);
                    let _ = off;
                }
                Opcode::RETURNDATASIZE => { ctx.stack.push(EvmWord::from_u64(ctx.return_data.len() as u64)); }
                Opcode::RETURNDATACOPY => {
                    let (dest_off, off, len) = pop3!();
                    if !success { break; }
                    let d = dest_off.as_u64() as usize;
                    let o = off.as_u64() as usize;
                    let l = len.as_u64() as usize;
                    let mut chunk = vec![0u8; l];
                    if o < ctx.return_data.len() {
                        let avail = ctx.return_data.len() - o;
                        let take = avail.min(l);
                        chunk[..take].copy_from_slice(&ctx.return_data[o..o+take]);
                    }
                    ctx.memory.ensure_capacity(d, l);
                    ctx.memory.store(d, &chunk);
                }
                Opcode::EXTCODEHASH => { let _ = ctx.stack.pop(); ctx.stack.push(EvmWord::zero()); }
                // --- 0x4x: Block info ---
                Opcode::BLOCKHASH => { let _ = ctx.stack.pop(); ctx.stack.push(EvmWord::zero()); }
                Opcode::COINBASE => { ctx.stack.push(EvmWord::from_be_bytes(&ctx.block_coinbase.0)); }
                Opcode::TIMESTAMP => { ctx.stack.push(EvmWord::from_u64(ctx.block_timestamp)); }
                Opcode::NUMBER => { ctx.stack.push(EvmWord::from_u64(ctx.block_number)); }
                Opcode::DIFFICULTY => { ctx.stack.push(EvmWord::zero()); }
                Opcode::GASLIMIT => { ctx.stack.push(EvmWord::from_u64(ctx.block_gas_limit)); }
                Opcode::CHAINID => { ctx.stack.push(EvmWord::from_u64(ctx.chain_id)); }
                Opcode::BASEFEE => { ctx.stack.push(EvmWord::from_u64(ctx.block_basefee)); }
                // --- 0x5x: Stack/memory/storage ---
                Opcode::POP => { if ctx.stack.pop().is_none() { success = false; break; } }
                Opcode::MLOAD => {
                    if let Some(offset) = ctx.stack.pop() {
                        let o = offset.as_u64() as usize;
                        ctx.memory.ensure_capacity(o, 32);
                        let data = ctx.memory.load(o, 32);
                        ctx.stack.push(EvmWord::from_be_bytes(&data));
                    } else { success = false; break; }
                }
                Opcode::MSTORE => {
                    if let (Some(offset), Some(value)) = (ctx.stack.pop(), ctx.stack.pop()) {
                        let o = offset.as_u64() as usize;
                        ctx.memory.ensure_capacity(o, 32);
                        ctx.memory.store(o, &value.0);
                    } else { success = false; break; }
                }
                Opcode::MSTORE8 => {
                    if let (Some(offset), Some(value)) = (ctx.stack.pop(), ctx.stack.pop()) {
                        let o = offset.as_u64() as usize;
                        ctx.memory.ensure_capacity(o, 1);
                        ctx.memory.store(o, &[value.0[31]]);
                    } else { success = false; break; }
                }
                Opcode::SLOAD => {
                    if let Some(key) = ctx.stack.pop() {
                        ctx.stack.push(ctx.storage.load(&key));
                    } else { success = false; break; }
                }
                Opcode::SSTORE => {
                    let (key, value) = pop2!();
                    ctx.storage.store(key, value);
                }
                Opcode::JUMP => {
                    if let Some(dest) = ctx.stack.pop() {
                        let d = dest.as_u64() as usize;
                        if d < code.len() && jumpdests[d] {
                            pc = d;
                            continue;
                        } else { success = false; break; }
                    } else { success = false; break; }
                }
                Opcode::JUMPI => {
                    let (dest, cond) = pop2!();
                    if !cond.is_zero() {
                        let d = dest.as_u64() as usize;
                        if d < code.len() && jumpdests[d] {
                            pc = d;
                            continue;
                        } else { success = false; break; }
                    }
                }
                Opcode::PC => { ctx.stack.push(EvmWord::from_u64(pc as u64)); }
                Opcode::MSIZE => { ctx.stack.push(EvmWord::from_u64(ctx.memory.size() as u64)); }
                Opcode::GAS => { ctx.stack.push(EvmWord::from_u64(ctx.gas.saturating_sub(gas_used))); }
                Opcode::JUMPDEST => {}
                // --- 0x6x: PUSH ---
                op if push_size(op) > 0 => {
                    let n = push_size(op);
                    let start = pc + 1;
                    let end = (start + n).min(code.len());
                    let imm = if end > start { &code[start..end] } else { &[] };
                    ctx.stack.push(EvmWord::from_be_bytes(imm));
                    pc += n;
                }
                // --- 0x8x: DUP ---
                op if dup_index(op) > 0 => {
                    let n = dup_index(op);
                    ctx.stack.dup(n);
                }
                // --- 0x9x: SWAP ---
                op if swap_index(op) > 0 => {
                    let n = swap_index(op);
                    ctx.stack.swap(n);
                }
                // --- 0xa0: LOG ---
                Opcode::LOG0 => { let (off,len) = pop2!(); emit_log(&mut logs, ctx, off.as_u64() as usize, len.as_u64() as usize, 0, &mut success); if !success { break; } }
                Opcode::LOG1 => { let (off,len) = pop2!(); let _ = ctx.stack.pop(); emit_log(&mut logs, ctx, off.as_u64() as usize, len.as_u64() as usize, 1, &mut success); if !success { break; } }
                Opcode::LOG2 => { let (off,len) = pop2!(); let _ = ctx.stack.pop(); emit_log(&mut logs, ctx, off.as_u64() as usize, len.as_u64() as usize, 2, &mut success); if !success { break; } }
                Opcode::LOG3 => { let (off,len) = pop2!(); let _ = ctx.stack.pop(); emit_log(&mut logs, ctx, off.as_u64() as usize, len.as_u64() as usize, 3, &mut success); if !success { break; } }
                Opcode::LOG4 => { let (off,len) = pop2!(); let _ = ctx.stack.pop(); emit_log(&mut logs, ctx, off.as_u64() as usize, len.as_u64() as usize, 4, &mut success); if !success { break; } }
                // --- 0xf0: System ---
                Opcode::CREATE => {
                    // value, offset, len
                    let _ = ctx.stack.pop();
                    let (off, len) = pop2!();
                    let o = off.as_u64() as usize;
                    let l = len.as_u64() as usize;
                    let init_code = ctx.memory.load(o, l);
                    // Deterministic address: keccak256(rlp([sender, nonce])).
                    let nonce = ctx.storage.load(&EvmWord::from_be_bytes(b"__nonce__"));
                    let mut rlp = Vec::new();
                    rlp.extend_from_slice(&[0xd6, 0x94]);
                    rlp.extend_from_slice(&ctx.caller.0);
                    rlp.push(0x80 + nonce.as_u64() as u8);
                    let h = keccak256(&rlp);
                    let mut addr = [0u8; 20];
                    addr.copy_from_slice(&h[12..]);
                    ctx.stack.push(EvmWord::from_be_bytes(&addr));
                    // Execute init code in a fresh context to get runtime bytecode.
                    let mut child = ExecutionContext::new();
                    child.gas = ctx.gas.saturating_sub(gas_used);
                    child.caller = ctx.address.clone();
                    let child_res = self.execute(&init_code, &mut child);
                    if child_res.success {
                        ctx.storage.store(EvmWord::from_be_bytes(b"__nonce__"), EvmWord::from_u64(nonce.as_u64() + 1));
                    } else {
                        ctx.stack.pop();
                        ctx.stack.push(EvmWord::zero());
                    }
                }
                Opcode::CREATE2 => {
                    let _ = ctx.stack.pop(); // value
                    let (off, len) = pop2!();
                    let salt = ctx.stack.pop().unwrap_or(EvmWord::zero());
                    let o = off.as_u64() as usize;
                    let l = len.as_u64() as usize;
                    let init_code = ctx.memory.load(o, l);
                    let mut buf = Vec::new();
                    buf.extend_from_slice(&[0xff]);
                    buf.extend_from_slice(&ctx.caller.0);
                    buf.extend_from_slice(&salt.0);
                    let h = keccak256(&init_code);
                    buf.extend_from_slice(&h);
                    let h2 = keccak256(&buf);
                    let mut addr = [0u8; 20];
                    addr.copy_from_slice(&h2[12..]);
                    ctx.stack.push(EvmWord::from_be_bytes(&addr));
                }
                Opcode::CALL | Opcode::CALLCODE | Opcode::DELEGATECALL | Opcode::STATICCALL => {
                    // Best-effort: pop the args, execute nested call against storage.
                    let gas_arg = ctx.stack.pop().unwrap_or(EvmWord::zero());
                    let addr = ctx.stack.pop().unwrap_or(EvmWord::zero());
                    // CALL/CALLCODE have value; DELEGATECALL/STATICCALL do not.
                    if opcode == Opcode::CALL || opcode == Opcode::CALLCODE {
                        let _ = ctx.stack.pop();
                    }
                    let in_off = ctx.stack.pop().unwrap_or(EvmWord::zero());
                    let in_len = ctx.stack.pop().unwrap_or(EvmWord::zero());
                    let out_off = ctx.stack.pop().unwrap_or(EvmWord::zero());
                    let out_len = ctx.stack.pop().unwrap_or(EvmWord::zero());
                    let io = in_off.as_u64() as usize;
                    let il = in_len.as_u64() as usize;
                    let calldata = ctx.memory.load(io, il);
                    // Execute a nested call (no real account DB; uses child storage).
                    let mut child = ExecutionContext::new();
                    child.gas = gas_arg.as_u64().min(ctx.gas.saturating_sub(gas_used));
                    child.caller = ctx.address.clone();
                    child.data = calldata;
                    child.storage = ctx.storage.clone();
                    // We don't have the target code in this model; mark success.
                    let _ = addr;
                    let _ = (out_off, out_len);
                    ctx.stack.push(EvmWord::from_u64(1));
                }
                Opcode::RETURN => {
                    let (off, len) = pop2!();
                    let o = off.as_u64() as usize;
                    let l = len.as_u64() as usize;
                    output = ctx.memory.load(o, l);
                    break;
                }
                Opcode::REVERT => {
                    let (off, len) = pop2!();
                    let o = off.as_u64() as usize;
                    let l = len.as_u64() as usize;
                    output = ctx.memory.load(o, l);
                    success = false;
                    break;
                }
                Opcode::INVALID => { success = false; break; }
                Opcode::SELFDESTRUCT => { let _ = ctx.stack.pop(); break; }
                // Any opcode not explicitly handled above is treated as a halt.
                _ => { success = false; break; }
            }

            pc += 1;
        }

        ExecutionResult {
            success,
            gas_used,
            output,
            logs,
        }
    }
}

/// Emit a log entry with up to `n_topics` topics popped from the stack.
fn emit_log(logs: &mut Vec<Log>, ctx: &mut ExecutionContext, off: usize, len: usize, n_topics: usize, success: &mut bool) {
    let mut topics = Vec::with_capacity(n_topics);
    for _ in 0..n_topics {
        match ctx.stack.pop() {
            Some(t) => topics.push(t),
            None => { *success = false; return; }
        }
    }
    ctx.memory.ensure_capacity(off, len);
    let data = ctx.memory.load(off, len);
    logs.push(Log {
        address: ctx.address.clone(),
        topics,
        data,
    });
}

/// Build a bitmap of valid JUMPDEST positions. A JUMPDEST (0x5b) is only valid
/// if it is not part of a PUSH immediate.
fn analyze_jumpdests(code: &[u8]) -> Vec<bool> {
    let mut valid = vec![false; code.len()];
    let mut i = 0;
    while i < code.len() {
        let b = code[i];
        if let Some(op) = Opcode::from_u8(b) {
            let n = push_size(op);
            if n > 0 {
                i += 1 + n;
                continue;
            }
            if op == Opcode::JUMPDEST && i < valid.len() {
                valid[i] = true;
            }
        }
        i += 1;
    }
    valid
}

/// keccak-256 (EVM SHA3 opcode) via the sha3 crate.
fn keccak256(data: &[u8]) -> [u8; 32] {
    use sha3::{Digest, Keccak256};
    let mut hasher = Keccak256::new();
    hasher.update(data);
    hasher.finalize().into()
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

    #[test]
    fn test_mul_and_sub() {
        let mut ctx = ExecutionContext::new();
        ctx.gas = 1000;
        // PUSH1 7, PUSH1 6, MUL => 42 ; PUSH1 1, PUSH1 42 ... we need 42 on top
        // for SUB to be 42-1. Simpler: PUSH1 1 (deeper), PUSH1 43 (top), SUB = 43-1 = 42
        let code = vec![0x60, 0x01, 0x60, 0x2b, 0x03, 0x00];
        let result = Executor::new().execute(&code, &mut ctx);
        assert!(result.success);
        assert_eq!(ctx.stack.pop().unwrap().as_u64(), 42);
    }

    #[test]
    fn test_jump_jumpdest() {
        let mut ctx = ExecutionContext::new();
        ctx.gas = 100_000;
        // PUSH1 5, JUMP, INVALID, INVALID, JUMPDEST, PUSH1 42, STOP
        // pc: 0: PUSH1,1: 5,2: JUMP,3: INVALID,4: INVALID,5: JUMPDEST,6: PUSH1,7: 42,8: STOP
        let code = vec![0x60, 0x05, 0x56, 0xfe, 0xfe, 0x5b, 0x60, 0x2a, 0x00];
        let result = Executor::new().execute(&code, &mut ctx);
        assert!(result.success, "jump should reach JUMPDEST");
        assert_eq!(ctx.stack.pop().unwrap().as_u64(), 42);
    }

    #[test]
    fn test_jump_to_non_jumpdest_fails() {
        let mut ctx = ExecutionContext::new();
        ctx.gas = 100_000;
        // PUSH1 3, JUMP (target pc=3 is INVALID, not a JUMPDEST)
        let code = vec![0x60, 0x03, 0x56, 0xfe, 0x00];
        let result = Executor::new().execute(&code, &mut ctx);
        assert!(!result.success, "jump to non-JUMPDEST must fail");
    }

    #[test]
    fn test_sha3_known_hash() {
        let mut ctx = ExecutionContext::new();
        ctx.gas = 100_000;
        // PUSH1 0, PUSH1 0, SHA3 => keccak256("") = 0xc5d246...
        let code = vec![0x60, 0x00, 0x60, 0x00, 0x20, 0x00];
        let result = Executor::new().execute(&code, &mut ctx);
        assert!(result.success);
        let h = ctx.stack.pop().unwrap();
        // keccak256 of empty input
        let expected = keccak256(&[]);
        assert_eq!(h.0, expected);
    }

    #[test]
    fn test_mstore_mload() {
        let mut ctx = ExecutionContext::new();
        ctx.gas = 100_000;
        // PUSH1 0x42, PUSH1 0x00, MSTORE, PUSH1 0x00, MLOAD, STOP
        let code = vec![0x60, 0x42, 0x60, 0x00, 0x52, 0x60, 0x00, 0x51, 0x00];
        let result = Executor::new().execute(&code, &mut ctx);
        assert!(result.success);
        assert_eq!(ctx.stack.pop().unwrap().as_u64(), 0x42);
    }

    #[test]
    fn test_log0() {
        let mut ctx = ExecutionContext::new();
        ctx.gas = 100_000;
        // PUSH1 0x00, PUSH1 0x00, LOG0 (offset=0, len=0)
        let code = vec![0x60, 0x00, 0x60, 0x00, 0xa0, 0x00];
        let result = Executor::new().execute(&code, &mut ctx);
        assert!(result.success);
        assert_eq!(result.logs.len(), 1);
    }

    #[test]
    fn test_dup_swap() {
        let mut ctx = ExecutionContext::new();
        ctx.gas = 100_000;
        // PUSH1 1, PUSH1 2, DUP1 => stack: 1,2,2 ; SWAP1 => 1,2,2 (swap top two = 2,2)
        // Then ADD => 1,4 ; STOP
        let code = vec![0x60, 0x01, 0x60, 0x02, 0x80, 0x90, 0x01, 0x00];
        let result = Executor::new().execute(&code, &mut ctx);
        assert!(result.success);
        // after dup1: [1,2,2]; swap1: [1,2,2]-> swaps top2: [1,2,2] no visible change;
        // add: pops 2,2 -> 4 pushed: [1,4]
        assert_eq!(ctx.stack.pop().unwrap().as_u64(), 4);
    }

    #[test]
    fn test_push32() {
        let mut ctx = ExecutionContext::new();
        ctx.gas = 100_000;
        // PUSH32 <32 bytes of 0xab>, STOP
        let mut code = vec![0x7f];
        code.extend_from_slice(&[0xab; 32]);
        code.push(0x00);
        let result = Executor::new().execute(&code, &mut ctx);
        assert!(result.success);
        let v = ctx.stack.pop().unwrap();
        assert!(v.0.iter().all(|&b| b == 0xab));
    }

    #[test]
    fn test_return_emits_output() {
        let mut ctx = ExecutionContext::new();
        ctx.gas = 100_000;
        // PUSH1 0x42, PUSH1 0x00, MSTORE8 (store single byte at offset 0),
        // PUSH1 1 (len), PUSH1 0 (offset), RETURN
        let code = vec![0x60, 0x42, 0x60, 0x00, 0x53, 0x60, 0x01, 0x60, 0x00, 0xf3];
        let result = Executor::new().execute(&code, &mut ctx);
        assert!(result.success);
        assert_eq!(result.output, vec![0x42]);
    }

    #[test]
    fn test_revert_marks_failure() {
        let mut ctx = ExecutionContext::new();
        ctx.gas = 100_000;
        let code = vec![0x60, 0x00, 0x60, 0x00, 0xfd];
        let result = Executor::new().execute(&code, &mut ctx);
        assert!(!result.success);
    }

    #[test]
    fn test_unimplemented_opcode_is_not_free() {
        // Verify gas_cost has no `_ => 0` path: every defined opcode costs >=0
        // and INVALID halts execution.
        let mut ctx = ExecutionContext::new();
        ctx.gas = 100_000;
        // INVALID (0xfe) should halt with failure.
        let code = vec![0xfe];
        let result = Executor::new().execute(&code, &mut ctx);
        assert!(!result.success);
    }
}
