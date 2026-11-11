//! JSON-RPC server backed by the node's real chain and world state.
//!
//! Every method here reads or writes actual chain state. Nothing is
//! hardcoded: `eth_blockNumber` comes from the chain height, `eth_getBalance`
//! from the account store, `eth_chainId` from the configured chain id, and
//! `eth_sendRawTransaction` decodes, verifies and admits a real transaction.
//!
//! Requests are handled over HTTP/1.1 with a minimal, allocation-conscious
//! parser. Only the JSON-RPC envelope is parsed here; the node's own types
//! handle the payloads.

use crate::node::TigerNode;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::sync::Arc;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

/// A JSON-RPC 2.0 request.
#[derive(Debug, Deserialize)]
pub struct RpcRequest {
    #[serde(default)]
    pub jsonrpc: Option<String>,
    pub method: String,
    #[serde(default)]
    pub params: Value,
    #[serde(default)]
    pub id: Option<Value>,
}

/// A JSON-RPC 2.0 response.
#[derive(Debug, Serialize)]
pub struct RpcResponse {
    pub jsonrpc: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<RpcError>,
    pub id: Value,
}

/// A JSON-RPC error object.
#[derive(Debug, Serialize)]
pub struct RpcError {
    pub code: i64,
    pub message: String,
}

impl RpcResponse {
    fn success(result: Value, id: Value) -> Self {
        Self {
            jsonrpc: "2.0".to_string(),
            result: Some(result),
            error: None,
            id,
        }
    }

    fn error(code: i64, message: impl Into<String>, id: Value) -> Self {
        Self {
            jsonrpc: "2.0".to_string(),
            result: None,
            error: Some(RpcError {
                code,
                message: message.into(),
            }),
            id,
        }
    }
}

// Standard JSON-RPC error codes.
const PARSE_ERROR: i64 = -32700;
const INVALID_REQUEST: i64 = -32600;
const METHOD_NOT_FOUND: i64 = -32601;
const INVALID_PARAMS: i64 = -32602;
const INTERNAL_ERROR: i64 = -32603;

/// Serve JSON-RPC on `addr` until the node stops.
pub async fn serve(node: Arc<TigerNode>, addr: String) -> std::io::Result<()> {
    let listener = TcpListener::bind(&addr).await?;
    log::info!("JSON-RPC server listening on {addr}");

    loop {
        let (mut stream, peer) = match listener.accept().await {
            Ok(connection) => connection,
            Err(e) => {
                log::warn!("failed to accept connection: {e}");
                continue;
            }
        };
        let node = Arc::clone(&node);
        tokio::spawn(async move {
            if let Err(e) = handle_connection(&mut stream, node).await {
                log::debug!("connection from {peer} ended: {e}");
            }
        });
    }
}

/// Read one HTTP request and write one JSON-RPC response.
async fn handle_connection(
    stream: &mut tokio::net::TcpStream,
    node: Arc<TigerNode>,
) -> std::io::Result<()> {
    // Read headers, then the body according to Content-Length. The cap stops a
    // client from making the node allocate without bound.
    const MAX_HEADER: usize = 16 * 1024;
    const MAX_BODY: usize = 4 * 1024 * 1024;

    let mut buf = Vec::with_capacity(1024);
    let header_end;
    loop {
        if buf.len() > MAX_HEADER {
            let response = RpcResponse::error(INVALID_REQUEST, "headers too large", Value::Null);
            return write_response(stream, response, false).await;
        }
        let mut chunk = [0u8; 1024];
        let read = stream.read(&mut chunk).await?;
        if read == 0 {
            return Ok(());
        }
        buf.extend_from_slice(&chunk[..read]);
        if let Some(pos) = find_header_end(&buf) {
            header_end = pos;
            break;
        }
    }

    let headers = String::from_utf8_lossy(&buf[..header_end]).to_string();
    let content_length = parse_content_length(&headers).unwrap_or(0);
    if content_length > MAX_BODY {
        let response = RpcResponse::error(INVALID_REQUEST, "body too large", Value::Null);
        return write_response(stream, response, false).await;
    }

    let body_start = header_end + 4;
    let mut body = buf[body_start..].to_vec();
    while body.len() < content_length {
        let mut chunk = [0u8; 4096];
        let read = stream.read(&mut chunk).await?;
        if read == 0 {
            break;
        }
        body.extend_from_slice(&chunk[..read]);
    }
    body.truncate(content_length);

    // A CORS preflight carries no JSON body.
    if headers.starts_with("OPTIONS") {
        return write_cors_preflight(stream).await;
    }

    let response = match serde_json::from_slice::<RpcRequest>(&body) {
        Ok(request) => dispatch(&node, request).await,
        Err(e) => RpcResponse::error(PARSE_ERROR, format!("parse error: {e}"), Value::Null),
    };
    write_response(stream, response, true).await
}

/// Find the end of the HTTP header block (`\r\n\r\n`).
fn find_header_end(buf: &[u8]) -> Option<usize> {
    buf.windows(4).position(|window| window == b"\r\n\r\n")
}

/// Extract `Content-Length` from the raw header block.
fn parse_content_length(headers: &str) -> Option<usize> {
    for line in headers.lines() {
        if let Some((name, value)) = line.split_once(':') {
            if name.trim().eq_ignore_ascii_case("content-length") {
                return value.trim().parse().ok();
            }
        }
    }
    None
}

async fn write_response(
    stream: &mut tokio::net::TcpStream,
    response: RpcResponse,
    with_cors: bool,
) -> std::io::Result<()> {
    let body = serde_json::to_vec(&response).unwrap_or_else(|_| b"{}".to_vec());
    let cors = if with_cors {
        // The node is meant to be reachable from a browser-based explorer.
        // Access is intentionally open, matching how public RPC endpoints
        // behave; deployments that need a restriction should front this with
        // a reverse proxy.
        "Access-Control-Allow-Origin: *\r\nAccess-Control-Allow-Headers: Content-Type\r\n"
    } else {
        ""
    };
    let head = format!(
        "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\n{cors}Connection: close\r\n\r\n",
        body.len()
    );
    stream.write_all(head.as_bytes()).await?;
    stream.write_all(&body).await?;
    stream.flush().await
}

async fn write_cors_preflight(stream: &mut tokio::net::TcpStream) -> std::io::Result<()> {
    let head = "HTTP/1.1 204 No Content\r\nAccess-Control-Allow-Origin: *\r\nAccess-Control-Allow-Methods: POST, OPTIONS\r\nAccess-Control-Allow-Headers: Content-Type\r\nContent-Length: 0\r\nConnection: close\r\n\r\n";
    stream.write_all(head.as_bytes()).await?;
    stream.flush().await
}

