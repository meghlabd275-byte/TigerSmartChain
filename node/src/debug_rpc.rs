//! `debug_*` JSON-RPC methods.
//!
//! `debug_traceTransaction` replays the target transaction against the state
//! its block started from, through the same `apply_transaction_traced` path the
//! chain uses, and reports every executed opcode and every state write. The
//! replay is exact rather than approximate: earlier blocks are re-applied from
//! the genesis allocation, and transactions preceding the target in its own
//! block are re-applied too, so the contract observes the state it really saw.

use crate::node::{state_from_genesis, transaction_to_request, TigerNode};
use serde_json::{json, Value};
use std::sync::Arc;
use tiger_evm::trace::{Step, StepTracerBuffer, TraceSummary};

/// Maximum steps retained per trace. A pathological contract can execute
/// millions of opcodes; bounding the buffer keeps a trace request from
/// exhausting node memory.
const MAX_TRACE_STEPS: usize = 200_000;

/// Decode a `0x`-prefixed hex quantity.
fn hex_to_u64(value: &str) -> Result<u64, String> {
    let digits = value.strip_prefix("0x").unwrap_or(value);
    if digits.is_empty() {
        return Err("empty hex quantity".to_string());
    }
    u64::from_str_radix(digits, 16).map_err(|e| format!("invalid hex quantity {value}: {e}"))
}

/// Whether the caller asked for memory to be included in each step.
fn wants_memory(trace_config: Option<&Value>) -> bool {
    trace_config
        .and_then(|c| c.get("enableMemory"))
        .and_then(|v| v.as_bool())
        .unwrap_or(false)
}

/// The `tracer` option, defaulting to the opcode logger.
fn tracer_name(trace_config: Option<&Value>) -> &str {
    trace_config
        .and_then(|c| c.get("tracer"))
        .and_then(|v| v.as_str())
        .unwrap_or("opcodeLogger")
}

/// Render a step as a `structLog`.
fn step_to_json(step: &Step) -> Value {
    let stack: Vec<String> = step.stack.iter().map(|w| format!("0x{}", tiger_blockchain::hex_encode(&w.0))).collect();

    let mut entry = json!({
        "pc": step.pc,
        "op": step.op,
        "gas": format!("0x{:x}", step.gas),
        "gasCost": format!("0x{:x}", step.gas_cost),
        "depth": step.depth,
        "stack": stack,
    });

    if let Some(memory) = &step.memory {
        // `memory` is a single hex string of the whole image, which is the
        // encoding clients expect (not a list of words).
        entry["memory"] = json!(format!("0x{}", tiger_blockchain::hex_encode(memory)));
    }
    if let Some(error) = &step.error {
        entry["error"] = json!(error);
    }

    entry
}

/// Render the final summary as the `result` object of a trace.
fn summary_to_json(summary: &TraceSummary, truncated: bool) -> Value {
    let mut result = json!({
        "gas": format!("0x{:x}", summary.gas_used),
        "returnValue": format!("0x{}", tiger_blockchain::hex_encode(&summary.output)),
        "failed": !summary.success,
        "structLogs": Value::Null,
    });

    if truncated {
        // A client must be able to tell a complete trace from a truncated one.
        result["truncated"] = json!(true);
    }

    result
}

/// Build the full response for a trace.
fn trace_response(tracer: &StepTracerBuffer) -> Result<Value, String> {
    let summary = tracer
        .summary()
        .ok_or_else(|| "execution produced no trace summary".to_string())?;

    let struct_logs: Vec<Value> = tracer.steps().iter().map(step_to_json).collect();

    let mut result = summary_to_json(summary, tracer.truncated());
    result["structLogs"] = json!(struct_logs);

    Ok(result)
}

