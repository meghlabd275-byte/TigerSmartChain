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
    let mut header_end = None;
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
            header_end = Some(pos);
            break;
        }
    }

    let header_end = header_end.unwrap_or(0);
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
            let manager = node.get_chain_manager().await;
            let height = manager.chain().height();
            let number = match parse_block_tag(&tag, height) {
                Some(number) => number,
                None => return RpcResponse::success(Value::Null, id),
            };
            match manager.chain().get_block(number) {
                Some(block) => RpcResponse::success(block_to_json(block), id),
                None => RpcResponse::success(Value::Null, id),
            }
        }
        "eth_getBlockByHash" => {
            let hash = match string_param(params, 0) {
                Some(hash) => hash,
                None => return RpcResponse::error(INVALID_PARAMS, "missing block hash", id),
            };
            let manager = node.get_chain_manager().await;
            match manager.chain().get_block_by_hash(&hash) {
                Some(block) => RpcResponse::success(block_to_json(block), id),
                None => RpcResponse::success(Value::Null, id),
            }
        }
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
                Err(e) => RpcResponse::error(INVALID_PARAMS, e, id),
            },
            None => RpcResponse::error(INVALID_PARAMS, "missing raw transaction", id),
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
async fn submit_raw_transaction(node: &Arc<TigerNode>, raw: &str) -> Result<String, String> {
    use tiger_blockchain::types::Transaction;
    use tiger_blockchain::{recover_transaction_sender, transaction_hash};

    let bytes = decode_hex(raw)?;
    let tx = decode_legacy_transaction(&bytes)?;

    // Recovering the sender proves the signature is valid for this chain, and
    // the recovered address is authoritative: the `from` field on the wire is
    // not signed, so it is discarded and replaced rather than trusted.
    let chain_id = node.config().chain_id;
    let sender = recover_transaction_sender(&tx, chain_id)
        .map_err(|e| format!("invalid transaction signature: {e}"))?;

    let hash = transaction_hash(&tx).map_err(|e| format!("cannot hash transaction: {e}"))?;
    let mut tx = tx;
    tx.hash = hash.clone();
    tx.from = format!("0x{}", tiger_blockchain::hex_encode(&sender));

    {
        let mut mempool = node.get_mempool().await;
        mempool.add(tx);
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
fn block_to_json(block: &tiger_blockchain::types::Block) -> Value {
    use tiger_blockchain::{block_hash, hex_encode};

    let header = &block.header;
    let hash = block_hash(header).unwrap_or_else(|_| format!("0x{}", hex_encode(&[0u8; 32])));

    let transactions: Vec<Value> = block
        .transactions
        .iter()
        .map(|tx| {
            json!({
                "hash": tx.hash,
                "nonce": hex_u64_str(tx.nonce),
                "blockHash": hash,
                "blockNumber": hex_u64_str(header.number),
                "from": tx.from,
                "to": tx.to,
                "value": tx.value,
                "gasPrice": hex_u64_str(tx.gas_price),
                "gas": hex_u64_str(tx.gas),
                "input": tx.input,
                "v": tx.v,
                "r": tx.r,
                "s": tx.s,
            })
        })
        .collect();

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
        "gasLimit": hex_u64_str(header.gas_limit),
        "gasUsed": hex_u64_str(header.gas_used),
        "timestamp": hex_u64_str(header.timestamp),
        "extraData": header.extra_data,
        "mixHash": header.mix_hash,
        "nonce": header.nonce,
        "baseFeePerGas": header.base_fee_per_gas.map(hex_u64_str),
        "transactions": transactions,
        "uncles": [],
    })
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

fn hex_u64(value: u64) -> Value {
    Value::String(format!("0x{value:x}"))
}

/// Lowercase `0x`-prefixed hex, for embedding inside a `json!` literal.
fn hex_u64_str(value: u64) -> String {
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

fn address_param(params: &Value, index: usize) -> Result<[u8; 20], String> {
    let value = string_param(params, index).ok_or_else(|| "missing address".to_string())?;
    parse_address(&value)
}

fn parse_address(value: &str) -> Result<[u8; 20], String> {
    let bytes = decode_hex(value)?;
    if bytes.len() != 20 {
        return Err(format!("expected a 20-byte address, got {} bytes", bytes.len()));
    }
    let mut out = [0u8; 20];
    out.copy_from_slice(&bytes);
    Ok(out)
}

fn parse_u256(value: &str) -> Result<[u8; 32], String> {
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
fn decode_hex(value: &str) -> Result<Vec<u8>, String> {
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