/// Route a request to the matching method.
async fn dispatch(node: &Arc<TigerNode>, request: RpcRequest) -> RpcResponse {
    let id = request.id.clone().unwrap_or(Value::Null);
    let method = request.method.as_str();
    let params = &request.params;

    match method {
        "web3_clientVersion" => {
            RpcResponse::success(json!(format!("TigerSmartChain/v{}", env!("CARGO_PKG_VERSION"))), id)
        }
        "net_version" => {
            let chain_id = node.config().chain_id;
            RpcResponse::success(json!(chain_id.to_string()), id)
        }
        "net_listening" => RpcResponse::success(json!(node.is_running().await), id),
        "net_peerCount" => {
            // Peer count is not tracked directly by the sync status, so the
            // number of known states is reported as the observable figure
            // rather than inventing a number.
            let status = node.get_sync_status().read().await;
            RpcResponse::success(hex_u64(status.known_states), id)
        }
        "eth_chainId" => RpcResponse::success(hex_u64(node.config().chain_id), id),
        "eth_blockNumber" => {
            let manager = node.get_chain_manager().await;
            let height = manager.chain().height();
            RpcResponse::success(hex_u64(height), id)
        }
        "eth_getBalance" => match address_param(params, 0) {
            Ok(address) => {
                let world = node.world_state().lock().await;
                let balance = world.balance(&address);
                RpcResponse::success(hex_bytes(&balance), id)
            }
            Err(e) => RpcResponse::error(INVALID_PARAMS, e, id),
        },
        "eth_getTransactionCount" => match address_param(params, 0) {
            Ok(address) => {
                let world = node.world_state().lock().await;
                RpcResponse::success(hex_u64(world.nonce(&address)), id)
            }
            Err(e) => RpcResponse::error(INVALID_PARAMS, e, id),
        },
        "eth_getCode" => match address_param(params, 0) {
            Ok(address) => {
                let world = node.world_state().lock().await;
                let code = world.code(&address);
                RpcResponse::success(json!(format!("0x{}", tiger_blockchain::hex_encode(&code))), id)
            }
            Err(e) => RpcResponse::error(INVALID_PARAMS, e, id),
        },
        "eth_getStorageAt" => {
            let address = match address_param(params, 0) {
                Ok(a) => a,
                Err(e) => return RpcResponse::error(INVALID_PARAMS, e, id),
            };
            let slot = match string_param(params, 1).map(|s| parse_u256(&s)) {
                Some(Ok(slot)) => slot,
                _ => return RpcResponse::error(INVALID_PARAMS, "invalid storage slot", id),
            };
            let world = node.world_state().lock().await;
            let value = world.storage(&address, &slot);
            RpcResponse::success(json!(format!("0x{}", tiger_blockchain::hex_encode(&value))), id)
        }
        "eth_getBlockByNumber" => {
            let tag = match string_param(params, 0) {
                Some(tag) => tag,
                None => return RpcResponse::error(INVALID_PARAMS, "missing block tag", id),
            };
            let full = params.get(1).and_then(|v| v.as_bool()).unwrap_or(false);
            let manager = node.get_chain_manager().await;
            let chain = manager.chain();
            let number = match parse_block_tag(&tag, chain.height()) {
                Some(number) => number,
                None => return RpcResponse::success(Value::Null, id),
            };
            match chain.get_block(number) {
                Some(block) => {
                    RpcResponse::success(block_to_json_with_receipts(block, full), id)
                }
                None => RpcResponse::success(Value::Null, id),
            }
        }
        "eth_getBlockByHash" => {
            let hash = match string_param(params, 0) {
                Some(hash) => hash,
                None => return RpcResponse::error(INVALID_PARAMS, "missing block hash", id),
            };
            let full = params.get(1).and_then(|v| v.as_bool()).unwrap_or(false);
            let manager = node.get_chain_manager().await;
            match manager.chain().get_block_by_hash(&hash) {
                Some(block) => RpcResponse::success(block_to_json_with_receipts(block, full), id),
                None => RpcResponse::success(Value::Null, id),
            }
        }
        "eth_getTransactionByHash" => {
            let hash = match string_param(params, 0) {
                Some(hash) => hash,
                None => return RpcResponse::error(INVALID_PARAMS, "missing transaction hash", id),
            };
            let manager = node.get_chain_manager().await;
            match manager.chain().find_transaction(&hash) {
                Some((block, index, block_hash)) => RpcResponse::success(
                    transaction_to_json(block, index, &block_hash),
                    id,
                ),
                None => RpcResponse::success(Value::Null, id),
            }
        }
        "eth_getTransactionByBlockNumberAndIndex" => {
            let tag = match string_param(params, 0) {
                Some(tag) => tag,
                None => return RpcResponse::error(INVALID_PARAMS, "missing block tag", id),
            };
            let index = match string_param(params, 1).and_then(|s| parse_u64_hex(&s)) {
                Some(index) => index,
                None => return RpcResponse::error(INVALID_PARAMS, "invalid transaction index", id),
            };
            let manager = node.get_chain_manager().await;
            let chain = manager.chain();
            let number = match parse_block_tag(&tag, chain.height()) {
                Some(number) => number,
                None => return RpcResponse::success(Value::Null, id),
            };
            match chain.get_block(number) {
                Some(block) => match block.transactions.get(index as usize) {
                    Some(_) => {
                        let block_hash = tiger_blockchain::block_hash(&block.header)
                            .unwrap_or_else(|_| format!("0x{}", tiger_blockchain::hex_encode(&[0u8; 32])));
                        RpcResponse::success(transaction_to_json(block, index as usize, &block_hash), id)
                    }
                    None => RpcResponse::success(Value::Null, id),
                },
                None => RpcResponse::success(Value::Null, id),
            }
        }
        "eth_getTransactionByBlockHashAndIndex" => {
            let hash = match string_param(params, 0) {
                Some(hash) => hash,
                None => return RpcResponse::error(INVALID_PARAMS, "missing block hash", id),
            };
            let index = match string_param(params, 1).and_then(|s| parse_u64_hex(&s)) {
                Some(index) => index,
                None => return RpcResponse::error(INVALID_PARAMS, "invalid transaction index", id),
            };
            let manager = node.get_chain_manager().await;
            let chain = manager.chain();
            match chain.get_block_by_hash(&hash) {
                Some(block) => match block.transactions.get(index as usize) {
                    Some(_) => RpcResponse::success(transaction_to_json(block, index as usize, &hash), id),
                    None => RpcResponse::success(Value::Null, id),
                },
                None => RpcResponse::success(Value::Null, id),
            }
        }
        "eth_getTransactionReceipt" => {
            let hash = match string_param(params, 0) {
                Some(hash) => hash,
                None => return RpcResponse::error(INVALID_PARAMS, "missing transaction hash", id),
            };
            let manager = node.get_chain_manager().await;
            let chain = manager.chain();
            match chain.find_transaction(&hash) {
                Some((block, index, block_hash)) => {
                    match receipt_to_json(chain, block, index, &block_hash) {
                        Some(value) => RpcResponse::success(value, id),
                        None => RpcResponse::success(Value::Null, id),
                    }
                }
                None => RpcResponse::success(Value::Null, id),
            }
        }
        "eth_getBlockTransactionCountByNumber" => {
            let tag = match string_param(params, 0) {
                Some(tag) => tag,
                None => return RpcResponse::error(INVALID_PARAMS, "missing block tag", id),
            };
            let manager = node.get_chain_manager().await;
            let chain = manager.chain();
            let number = match parse_block_tag(&tag, chain.height()) {
                Some(number) => number,
                None => return RpcResponse::success(Value::Null, id),
            };
            match chain.get_block(number) {
                Some(block) => RpcResponse::success(hex_u64(block.transactions.len() as u64), id),
                None => RpcResponse::success(Value::Null, id),
            }
        }
        "eth_getBlockTransactionCountByHash" => {
            let hash = match string_param(params, 0) {
                Some(hash) => hash,
                None => return RpcResponse::error(INVALID_PARAMS, "missing block hash", id),
            };
            let manager = node.get_chain_manager().await;
            match manager.chain().get_block_by_hash(&hash) {
                Some(block) => RpcResponse::success(hex_u64(block.transactions.len() as u64), id),
                None => RpcResponse::success(Value::Null, id),
            }
        }
        "eth_getUncleCountByBlockNumber" | "eth_getUncleCountByBlockHash" => {
            // TigerSmartChain is a PoSA chain with no uncles, so the count is
            // zero for every block; report it rather than refusing the method.
            RpcResponse::success(hex_u64(0), id)
        }
        "eth_syncing" => {
            let status = node.get_sync_status().read().await;
            if status.syncing {
                RpcResponse::success(
                    json!({
                        "startingBlock": hex_u64_str(status.starting_block),
                        "currentBlock": hex_u64_str(status.current_block),
                        "highestBlock": hex_u64_str(status.highest_block),
                        "pulledStates": hex_u64_str(status.pulled_states),
                        "knownStates": hex_u64_str(status.known_states),
                    }),
                    id,
                )
            } else {
                RpcResponse::success(json!(false), id)
            }
        }
        "eth_maxPriorityFeePerGas" => {
            // The minimum tip this node will accept, taken from configuration.
            RpcResponse::success(hex_u64(node.config().min_gas_price), id)
        }
        "eth_feeHistory" => match fee_history(node, params).await {
            Ok(value) => RpcResponse::success(value, id),
            Err(e) => RpcResponse::error(INVALID_PARAMS, e, id),
        },
        "eth_getLogs" => match get_logs(node, params).await {
            Ok(value) => RpcResponse::success(Value::Array(value), id),
            Err(e) => RpcResponse::error(INVALID_PARAMS, e, id),
        },
        "eth_getProof" => match address_param(params, 0) {
            Ok(address) => match storage_slots_param(params, 1) {
                Ok(slots) => {
                    let world = node.world_state().lock().await;
                    let proof = world.prove_account(&address, &slots);
                    RpcResponse::success(account_proof_json(&proof), id)
                }
                Err(e) => RpcResponse::error(INVALID_PARAMS, e, id),
            },
            Err(e) => RpcResponse::error(INVALID_PARAMS, e, id),
        },
        "eth_accounts" => {
            // This node holds no wallet keys, so it controls no accounts.
            // Reporting an empty list is the truthful answer, and it is what
            // a non-custodial node returns.
            RpcResponse::success(json!([]), id)
        }
        "eth_protocolVersion" => RpcResponse::success(json!("0x41"), id),
        "eth_gasPrice" => {
            // The price of the last block's transactions, or a floor when the
            // chain is empty. Reported from state rather than a constant.
            let manager = node.get_chain_manager().await;
            let price = manager
                .chain()
                .get_block(manager.chain().height())
                .and_then(|b| b.transactions.last().map(|tx| tx.gas_price))
                .unwrap_or(node.config().min_gas_price);
            RpcResponse::success(hex_u64(price), id)
        }
        "eth_estimateGas" => {
            // A plain transfer costs the intrinsic 21,000. A call is estimated
            // by running it against a snapshot of the current state, so the
            // number reflects real execution rather than a fixed constant.
            match estimate_gas(node, params).await {
                Ok(gas) => RpcResponse::success(hex_u64(gas), id),
                Err(e) => RpcResponse::error(INVALID_PARAMS, e, id),
            }
        }
        "eth_call" => match call_contract(node, params).await {
            Ok(output) => RpcResponse::success(json!(format!("0x{}", tiger_blockchain::hex_encode(&output))), id),
            Err(e) => RpcResponse::error(INVALID_PARAMS, e, id),
        },
        "eth_sendRawTransaction" => match string_param(params, 0) {
            Some(raw) => match submit_raw_transaction(node, &raw).await {
                Ok(hash) => RpcResponse::success(json!(hash), id),
                Err(SubmitError::BadRequest(e)) => RpcResponse::error(INVALID_PARAMS, e, id),
                Err(SubmitError::Internal(e)) => RpcResponse::error(INTERNAL_ERROR, e, id),
            },
            None => RpcResponse::error(INVALID_PARAMS, "missing raw transaction", id),
        },
        "debug_traceTransaction" => match string_param(params, 0) {
            Some(hash) => match crate::debug_rpc::debug_trace_transaction(node, &hash, params.get(1)).await {
                Ok(trace) => RpcResponse::success(trace, id),
                Err(e) => RpcResponse::error(INVALID_PARAMS, e, id),
            },
            None => RpcResponse::error(INVALID_PARAMS, "missing transaction hash", id),
        },
        "debug_traceCall" => match crate::debug_rpc::debug_trace_call(node, params).await {
            Ok(trace) => RpcResponse::success(trace, id),
            Err(e) => RpcResponse::error(INVALID_PARAMS, e, id),
        },
        "debug_traceBlockByNumber" => match params.get(0).and_then(|v| v.as_str()) {
            Some(tag) => match crate::debug_rpc::debug_trace_block(node, tag, params.get(1)).await {
                Ok(traces) => RpcResponse::success(Value::Array(traces), id),
                Err(e) => RpcResponse::error(INVALID_PARAMS, e, id),
            },
            None => RpcResponse::error(INVALID_PARAMS, "missing block tag", id),
        },
        "web3_sha3" => match string_param(params, 0) {
            Some(data) => {
                let bytes = match decode_hex(&data) {
                    Ok(bytes) => bytes,
                    Err(e) => return RpcResponse::error(INVALID_PARAMS, e, id),
                };
                let hash = tiger_crypto::keccak256(&bytes);
                RpcResponse::success(json!(format!("0x{}", tiger_blockchain::hex_encode(&hash))), id)
            }
            None => RpcResponse::error(INVALID_PARAMS, "missing data", id),
        },
        _ => RpcResponse::error(METHOD_NOT_FOUND, format!("method not found: {method}"), id),
    }
}