/// Rebuild the world state at the start of `block_number`.
///
/// This replays the genesis allocation and then every transaction of every
/// earlier block, which is what the target block's first transaction observes.
async fn state_at_block_start(node: &Arc<TigerNode>, block_number: u64) -> tiger_state::account::WorldState {
    use tiger_evm::transition::{apply_transaction, BlockContext};

    let mut state = state_from_genesis(node.genesis());
    let chain_id = node.config().chain_id;
    let manager = node.get_chain_manager().await;
    let chain = manager.chain();

    for number in 0..block_number {
        let block = match chain.get_block(number) {
            Some(block) => block,
            None => continue,
        };
        let ctx = BlockContext {
            chain_id,
            number: block.header.number,
            timestamp: block.header.timestamp,
            gas_limit: block.header.gas_limit,
            coinbase: crate::rpc_server::parse_address(&block.header.miner)
                .unwrap_or([0u8; 20]),
            base_fee: block.header.base_fee_per_gas.unwrap_or(0),
        };
        for tx in &block.transactions {
            if let Some(request) = transaction_to_request(tx, chain_id) {
                // A rejected or reverted transaction still consumed gas and
                // advanced a nonce; `apply_transaction` performs that
                // accounting, so its result is not an error here.
                let _ = apply_transaction(&mut state, &request, &ctx);
            }
        }
    }

    state
}

/// `debug_traceTransaction`.
pub async fn debug_trace_transaction(
    node: &Arc<TigerNode>,
    tx_hash: &str,
    trace_config: Option<&Value>,
) -> Result<Value, String> {
    let chain_id = node.config().chain_id;

    // Locate the transaction and the block it belongs to.
    let (block_number, tx_index, target) = {
        let manager = node.get_chain_manager().await;
        let chain = manager.chain();
        let mut found = None;
        for number in 0..=chain.height() {
            if let Some(block) = chain.get_block(number) {
                if let Some(index) = block
                    .transactions
                    .iter()
                    .position(|tx| tx.hash.eq_ignore_ascii_case(tx_hash))
                {
                    found = Some((number, index, block.transactions[index].clone()));
                    break;
                }
            }
        }
        found.ok_or_else(|| format!("transaction {tx_hash} not found"))?
    };

    let request = transaction_to_request(&target, chain_id)
        .ok_or_else(|| format!("transaction {tx_hash} has an unrecoverable signature"))?;

    // Rebuild the state the block started from, then re-apply the transactions
    // that precede the target within the block.
    let mut state = state_at_block_start(node, block_number).await;

    let (block_ctx, preceding) = {
        let manager = node.get_chain_manager().await;
        let chain = manager.chain();
        let block = chain
            .get_block(block_number)
            .ok_or_else(|| format!("block {block_number} is missing"))?;
        let ctx = tiger_evm::transition::BlockContext {
            chain_id,
            number: block.header.number,
            timestamp: block.header.timestamp,
            gas_limit: block.header.gas_limit,
            coinbase: crate::rpc_server::parse_address(&block.header.miner).unwrap_or([0u8; 20]),
            base_fee: block.header.base_fee_per_gas.unwrap_or(0),
        };
        let preceding: Vec<_> = block.transactions[..tx_index].to_vec();
        (ctx, preceding)
    };

    for tx in &preceding {
        if let Some(request) = transaction_to_request(tx, chain_id) {
            let _ = tiger_evm::transition::apply_transaction(&mut state, &request, &block_ctx);
        }
    }

    // Execute the target through the traced transition path.
    let mut tracer = if wants_memory(trace_config) {
        StepTracerBuffer::with_memory(MAX_TRACE_STEPS)
    } else {
        StepTracerBuffer::new(MAX_TRACE_STEPS)
    };

    let outcome = tiger_evm::transition::apply_transaction_traced(
        &mut state,
        &request,
        &block_ctx,
        &mut tracer,
    )
    .map_err(|e| format!("trace replay failed: {e}"))?;

    let _ = tracer_name(trace_config);

    let (outcome, diffs) = outcome;

    let mut response = trace_response(&tracer)?;

    // `gas` is the transaction's total consumption, which includes the
    // intrinsic charge the interpreter never sees.
    response["gas"] = json!(format!("0x{:x}", outcome.gas_used));

    // The state writes the transaction performed, which `debug_traceTransaction`
    // reports separately from the opcode log.
    response["stateDiff"] = state_diff_to_json(&diffs);

    Ok(response)
}

