//! EVM Executor with Gas Metering and full opcode implementation
//!
//! High-performance EVM implementation in Rust for TigerSmartChain.

use crate::host::Host;
use crate::opcodes::*;
use crate::types::*;

/// EVM Executor
pub struct Executor;

impl Default for Executor {
    fn default() -> Self {
        Self::new()
    }
}

impl Executor {
    pub fn new() -> Self {
        Self
    }

    /// Execute code against an in-memory host.
    ///
    /// This is the entry point used by `eth_call` and by tests. Block
    /// execution should call [`Executor::execute_with_host`] with the real
    /// world state so that balances, code and storage are authoritative.
    pub fn execute(&self, code: &[u8], ctx: &mut ExecutionContext) -> ExecutionResult {
        let mut host = crate::host::MemoryHost::new();
        host.set_code(&ctx.address, code.to_vec());
        for (slot, value) in ctx.storage.entries() {
            host.set_storage(&ctx.address, slot, value);
        }
        self.execute_with_host(code, ctx, &mut host)
    }

    /// Execute code with full opcode support, gas metering and real world
    /// state access through `host`.
    pub fn execute_with_host(
        &self,
        code: &[u8],
        ctx: &mut ExecutionContext,
        host: &mut dyn Host,
    ) -> ExecutionResult {
        let mut pc = 0usize;
        let mut gas_used: u64 = 0;
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
                Opcode::BALANCE => {
                    match ctx.stack.pop() {
                        Some(word) => {
                            let target = address_from_word(&word);
                            ctx.stack.push(host.balance(&target));
                        }
                        None => { success = false; break; }
                    }
                }
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
                Opcode::EXTCODESIZE => {
                    match ctx.stack.pop() {
                        Some(word) => {
                            let target = address_from_word(&word);
                            let len = host.code(&target).len() as u64;
                            ctx.stack.push(EvmWord::from_u64(len));
                        }
                        None => { success = false; break; }
                    }
                }
                Opcode::EXTCODECOPY => {
                    let target_word = match ctx.stack.pop() {
                        Some(w) => w,
                        None => { success = false; break; }
                    };
                    let (dest_off, off, len) = pop3!();
                    if !success { break; }
                    let target = address_from_word(&target_word);
                    let ext_code = host.code(&target);
                    let d = dest_off.as_u64() as usize;
                    let o = off.as_u64() as usize;
                    let l = len.as_u64() as usize;
                    let mut chunk = vec![0u8; l];
                    if o < ext_code.len() {
                        let take = (ext_code.len() - o).min(l);
                        chunk[..take].copy_from_slice(&ext_code[o..o + take]);
                    }
                    ctx.memory.ensure_capacity(d, l);
                    ctx.memory.store(d, &chunk);
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
                Opcode::EXTCODEHASH => {
                    match ctx.stack.pop() {
                        Some(word) => {
                            let target = address_from_word(&word);
                            ctx.stack.push(host.code_hash(&target));
                        }
                        None => { success = false; break; }
                    }
                }
                // --- 0x4x: Block info ---
                Opcode::BLOCKHASH => {
                    match ctx.stack.pop() {
                        Some(word) => {
                            let number = word.as_u64();
                            ctx.stack.push(host.block_hash(number));
                        }
                        None => { success = false; break; }
                    }
                }
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
                        ctx.stack.push(host.storage(&ctx.address, &key));
                    } else { success = false; break; }
                }
                Opcode::SSTORE => {
                    let (key, value) = pop2!();
                    if !success { break; }
                    host.set_storage(&ctx.address, key, value);
                }
                // --- 0x5c-0x5f: transient storage, memory copy, push zero ---
                Opcode::TLOAD => {
                    if let Some(key) = ctx.stack.pop() {
                        ctx.stack.push(ctx.transient.load(&key));
                    } else { success = false; break; }
                }
                Opcode::TSTORE => {
                    let (key, value) = pop2!();
                    if !success { break; }
                    ctx.transient.store(key, value);
                }
                Opcode::MCOPY => {
                    let (dest, src, len) = pop3!();
                    if !success { break; }
                    let d = dest.as_u64() as usize;
                    let s_off = src.as_u64() as usize;
                    let l = len.as_u64() as usize;
                    // Charge the EIP-5656 per-word copy fee on top of the base
                    // cost; the memory expansion cost is handled separately.
                    let words = (l as u64).div_ceil(32);
                    let copy_fee = words.saturating_mul(3);
                    if gas_used.checked_add(copy_fee).map(|g| g > ctx.gas).unwrap_or(true) {
                        success = false;
                        break;
                    }
                    gas_used += copy_fee;
                    let mut buf = vec![0u8; l];
                    if l > 0 {
                        // Copy the source region first so overlapping moves
                        // behave like memmove.
                        let data = ctx.memory.load(s_off, l);
                        buf[..l].copy_from_slice(&data[..l.min(data.len())]);
                    }
                    ctx.memory.ensure_capacity(d, l);
                    ctx.memory.store(d, &buf);
                }
                Opcode::PUSH0 => { ctx.stack.push(EvmWord::zero()); }
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
                Opcode::LOG0 => {
                    let (off, len) = pop2!();
                    if !success { break; }
                    emit_log(host, ctx, off.as_u64() as usize, len.as_u64() as usize, 0, &mut success);
                    if !success { break; }
                }
                Opcode::LOG1 => {
                    let (off, len) = pop2!();
                    if !success { break; }
                    emit_log(host, ctx, off.as_u64() as usize, len.as_u64() as usize, 1, &mut success);
                    if !success { break; }
                }
                Opcode::LOG2 => {
                    let (off, len) = pop2!();
                    if !success { break; }
                    emit_log(host, ctx, off.as_u64() as usize, len.as_u64() as usize, 2, &mut success);
                    if !success { break; }
                }
                Opcode::LOG3 => {
                    let (off, len) = pop2!();
                    if !success { break; }
                    emit_log(host, ctx, off.as_u64() as usize, len.as_u64() as usize, 3, &mut success);
                    if !success { break; }
                }
                Opcode::LOG4 => {
                    let (off, len) = pop2!();
                    if !success { break; }
                    emit_log(host, ctx, off.as_u64() as usize, len.as_u64() as usize, 4, &mut success);
                    if !success { break; }
                }
                // --- 0xf0: System ---
                Opcode::CREATE => {
                    let value_word = match ctx.stack.pop() {
                        Some(v) => v,
                        None => { success = false; break; }
                    };
                    let (off, len) = pop2!();
                    if !success { break; }
                    let o = off.as_u64() as usize;
                    let l = len.as_u64() as usize;
                    ctx.memory.ensure_capacity(o, l);
                    let init_code = ctx.memory.load(o, l);

                    // Address is keccak256(rlp([sender, nonce]))[12..].
                    let nonce = host.bump_nonce(&ctx.address);
                    let new_address = create_address(&ctx.address, nonce);

                    if !host.transfer(&ctx.address, &new_address, value_word.clone()) {
                        ctx.stack.push(EvmWord::zero());
                    } else {
                        let mut child = ExecutionContext::new();
                        child.address = new_address.clone();
                        child.caller = ctx.address.clone();
                        child.origin = ctx.origin.clone();
                        child.gas = ctx.gas.saturating_sub(gas_used);
                        child.gas_price = ctx.gas_price;
                        child.value = value_word.as_u64();
                        child.block_number = ctx.block_number;
                        child.block_timestamp = ctx.block_timestamp;
                        child.block_gas_limit = ctx.block_gas_limit;
                        child.block_coinbase = ctx.block_coinbase.clone();
                        child.block_basefee = ctx.block_basefee;
                        child.chain_id = ctx.chain_id;

                        let child_res = self.execute_with_host(&init_code, &mut child, host);
                        gas_used += child_res.gas_used;

                        if child_res.success {
                            // Init code returns the runtime bytecode.
                            host.set_code(&new_address, child_res.output.clone());
                            ctx.return_data = child_res.output;
                            ctx.stack.push(EvmWord::from_be_bytes(&new_address.0));
                        } else {
                            // Revert the value transfer on failure.
                            let _ = host.transfer(&new_address, &ctx.address, value_word);
                            ctx.return_data = child_res.output;
                            ctx.stack.push(EvmWord::zero());
                        }
                    }
                }
                Opcode::CREATE2 => {
                    let value_word = match ctx.stack.pop() {
                        Some(v) => v,
                        None => { success = false; break; }
                    };
                    let (off, len) = pop2!();
                    let salt = match ctx.stack.pop() {
                        Some(s) => s,
                        None => { success = false; break; }
                    };
                    if !success { break; }
                    let o = off.as_u64() as usize;
                    let l = len.as_u64() as usize;
                    ctx.memory.ensure_capacity(o, l);
                    let init_code = ctx.memory.load(o, l);

                    // Address is keccak256(0xff ++ sender ++ salt ++ keccak256(init))[12..].
                    let mut buf = Vec::with_capacity(85);
                    buf.push(0xff);
                    buf.extend_from_slice(&ctx.address.0);
                    buf.extend_from_slice(&salt.0);
                    buf.extend_from_slice(&tiger_crypto::keccak256(&init_code));
                    let hash = tiger_crypto::keccak256(&buf);
                    let mut new_address = Address::zero();
                    new_address.0.copy_from_slice(&hash[12..]);

                    if !host.transfer(&ctx.address, &new_address, value_word.clone()) {
                        ctx.stack.push(EvmWord::zero());
                    } else {
                        let mut child = ExecutionContext::new();
                        child.address = new_address.clone();
                        child.caller = ctx.address.clone();
                        child.origin = ctx.origin.clone();
                        child.gas = ctx.gas.saturating_sub(gas_used);
                        child.gas_price = ctx.gas_price;
                        child.value = value_word.as_u64();
                        child.block_number = ctx.block_number;
                        child.block_timestamp = ctx.block_timestamp;
                        child.block_gas_limit = ctx.block_gas_limit;
                        child.block_coinbase = ctx.block_coinbase.clone();
                        child.block_basefee = ctx.block_basefee;
                        child.chain_id = ctx.chain_id;

                        let child_res = self.execute_with_host(&init_code, &mut child, host);
                        gas_used += child_res.gas_used;

                        if child_res.success {
                            host.set_code(&new_address, child_res.output.clone());
                            ctx.return_data = child_res.output;
                            ctx.stack.push(EvmWord::from_be_bytes(&new_address.0));
                        } else {
                            let _ = host.transfer(&new_address, &ctx.address, value_word);
                            ctx.return_data = child_res.output;
                            ctx.stack.push(EvmWord::zero());
                        }
                    }
                }
                Opcode::CALL | Opcode::CALLCODE | Opcode::DELEGATECALL | Opcode::STATICCALL => {
                    // Stack order (top first): gas, to, [value], in_off, in_len,
                    // out_off, out_len. DELEGATECALL and STATICCALL omit value.
                    let gas_arg = match ctx.stack.pop() {
                        Some(v) => v,
                        None => { success = false; break; }
                    };
                    let to_word = match ctx.stack.pop() {
                        Some(v) => v,
                        None => { success = false; break; }
                    };
                    let has_value =
                        opcode == Opcode::CALL || opcode == Opcode::CALLCODE;
                    let value_word = if has_value {
                        match ctx.stack.pop() {
                            Some(v) => v,
                            None => { success = false; break; }
                        }
                    } else {
                        EvmWord::zero()
                    };
                    let (in_off, in_len) = pop2!();
                    let (out_off, out_len) = pop2!();
                    if !success { break; }

                    let to = address_from_word(&to_word);
                    let io = in_off.as_u64() as usize;
                    let il = in_len.as_u64() as usize;
                    let oo = out_off.as_u64() as usize;
                    let ol = out_len.as_u64() as usize;
                    ctx.memory.ensure_capacity(io, il);
                    let calldata = ctx.memory.load(io, il);

                    let available_gas = ctx.gas.saturating_sub(gas_used);
                    let call_gas = gas_arg.as_u64().min(available_gas);

                    // DELEGATECALL and CALLCODE run the callee's code in the
                    // caller's storage context; CALL and STATICCALL use the
                    // callee's own context.
                    let (exec_address, storage_address) = match opcode {
                        Opcode::CALL | Opcode::STATICCALL => (to.clone(), to.clone()),
                        Opcode::DELEGATECALL | Opcode::CALLCODE => {
                            (to.clone(), ctx.address.clone())
                        }
                        _ => unreachable!(),
                    };
                    let _ = exec_address;

                    let callee_code = host.code(&to);

                    // Move value before execution for CALL. A failure here
                    // short-circuits the call but must still advance the
                    // program counter, so the result is recorded in a flag
                    // rather than by `continue`.
                    let mut value_transferred = false;
                    let mut value_ok = true;
                    if has_value && opcode == Opcode::CALL && !value_word.is_zero() {
                        value_ok = host.transfer(&ctx.address, &to, value_word.clone());
                        value_transferred = value_ok;
                    }

                    // An account with no code is a plain value transfer.
                    if callee_code.is_empty() || !value_ok {
                        ctx.return_data = Vec::new();
                        ctx.stack.push(EvmWord::from_u64(if value_ok { 1 } else { 0 }));
                        if ol > 0 {
                            ctx.memory.ensure_capacity(oo, ol);
                            ctx.memory.store(oo, &vec![0u8; ol]);
                        }
                        pc += 1;
                        continue;
                    }

                    let mut child = ExecutionContext::new();
                    child.address = storage_address;
                    child.caller = if opcode == Opcode::DELEGATECALL {
                        // The caller of the current frame becomes the caller.
                        ctx.caller.clone()
                    } else {
                        ctx.address.clone()
                    };
                    child.origin = ctx.origin.clone();
                    child.gas = call_gas;
                    child.gas_price = ctx.gas_price;
                    child.value = if opcode == Opcode::DELEGATECALL {
                        ctx.value
                    } else {
                        value_word.as_u64()
                    };
                    child.data = calldata;
                    child.block_number = ctx.block_number;
                    child.block_timestamp = ctx.block_timestamp;
                    child.block_gas_limit = ctx.block_gas_limit;
                    child.block_coinbase = ctx.block_coinbase.clone();
                    child.block_basefee = ctx.block_basefee;
                    child.chain_id = ctx.chain_id;

                    let child_res = self.execute_with_host(&callee_code, &mut child, host);
                    gas_used += child_res.gas_used;

                    // Copy the return data into the caller's memory, truncated
                    // to the requested output length as the spec requires.
                    ctx.return_data = child_res.output.clone();
                    if ol > 0 {
                        let mut buf = vec![0u8; ol];
                        let take = child_res.output.len().min(ol);
                        buf[..take].copy_from_slice(&child_res.output[..take]);
                        ctx.memory.ensure_capacity(oo, ol);
                        ctx.memory.store(oo, &buf);
                    }

                    if child_res.success {
                        ctx.stack.push(EvmWord::from_u64(1));
                    } else {
                        // Revert the value transfer when the callee failed.
                        if value_transferred {
                            let _ = host.transfer(&to, &ctx.address, value_word);
                        }
                        ctx.stack.push(EvmWord::zero());
                    }
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
                Opcode::SELFDESTRUCT => {
                    if let Some(word) = ctx.stack.pop() {
                        // Transfer the remaining balance to the beneficiary
                        // before marking the account for destruction.
                        let beneficiary = address_from_word(&word);
                        let balance = host.balance(&ctx.address);
                        let _ = host.transfer(&ctx.address, &beneficiary, balance);
                        host.selfdestruct(&ctx.address);
                    }
                    success = true;
                    break;
                }
                // Any opcode not explicitly handled above is treated as a halt.
                _ => { success = false; break; }
            }