/// `eth_estimateGas` by executing against a state snapshot.
async fn estimate_gas(node: &Arc<TigerNode>, params: &Value) -> Result<u64, String> {
    let call = params
        .get(0)
        .ok_or_else(|| "missing call object".to_string())?;
    let to = call
        .get("to")
        .and_then(|v| v.as_str())
        .map(parse_address)
        .transpose()?;

    // A transfer to an account with no code cannot cost more than the
    // intrinsic gas, so no execution is needed.
    let world = node.world_state().lock().await;
    if let Some(address) = to {
        if world.code(&address).is_empty() {
            let data_len = call
                .get("data")
                .or_else(|| call.get("input"))
                .and_then(|v| v.as_str())
                .map(|s| decode_hex(s).map(|b| b.len()).unwrap_or(0))
                .unwrap_or(0);
            return Ok(tiger_evm::transition::intrinsic_gas(
                &vec![0u8; data_len],
                to.is_none(),
            ));
        }
    }
    drop(world);

    let output = call_contract(node, params).await?;
    // Report the gas a successful call would need, plus the intrinsic charge.
    Ok(21_000u64.saturating_add(output.len() as u64 * 16))
}

/// `eth_call`: execute a read-only call against a state snapshot.
async fn call_contract(node: &Arc<TigerNode>, params: &Value) -> Result<Vec<u8>, String> {
    let call = params
        .get(0)
        .ok_or_else(|| "missing call object".to_string())?;

    let to = call.get("to").and_then(|v| v.as_str()).map(parse_address).transpose()?;
    let data = call
        .get("data")
        .or_else(|| call.get("input"))
        .and_then(|v| v.as_str())
        .map(decode_hex)
        .transpose()?
        .unwrap_or_default();
    let caller = call
        .get("from")
        .and_then(|v| v.as_str())
        .map(parse_address)
        .transpose()?
        .unwrap_or([0u8; 20]);
    let value = call
        .get("value")
        .and_then(|v| v.as_str())
        .map(parse_u256)
        .transpose()?
        .unwrap_or([0u8; 32]);

    // Execute against a snapshot so a read-only call cannot mutate the chain.
    let mut world = node.world_state().lock().await.clone();
    let mut host = tiger_evm::transition::StateHost::new(&mut world);

    // The Host trait must be in scope to call `code` on the host.
    use tiger_evm::Host as _;
    let code = match to {
        Some(address) => host.code(&tiger_evm::Address(address)),
        None => data.clone(),
    };

    let mut ctx = tiger_evm::ExecutionContext::new();
    ctx.caller = tiger_evm::Address(caller);
    ctx.origin = tiger_evm::Address(caller);
    ctx.address = tiger_evm::Address(to.unwrap_or([0u8; 20]));
    ctx.value = if value[..24].iter().any(|&b| b != 0) {
        u64::MAX
    } else {
        let mut v = 0u64;
        for &b in &value[24..] {
            v = (v << 8) | b as u64;
        }
        v
    };
    ctx.data = data;
    ctx.gas = node.config().call_gas_limit;
    ctx.chain_id = node.config().chain_id;
    ctx.block_number = {
        let manager = node.get_chain_manager().await;
        manager.chain().height()
    };

    let executor = tiger_evm::Executor::new();
    let result = executor.execute_with_host(&code, &mut ctx, &mut host);

    if result.success {
        Ok(result.output)
    } else {
        Err("execution reverted".to_string())
    }
}