/// Render state writes as a `stateDiff` object keyed by address.
fn state_diff_to_json(diffs: &tiger_evm::transition::TraceDiffs) -> Value {
    use std::collections::BTreeMap;

    let mut by_address: BTreeMap<String, Value> = BTreeMap::new();

    for change in &diffs.balances {
        let address = format!("0x{}", tiger_blockchain::hex_encode(&change.address.0));
        let entry = by_address
            .entry(address)
            .or_insert_with(|| json!({ "storage": {} }));
        entry["balance"] = json!({
            "*": { "from": format!("0x{}", tiger_blockchain::hex_encode(&change.before.0)), "to": format!("0x{}", tiger_blockchain::hex_encode(&change.after.0)) }
        });
    }

    for change in &diffs.storage {
        let address = format!("0x{}", tiger_blockchain::hex_encode(&change.address.0));
        let entry = by_address
            .entry(address)
            .or_insert_with(|| json!({ "storage": {} }));
        let key = format!("0x{}", tiger_blockchain::hex_encode(&change.slot.0));
        entry["storage"][key] = json!({
            "*": { "from": format!("0x{}", tiger_blockchain::hex_encode(&change.before.0)), "to": format!("0x{}", tiger_blockchain::hex_encode(&change.after.0)) }
        });
    }

    json!(by_address)
}

/// `debug_traceCall`: trace a call against current state without committing it.
pub async fn debug_trace_call(
    node: &Arc<TigerNode>,
    params: &Value,
) -> Result<Value, String> {
    use tiger_evm::{Address, ExecutionContext, Executor, Host as _};

    let call = params
        .get(0)
        .ok_or_else(|| "missing call object".to_string())?;

    let to = call
        .get("to")
        .and_then(|v| v.as_str())
        .map(crate::rpc_server::parse_address)
        .transpose()?;
    let data = call
        .get("data")
        .or_else(|| call.get("input"))
        .and_then(|v| v.as_str())
        .map(crate::rpc_server::decode_hex)
        .transpose()?
        .unwrap_or_default();
    let caller = call
        .get("from")
        .and_then(|v| v.as_str())
        .map(crate::rpc_server::parse_address)
        .transpose()?
        .unwrap_or([0u8; 20]);

    // Trace against a snapshot so the call cannot mutate the chain.
    let mut world = node.world_state().lock().await.clone();
    let mut host = tiger_evm::transition::StateHost::new(&mut world);

    let code = match to {
        Some(address) => host.code(&Address(address)),
        None => data.clone(),
    };

    let mut ctx = ExecutionContext::new();
    ctx.caller = Address(caller);
    ctx.origin = Address(caller);
    ctx.address = Address(to.unwrap_or([0u8; 20]));
    ctx.data = data;
    ctx.gas = node.config().call_gas_limit;
    ctx.chain_id = node.config().chain_id;
    ctx.block_number = node.get_chain_manager().await.chain().height();

    let mut tracer = if wants_memory(params.get(1)) {
        StepTracerBuffer::with_memory(MAX_TRACE_STEPS)
    } else {
        StepTracerBuffer::new(MAX_TRACE_STEPS)
    };

    Executor::new().execute_traced_with_host(&code, &mut ctx, &mut host, &mut tracer);

    trace_response(&tracer)
}