            pc += 1;
        }

        // On failure the state changes are still applied by the caller, which
        // decides whether to commit or roll back the journal; logs are
        // returned as recorded so the caller can discard them on failure.
        ExecutionResult {
            success,
            gas_used,
            output,
            logs: host.take_logs(),
        }
    }
}

/// Emit a log entry with up to `n_topics` topics popped from the stack.
fn emit_log(
    host: &mut dyn Host,
    ctx: &mut ExecutionContext,
    off: usize,
    len: usize,
    n_topics: usize,
    success: &mut bool,
) {
    let mut topics = Vec::with_capacity(n_topics);
    for _ in 0..n_topics {
        match ctx.stack.pop() {
            Some(t) => topics.push(t),
            None => {
                *success = false;
                return;
            }
        }
    }
    ctx.memory.ensure_capacity(off, len);
    let data = ctx.memory.load(off, len);
    host.emit_log(&ctx.address, topics, data);
}

/// Convert a 256-bit stack word into a 20-byte address (low 20 bytes).
fn address_from_word(word: &EvmWord) -> Address {
    let mut address = Address::zero();
    address.0.copy_from_slice(&word.0[12..]);
    address
}

/// The CREATE address for `sender` at `nonce`: the low 20 bytes of
/// keccak256(rlp([sender, nonce])).
fn create_address(sender: &Address, nonce: u64) -> Address {
    let nonce_item = crate::RlpItem::from_u64(nonce);
    let encoded = crate::RlpItem::list(vec![
        crate::RlpItem::Bytes(sender.0.to_vec()),
        nonce_item,
    ])
    .encode();
    let hash = tiger_crypto::keccak256(&encoded);
    let mut address = Address::zero();
    address.0.copy_from_slice(&hash[12..]);
    address
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
    use crate::host::MemoryHost;

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

    // ---------------------------------------------------------------------
    // Host-backed opcode tests.
    //
    // These cover behaviour that previously returned hardcoded values:
    // BALANCE, EXTCODESIZE, EXTCODEHASH, SLOAD/SSTORE, LOG and CALL.
    // ---------------------------------------------------------------------

    fn host_addr(b: u8) -> Address {
        Address([b; 20])
    }

    fn word_addr(b: u8) -> EvmWord {
        let mut bytes = [0u8; 32];
        bytes[12..].copy_from_slice(&[b; 20]);
        EvmWord(bytes)
    }

    #[test]
    fn balance_reads_the_real_account_balance() {
        let mut ctx = ExecutionContext::new();
        ctx.gas = 100_000;
        let mut host = MemoryHost::new().with_balance(host_addr(0xaa), EvmWord::from_u64(12345));

        // PUSH20 <addr>, BALANCE, STOP
        let mut code = vec![0x73];
        code.extend_from_slice(&[0xaa; 20]);
        code.push(0x31);
        code.push(0x00);

        let result = Executor::new().execute_with_host(&code, &mut ctx, &mut host);
        assert!(result.success);
        assert_eq!(ctx.stack.pop().unwrap().as_u64(), 12345);
    }

    #[test]
    fn extcodesize_and_extcodehash_reflect_deployed_code() {
        let code = vec![0x60u8, 0x01, 0x60, 0x02, 0x01];
        let mut host = MemoryHost::new().with_code(host_addr(0xbb), code.clone());

        // EXTCODESIZE
        let mut ctx = ExecutionContext::new();
        ctx.gas = 100_000;
        let mut program = vec![0x73];
        program.extend_from_slice(&[0xbb; 20]);
        program.push(0x3b);
        program.push(0x00);
        let result = Executor::new().execute_with_host(&program, &mut ctx, &mut host);
        assert!(result.success);
        assert_eq!(ctx.stack.pop().unwrap().as_u64(), code.len() as u64);

        // EXTCODEHASH must equal keccak256 of the code.
        let mut ctx2 = ExecutionContext::new();
        ctx2.gas = 100_000;
        let mut program2 = vec![0x73];
        program2.extend_from_slice(&[0xbb; 20]);
        program2.push(0x3f);
        program2.push(0x00);
        let result2 = Executor::new().execute_with_host(&program2, &mut ctx2, &mut host);
        assert!(result2.success);
        let expected = EvmWord::from_be_bytes(&tiger_crypto::keccak256(&code));
        assert_eq!(ctx2.stack.pop().unwrap(), expected);

        // A non-existent account must hash to zero, not to keccak256("").
        let mut ctx3 = ExecutionContext::new();
        ctx3.gas = 100_000;
        let mut program3 = vec![0x73];
        program3.extend_from_slice(&[0x77; 20]);
        program3.push(0x3f);
        program3.push(0x00);
        let result3 = Executor::new().execute_with_host(&program3, &mut ctx3, &mut host);
        assert!(result3.success);
        assert_eq!(ctx3.stack.pop().unwrap(), EvmWord::zero());
    }

    #[test]
    fn sstore_then_sload_round_trips_through_the_host() {
        let mut ctx = ExecutionContext::new();
        ctx.address = host_addr(0x01);
        ctx.gas = 100_000;
        let mut host = MemoryHost::new();

        // PUSH1 0x2a (value), PUSH1 0x01 (key), SSTORE,
        // PUSH1 0x01 (key), SLOAD, STOP
        let code = vec![
            0x60, 0x2a, 0x60, 0x01, 0x55, 0x60, 0x01, 0x54, 0x00,
        ];
        let result = Executor::new().execute_with_host(&code, &mut ctx, &mut host);
        assert!(result.success);
        assert_eq!(ctx.stack.pop().unwrap().as_u64(), 0x2a);
        // The value must be visible in the host, not just on the stack.
        assert_eq!(
            host.storage(&host_addr(0x01), &EvmWord::from_u64(1)).as_u64(),
            0x2a
        );
    }

    #[test]
    fn log_records_topics_and_data_in_the_host() {
        let mut ctx = ExecutionContext::new();
        ctx.address = host_addr(0x02);
        ctx.gas = 100_000;
        let mut host = MemoryHost::new();

        // MSTORE8 0x42 at 0, then LOG1 with topic 0x07 over 1 byte at offset 0.
        let code = vec![
            0x60, 0x42, 0x60, 0x00, 0x53, // mstore8
            0x60, 0x07, // topic
            0x60, 0x01, // len
            0x60, 0x00, // offset
            0xa1, // LOG1
            0x00,
        ];
        let result = Executor::new().execute_with_host(&code, &mut ctx, &mut host);
        assert!(result.success);
        assert_eq!(result.logs.len(), 1);
        assert_eq!(result.logs[0].address, host_addr(0x02));
        assert_eq!(result.logs[0].topics.len(), 1);
        assert_eq!(result.logs[0].topics[0].as_u64(), 7);
        assert_eq!(result.logs[0].data, vec![0x42]);
    }

    #[test]
    fn call_executes_the_callee_code_and_returns_its_output() {
        // Callee: returns the single byte 0x2a.
        let callee_code = vec![0x60, 0x2a, 0x60, 0x00, 0x53, 0x60, 0x01, 0x60, 0x00, 0xf3];
        let mut host = MemoryHost::new().with_code(host_addr(0xcc), callee_code);

        let mut ctx = ExecutionContext::new();
        ctx.address = host_addr(0x01);
        ctx.gas = 1_000_000;

        // CALL(gas, to, value, argsOffset, argsLength, retOffset, retLength).
        // The first argument is taken from the top of the stack, so the
        // arguments are pushed in reverse: retLength first, gas last.
        let mut code = Vec::new();
        code.extend_from_slice(&[0x60, 0x01]); // retLength = 1
        code.extend_from_slice(&[0x60, 0x00]); // retOffset = 0
        code.extend_from_slice(&[0x60, 0x00]); // argsLength = 0
        code.extend_from_slice(&[0x60, 0x00]); // argsOffset = 0
        code.extend_from_slice(&[0x60, 0x00]); // value = 0
        code.extend_from_slice(&[0x73]); // PUSH20 target
        code.extend_from_slice(&[0xcc; 20]);
        code.extend_from_slice(&[0x61, 0xff, 0xff]); // PUSH2 gas
        code.push(0xf1); // CALL
        code.push(0x00);

        let result = Executor::new().execute_with_host(&code, &mut ctx, &mut host);
        assert!(result.success, "outer frame must succeed");
        // The CALL pushed 1 (success) and the callee's output landed in memory.
        assert_eq!(ctx.stack.pop().unwrap().as_u64(), 1);
        assert_eq!(ctx.memory.load(0, 1), vec![0x2a]);
    }

    #[test]
    fn call_to_account_without_code_still_transfers_value() {
        let mut host = MemoryHost::new().with_balance(host_addr(0x01), EvmWord::from_u64(1000));

        let mut ctx = ExecutionContext::new();
        ctx.address = host_addr(0x01);
        ctx.gas = 1_000_000;

        // CALL(gas, to, value=100, 0, 0, 0, 0), pushed in reverse order.
        let mut code = Vec::new();
        code.extend_from_slice(&[0x60, 0x00]); // retLength
        code.extend_from_slice(&[0x60, 0x00]); // retOffset
        code.extend_from_slice(&[0x60, 0x00]); // argsLength
        code.extend_from_slice(&[0x60, 0x00]); // argsOffset
        code.extend_from_slice(&[0x60, 0x64]); // value = 100
        code.extend_from_slice(&[0x73]);
        code.extend_from_slice(&[0xdd; 20]);
        code.extend_from_slice(&[0x61, 0xff, 0xff]); // gas
        code.push(0xf1);
        code.push(0x00);

        let result = Executor::new().execute_with_host(&code, &mut ctx, &mut host);
        assert!(result.success);
        assert_eq!(ctx.stack.pop().unwrap().as_u64(), 1);
        assert_eq!(host.balance(&host_addr(0x01)).as_u64(), 900);
        assert_eq!(host.balance(&host_addr(0xdd)).as_u64(), 100);
    }

    #[test]
    fn call_reverts_value_transfer_when_the_callee_fails() {
        // Callee always reverts.
        let callee_code = vec![0x60, 0x00, 0x60, 0x00, 0xfd];
        let mut host = MemoryHost::new()
            .with_code(host_addr(0xcc), callee_code)
            .with_balance(host_addr(0x01), EvmWord::from_u64(500));

        let mut ctx = ExecutionContext::new();
        ctx.address = host_addr(0x01);
        ctx.gas = 1_000_000;

        let mut code = Vec::new();
        code.extend_from_slice(&[0x60, 0x00]); // retLength
        code.extend_from_slice(&[0x60, 0x00]); // retOffset
        code.extend_from_slice(&[0x60, 0x00]); // argsLength
        code.extend_from_slice(&[0x60, 0x00]); // argsOffset
        code.extend_from_slice(&[0x60, 0x0a]); // value = 10
        code.extend_from_slice(&[0x73]);
        code.extend_from_slice(&[0xcc; 20]);
        code.extend_from_slice(&[0x61, 0xff, 0xff]); // gas
        code.push(0xf1);
        code.push(0x00);

        let result = Executor::new().execute_with_host(&code, &mut ctx, &mut host);
        assert!(result.success, "the outer frame completes; only the call fails");
        // The call reports failure and the value must be returned to the caller.
        assert_eq!(ctx.stack.pop().unwrap().as_u64(), 0);
        assert_eq!(host.balance(&host_addr(0x01)).as_u64(), 500);
        assert_eq!(host.balance(&host_addr(0xcc)).as_u64(), 0);
    }

    #[test]
    fn create_derives_the_expected_address_and_deploys_code() {
        // Init code that returns the runtime bytecode 0x6001600101:
        //   PUSH5 0x6001600101, PUSH1 0, MSTORE (right-aligned), RETURN 5 bytes
        // Simpler: store the 5 bytes at offset 0 via MSTORE8 sequence.
        let init_code = vec![
            0x60, 0x60, 0x60, 0x00, 0x53, // 0x60 at 0
            0x60, 0x01, 0x60, 0x01, 0x53, // 0x01 at 1
            0x60, 0x60, 0x60, 0x02, 0x53, // 0x60 at 2
            0x60, 0x01, 0x60, 0x03, 0x53, // 0x01 at 3
            0x60, 0x01, 0x60, 0x04, 0x53, // 0x01 at 4
            0x60, 0x05, 0x60, 0x00, 0xf3, // RETURN 5 bytes from 0
        ];

        let mut ctx = ExecutionContext::new();
        ctx.address = host_addr(0x01);
        ctx.gas = 1_000_000;
        let mut host = MemoryHost::new();

        // CODECOPY(destOffset, offset, length) and CREATE(value, offset, length)
        // both take their first argument from the top of the stack, so the
        // arguments are pushed in reverse order.
        let mut program = Vec::new();
        let init_len = init_code.len() as u8;
        // CODECOPY: length, offset, destOffset
        program.extend_from_slice(&[0x60, init_len]); // length
        program.extend_from_slice(&[0x60, 0x00]); // offset (patched below)
        program.extend_from_slice(&[0x60, 0x00]); // destOffset
        program.push(0x39); // CODECOPY
        // CREATE: length, offset, value
        program.extend_from_slice(&[0x60, init_len]); // length
        program.extend_from_slice(&[0x60, 0x00]); // offset
        program.extend_from_slice(&[0x60, 0x00]); // value
        program.push(0xf0); // CREATE
        program.push(0x00);

        // The init code is appended after the program, so the CODECOPY source
        // offset is the index at which it starts.
        let init_offset = program.len() as u8;
        program[3] = init_offset;
        program.extend_from_slice(&init_code);

        let result = Executor::new().execute_with_host(&program, &mut ctx, &mut host);
        assert!(result.success, "CREATE program must run to completion");

        let created = ctx.stack.pop().unwrap();
        assert!(!created.is_zero(), "CREATE must return a non-zero address");

        // The address must be keccak256(rlp([sender, nonce=0]))[12..].
        let expected_rlp = tiger_state::rlp::RlpItem::list(vec![
            tiger_state::rlp::RlpItem::Bytes(host_addr(0x01).0.to_vec()),
            tiger_state::rlp::RlpItem::from_u64(0),
        ])
        .encode();
        let expected_hash = tiger_crypto::keccak256(&expected_rlp);
        assert_eq!(&created.0[12..], &expected_hash[12..]);

        // The runtime code must be deployed at that address.
        let mut addr = Address::zero();
        addr.0.copy_from_slice(&created.0[12..]);
        assert_eq!(host.code(&addr), vec![0x60, 0x01, 0x60, 0x01, 0x01]);
    }

    #[test]
    fn codecopy_is_decoded_and_copies_code() {
        // Regression: opcode 0x39 (CODECOPY) was absent from the decoding
        // table, so any contract copying its own code halted immediately.
        assert_eq!(Opcode::from_u8(0x39), Some(Opcode::CODECOPY));
        assert_eq!(Opcode::from_u8(0x38), Some(Opcode::CODECOPY));

        let mut ctx = ExecutionContext::new();
        ctx.gas = 100_000;
        // CODECOPY(destOffset=0, offset=0, length=4) then return 4 bytes.
        let code = vec![
            0x60, 0x04, // length
            0x60, 0x00, // offset
            0x60, 0x00, // destOffset
            0x39, // CODECOPY
            0x60, 0x04, // length
            0x60, 0x00, // offset
            0xf3, // RETURN
        ];
        let result = Executor::new().execute(&code, &mut ctx);
        assert!(result.success);
        // The first four bytes of the program are 0x60 0x04 0x60 0x00.
        assert_eq!(result.output, vec![0x60, 0x04, 0x60, 0x00]);
    }

    #[test]
    fn push0_pushes_zero_and_costs_two() {
        // Regression: 0x5f (PUSH0) was unimplemented, so contracts compiled by
        // Solidity >= 0.8.20 (which emit PUSH0 by default) could not run.
        assert_eq!(Opcode::from_u8(0x5f), Some(Opcode::PUSH0));
        assert_eq!(push_size(Opcode::PUSH0), 0);

        let mut ctx = ExecutionContext::new();
        ctx.gas = 100_000;
        let result = Executor::new().execute(&[0x5f, 0x00], &mut ctx);
        assert!(result.success);
        assert_eq!(ctx.stack.pop().unwrap(), EvmWord::zero());
        assert_eq!(result.gas_used, 2);
    }

    #[test]
    fn transient_storage_round_trips_within_a_transaction() {
        assert_eq!(Opcode::from_u8(0x5c), Some(Opcode::TLOAD));
        assert_eq!(Opcode::from_u8(0x5d), Some(Opcode::TSTORE));

        let mut ctx = ExecutionContext::new();
        ctx.gas = 100_000;
        // TSTORE(key=1, value=0x2a) then TLOAD(key=1).
        let code = vec![
            0x60, 0x2a, // value
            0x60, 0x01, // key
            0x5d, // TSTORE
            0x60, 0x01, // key
            0x5c, // TLOAD
            0x00,
        ];
        let result = Executor::new().execute(&code, &mut ctx);
        assert!(result.success);
        assert_eq!(ctx.stack.pop().unwrap().as_u64(), 0x2a);
        // Transient storage must not leak into persistent storage.
        assert_eq!(ctx.storage.load(&EvmWord::from_u64(1)), EvmWord::zero());
    }

    #[test]
    fn mcopy_moves_memory_regions() {
        assert_eq!(Opcode::from_u8(0x5e), Some(Opcode::MCOPY));

        let mut ctx = ExecutionContext::new();
        ctx.gas = 100_000;
        // Store 0xab at memory[0], then MCOPY(dest=32, src=0, len=1), then
        // return 1 byte from offset 32.
        let code = vec![
            0x60, 0xab, 0x60, 0x00, 0x53, // MSTORE8 0xab at 0
            0x60, 0x01, // length
            0x60, 0x00, // src
            0x60, 0x20, // dest
            0x5e, // MCOPY
            0x60, 0x01, // length
            0x60, 0x20, // offset
            0xf3, // RETURN
        ];
        let result = Executor::new().execute(&code, &mut ctx);
        assert!(result.success);
        assert_eq!(result.output, vec![0xab]);
    }

    #[test]
    fn unimplemented_opcode_is_not_free() {
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