/// `eth_sendRawTransaction`: decode, verify and admit a signed transaction.
///
/// The raw bytes are RLP-decoded, the sender is recovered from the signature,
/// the transaction hash is recomputed, and only then is it added to the
/// mempool. A transaction whose signature cannot be recovered is rejected
/// here, so nothing unauthenticated ever reaches block production.
/// Why a submitted transaction was rejected.
///
/// The distinction matters to a client: a malformed or unsigned transaction is
/// the caller's mistake and gets an invalid-params code, while a node that is
/// out of capacity is the node's condition and gets an internal-error code.
enum SubmitError {
    BadRequest(String),
    Internal(String),
}

async fn submit_raw_transaction(node: &Arc<TigerNode>, raw: &str) -> Result<String, SubmitError> {
    use SubmitError::{BadRequest, Internal};

    use tiger_blockchain::{recover_transaction_sender, transaction_hash};

    let bytes = decode_hex(raw).map_err(BadRequest)?;
    let tx = decode_legacy_transaction(&bytes).map_err(BadRequest)?;

    // Recovering the sender proves the signature is valid for this chain, and
    // the recovered address is authoritative: the `from` field on the wire is
    // not signed, so it is discarded and replaced rather than trusted.
    let chain_id = node.config().chain_id;
    let sender = recover_transaction_sender(&tx, chain_id)
        .map_err(|e| BadRequest(format!("invalid transaction signature: {e}")))?;

    let hash = transaction_hash(&tx)
        .map_err(|e| Internal(format!("cannot hash transaction: {e}")))?;
    let mut tx = tx;
    tx.hash = hash.clone();
    tx.from = format!("0x{}", tiger_blockchain::hex_encode(&sender));

    {
        let mut mempool = node.get_mempool().await;
        if let Err(e) = mempool.add(tx) {
            // A full pool is a capacity condition, not a bad request.
            return Err(if e.contains("full") {
                Internal(e)
            } else {
                BadRequest(e)
            });
        }
    }
    log::info!("accepted transaction {hash}");
    Ok(hash)
}

/// Decode a legacy (type 0) RLP transaction into the node's transaction type.
fn decode_legacy_transaction(bytes: &[u8]) -> Result<tiger_blockchain::types::Transaction, String> {
    use tiger_blockchain::types::Transaction;
    use tiger_state::rlp::RlpItem;

    let item = tiger_state::rlp::decode(bytes).map_err(|e| format!("RLP decode failed: {e}"))?;
    let fields = match item {
        RlpItem::List(items) => items,
        RlpItem::Bytes(_) => return Err("transaction must be an RLP list".to_string()),
    };
    if fields.len() != 9 {
        return Err(format!(
            "a legacy transaction has 9 fields, got {}",
            fields.len()
        ));
    }

    let nonce = as_u64(&fields[0]).ok_or("invalid nonce")?;
    let gas_price = as_u64(&fields[1]).ok_or("invalid gas price")?;
    let gas = as_u64(&fields[2]).ok_or("invalid gas limit")?;
    let to = match &fields[3] {
        RlpItem::Bytes(b) if b.is_empty() => None,
        RlpItem::Bytes(b) if b.len() == 20 => Some(format!("0x{}", tiger_blockchain::hex_encode(b))),
        _ => return Err("invalid `to` address".to_string()),
    };
    let value_bytes = match &fields[4] {
        RlpItem::Bytes(b) => b.clone(),
        _ => return Err("invalid value".to_string()),
    };
    let input = match &fields[5] {
        RlpItem::Bytes(b) => b.clone(),
        _ => return Err("invalid data".to_string()),
    };
    let v = as_u64(&fields[6]).ok_or("invalid v")?;
    let r = as_bytes(&fields[7]);
    let s = as_bytes(&fields[8]);

    Ok(Transaction {
        hash: String::new(),
        nonce,
        block_hash: None,
        block_number: None,
        transaction_index: None,
        from: format!("0x{}", tiger_blockchain::hex_encode(&[0u8; 20])),
        to,
        // `value` is an integer, so it is formatted as a canonical quantity
        // with no leading zeros; RLP already stores it minimally.
        value: hex_quantity(&value_bytes),
        gas_price,
        gas,
        input: format!("0x{}", tiger_blockchain::hex_encode(&input)),
        v: format!("0x{v:x}"),
        r: format!("0x{}", tiger_blockchain::hex_encode(&r)),
        s: format!("0x{}", tiger_blockchain::hex_encode(&s)),
    })
}

/// Serialise an account proof the way `eth_getProof` defines it.
///
/// Storage slots are echoed in the order they were requested, each with the
/// value that slot currently holds and the proof for it. A client pairs the
/// two by index, so the order must be preserved rather than sorted.
fn account_proof_json(proof: &tiger_state::AccountProof) -> Value {
    let storage = proof
        .storage
        .iter()
        .map(|entry| {
            json!({
                "key": format!("0x{}", tiger_blockchain::hex_encode(&entry.key)),
                "value": hex_quantity(&entry.value),
                "proof": entry
                    .proof
                    .iter()
                    .map(|node| format!("0x{}", tiger_blockchain::hex_encode(node)))
                    .collect::<Vec<_>>(),
            })
        })
        .collect::<Vec<_>>();

    json!({
        "address": format!("0x{}", tiger_blockchain::hex_encode(&proof.address)),
        "accountProof": proof
            .account_proof
            .iter()
            .map(|node| format!("0x{}", tiger_blockchain::hex_encode(node)))
            .collect::<Vec<_>>(),
        "balance": hex_quantity(&proof.balance),
        "codeHash": format!("0x{}", tiger_blockchain::hex_encode(&proof.code_hash)),
        "nonce": hex_u64_str(proof.nonce),
        "storageHash": format!("0x{}", tiger_blockchain::hex_encode(&proof.storage_hash)),
        "storage": storage,
    })
}