/// `debug_traceBlockByNumber`: trace every transaction in a block.
pub async fn debug_trace_block(
    node: &Arc<TigerNode>,
    block_tag: &str,
    trace_config: Option<&Value>,
) -> Result<Vec<Value>, String> {
    let manager = node.get_chain_manager().await;
    let height = manager.chain().height();
    drop(manager);

    let number = match block_tag {
        "latest" | "pending" | "safe" | "finalized" => height,
        "earliest" => 0,
        tag => hex_to_u64(tag)?,
    };

    let hashes: Vec<String> = {
        let manager = node.get_chain_manager().await;
        let chain = manager.chain();
        chain
            .get_block(number)
            .ok_or_else(|| format!("block {number} not found"))?
            .transactions
            .iter()
            .map(|tx| tx.hash.clone())
            .collect()
    };

    let mut traces = Vec::with_capacity(hashes.len());
    for hash in &hashes {
        let trace = debug_trace_transaction(node, hash, trace_config).await?;
        traces.push(json!({ "txHash": hash, "result": trace }));
    }

    Ok(traces)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tiger_blockchain::types::{Account, Genesis, Transaction};
    use tiger_blockchain::ChainConfig;

    fn test_genesis(alloc: Vec<(&str, Account)>) -> Genesis {
        Genesis {
            config: ChainConfig {
                chain_id: 6666,
                london_block: Some(0),
                ..Default::default()
            },
            nonce: 0,
            timestamp: "0x0".to_string(),
            extra_data: "0x".to_string(),
            gas_limit: "0x1c9c380".to_string(),
            difficulty: "0x1".to_string(),
            mix_hash: format!("0x{}", "0".repeat(64)),
            coinbase: "0x0000000000000000000000000000000000000000".to_string(),
            alloc: alloc
                .into_iter()
                .map(|(address, account)| (address.to_string(), account))
                .collect(),
        }
    }

    fn account_with_balance(balance: u64) -> Account {
        Account {
            balance: format!("0x{balance:x}"),
            nonce: Some(0),
            code: None,
            storage: Default::default(),
        }
    }

    fn account_with_code(balance: u64, code: &[u8]) -> Account {
        Account {
            balance: format!("0x{balance:x}"),
            nonce: Some(0),
            code: Some(format!("0x{}", tiger_blockchain::hex_encode(code))),
            storage: Default::default(),
        }
    }

    /// Format a u64 as a 32-byte big-endian hex word, the way an `EvmWord`
    /// renders in a JSON-RPC response.
    fn hex_word(value: u64) -> String {
        format!("0x{:064x}", value)
    }

    fn slot_one() -> [u8; 32] {
        let mut slot = [0u8; 32];
        slot[31] = 1;
        slot
    }

    /// Sign a transaction whose `from` is deliberately wrong, so the test also
    /// proves the trace uses the recovered sender rather than the wire field.
    fn signed_tx(key: &[u8; 32], nonce: u64, to: &str, value: &str, gas: u64, data: &[u8]) -> Transaction {
        let mut tx = Transaction {
            hash: String::new(),
            nonce,
            block_hash: None,
            block_number: None,
            transaction_index: None,
            from: format!("0x{}", "00".repeat(20)),
            to: Some(to.to_string()),
            value: value.to_string(),
            gas_price: 1,
            gas,
            input: format!("0x{}", tiger_blockchain::hex_encode(data)),
            v: "0x0".to_string(),
            r: "0x0".to_string(),
            s: "0x0".to_string(),
            tx_type: 0,
            access_list: None,
            max_priority_fee_per_gas: None,
            chain_id: None,
        };
        tiger_blockchain::sign_transaction(&mut tx, 6666, key).unwrap();
        tx
    }

    fn address_of(key: &[u8; 32]) -> String {
        format!(
            "0x{}",
            tiger_blockchain::hex_encode(&tiger_crypto::private_key_to_address(key).unwrap())
        )
    }

    /// A node whose genesis pre-funds `alloc`, with the given blocks appended.
    async fn node_with_blocks(
        alloc: Vec<(&str, Account)>,
        blocks: Vec<Vec<Transaction>>,
    ) -> Arc<TigerNode> {
        let mut config = crate::config::Config::default_testnet();
        config.chain_id = 6666;
        let node = Arc::new(
            TigerNode::from_genesis(config, test_genesis(alloc))
                .await
                .unwrap(),
        );

        {
            let mut cm = node.get_chain_manager().await;
            let genesis_block = crate::node::genesis_block_from(node.genesis());
            cm.process_block(genesis_block).unwrap();
        }

        let miner = format!("0x{}", "aa".repeat(20));
        for (index, txs) in blocks.into_iter().enumerate() {
            let number = (index + 1) as u64;
            let mut cm = node.get_chain_manager().await;
            let mut world = node.world_state().lock().await;
            crate::node::produce_block(&mut cm, &mut world, &txs, number, &miner, 6666).unwrap();
        }

        node
    }

    #[test]
    fn genesis_state_applies_balances_and_code() {
        let funded = "0x1111111111111111111111111111111111111111";
        let contract = "0x3333333333333333333333333333333333333333";
        let genesis = test_genesis(vec![
            (funded, account_with_balance(1_000_000)),
            (contract, account_with_code(0, &[0x60, 0x00, 0x60, 0x00, 0xf3])),
        ]);

        let state = state_from_genesis(&genesis);
        let mut expected = [0u8; 32];
        expected[24..].copy_from_slice(&1_000_000u64.to_be_bytes());
        assert_eq!(
            state.balance(&crate::rpc_server::parse_address(funded).unwrap()),
            expected
        );
        assert_eq!(
            state.code(&crate::rpc_server::parse_address(contract).unwrap()),
            vec![0x60, 0x00, 0x60, 0x00, 0xf3]
        );
    }

    #[test]
    fn hex_quantities_decode_and_reject_garbage() {
        assert_eq!(hex_to_u64("0x0"), Ok(0));
        assert_eq!(hex_to_u64("0x1c9c380"), Ok(30_000_000));
        assert_eq!(hex_to_u64("10"), Ok(16));
        assert!(hex_to_u64("0x").is_err());
        assert!(hex_to_u64("0xzz").is_err());
    }

    #[test]
    fn memory_capture_is_opt_in() {
        assert!(!wants_memory(None));
        assert!(!wants_memory(Some(&json!({}))));
        assert!(!wants_memory(Some(&json!({ "enableMemory": false }))));
        assert!(wants_memory(Some(&json!({ "enableMemory": true }))));
    }

    #[test]
    fn tracer_defaults_to_the_opcode_logger() {
        assert_eq!(tracer_name(None), "opcodeLogger");
        assert_eq!(tracer_name(Some(&json!({}))), "opcodeLogger");
        assert_eq!(
            tracer_name(Some(&json!({ "tracer": "callTracer" }))),
            "callTracer"
        );
    }

    #[test]
    fn a_step_renders_as_a_struct_log() {
        let step = Step {
            pc: 4,
            opcode: 0x01,
            op: "ADD",
            gas: 99_000,
            gas_cost: 3,
            depth: 1,
            stack: vec![tiger_evm::EvmWord::from_u64(5)],
            memory: None,
            error: None,
        };

        let rendered = step_to_json(&step);
        assert_eq!(rendered["pc"], json!(4));
        assert_eq!(rendered["op"], json!("ADD"));
        assert_eq!(rendered["gas"], json!("0x182b8"));
        assert_eq!(rendered["gasCost"], json!("0x3"));
        assert_eq!(rendered["depth"], json!(1));
        // Memory is omitted unless the tracer captured it.
        assert!(rendered.get("memory").is_none());
        assert!(rendered.get("error").is_none());
        assert_eq!(rendered["stack"][0], json!(hex_word(5)));
    }

    #[test]
    fn a_step_with_memory_renders_the_image_as_one_hex_string() {
        let step = Step {
            pc: 0,
            opcode: 0x52,
            op: "MSTORE",
            gas: 100,
            gas_cost: 3,
            depth: 1,
            stack: Vec::new(),
            memory: Some(vec![0xde, 0xad, 0xbe, 0xef]),
            error: Some("out of gas".to_string()),
        };

        let rendered = step_to_json(&step);
        assert_eq!(rendered["memory"], json!("0xdeadbeef"));
        assert_eq!(rendered["error"], json!("out of gas"));
    }

    #[test]
    fn a_state_diff_is_keyed_by_address() {
        use tiger_evm::diff::{BalanceDiff, StorageDiff};
        use tiger_evm::{Address, EvmWord};

        let diffs = tiger_evm::transition::TraceDiffs {
            storage: vec![StorageDiff {
                address: Address([0x11; 20]),
                slot: EvmWord::from_u64(7),
                before: EvmWord::zero(),
                after: EvmWord::from_u64(42),
            }],
            balances: vec![BalanceDiff {
                address: Address([0x11; 20]),
                before: EvmWord::from_u64(100),
                after: EvmWord::from_u64(58),
            }],
        };

        let rendered = state_diff_to_json(&diffs);
        let address = format!("0x{}", "11".repeat(20));
        assert_eq!(rendered[&address]["balance"]["*"]["from"], json!(hex_word(100)));
        assert_eq!(rendered[&address]["balance"]["*"]["to"], json!(hex_word(58)));
        let slot = hex_word(7);
        assert_eq!(
            rendered[&address]["storage"][&slot]["*"]["to"],
            json!(hex_word(42))
        );
    }

    #[tokio::test]
    async fn tracing_an_unknown_transaction_is_an_error() {
        let node = node_with_blocks(vec![], vec![]).await;
        let err = debug_trace_transaction(&node, "0xdeadbeef", None)
            .await
            .unwrap_err();
        assert!(err.contains("not found"), "unexpected error: {err}");
    }

    #[tokio::test]
    async fn tracing_a_contract_call_reports_every_opcode() {
        let key = [0x46u8; 32];
        let contract = [0x33u8; 20];
        let contract_hex = format!("0x{}", tiger_blockchain::hex_encode(&contract));

        // PUSH1 0x01, PUSH1 0x02, ADD, PUSH1 0x00, MSTORE, PUSH1 0x20,
        // PUSH1 0x00, RETURN
        let code = vec![
            0x60, 0x01, 0x60, 0x02, 0x01, 0x60, 0x00, 0x52, 0x60, 0x20, 0x60, 0x00, 0xf3,
        ];

        let tx = signed_tx(&key, 0, &contract_hex, "0x0", 100_000, &[]);

        let node = node_with_blocks(
            vec![
                (address_of(&key).as_str(), account_with_balance(1_000_000)),
                (contract_hex.as_str(), account_with_code(0, &code)),
            ],
            vec![vec![tx.clone()]],
        )
        .await;

        let trace = debug_trace_transaction(&node, &tx.hash, None).await.unwrap();

        let logs = trace["structLogs"].as_array().expect("structLogs array");
        let names: Vec<&str> = logs.iter().map(|l| l["op"].as_str().unwrap()).collect();
        assert_eq!(
            names,
            vec!["PUSH1", "PUSH1", "ADD", "PUSH1", "MSTORE", "PUSH1", "PUSH1", "RETURN"]
        );

        assert_eq!(trace["failed"], json!(false));
        // The return value is the 32-byte word holding 3.
        assert_eq!(trace["returnValue"], json!(hex_word(3)));
    }

    #[tokio::test]
    async fn tracing_reports_the_storage_writes_a_transaction_made() {
        let key = [0x47u8; 32];
        let contract = [0x44u8; 20];
        let contract_hex = format!("0x{}", tiger_blockchain::hex_encode(&contract));

        // PUSH1 0x2a, PUSH1 0x01, SSTORE, STOP
        let code = vec![0x60, 0x2a, 0x60, 0x01, 0x55, 0x00];

        let tx = signed_tx(&key, 0, &contract_hex, "0x0", 100_000, &[]);

        let node = node_with_blocks(
            vec![
                (address_of(&key).as_str(), account_with_balance(1_000_000)),
                (contract_hex.as_str(), account_with_code(0, &code)),
            ],
            vec![vec![tx.clone()]],
        )
        .await;

        let trace = debug_trace_transaction(&node, &tx.hash, None).await.unwrap();

        let slot = hex_word(1);
        let to = trace["stateDiff"][&contract_hex]["storage"][&slot]["*"]["to"]
            .as_str()
            .expect("the storage write is reported");
        assert_eq!(to, hex_word(42));

        // The write is also visible on the chain, because the transaction was
        // really applied when its block was produced.
        let stored = node.world_state().lock().await.storage(&contract, &slot_one());
        let mut expected = [0u8; 32];
        expected[31] = 42;
        assert_eq!(stored, expected);
    }

    #[tokio::test]
    async fn tracing_a_transfer_reports_the_balance_change() {
        let key = [0x48u8; 32];
        let recipient_hex = format!("0x{}", "22".repeat(20));

        let tx = signed_tx(&key, 0, &recipient_hex, "0x4d2", 21_000, &[]);

        let node = node_with_blocks(
            vec![(address_of(&key).as_str(), account_with_balance(1_000_000))],
            vec![vec![tx.clone()]],
        )
        .await;

        let trace = debug_trace_transaction(&node, &tx.hash, None).await.unwrap();

        // A transfer runs no code, so there are no struct logs...
        assert!(trace["structLogs"].as_array().unwrap().is_empty());
        // ...but the recipient's balance change is reported.
        let to = trace["stateDiff"][&recipient_hex]["balance"]["*"]["to"]
            .as_str()
            .expect("the balance change is reported");
        assert_eq!(to, hex_word(0x4d2));
    }

    #[tokio::test]
    async fn tracing_replays_the_transactions_before_it_in_the_block() {
        // The traced contract returns its own balance, which only the earlier
        // transaction in the block puts there. A trace that skipped the earlier
        // transaction would report a zero balance.
        let key_a = [0x51u8; 32];
        let key_b = [0x52u8; 32];

        let reader = [0x57u8; 20];
        let reader_hex = format!("0x{}", tiger_blockchain::hex_encode(&reader));

        // SELFBALANCE, PUSH1 0x00, MSTORE, PUSH1 0x20, PUSH1 0x00, RETURN
        let reader_code = vec![0x47, 0x60, 0x00, 0x52, 0x60, 0x20, 0x60, 0x00, 0xf3];

        // The first transaction funds the contract; the second reads it back.
        let tx_a = signed_tx(&key_a, 0, &reader_hex, "0x9", 100_000, &[]);
        let tx_b = signed_tx(&key_b, 0, &reader_hex, "0x0", 100_000, &[]);

        let node = node_with_blocks(
            vec![
                (address_of(&key_a).as_str(), account_with_balance(1_000_000)),
                (address_of(&key_b).as_str(), account_with_balance(1_000_000)),
                (reader_hex.as_str(), account_with_code(0, &reader_code)),
            ],
            vec![vec![tx_a, tx_b.clone()]],
        )
        .await;

        let trace = debug_trace_transaction(&node, &tx_b.hash, None).await.unwrap();

        assert_eq!(
            trace["returnValue"],
            json!(hex_word(9)),
            "the earlier transaction in the block must be replayed"
        );
    }

    #[tokio::test]
    async fn tracing_a_block_returns_one_entry_per_transaction() {
        let key = [0x60u8; 32];
        let recipient_hex = format!("0x{}", "22".repeat(20));

        let tx = signed_tx(&key, 0, &recipient_hex, "0x1", 21_000, &[]);

        let node = node_with_blocks(
            vec![(address_of(&key).as_str(), account_with_balance(1_000_000))],
            vec![vec![tx.clone()]],
        )
        .await;

        let traces = debug_trace_block(&node, "latest", None).await.unwrap();
        assert_eq!(traces.len(), 1);
        assert_eq!(traces[0]["txHash"], json!(tx.hash));

        // "earliest" is genesis, which holds no transactions.
        let genesis_traces = debug_trace_block(&node, "earliest", None).await.unwrap();
        assert!(genesis_traces.is_empty());
    }

    #[tokio::test]
    async fn tracing_a_call_does_not_mutate_the_chain() {
        let contract = [0x77u8; 20];
        let contract_hex = format!("0x{}", tiger_blockchain::hex_encode(&contract));

        // PUSH1 0x09, PUSH1 0x01, SSTORE, STOP
        let code = vec![0x60, 0x09, 0x60, 0x01, 0x55, 0x00];

        let node = node_with_blocks(
            vec![(contract_hex.as_str(), account_with_code(0, &code))],
            vec![],
        )
        .await;

        let before = node.world_state().lock().await.storage(&contract, &slot_one());
        assert_eq!(before, [0u8; 32]);

        let params = json!([{ "to": contract_hex, "data": "0x" }]);
        let trace = debug_trace_call(&node, &params).await.unwrap();
        assert!(trace["structLogs"].as_array().unwrap().len() >= 4);

        // The write happened in the trace but not on the chain.
        let after = node.world_state().lock().await.storage(&contract, &slot_one());
        assert_eq!(after, [0u8; 32], "debug_traceCall must not commit state");
    }

    #[tokio::test]
    async fn tracing_with_memory_enabled_includes_the_memory_image() {
        let contract = [0x88u8; 20];
        let contract_hex = format!("0x{}", tiger_blockchain::hex_encode(&contract));

        // PUSH1 0x2a, PUSH1 0x00, MSTORE, STOP
        let code = vec![0x60, 0x2a, 0x60, 0x00, 0x52, 0x00];

        let node = node_with_blocks(
            vec![(contract_hex.as_str(), account_with_code(0, &code))],
            vec![],
        )
        .await;

        let params = json!([
            { "to": contract_hex, "data": "0x" },
            { "enableMemory": true }
        ]);
        let trace = debug_trace_call(&node, &params).await.unwrap();

        let logs = trace["structLogs"].as_array().unwrap();
        let last = logs.last().unwrap();
        let memory = last["memory"].as_str().expect("memory is present");
        // MSTORE wrote 42 into the low byte of the first word.
        assert!(memory.ends_with("2a"), "memory was {memory}");
    }
}