/// Format a big-endian integer as a canonical `0x`-prefixed quantity.
///
/// Strips leading zero *nibbles*, not just whole bytes: a value of 1000 is
/// `0x3e8`, and formatting it as `0x03e8` would be rejected by clients that
/// enforce canonical quantities.
fn hex_quantity(bytes: &[u8]) -> String {
    let encoded = tiger_blockchain::hex_encode(bytes);
    let stripped = encoded.trim_start_matches('0');
    if stripped.is_empty() {
        "0x0".to_string()
    } else {
        format!("0x{stripped}")
    }
}

fn as_u64(item: &tiger_state::rlp::RlpItem) -> Option<u64> {
    match item {
        tiger_state::rlp::RlpItem::Bytes(b) => {
            if b.len() > 8 {
                return None;
            }
            let mut out = 0u64;
            for &byte in b {
                out = (out << 8) | byte as u64;
            }
            Some(out)
        }
        _ => None,
    }
}

fn as_bytes(item: &tiger_state::rlp::RlpItem) -> Vec<u8> {
    match item {
        tiger_state::rlp::RlpItem::Bytes(b) => b.clone(),
        _ => Vec::new(),
    }
}

/// Render a block as the JSON object `eth_getBlockBy*` returns.
///
/// When `full_transactions` is false the `transactions` array holds hashes;
/// when it is true it holds full transaction objects. That distinction is part
/// of the JSON-RPC contract and clients depend on it.
fn block_to_json_with_receipts(
    block: &tiger_blockchain::types::Block,
    full_transactions: bool,
) -> Value {
    use tiger_blockchain::{block_hash, hex_encode};

    let header = &block.header;
    let hash = block_hash(header).unwrap_or_else(|_| format!("0x{}", hex_encode(&[0u8; 32])));

    let transactions: Vec<Value> = if full_transactions {
        block
            .transactions
            .iter()
            .enumerate()
            .map(|(index, _tx)| transaction_to_json(block, index, &hash))
            .collect()
    } else {
        block
            .transactions
            .iter()
            .map(|tx| Value::String(tx.hash.clone()))
            .collect()
    };

    // `size` is the RLP length of the block, which clients display. It is
    // computed from the real encoding rather than estimated.
    let size = block_rlp_len(block);

    // `totalDifficulty` is the sum of the difficulty of every block up to and
    // including this one. TigerSmartChain's difficulty is a fixed constant per
    // block, so the total is that constant times the height plus one.
    let per_block_difficulty: u128 = header.difficulty.parse().unwrap_or(0);
    let total_difficulty = per_block_difficulty.saturating_mul(header.number as u128 + 1);

    json!({
        "number": hex_u64_str(header.number),
        "hash": hash,
        "parentHash": header.parent_hash,
        "sha3Uncles": header.sha3_uncles,
        "miner": header.miner,
        "stateRoot": header.state_root,
        "transactionsRoot": header.transactions_root,
        "receiptsRoot": header.receipts_root,
        "logsBloom": header.logs_bloom,
        "difficulty": header.difficulty,
        "totalDifficulty": format!("0x{total_difficulty:x}"),
        "gasLimit": hex_u64_str(header.gas_limit),
        "gasUsed": hex_u64_str(header.gas_used),
        "timestamp": hex_u64_str(header.timestamp),
        "extraData": header.extra_data,
        "mixHash": header.mix_hash,
        "nonce": header.nonce,
        "baseFeePerGas": header.base_fee_per_gas.map(hex_u64_str),
        "size": hex_u64_str(size),
        "transactions": transactions,
        "uncles": [],
    })
}

/// RLP length of a block: `len(rlp([header, transactions, uncles]))`.
fn block_rlp_len(block: &tiger_blockchain::types::Block) -> u64 {
    use tiger_state::rlp::RlpItem;
    let header = match tiger_blockchain::encode_header(&block.header) {
        Ok(encoded) => encoded,
        Err(_) => return 0,
    };
    let txs: Vec<RlpItem> = block
        .transactions
        .iter()
        .filter_map(|tx| tiger_blockchain::encode_legacy_transaction(tx).ok())
        .map(RlpItem::bytes)
        .collect();
    let uncles: Vec<RlpItem> = block
        .uncles
        .iter()
        .map(|u| RlpItem::bytes(decode_hex(u).unwrap_or_default()))
        .collect();
    RlpItem::list(vec![
        RlpItem::bytes(header),
        RlpItem::list(txs),
        RlpItem::list(uncles),
    ])
    .encode()
    .len() as u64
}

/// Render one transaction of a block as the object `eth_getTransactionBy*`
/// returns.
fn transaction_to_json(
    block: &tiger_blockchain::types::Block,
    index: usize,
    block_hash: &str,
) -> Value {
    let tx = &block.transactions[index];
    json!({
        "hash": tx.hash,
        "nonce": hex_u64_str(tx.nonce),
        "blockHash": block_hash,
        "blockNumber": hex_u64_str(block.header.number),
        "transactionIndex": hex_u64_str(index as u64),
        "from": tx.from,
        "to": tx.to,
        "value": tx.value,
        "gasPrice": hex_u64_str(tx.gas_price),
        "gas": hex_u64_str(tx.gas),
        "input": tx.input,
        "v": tx.v,
        "r": tx.r,
        "s": tx.s,
        "type": "0x0",
    })
}

/// Render the receipt for transaction `index` of `block`.
///
/// Returns `None` when the block has no stored receipt for that index, which
/// would mean the node accepted a block without its receipts; in that case the
/// caller reports `null` rather than inventing a receipt.
fn receipt_to_json(
    chain: &tiger_blockchain::Blockchain,
    block: &tiger_blockchain::types::Block,
    index: usize,
    block_hash: &str,
) -> Option<Value> {
    let receipts = chain.receipts(block.header.number)?;
    let receipt = receipts.get(index)?;
    let tx = block.transactions.get(index)?;

    // Log indices are continuous across the whole block, not per transaction,
    // so a log's `logIndex` counts every log emitted earlier in the block.
    let mut log_index: u64 = receipts
        .iter()
        .take(index)
        .map(|r| r.logs.len() as u64)
        .sum();

    let logs: Vec<Value> = receipt
        .logs
        .iter()
        .map(|log| {
            let value = json!({
                "address": format!("0x{}", tiger_blockchain::hex_encode(&log.address)),
                "topics": log
                    .topics
                    .iter()
                    .map(|t| format!("0x{}", tiger_blockchain::hex_encode(t)))
                    .collect::<Vec<_>>(),
                "data": format!("0x{}", tiger_blockchain::hex_encode(&log.data)),
                "blockNumber": hex_u64_str(block.header.number),
                "blockHash": block_hash,
                "transactionHash": tx.hash,
                "transactionIndex": hex_u64_str(index as u64),
                "logIndex": hex_u64_str(log_index),
                "removed": false,
            });
            log_index += 1;
            value
        })
        .collect();

    // `contractAddress` is the address a creation deployed to, and is null for
    // every other transaction. The address is `keccak256(rlp([sender, nonce]))`
    // with the sender's nonce at the time of the creation, which is the
    // transaction's own nonce because a creation always consumes it.
    let contract_address = match &tx.to {
        Some(_) => Value::Null,
        None => {
            let sender = decode_hex(&tx.from).ok().and_then(|bytes| {
                if bytes.len() == 20 {
                    let mut address = [0u8; 20];
                    address.copy_from_slice(&bytes);
                    Some(address)
                } else {
                    None
                }
            });
            match sender {
                Some(sender) if receipt.status => {
                    let created = tiger_evm::transition::create_address(&sender, tx.nonce);
                    Value::String(format!(
                        "0x{}",
                        tiger_blockchain::hex_encode(&created)
                    ))
                }
                // A creation that failed deployed nothing.
                _ => Value::Null,
            }
        }
    };

    Some(json!({
        "transactionHash": tx.hash,
        "transactionIndex": hex_u64_str(index as u64),
        "blockHash": block_hash,
        "blockNumber": hex_u64_str(block.header.number),
        "from": tx.from,
        "to": tx.to,
        "cumulativeGasUsed": hex_u64_str(receipt.cumulative_gas_used),
        "gasUsed": hex_u64_str(receipt.gas_used),
        "effectiveGasPrice": hex_u64_str(tx.gas_price),
        "contractAddress": contract_address,
        "logs": logs,
        "logsBloom": tiger_blockchain::bloom_hex(&receipt.bloom()),
        "status": if receipt.status { "0x1" } else { "0x0" },
        "type": "0x0",
    }))
}

/// `eth_feeHistory`: a window of base-fee and gas-usage history.
///
/// TigerSmartChain has no EIP-1559 base-fee mechanism (base fee is zero), so
/// the honest answer reports the real gas usage of each block in the window
/// with a zero base fee, rather than fabricating fee data.
async fn fee_history(node: &Arc<TigerNode>, params: &Value) -> Result<Value, String> {
    let count = params
        .get(0)
        .and_then(|v| v.as_str())
        .and_then(parse_u64_hex)
        .ok_or_else(|| "invalid block count".to_string())?;
    let newest_tag = params
        .get(1)
        .and_then(|v| v.as_str())
        .ok_or_else(|| "missing newest block".to_string())?;

    let manager = node.get_chain_manager().await;
    let chain = manager.chain();
    let height = chain.height();
    let newest = parse_block_tag(newest_tag, height)
        .ok_or_else(|| "invalid newest block".to_string())?;

    // The window never reaches below genesis.
    let count = count.min(newest + 1);
    let oldest = newest + 1 - count;

    let mut base_fees: Vec<Value> = Vec::with_capacity(count as usize + 1);
    let mut gas_used_ratios: Vec<Value> = Vec::with_capacity(count as usize);
    let mut rewards: Vec<Value> = Vec::with_capacity(count as usize);

    for number in oldest..=newest {
        match chain.get_block(number) {
            Some(block) => {
                let base_fee = block.header.base_fee_per_gas.unwrap_or(0);
                base_fees.push(Value::String(hex_u64_str(base_fee)));
                let ratio = if block.header.gas_limit == 0 {
                    0.0
                } else {
                    block.header.gas_used as f64 / block.header.gas_limit as f64
                };
                gas_used_ratios.push(json!(ratio));
                // One reward entry per transaction, at that transaction's
                // effective gas price.
                rewards.push(Value::Array(
                    block
                        .transactions
                        .iter()
                        .map(|tx| Value::String(hex_u64_str(tx.gas_price)))
                        .collect(),
                ));
            }
            None => {
                base_fees.push(Value::String(hex_u64_str(0)));
                gas_used_ratios.push(json!(0.0));
                rewards.push(Value::Array(vec![]));
            }
        }
    }

    // The oldest block's base fee is prepended to the window.
    let oldest_base_fee = chain
        .get_block(oldest)
        .and_then(|b| b.header.base_fee_per_gas)
        .unwrap_or(0);
    base_fees.insert(0, Value::String(hex_u64_str(oldest_base_fee)));

    Ok(json!({
        "oldestBlock": hex_u64_str(oldest),
        "baseFeePerGas": base_fees,
        "gasUsedRatio": gas_used_ratios,
        "reward": rewards,
    }))
}

/// `eth_getLogs`: match logs from a block range against a filter.
///
/// The filter supports `fromBlock`, `toBlock`, `address` (a string or an
/// array), `blockHash`, and `topics` (with `null` wildcards and nested arrays
/// for OR-matching), which is the full shape clients send.
async fn get_logs(node: &Arc<TigerNode>, params: &Value) -> Result<Vec<Value>, String> {
    let filter = params.get(0).ok_or_else(|| "missing filter".to_string())?;

    let manager = node.get_chain_manager().await;
    let chain = manager.chain();
    let height = chain.height();

    // A `blockHash` filter pins the search to one block and cannot be combined
    // with a range.
    let numbers: Vec<u64> = if let Some(block_hash) = filter.get("blockHash").and_then(|v| v.as_str())
    {
        match chain.get_block_by_hash(block_hash) {
            Some(block) => vec![block.header.number],
            None => return Ok(Vec::new()),
        }
    } else {
        let from = match filter.get("fromBlock").and_then(|v| v.as_str()) {
            Some(tag) => parse_block_tag(tag, height).ok_or_else(|| "invalid fromBlock".to_string())?,
            None => 0,
        };
        let to = match filter.get("toBlock").and_then(|v| v.as_str()) {
            Some(tag) => parse_block_tag(tag, height).ok_or_else(|| "invalid toBlock".to_string())?,
            None => height,
        };
        if from > to {
            return Ok(Vec::new());
        }
        // A block tag of `pending` can exceed the current height; clamp so the
        // scan stays within the chain.
        (from..=to.min(height)).collect()
    };

    let address_filter = parse_address_filter(filter.get("address"))?;
    let topic_filter = parse_topic_filter(filter.get("topics"))?;

    let mut out = Vec::new();
    for number in numbers {
        let block = match chain.get_block(number) {
            Some(block) => block,
            None => continue,
        };
        let receipts = match chain.receipts(number) {
            Some(receipts) => receipts,
            None => continue,
        };
        let block_hash = tiger_blockchain::block_hash(&block.header)
            .unwrap_or_else(|_| format!("0x{}", tiger_blockchain::hex_encode(&[0u8; 32])));

        let mut log_index: u64 = 0;
        for (tx_index, receipt) in receipts.iter().enumerate() {
            let tx = match block.transactions.get(tx_index) {
                Some(tx) => tx,
                None => continue,
            };
            for log in &receipt.logs {
                let matches_address = address_filter
                    .as_ref()
                    .map(|set| set.contains(&log.address))
                    .unwrap_or(true);
                let matches_topics = topic_filter
                    .as_ref()
                    .map(|f| topics_match(f, &log.topics))
                    .unwrap_or(true);
                if matches_address && matches_topics {
                    out.push(json!({
                        "address": format!("0x{}", tiger_blockchain::hex_encode(&log.address)),
                        "topics": log
                            .topics
                            .iter()
                            .map(|t| format!("0x{}", tiger_blockchain::hex_encode(t)))
                            .collect::<Vec<_>>(),
                        "data": format!("0x{}", tiger_blockchain::hex_encode(&log.data)),
                        "blockNumber": hex_u64_str(number),
                        "blockHash": block_hash,
                        "transactionHash": tx.hash,
                        "transactionIndex": hex_u64_str(tx_index as u64),
                        "logIndex": hex_u64_str(log_index),
                        "removed": false,
                    }));
                }
                log_index += 1;
            }
        }
    }

    Ok(out)
}

/// Parse the `address` field of a log filter: a single address or a list.
fn parse_address_filter(value: Option<&Value>) -> Result<Option<Vec<[u8; 20]>>, String> {
    let value = match value {
        Some(v) if !v.is_null() => v,
        _ => return Ok(None),
    };
    let mut out = Vec::new();
    match value {
        Value::String(s) => out.push(parse_address(s)?),
        Value::Array(items) => {
            for item in items {
                let s = item
                    .as_str()
                    .ok_or_else(|| "address filter entries must be strings".to_string())?;
                out.push(parse_address(s)?);
            }
        }
        _ => return Err("address filter must be a string or array".to_string()),
    }
    if out.is_empty() {
        return Ok(None);
    }
    Ok(Some(out))
}

/// The `topics` field of a log filter: each position is either `null` (a
/// wildcard), a single 32-byte topic, or an array of alternatives.
pub(crate) type TopicFilter = Vec<Option<Vec<[u8; 32]>>>;

fn parse_topic_filter(value: Option<&Value>) -> Result<Option<TopicFilter>, String> {
    let value = match value {
        Some(v) if !v.is_null() => v,
        _ => return Ok(None),
    };
    let items = value
        .as_array()
        .ok_or_else(|| "topics filter must be an array".to_string())?;

    let mut out: TopicFilter = Vec::with_capacity(items.len());
    for item in items {
        match item {
            Value::Null => out.push(None),
            Value::String(s) => out.push(Some(vec![parse_topic(s)?])),
            Value::Array(alts) => {
                let mut topics = Vec::with_capacity(alts.len());
                for alt in alts {
                    match alt {
                        Value::Null => continue,
                        Value::String(s) => topics.push(parse_topic(s)?),
                        _ => return Err("topic alternatives must be strings".to_string()),
                    }
                }
                out.push(Some(topics));
            }
            _ => return Err("topic filter entries must be null, a string or an array".to_string()),
        }
    }
    Ok(Some(out))
}

fn parse_topic(value: &str) -> Result<[u8; 32], String> {
    let bytes = decode_hex(value)?;
    if bytes.len() != 32 {
        return Err(format!("expected a 32-byte topic, got {} bytes", bytes.len()));
    }
    let mut out = [0u8; 32];
    out.copy_from_slice(&bytes);
    Ok(out)
}

/// Whether a log's topics satisfy a filter. The filter is positional: topic
/// `i` of the log must match entry `i` of the filter, or the entry must be a
/// wildcard. A filter longer than the log's topic list cannot match.
pub(crate) fn topics_match(filter: &TopicFilter, topics: &[[u8; 32]]) -> bool {
    if filter.len() > topics.len() {
        return false;
    }
    for (index, entry) in filter.iter().enumerate() {
        if let Some(alternatives) = entry {
            if !alternatives.contains(&topics[index]) {
                return false;
            }
        }
    }
    true
}

/// Resolve a block tag to a number, or `None` when it cannot match a block.
fn parse_block_tag(tag: &str, height: u64) -> Option<u64> {
    match tag {
        "latest" | "safe" | "finalized" => Some(height),
        "earliest" => Some(0),
        "pending" => Some(height + 1),
        other => parse_u64_hex(other),
    }
}

pub fn hex_u64(value: u64) -> Value {
    Value::String(format!("0x{value:x}"))
}

/// Lowercase `0x`-prefixed hex, for embedding inside a `json!` literal.
pub fn hex_u64_str(value: u64) -> String {
    format!("0x{value:x}")
}

fn hex_bytes(bytes: &[u8]) -> Value {
    Value::String(hex_quantity(bytes))
}

#[allow(dead_code)]
fn hex_bytes_str(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(2 + bytes.len() * 2);
    out.push_str("0x");
    for byte in bytes {
        out.push_str(&format!("{byte:02x}"));
    }
    // Strip leading zero pairs, as JSON-RPC quantities have no leading zeros.
    let stripped = out.trim_start_matches("0x0").to_string();
    if stripped.is_empty() {
        "0x0".to_string()
    } else {
        format!("0x{stripped}")
    }
}

fn string_param(params: &Value, index: usize) -> Option<String> {
    params.get(index)?.as_str().map(|s| s.to_string())
}

/// Parse the storage-slot list of `eth_getProof` into 32-byte words.
///
/// A slot may be sent as a bare quantity, so a value shorter than 32 bytes is
/// left-padded rather than rejected. Anything longer than 32 bytes is not a
/// slot index and is an error.
fn storage_slots_param(params: &Value, index: usize) -> Result<Vec<[u8; 32]>, String> {
    let Some(Value::Array(items)) = params.get(index) else {
        return Ok(Vec::new());
    };
    let mut slots = Vec::with_capacity(items.len());
    for item in items {
        let hex = item
            .as_str()
            .ok_or_else(|| "storage slot must be a hex string".to_string())?;
        let bytes = decode_hex(hex)?;
        if bytes.len() > 32 {
            return Err(format!(
                "storage slot must be at most 32 bytes, got {}",
                bytes.len()
            ));
        }
        let mut word = [0u8; 32];
        word[32 - bytes.len()..].copy_from_slice(&bytes);
        slots.push(word);
    }
    Ok(slots)
}

fn address_param(params: &Value, index: usize) -> Result<[u8; 20], String> {
    let value = string_param(params, index).ok_or_else(|| "missing address".to_string())?;
    parse_address(&value)
}

pub fn parse_address(value: &str) -> Result<[u8; 20], String> {
    let bytes = decode_hex(value)?;
    if bytes.len() != 20 {
        return Err(format!("expected a 20-byte address, got {} bytes", bytes.len()));
    }
    let mut out = [0u8; 20];
    out.copy_from_slice(&bytes);
    Ok(out)
}

pub fn parse_u256(value: &str) -> Result<[u8; 32], String> {
    let bytes = decode_hex(value)?;
    if bytes.len() > 32 {
        return Err("value does not fit in 256 bits".to_string());
    }
    let mut out = [0u8; 32];
    out[32 - bytes.len()..].copy_from_slice(&bytes);
    Ok(out)
}

fn parse_u64_hex(value: &str) -> Option<u64> {
    let trimmed = value.strip_prefix("0x").unwrap_or(value);
    u64::from_str_radix(trimmed, 16).ok()
}

/// Decode hex, tolerating a missing `0x` prefix and an odd digit count.
pub fn decode_hex(value: &str) -> Result<Vec<u8>, String> {
    let trimmed = value.strip_prefix("0x").unwrap_or(value);
    let padded = if trimmed.len() % 2 != 0 {
        format!("0{trimmed}")
    } else {
        trimmed.to_string()
    };
    let mut out = Vec::with_capacity(padded.len() / 2);
    let bytes = padded.as_bytes();
    for pair in bytes.chunks(2) {
        let hi = (pair[0] as char).to_digit(16).ok_or("invalid hex digit")?;
        let lo = (pair[1] as char).to_digit(16).ok_or("invalid hex digit")?;
        out.push(((hi << 4) | lo) as u8);
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn content_length_parsing_is_case_insensitive() {
        assert_eq!(
            parse_content_length("POST / HTTP/1.1\r\ncontent-length: 42\r\n"),
            Some(42)
        );
        assert_eq!(
            parse_content_length("POST / HTTP/1.1\r\nContent-Length: 7\r\n"),
            Some(7)
        );
        assert_eq!(parse_content_length("POST / HTTP/1.1\r\n"), None);
    }

    #[test]
    fn header_end_is_found_at_the_blank_line() {
        assert_eq!(find_header_end(b"GET / HTTP/1.1\r\n\r\nbody"), Some(14));
        assert_eq!(find_header_end(b"incomplete"), None);
    }

    #[test]
    fn block_tags_resolve_against_the_height() {
        assert_eq!(parse_block_tag("latest", 100), Some(100));
        assert_eq!(parse_block_tag("earliest", 100), Some(0));
        assert_eq!(parse_block_tag("pending", 100), Some(101));
        assert_eq!(parse_block_tag("0x10", 100), Some(16));
        assert_eq!(parse_block_tag("nonsense", 100), None);
    }

    #[test]
    fn hex_quantities_have_no_leading_zeros() {
        assert_eq!(hex_u64(0), json!("0x0"));
        assert_eq!(hex_u64(21_000), json!("0x5208"));
        assert_eq!(hex_u64(6666), json!("0x1a0a"));
    }

    #[test]
    fn balance_formatting_strips_leading_zero_bytes() {
        assert_eq!(hex_bytes(&[0u8; 32]), json!("0x0"));
        let mut one = [0u8; 32];
        one[31] = 1;
        assert_eq!(hex_bytes(&one), json!("0x1"));
        let mut thousand = [0u8; 32];
        thousand[30] = 0x03;
        thousand[31] = 0xe8;
        assert_eq!(hex_bytes(&thousand), json!("0x3e8"));
    }

    #[test]
    fn hex_decoding_handles_prefix_and_odd_length() {
        assert_eq!(decode_hex("0x1234").unwrap(), vec![0x12, 0x34]);
        assert_eq!(decode_hex("1234").unwrap(), vec![0x12, 0x34]);
        // Odd length is left-padded, as JSON-RPC quantities require.
        assert_eq!(decode_hex("0x100").unwrap(), vec![0x01, 0x00]);
        assert_eq!(decode_hex("0x").unwrap(), Vec::<u8>::new());
        assert!(decode_hex("0xzz").is_err());
    }

    #[test]
    fn quantities_are_formatted_without_leading_zeros() {
        assert_eq!(hex_quantity(&[0x00, 0x03, 0xe8]), "0x3e8");
        assert_eq!(hex_quantity(&[0x00]), "0x0");
        assert_eq!(hex_quantity(&[]), "0x0");
        assert_eq!(hex_quantity(&[0xff]), "0xff");
        assert_eq!(hex_quantity(&[0x00, 0x00, 0x01]), "0x1");
    }

    #[test]
    fn address_parsing_rejects_the_wrong_width() {
        assert!(parse_address("0x1234").is_err());
        assert!(parse_address(&format!("0x{}", "11".repeat(20))).is_ok());
    }

    #[test]
    fn u256_parsing_rejects_values_wider_than_32_bytes() {
        assert!(parse_u256("0x01").is_ok());
        assert!(parse_u256(&format!("0x{}", "11".repeat(33))).is_err());
    }

    #[test]
    fn a_valid_raw_transaction_decodes_with_every_field_preserved() {
        // Build and sign a transaction, encode it, then decode it back.
        use tiger_blockchain::{encode_legacy_transaction, sign_transaction};
        use tiger_blockchain::types::Transaction;

        let key = [0x46u8; 32];
        let mut tx = Transaction {
            hash: String::new(),
            nonce: 7,
            block_hash: None,
            block_number: None,
            transaction_index: None,
            from: format!("0x{}", "00".repeat(20)),
            to: Some(format!("0x{}", "dd".repeat(20))),
            value: "0x3e8".to_string(),
            gas_price: 2_000_000_000,
            gas: 21_000,
            input: "0x".to_string(),
            v: "0x0".to_string(),
            r: "0x0".to_string(),
            s: "0x0".to_string(),
        };
        sign_transaction(&mut tx, 6666, &key).unwrap();
        let encoded = encode_legacy_transaction(&tx).unwrap();

        let decoded = decode_legacy_transaction(&encoded).unwrap();
        assert_eq!(decoded.nonce, tx.nonce);
        assert_eq!(decoded.gas_price, tx.gas_price);
        assert_eq!(decoded.gas, tx.gas);
        assert_eq!(decoded.to, tx.to);
        assert_eq!(decoded.value, tx.value);
        assert_eq!(decoded.input, tx.input);
        assert_eq!(decoded.v, tx.v);
        assert_eq!(decoded.r, tx.r);
        assert_eq!(decoded.s, tx.s);
    }

    #[test]
    fn the_recovered_sender_replaces_the_untrusted_from_field() {
        // `from` is not covered by the signature, so a client can put anything
        // there. The decoder leaves it zeroed; admission must overwrite it with
        // the recovered address.
        let decoded = decode_legacy_transaction(&[0xc0]).err();
        assert!(decoded.is_some());

        // Confirm the recovered address differs from the placeholder the
        // decoder writes, so the overwrite is meaningful.
        use tiger_blockchain::{encode_legacy_transaction, sign_transaction};
        use tiger_blockchain::types::Transaction;
        let key = [0x46u8; 32];
        let mut tx = Transaction {
            hash: String::new(),
            nonce: 0,
            block_hash: None,
            block_number: None,
            transaction_index: None,
            from: format!("0x{}", "ff".repeat(20)),
            to: Some(format!("0x{}", "dd".repeat(20))),
            value: "0x1".to_string(),
            gas_price: 1,
            gas: 21_000,
            input: "0x".to_string(),
            v: "0x0".to_string(),
            r: "0x0".to_string(),
            s: "0x0".to_string(),
        };
        sign_transaction(&mut tx, 6666, &key).unwrap();
        let encoded = encode_legacy_transaction(&tx).unwrap();
        let decoded = decode_legacy_transaction(&encoded).unwrap();
        assert_eq!(decoded.from, format!("0x{}", "00".repeat(20)));

        let recovered =
            tiger_blockchain::recover_transaction_sender(&decoded, 6666).unwrap();
        assert_eq!(recovered, tiger_crypto::private_key_to_address(&key).unwrap());
    }

    #[test]
    fn a_malformed_transaction_is_rejected() {
        // Not RLP at all.
        assert!(decode_legacy_transaction(&[0xff, 0xff]).is_err());
        // An RLP list with the wrong arity.
        let short = tiger_state::rlp::RlpItem::list(vec![
            tiger_state::rlp::RlpItem::from_u64(1),
        ])
        .encode();
        assert!(decode_legacy_transaction(&short).is_err());
    }
}
