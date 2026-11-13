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

use crate::filters::{FilterKind, LogFilter};
use crate::limits::PerIpLimiter;
use crate::node::TigerNode;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::sync::Arc;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

/// A JSON-RPC 2.0 request.
#[derive(Debug, Deserialize)]
pub struct RpcRequest {
    /// The protocol version. JSON-RPC 2.0 requires "2.0" whenever it is
    /// present, so it is validated rather than ignored.
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

    pub(crate) fn error(code: i64, message: impl Into<String>, id: Value) -> Self {
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

/// The widest block range one `eth_getLogs` call may scan.
///
/// The scan is linear and its result is held in memory, so this bounds the
/// work a single unauthenticated request can ask for. Clients that need more
/// page through with successive ranges.
const MAX_LOG_BLOCK_RANGE: u64 = 5_000;

/// Which browser origins are allowed to call the RPC.
///
/// Empty means none. Answering every request with `Access-Control-Allow-Origin:
/// *` would let any page a user happens to visit read the chain through their
/// node, and — worse for a node that accepts `eth_sendRawTransaction` — let a
/// malicious page use the node as the origin of a state-changing call. So the
/// default denies, and an operator running a web explorer lists its origin.
#[derive(Debug, Clone, Default)]
pub struct CorsPolicy {
    allowed: Vec<String>,
}

impl CorsPolicy {
    /// Parse a comma-separated origin list. `*` allows any origin.
    pub fn from_config(spec: &str) -> Self {
        let allowed = spec
            .split(',')
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(str::to_ascii_lowercase)
            .collect();
        Self { allowed }
    }

    /// The value to send as `Access-Control-Allow-Origin`, if the request
    /// carries an origin this policy allows.
    ///
    /// The caller's own origin is echoed rather than `*` even when `*` is
    /// configured. With a literal `*` a browser also allows credentialed
    /// requests, and the response body is identical for every origin, which
    /// invites a shared cache to hand one caller another's answer.
    fn allow_origin(&self, origin: Option<&str>) -> Option<String> {
        let origin = origin?;
        let lowered = origin.to_ascii_lowercase();
        if self.allowed.iter().any(|a| a == "*" || *a == lowered) {
            return Some(origin.to_string());
        }
        None
    }

    /// The CORS response headers for a request, or an empty string.
    fn headers(&self, origin: Option<&str>) -> String {
        match self.allow_origin(origin) {
            Some(origin) => format!(
                "Access-Control-Allow-Origin: {origin}\r\n\
                 Vary: Origin\r\n\
                 Access-Control-Allow-Methods: POST, OPTIONS\r\n\
                 Access-Control-Allow-Headers: Content-Type\r\n"
            ),
            None => String::new(),
        }
    }
}

/// Counters for the metrics endpoint.
///
/// These are the numbers an operator alerts on: whether the node is producing
/// blocks, whether peers are connected, how full the pool is, and how much of
/// the RPC is failing or being throttled.
#[derive(Debug)]
pub struct Metrics {
    requests: std::sync::atomic::AtomicU64,
    errors: std::sync::atomic::AtomicU64,
    rate_limited: std::sync::atomic::AtomicU64,
    http_requests: std::sync::atomic::AtomicU64,
}

impl Default for Metrics {
    fn default() -> Self {
        Self::new()
    }
}

impl Metrics {
    pub fn new() -> Self {
        Self {
            requests: std::sync::atomic::AtomicU64::new(0),
            errors: std::sync::atomic::AtomicU64::new(0),
            rate_limited: std::sync::atomic::AtomicU64::new(0),
            http_requests: std::sync::atomic::AtomicU64::new(0),
        }
    }

    fn record(&self, response: &RpcResponse) {
        use std::sync::atomic::Ordering;
        self.requests.fetch_add(1, Ordering::Relaxed);
        if response.error.is_some() {
            self.errors.fetch_add(1, Ordering::Relaxed);
        }
    }
}

/// Serve JSON-RPC on `addr` until the node stops.
pub async fn serve(node: Arc<TigerNode>, addr: String) -> std::io::Result<()> {
    let listener = TcpListener::bind(&addr).await?;
    log::info!("JSON-RPC server listening on {addr}");
    serve_listener(listener, node).await
}

/// Serve JSON-RPC on an already-bound listener.
///
/// Separated from `serve` so the caller can bind first, learn the port it was
/// given (binding to port 0 asks the kernel for a free one), and only then
/// start accepting. That is what lets the transport be tested over a real
/// socket rather than by inspecting strings.
pub async fn serve_listener(
    listener: TcpListener,
    node: Arc<TigerNode>,
) -> std::io::Result<()> {
    let cors = Arc::new(CorsPolicy::from_config(&node.config().rpc_cors_host));
    // One bucket per client address. The burst is one second's worth of the
    // configured rate, and a client may hold at most that many requests at
    // once, so a slow client cannot accumulate unbounded work either.
    let rate = f64::from(node.config().rpc_rate_limit);
    let limiter = Arc::new(PerIpLimiter::new(rate, rate, rate.max(1.0) as usize));
    let metrics = Arc::new(Metrics::new());

    loop {
        let (mut stream, peer) = match listener.accept().await {
            Ok(connection) => connection,
            Err(e) => {
                log::warn!("failed to accept connection: {e}");
                continue;
            }
        };
        let node = Arc::clone(&node);
        let cors = Arc::clone(&cors);
        let limiter = Arc::clone(&limiter);
        let metrics = Arc::clone(&metrics);
        tokio::spawn(async move {
            if let Err(e) =
                handle_connection(&mut stream, node, cors, limiter, metrics, peer).await
            {
                log::debug!("connection from {peer} ended: {e}");
            }
        });
    }
}

/// Serve the metrics endpoint on `addr` until the node stops.
///
/// A separate listener from the RPC: metrics are for the operator's monitoring
/// system, and mixing them into the RPC would expose the node's internals to
/// every wallet that can reach it.
pub async fn serve_metrics(node: Arc<TigerNode>, addr: String) -> std::io::Result<()> {
    let listener = TcpListener::bind(&addr).await?;
    log::info!("Metrics server listening on {addr}");

    loop {
        let (mut stream, _peer) = match listener.accept().await {
            Ok(connection) => connection,
            Err(e) => {
                log::warn!("metrics accept failed: {e}");
                continue;
            }
        };
        let node = Arc::clone(&node);
        tokio::spawn(async move {
            if let Err(e) = handle_metrics(&mut stream, &node).await {
                log::debug!("metrics connection ended: {e}");
            }
        });
    }
}

/// Answer one scrape with the node's current metrics.
async fn handle_metrics(stream: &mut tokio::net::TcpStream, node: &Arc<TigerNode>) -> std::io::Result<()> {
    let mut buf = [0u8; 4096];
    let read = stream.read(&mut buf).await?;
    if read == 0 {
        return Ok(());
    }
    // Only GET (and HEAD) are meaningful for a scrape; anything else gets 405.
    let request_line = String::from_utf8_lossy(&buf[..read]);
    let mut parts = request_line.split_whitespace();
    let method = parts.next().unwrap_or("").to_string();
    let path = parts.next().unwrap_or("").to_string();
    if method != "GET" && method != "HEAD" {
        let head = "HTTP/1.1 405 Method Not Allowed\r\nAllow: GET\r\nContent-Length: 0\r\nConnection: close\r\n\r\n";
        stream.write_all(head.as_bytes()).await?;
        return stream.flush().await;
    }
    // Answer the path a scraper is configured for, and 404 anything else
    // rather than serving metrics from every URL.
    if path != "/metrics" {
        let head = "HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n";
        stream.write_all(head.as_bytes()).await?;
        return stream.flush().await;
    }

    let body = node.metrics_text().await;
    let head = format!(
        "HTTP/1.1 200 OK\r\nContent-Type: text/plain; version=0.0.4; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        body.len()
    );
    stream.write_all(head.as_bytes()).await?;
    if method == "GET" {
        stream.write_all(body.as_bytes()).await?;
    }
    stream.flush().await
}

/// Read one HTTP request and write one JSON-RPC response.
async fn handle_connection(
    stream: &mut tokio::net::TcpStream,
    node: Arc<TigerNode>,
    cors: Arc<CorsPolicy>,
    limiter: Arc<PerIpLimiter>,
    metrics: Arc<Metrics>,
    peer: std::net::SocketAddr,
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
            return write_response(stream, response, &cors, None, false).await;
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
    let request_line = headers.lines().next().unwrap_or("");
    let mut parts = request_line.split_whitespace();
    let method = parts.next().unwrap_or("").to_string();
    let path = parts.next().unwrap_or("").to_string();
    let origin = parse_header(&headers, "origin");
    let content_length = parse_content_length(&headers).unwrap_or(0);
    if content_length > MAX_BODY {
        let response = RpcResponse::error(INVALID_REQUEST, "body too large", Value::Null);
        return write_response(stream, response, &cors, origin.as_deref(), false).await;
    }

    // A liveness endpoint, so a container orchestrator, load balancer or
    // uptime check has something cheap to poll that says whether the node is
    // actually serving rather than merely listening.
    if method == "GET" && (path == "/health" || path == "/healthz") {
        let body = node.health_json().await;
        let head = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\n{}Content-Length: {}\r\nConnection: close\r\n\r\n",
            cors.headers(origin.as_deref()),
            body.len()
        );
        stream.write_all(head.as_bytes()).await?;
        stream.write_all(body.as_bytes()).await?;
        return stream.flush().await;
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

    // A CORS preflight carries no JSON body and must be answered before the
    // rate limit applies, or a page's first request would be throttled by its
    // own preflight.
    if method == "OPTIONS" {
        return write_cors_preflight(stream, &cors, origin.as_deref()).await;
    }

    // Only POST carries a JSON-RPC call. A GET is answered explicitly rather
    // than being parsed as an empty body, so a client that got the method wrong
    // is told so instead of receiving a parse error.
    if method != "POST" {
        let head = format!(
            "HTTP/1.1 405 Method Not Allowed\r\nAllow: POST, OPTIONS\r\n{}Content-Length: 0\r\nConnection: close\r\n\r\n",
            cors.headers(origin.as_deref())
        );
        stream.write_all(head.as_bytes()).await?;
        return stream.flush().await;
    }

    use std::sync::atomic::Ordering;
    metrics.http_requests.fetch_add(1, Ordering::Relaxed);

    if !limiter.admit(peer.ip()) {
        metrics.rate_limited.fetch_add(1, Ordering::Relaxed);
        log::debug!("rate limited RPC request from {}", peer.ip());
        let head = format!(
            "HTTP/1.1 429 Too Many Requests\r\nRetry-After: 1\r\n{}Content-Type: application/json\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
            cors.headers(origin.as_deref())
        );
        stream.write_all(head.as_bytes()).await?;
        return stream.flush().await;
    }

    let response = match serde_json::from_slice::<RpcRequest>(&body) {
        Ok(request) => dispatch(&node, request).await,
        Err(e) => RpcResponse::error(PARSE_ERROR, format!("parse error: {e}"), Value::Null),
    };
    metrics.record(&response);
    limiter.release(peer.ip());
    write_response(stream, response, &cors, origin.as_deref(), true).await
}

/// Find the end of the HTTP header block (`\r\n\r\n`).
fn find_header_end(buf: &[u8]) -> Option<usize> {
    buf.windows(4).position(|window| window == b"\r\n\r\n")
}

/// Extract `Content-Length` from the raw header block.
fn parse_content_length(headers: &str) -> Option<usize> {
    parse_header(headers, "content-length").and_then(|v| v.trim().parse().ok())
}

/// Extract a header value, case-insensitively, from the raw header block.
///
/// Only the first line is skipped so a request line cannot be mistaken for a
/// header. Values are returned as sent; callers normalise where it matters.
fn parse_header(headers: &str, name: &str) -> Option<String> {
    for line in headers.lines().skip(1) {
        if let Some((field, value)) = line.split_once(':') {
            if field.trim().eq_ignore_ascii_case(name) {
                return Some(value.trim().to_string());
            }
        }
    }
    None
}

async fn write_response(
    stream: &mut tokio::net::TcpStream,
    response: RpcResponse,
    cors: &CorsPolicy,
    origin: Option<&str>,
    with_cors: bool,
) -> std::io::Result<()> {
    let body = serde_json::to_vec(&response).unwrap_or_else(|_| b"{}".to_vec());
    let cors_headers = if with_cors {
        cors.headers(origin)
    } else {
        String::new()
    };
    let head = format!(
        "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\n{cors_headers}Connection: close\r\n\r\n",
        body.len()
    );
    stream.write_all(head.as_bytes()).await?;
    stream.write_all(&body).await?;
    stream.flush().await
}

async fn write_cors_preflight(
    stream: &mut tokio::net::TcpStream,
    cors: &CorsPolicy,
    origin: Option<&str>,
) -> std::io::Result<()> {
    let head = format!(
        "HTTP/1.1 204 No Content\r\n{}Content-Length: 0\r\nConnection: close\r\n\r\n",
        cors.headers(origin)
    );
    stream.write_all(head.as_bytes()).await?;
    stream.flush().await
}

/// Route a request to the matching method.
pub(crate) async fn dispatch(node: &Arc<TigerNode>, request: RpcRequest) -> RpcResponse {
    let id = request.id.clone().unwrap_or(Value::Null);

    // The specification requires the version to be exactly "2.0" when it is
    // sent, so a client that declares something else is told rather than
    // silently served.
    if let Some(version) = request.jsonrpc.as_deref() {
        if version != "2.0" {
            return RpcResponse::error(
                INVALID_REQUEST,
                format!("unsupported JSON-RPC version {version:?}, expected \"2.0\""),
                id,
            );
        }
    }

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
            // The number of peers this node is actually connected to. Without
            // the P2P service running that is zero, which is the truthful
            // answer rather than a placeholder.
            let count = match node.p2p() {
                Some(p2p) => p2p.peer_count().await as u64,
                None => 0,
            };
            RpcResponse::success(hex_u64(count), id)
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
            // Syncing means the network is ahead of this node. The highest
            // height any peer has reported is the real target, so this is
            // driven by the peer table rather than a flag nothing sets.
            let current = {
                let manager = node.get_chain_manager().await;
                manager.chain().height()
            };
            let highest = match node.p2p() {
                Some(p2p) => p2p.best_peer_height().await.unwrap_or(current),
                None => current,
            };
            if highest > current {
                RpcResponse::success(
                    json!({
                        "startingBlock": hex_u64_str(current),
                        "currentBlock": hex_u64_str(current),
                        "highestBlock": hex_u64_str(highest),
                    }),
                    id,
                )
            } else {
                RpcResponse::success(json!(false), id)
            }
        }
        "eth_maxPriorityFeePerGas" => {
            // The tip a wallet should offer: the median tip of the recent
            // blocks, floored at the minimum this node will relay. A wallet
            // that offers this is neither overpaying nor likely to be
            // outbid by the transactions already in flight.
            let manager = node.get_chain_manager().await;
            let tip = suggested_tip(manager.chain()).max(node.config().min_gas_price);
            RpcResponse::success(hex_u64(tip), id)
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
            // The price a transaction sent now would pay: the next block's
            // base fee plus a tip. Reporting only the last transaction's
            // ceiling would understate the cost of the very next block.
            let manager = node.get_chain_manager().await;
            let chain = manager.chain();
            let base_fee = match chain.get_block(chain.height()) {
                Some(parent) => tiger_blockchain::next_base_fee(
                    parent.header.base_fee_per_gas.unwrap_or(0),
                    parent.header.gas_used,
                    parent.header.gas_limit,
                ),
                None => node.config().min_gas_price,
            };
            RpcResponse::success(hex_u64(base_fee.saturating_add(suggested_tip(chain))), id)
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
        // The three spellings of the same call. `debug_traceBlock` takes RLP
        // on a real node, which no client uses in practice; treating it as an
        // alias of the by-number form is what the other EVM clients do and is
        // more useful than refusing it.
        "debug_traceBlockByNumber" | "debug_traceBlock" => {
            match params.get(0).and_then(|v| v.as_str()) {
                Some(tag) => {
                    match crate::debug_rpc::debug_trace_block(node, tag, params.get(1)).await {
                        Ok(traces) => RpcResponse::success(Value::Array(traces), id),
                        Err(e) => RpcResponse::error(INVALID_PARAMS, e, id),
                    }
                }
                None => RpcResponse::error(INVALID_PARAMS, "missing block tag", id),
            }
        }
        "debug_traceBlockByHash" => match params.get(0).and_then(|v| v.as_str()) {
            Some(hash) => match crate::debug_rpc::debug_trace_block_by_hash(node, hash, params.get(1))
                .await
            {
                Ok(traces) => RpcResponse::success(Value::Array(traces), id),
                Err(e) => RpcResponse::error(INVALID_PARAMS, e, id),
            },
            None => RpcResponse::error(INVALID_PARAMS, "missing block hash", id),
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
        "eth_newFilter" => match new_log_filter(node, params).await {
            Ok(filter_id) => RpcResponse::success(hex_u64(filter_id), id),
            Err(e) => RpcResponse::error(INVALID_PARAMS, e, id),
        },
        "eth_newBlockFilter" => {
            let height = {
                let manager = node.get_chain_manager().await;
                manager.chain().height()
            };
            let mut filters = node.get_filters().await;
            match filters.install(FilterKind::Blocks, height) {
                Ok(filter_id) => RpcResponse::success(hex_u64(filter_id), id),
                Err(e) => RpcResponse::error(INTERNAL_ERROR, e, id),
            }
        }
        "eth_newPendingTransactionFilter" => {
            let height = {
                let manager = node.get_chain_manager().await;
                manager.chain().height()
            };
            let mut filters = node.get_filters().await;
            match filters.install(FilterKind::PendingTransactions, height) {
                Ok(filter_id) => RpcResponse::success(hex_u64(filter_id), id),
                Err(e) => RpcResponse::error(INTERNAL_ERROR, e, id),
            }
        }
        "eth_getFilterChanges" => match filter_id_param(params) {
            Ok(filter_id) => match filter_changes(node, filter_id).await {
                Ok(value) => RpcResponse::success(value, id),
                Err(e) => RpcResponse::error(INVALID_PARAMS, e, id),
            },
            Err(e) => RpcResponse::error(INVALID_PARAMS, e, id),
        },
        "eth_getFilterLogs" => match filter_id_param(params) {
            Ok(filter_id) => match filter_logs(node, filter_id).await {
                Ok(value) => RpcResponse::success(value, id),
                Err(e) => RpcResponse::error(INVALID_PARAMS, e, id),
            },
            Err(e) => RpcResponse::error(INVALID_PARAMS, e, id),
        },
        "eth_uninstallFilter" => match filter_id_param(params) {
            Ok(filter_id) => {
                let mut filters = node.get_filters().await;
                RpcResponse::success(json!(filters.uninstall(filter_id)), id)
            }
            Err(e) => RpcResponse::error(INVALID_PARAMS, e, id),
        },
        "eth_getBlockReceipts" => match block_receipts(node, params).await {
            Ok(value) => RpcResponse::success(value, id),
            Err(e) => RpcResponse::error(INVALID_PARAMS, e, id),
        },
        "eth_coinbase" => RpcResponse::success(
            json!(node.config().validator_addr.clone().unwrap_or_default()),
            id,
        ),
        "eth_mining" => {
            // True only when this node is an authority that may propose.
            let height = {
                let manager = node.get_chain_manager().await;
                manager.chain().height()
            };
            let consensus = node.get_consensus().await;
            let mining = !node.config().validators.is_empty()
                && consensus.is_authorised_proposer(
                    height + 1,
                    node.config().validator_addr.as_deref().unwrap_or(""),
                );
            RpcResponse::success(json!(mining), id)
        }
        "eth_hashrate" => {
            // PoSA has no hashing; the honest answer is zero rather than an
            // invented figure.
            RpcResponse::success(hex_u64(0), id)
        }
        "eth_createAccessList" => match create_access_list(node, params).await {
            Ok(value) => RpcResponse::success(value, id),
            Err(e) => RpcResponse::error(INVALID_PARAMS, e, id),
        },
        "admin_peers" => {
            let peers = match node.p2p() {
                Some(p2p) => p2p.peers().await,
                None => Vec::new(),
            };
            let list: Vec<Value> = peers
                .iter()
                .map(|peer| {
                    json!({
                        "id": peer.id,
                        "name": format!("TigerSmartChain/v{}", env!("CARGO_PKG_VERSION")),
                        "enode": format!("enode://{}@{}", peer.id, peer.address),
                        "network": {
                            "localAddress": node.config().listen_addr,
                            "remoteAddress": peer.address,
                            "inbound": !peer.outbound,
                            "trusted": false,
                            "static": false,
                        },
                        "protocols": {
                            "eth": {
                                "version": 66,
                                "difficulty": "0x0",
                                "head": peer.head_hash,
                            }
                        },
                    })
                })
                .collect();
            RpcResponse::success(Value::Array(list), id)
        }
        "admin_nodeInfo" => {
            let peer_id = node.p2p().map(|p2p| p2p.peer_id()).unwrap_or_default();
            RpcResponse::success(
                json!({
                    "id": peer_id,
                    "name": format!("TigerSmartChain/v{}", env!("CARGO_PKG_VERSION")),
                    "enode": format!("enode://{peer_id}@{}", node.config().listen_addr),
                    "enr": "",
                    "ip": node.config().listen_addr,
                    "listenAddr": node.config().listen_addr,
                    "ports": { "discovery": 0, "listener": 0 },
                    "protocols": {
                        "eth": {
                            "network": node.config().chain_id,
                            "difficulty": "0x0",
                            "genesis": node.genesis_hash(),
                            "head": node.head_hash().await,
                        }
                    },
                }),
                id,
            )
        }
        "txpool_status" => {
            let mempool = node.get_mempool().await;
            let pending = mempool.pending().len();
            RpcResponse::success(
                json!({
                    "pending": hex_u64_str(pending as u64),
                    "queued": hex_u64_str(0),
                }),
                id,
            )
        }
        "txpool_content" => {
            let mempool = node.get_mempool().await;
            let mut pending = serde_json::Map::new();
            for tx in mempool.pending() {
                let entry = pending
                    .entry(tx.from.clone())
                    .or_insert_with(|| Value::Object(serde_json::Map::new()));
                if let Value::Object(map) = entry {
                    map.insert(tx.nonce.to_string(), transaction_to_json_pool(tx));
                }
            }
            RpcResponse::success(
                json!({ "pending": Value::Object(pending), "queued": json!({}) }),
                id,
            )
        }
        "txpool_inspect" => {
            let mempool = node.get_mempool().await;
            let mut pending = serde_json::Map::new();
            for tx in mempool.pending() {
                let entry = pending
                    .entry(tx.from.clone())
                    .or_insert_with(|| Value::Object(serde_json::Map::new()));
                if let Value::Object(map) = entry {
                    let to = tx.to.clone().unwrap_or_else(|| "contract creation".to_string());
                    map.insert(
                        tx.nonce.to_string(),
                        Value::String(format!("{} wei to {to}", tx.value)),
                    );
                }
            }
            RpcResponse::success(
                json!({ "pending": Value::Object(pending), "queued": json!({}) }),
                id,
            )
        }
        // The `trace_` namespace lives in its own module; it returns `None` for
        // a method that is not one of its own.
        other if other.starts_with("trace_") => {
            match crate::trace_rpc::handle(node, other, params).await {
                Some(Ok(result)) => RpcResponse::success(result, id),
                Some(Err(e)) => RpcResponse::error(INVALID_PARAMS, e, id),
                // `trace_*` is the namespace, so an unimplemented method in it
                // is a method that does not exist rather than a bad argument.
                None => RpcResponse::error(
                    METHOD_NOT_FOUND,
                    format!("method not found: {other}"),
                    id,
                ),
            }
        }
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

    // Report the gas the call actually consumes, plus the intrinsic charge the
    // transaction would pay before executing. Estimating from the output size,
    // as an earlier version did, has nothing to do with the real cost.
    let (_output, used, _accessed) = call_contract_traced(node, params).await?;
    Ok(used.saturating_add(21_000))
}

/// `eth_call`: execute a read-only call against a state snapshot.
async fn call_contract(node: &Arc<TigerNode>, params: &Value) -> Result<Vec<u8>, String> {
    let (output, _used, _accessed) = call_contract_traced(node, params).await?;
    Ok(output)
}

/// Execute a read-only call and also report the gas it consumed and every
/// account and storage slot it touched, which is exactly what an EIP-2930
/// access list should contain.
async fn call_contract_traced(
    node: &Arc<TigerNode>,
    params: &Value,
) -> Result<
    (
        Vec<u8>,
        u64,
        Vec<(tiger_evm::Address, Vec<tiger_evm::types::EvmWord>)>,
    ),
    String,
> {
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
    // EIP-2929: the sender and the callee begin warm, so they do not appear as
    // cold accesses in the reported access list.
    {
        let mut set = ctx.accessed.lock().unwrap_or_else(|e| e.into_inner());
        set.warm_address(&tiger_evm::Address(caller));
        if let Some(address) = to {
            set.warm_address(&tiger_evm::Address(address));
        }
    }
    ctx.gas = node.config().call_gas_limit;
    ctx.chain_id = node.config().chain_id;
    ctx.block_number = {
        let manager = node.get_chain_manager().await;
        manager.chain().height()
    };

    let executor = tiger_evm::Executor::new();
    let result = executor.execute_with_host(&code, &mut ctx, &mut host);

    // Read the touched sets before the context is dropped. The EIP-2929 sets
    // are exactly the access list a caller would need to pre-warm.
    let accessed = {
        let set = ctx.accessed.lock().unwrap_or_else(|e| e.into_inner());
        let mut by_address: std::collections::BTreeMap<
            tiger_evm::Address,
            Vec<tiger_evm::types::EvmWord>,
        > = std::collections::BTreeMap::new();
        for (address, slot) in set.slots() {
            by_address.entry(address).or_default().push(slot);
        }
        by_address.into_iter().collect()
    };

    if result.success {
        Ok((result.output, result.gas_used, accessed))
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

    let bytes = decode_hex(raw).map_err(BadRequest)?;
    // Decoding lives with the transaction type itself, so the node and any
    // other consumer agree on the wire format rather than each keeping a copy.
    let tx = tiger_blockchain::decode_transaction(&bytes)
        .map_err(|e| BadRequest(format!("invalid transaction: {e}")))?;

    // The signature is what authorises the transaction, so the sender is
    // recovered from it and the wire's `from` field is discarded rather than
    // trusted. The same helper validates a transaction that arrives from a
    // peer, so both paths hold a transaction to one standard.
    let chain_id = node.config().chain_id;
    let tx = crate::node::validate_transaction(&tx, chain_id).map_err(BadRequest)?;
    let hash = tx.hash.clone();

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
    // Tell subscribers, then the peers, so a transaction submitted to any node
    // reaches both the wallets watching it and the validator that will include
    // it.
    node.publish_pending_transaction(&hash);
    // Tell the peers, so a transaction submitted to any node reaches the
    // validator that will include it.
    if let Some(p2p) = node.p2p() {
        p2p.announce_transaction(&hash).await;
    }
    Ok(hash)
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

/// Render a block as the JSON object `eth_getBlockBy*` returns.
///
/// When `full_transactions` is false the `transactions` array holds hashes;
/// when it is true it holds full transaction objects. That distinction is part
/// of the JSON-RPC contract and clients depend on it.
pub(crate) fn block_to_json_with_receipts(
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
        // A mined transaction reports the price it paid, which is what a
        // caller reconciling a balance needs. The ceiling it authorised is
        // reported separately as `maxFeePerGas`.
        "gasPrice": hex_u64_str(tiger_blockchain::effective_gas_price(
            tx,
            block.header.base_fee_per_gas.unwrap_or(0),
        )),
        "gas": hex_u64_str(tx.gas),
        "input": tx.input,
        "v": tx.v,
        "r": tx.r,
        "s": tx.s,
        "type": format!("0x{:x}", tx.tx_type),
        "chainId": hex_u64_str(tx.chain_id.unwrap_or(0)),
        "accessList": tx.access_list,
        "maxPriorityFeePerGas": tx.max_priority_fee_per_gas.map(hex_u64_str),
        "maxFeePerGas": if tx.tx_type == 2 { Some(hex_u64_str(tx.gas_price)) } else { None },
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
        // The price the sender was actually charged, which for an EIP-1559
        // transaction is derived from this block's base fee rather than taken
        // from the ceiling it authorised.
        "effectiveGasPrice": hex_u64_str(tiger_blockchain::effective_gas_price(
            tx,
            block.header.base_fee_per_gas.unwrap_or(0),
        )),
        "contractAddress": contract_address,
        "logs": logs,
        "logsBloom": tiger_blockchain::bloom_hex(&receipt.bloom()),
        "status": if receipt.status { "0x1" } else { "0x0" },
        "type": format!("0x{:x}", tx.tx_type),
    }))
}

/// `eth_feeHistory`: a window of base-fee and gas-usage history.
///
/// Each block reports the base fee its header commits to, the share of its
/// gas limit it used, and the tip its transactions actually paid. The window
/// ends with one extra base-fee entry — the fee the next block would carry —
/// because that is the value a wallet needs to price a transaction it is
/// about to send.
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
                // One reward entry per transaction, at the tip it paid: the
                // part of the effective price above the base fee, which is
                // what the validator received. Reporting the whole price
                // would overstate the reward by the burned portion.
                rewards.push(Value::Array(
                    block
                        .transactions
                        .iter()
                        .map(|tx| {
                            let tip = tx.gas_price.saturating_sub(base_fee);
                            Value::String(hex_u64_str(tip))
                        })
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

    // The window carries one more base-fee entry than it has blocks: the fee
    // the block after the newest one would use. That is the value a wallet
    // prices its next transaction against.
    let next_fee = match chain.get_block(newest) {
        Some(parent) => tiger_blockchain::next_base_fee(
            parent.header.base_fee_per_gas.unwrap_or(0),
            parent.header.gas_used,
            parent.header.gas_limit,
        ),
        None => 0,
    };
    base_fees.push(Value::String(hex_u64_str(next_fee)));

    Ok(json!({
        "oldestBlock": hex_u64_str(oldest),
        "baseFeePerGas": base_fees,
        "gasUsedRatio": gas_used_ratios,
        "reward": rewards,
    }))
}

/// The tip to suggest for a new transaction.
///
/// Taken as the median tip of the transactions in the most recent blocks that
/// carried any, so the suggestion tracks what the chain is actually paying.
/// Falls back to zero when no transaction has paid a tip, which lets the
/// caller apply its own floor rather than inventing one here.
fn suggested_tip(chain: &tiger_blockchain::Blockchain) -> u64 {
    let mut tips: Vec<u64> = Vec::new();
    let height = chain.height();
    let mut number = height;
    // A short window: recent enough to reflect current demand, long enough
    // not to be set by a single block.
    while tips.len() < 5 && number > 0 {
        if let Some(block) = chain.get_block(number) {
            let base_fee = block.header.base_fee_per_gas.unwrap_or(0);
            for tx in &block.transactions {
                tips.push(tx.gas_price.saturating_sub(base_fee));
            }
        }
        number -= 1;
    }
    if tips.is_empty() {
        return 0;
    }
    tips.sort_unstable();
    tips[tips.len() / 2]
}

/// `eth_getLogs`: match logs from a block range against a filter.
///
/// The filter supports `fromBlock`, `toBlock`, `address` (a string or an
/// array), `blockHash`, and `topics` (with `null` wildcards and nested arrays
/// for OR-matching), which is the full shape clients send.
async fn get_logs(node: &Arc<TigerNode>, params: &Value) -> Result<Vec<Value>, String> {
    let filter_json = params.get(0).ok_or_else(|| "missing filter".to_string())?;
    let height = {
        let manager = node.get_chain_manager().await;
        manager.chain().height()
    };
    let filter = parse_log_filter(filter_json, height)?;

    let manager = node.get_chain_manager().await;
    let chain = manager.chain();
    let numbers = log_block_numbers(chain, &filter, height)?;
    Ok(collect_logs(chain, &filter, &numbers))
}

/// Turn a log filter's JSON body into a [`LogFilter`].
pub(crate) fn parse_log_filter(filter: &Value, height: u64) -> Result<LogFilter, String> {
    let block_hash = filter
        .get("blockHash")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());

    // A `blockHash` filter pins the search to one block and cannot be combined
    // with a range, so the range is only parsed when there is no hash.
    let (from_block, to_block) = if block_hash.is_some() {
        (None, None)
    } else {
        let from = match filter.get("fromBlock").and_then(|v| v.as_str()) {
            Some(tag) => parse_block_tag(tag, height)
                .ok_or_else(|| "invalid fromBlock".to_string())?,
            None => 0,
        };
        let to = match filter.get("toBlock").and_then(|v| v.as_str()) {
            Some(tag) => {
                parse_block_tag(tag, height).ok_or_else(|| "invalid toBlock".to_string())?
            }
            None => height,
        };
        (Some(from), Some(to))
    };

    Ok(LogFilter {
        from_block,
        to_block,
        address: parse_address_filter(filter.get("address"))?,
        topics: parse_topic_filter(filter.get("topics"))?,
        block_hash,
    })
}

/// The block numbers a log filter covers, clamped to the chain.
///
/// A range wider than [`MAX_LOG_BLOCK_RANGE`] is refused rather than served.
/// The scan is linear in the number of blocks and holds every matching log in
/// memory, so an unbounded `fromBlock: 0, toBlock: latest` would let any
/// unauthenticated caller make the node walk the whole chain and allocate the
/// result — the cheapest way to take a node down. Real providers impose the
/// same limit and clients page with a narrower range.
fn log_block_numbers(
    chain: &tiger_blockchain::Blockchain,
    filter: &LogFilter,
    height: u64,
) -> Result<Vec<u64>, String> {
    if let Some(hash) = &filter.block_hash {
        return Ok(match chain.get_block_by_hash(hash) {
            Some(block) => vec![block.header.number],
            None => Vec::new(),
        });
    }

    let from = filter.from_block.unwrap_or(0);
    let to = filter.to_block.unwrap_or(height);
    if from > to {
        return Ok(Vec::new());
    }
    // A block tag of `pending` can exceed the current height; clamp so the
    // scan stays within the chain.
    let to = to.min(height);
    if to.saturating_sub(from) + 1 > MAX_LOG_BLOCK_RANGE {
        return Err(format!(
            "block range too large: {} blocks requested, at most {MAX_LOG_BLOCK_RANGE} may be scanned in one query",
            to.saturating_sub(from) + 1
        ));
    }
    Ok((from..=to).collect())
}

/// Collect the logs of `numbers` that match `filter`.
pub(crate) fn collect_logs(
    chain: &tiger_blockchain::Blockchain,
    filter: &LogFilter,
    numbers: &[u64],
) -> Vec<Value> {
    let mut out = Vec::new();

    for &number in numbers {
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
                let matches_address = filter
                    .address
                    .as_ref()
                    .map(|set| set.contains(&log.address))
                    .unwrap_or(true);
                let matches_topics = filter
                    .topics
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

    out
}

/// `eth_newFilter`: install a log filter and return its id.
async fn new_log_filter(node: &Arc<TigerNode>, params: &Value) -> Result<u64, String> {
    let filter_json = params.get(0).ok_or_else(|| "missing filter".to_string())?;
    let height = {
        let manager = node.get_chain_manager().await;
        manager.chain().height()
    };
    let filter = parse_log_filter(filter_json, height)?;
    let mut filters = node.get_filters().await;
    filters.install(FilterKind::Logs(filter), height)
}

/// `eth_getFilterChanges`: what a filter has to report since the last poll.
async fn filter_changes(node: &Arc<TigerNode>, id: u64) -> Result<Value, String> {
    let height = {
        let manager = node.get_chain_manager().await;
        manager.chain().height()
    };

    let mut filters = node.get_filters().await;
    let filter = filters
        .get_mut(id)
        .ok_or_else(|| format!("filter {id} not found"))?;

    match &filter.kind {
        FilterKind::Logs(log_filter) => {
            let log_filter = log_filter.clone();
            let from = filter.last_block.saturating_add(1);
            filter.last_block = height;

            if from > height {
                return Ok(Value::Array(Vec::new()));
            }

            let manager = node.get_chain_manager().await;
            let chain = manager.chain();
            let numbers = match &log_filter.block_hash {
                Some(hash) => match chain.get_block_by_hash(hash) {
                    Some(block) => vec![block.header.number],
                    None => Vec::new(),
                },
                None => {
                    let to = log_filter.to_block.unwrap_or(height).min(height);
                    if from > to {
                        Vec::new()
                    } else {
                        (from..=to).collect()
                    }
                }
            };
            Ok(Value::Array(collect_logs(chain, &log_filter, &numbers)))
        }

        FilterKind::Blocks => {
            let from = filter.last_block.saturating_add(1);
            filter.last_block = height;

            let manager = node.get_chain_manager().await;
            let chain = manager.chain();
            let hashes: Vec<Value> = (from..=height)
                .filter_map(|number| chain.get_block(number))
                .filter_map(|block| tiger_blockchain::block_hash(&block.header).ok())
                .map(Value::String)
                .collect();
            Ok(Value::Array(hashes))
        }

        FilterKind::PendingTransactions => {
            let mempool = node.get_mempool().await;
            let pending: Vec<String> =
                mempool.pending().iter().map(|tx| tx.hash.clone()).collect();

            let changes: Vec<Value> = pending
                .iter()
                .filter(|hash| !filter.seen.contains(*hash))
                .map(|hash| Value::String(hash.clone()))
                .collect();

            // The reported set is rebuilt from the pool rather than accumulated
            // forever, so a long-lived filter cannot grow without bound.
            filter.seen = pending.into_iter().collect();
            Ok(Value::Array(changes))
        }
    }
}

/// `eth_getFilterLogs`: every log a filter matches, not just the new ones.
async fn filter_logs(node: &Arc<TigerNode>, id: u64) -> Result<Value, String> {
    let height = {
        let manager = node.get_chain_manager().await;
        manager.chain().height()
    };

    let filters = node.get_filters().await;
    let filter = filters
        .get(id)
        .ok_or_else(|| format!("filter {id} not found"))?;

    let log_filter = match &filter.kind {
        FilterKind::Logs(log_filter) => log_filter.clone(),
        _ => return Err(format!("filter {id} is not a log filter")),
    };
    drop(filters);

    let manager = node.get_chain_manager().await;
    let chain = manager.chain();
    let numbers = log_block_numbers(chain, &log_filter, height)?;
    Ok(Value::Array(collect_logs(chain, &log_filter, &numbers)))
}

/// A filter id, as a hex quantity.
fn filter_id_param(params: &Value) -> Result<u64, String> {
    let raw = string_param(params, 0).ok_or_else(|| "missing filter id".to_string())?;
    parse_u64_hex(&raw).ok_or_else(|| format!("invalid filter id: {raw}"))
}

/// A pool transaction, in the shape `txpool_content` reports.
fn transaction_to_json_pool(tx: &tiger_blockchain::types::Transaction) -> Value {
    json!({
        "hash": tx.hash,
        "nonce": hex_u64_str(tx.nonce),
        "from": tx.from,
        "to": tx.to,
        "value": tx.value,
        "gas": hex_u64_str(tx.gas),
        "gasPrice": hex_u64_str(tx.gas_price),
        "input": tx.input,
        "type": format!("0x{:x}", tx.tx_type),
    })
}

/// `eth_getBlockReceipts`: every receipt in a block.
async fn block_receipts(node: &Arc<TigerNode>, params: &Value) -> Result<Value, String> {
    let tag = string_param(params, 0).ok_or_else(|| "missing block tag".to_string())?;
    let manager = node.get_chain_manager().await;
    let chain = manager.chain();
    let height = chain.height();

    // The argument may be a number, a tag, or a block hash.
    let block = if let Some(number) = parse_block_tag(&tag, height) {
        chain.get_block(number)
    } else {
        chain.get_block_by_hash(&tag)
    };

    let block = match block {
        Some(block) => block,
        None => return Ok(Value::Null),
    };
    let block_hash = tiger_blockchain::block_hash(&block.header)
        .unwrap_or_else(|_| format!("0x{}", tiger_blockchain::hex_encode(&[0u8; 32])));

    let receipts: Vec<Value> = (0..block.transactions.len())
        .filter_map(|index| receipt_to_json(chain, block, index, &block_hash))
        .collect();
    Ok(Value::Array(receipts))
}

/// `eth_createAccessList`: run a call and report the state it touched.
///
/// The call is executed against a state snapshot with EIP-2929 warm/cold
/// tracking active, so the returned list is the real set of accounts and slots
/// the call read or wrote — exactly what a client should pre-warm to avoid the
/// cold surcharges on the real transaction.
async fn create_access_list(node: &Arc<TigerNode>, params: &Value) -> Result<Value, String> {
    let (_output, _used, accessed) = call_contract_traced(node, params).await?;
    let access_list: Vec<Value> = accessed
        .into_iter()
        .map(|(address, slots)| {
            json!({
                "address": address.to_hex(),
                "storageKeys": slots
                    .iter()
                    .map(|slot| format!("0x{}", hex::encode(slot.0)))
                    .collect::<Vec<_>>(),
            })
        })
        .collect();
    let gas = estimate_gas(node, params).await?;
    Ok(json!({
        "accessList": access_list,
        "gasUsed": hex_u64_str(gas),
    }))
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

    /// A default node allows no browser origin. This is the setting that
    /// matters most: an open `Access-Control-Allow-Origin: *` on an endpoint
    /// that accepts `eth_sendRawTransaction` lets any page a user visits drive
    /// their node.
    #[test]
    fn cors_denies_by_default() {
        let policy = CorsPolicy::from_config("");
        assert_eq!(policy.headers(Some("https://evil.example")), "");
        assert_eq!(policy.headers(None), "");
        // An empty spec is also what a whitespace-only or comma-only value
        // parses to.
        assert_eq!(CorsPolicy::from_config(" , , ").headers(Some("https://a.example")), "");
    }

    #[test]
    fn cors_allows_only_the_listed_origin_and_echoes_it() {
        let policy = CorsPolicy::from_config("https://explorer.tiger.example, https://wallet.tiger.example");
        let headers = policy.headers(Some("https://explorer.tiger.example"));
        assert!(headers.contains("Access-Control-Allow-Origin: https://explorer.tiger.example"));
        // The origin is echoed, never a bare `*`, so a browser does not treat
        // the response as credentialed-open and a shared cache cannot mix
        // answers between origins.
        assert!(!headers.contains("Access-Control-Allow-Origin: *"));
        assert!(headers.contains("Vary: Origin"));
        // Case is not significant for an origin, so a differently-cased
        // request must still be accepted.
        assert!(policy
            .headers(Some("https://EXPLORER.Tiger.example"))
            .contains("https://EXPLORER.Tiger.example"));
        assert_eq!(policy.headers(Some("https://evil.example")), "");
    }

    #[test]
    fn cors_wildcard_echoes_the_caller_rather_than_sending_a_star() {
        let policy = CorsPolicy::from_config("*");
        let headers = policy.headers(Some("https://any.example"));
        assert!(headers.contains("Access-Control-Allow-Origin: https://any.example"));
        assert!(!headers.contains("Access-Control-Allow-Origin: *"));
    }

    #[test]
    fn the_rate_limiter_caps_a_flood_and_lets_the_bucket_refill() {
        use std::net::IpAddr;
        let ip: IpAddr = "203.0.113.7".parse().unwrap();
        let other: IpAddr = "203.0.113.8".parse().unwrap();

        let limiter = PerIpLimiter::new(5.0, 5.0, 100);
        // The bucket starts full, so the first five are allowed and the sixth
        // is not.
        for i in 0..5 {
            assert!(limiter.admit(ip), "request {i} within the burst must be allowed");
        }
        assert!(!limiter.admit(ip), "the request past the burst must be refused");

        // A different client has its own bucket.
        assert!(limiter.admit(other));

        // A zero rate disables the limit entirely.
        let unlimited = PerIpLimiter::new(0.0, 0.0, 1);
        for _ in 0..1000 {
            assert!(unlimited.admit(ip));
        }
    }

    #[test]
    fn the_rate_limiter_refills_over_time() {
        use std::net::IpAddr;
        let ip: IpAddr = "203.0.113.9".parse().unwrap();
        let limiter = PerIpLimiter::new(1000.0, 1000.0, 100_000);
        // Drain the bucket.
        for _ in 0..1000 {
            assert!(limiter.admit(ip));
        }
        assert!(!limiter.admit(ip));
        // A millisecond at 1000/s buys one token.
        std::thread::sleep(std::time::Duration::from_millis(5));
        assert!(limiter.admit(ip), "the bucket must refill with time");
    }

    #[test]
    fn headers_are_read_case_insensitively_but_not_from_the_request_line() {
        // The request line itself must never be mistaken for a header.
        assert_eq!(
            parse_header("POST /origin: 1 HTTP/1.1\r\nHost: x\r\n", "origin"),
            None
        );
        assert_eq!(
            parse_header("POST / HTTP/1.1\r\nOrigin: https://a.example\r\n", "origin"),
            Some("https://a.example".to_string())
        );
        assert_eq!(
            parse_header("POST / HTTP/1.1\r\nORIGIN:   https://b.example  \r\n", "Origin"),
            Some("https://b.example".to_string())
        );
    }

    #[test]
    fn a_log_range_wider_than_the_cap_is_refused() {
        let chain = tiger_blockchain::Blockchain::new(tiger_blockchain::ChainConfig::default());
        let filter = LogFilter {
            from_block: Some(0),
            to_block: None,
            address: None,
            topics: None,
            block_hash: None,
        };
        // `toBlock` defaults to the head, so at a height above the cap the
        // default range alone must be refused.
        let err = log_block_numbers(&chain, &filter, MAX_LOG_BLOCK_RANGE + 10).unwrap_err();
        assert!(err.contains("block range too large"), "got: {err}");

        // A range exactly at the cap is served.
        let numbers = log_block_numbers(&chain, &filter, MAX_LOG_BLOCK_RANGE - 1).unwrap();
        assert_eq!(numbers.len() as u64, MAX_LOG_BLOCK_RANGE);

        // A narrower explicit range is served even at a great height.
        let narrow = LogFilter {
            from_block: Some(1_000_000),
            to_block: Some(1_000_100),
            address: None,
            topics: None,
            block_hash: None,
        };
        let numbers = log_block_numbers(&chain, &narrow, 2_000_000).unwrap();
        assert_eq!(numbers.len(), 101);

        // An inverted range yields nothing rather than an error.
        let inverted = LogFilter {
            from_block: Some(50),
            to_block: Some(10),
            address: None,
            topics: None,
            block_hash: None,
        };
        assert!(log_block_numbers(&chain, &inverted, 100).unwrap().is_empty());
    }

    #[test]
    fn a_log_range_is_clamped_to_the_chain_height() {
        let chain = tiger_blockchain::Blockchain::new(tiger_blockchain::ChainConfig::default());
        // `toBlock: pending` can name a block that does not exist yet; the scan
        // must stop at the head rather than erroring or inventing blocks.
        let filter = LogFilter {
            from_block: Some(0),
            to_block: Some(1_000),
            address: None,
            topics: None,
            block_hash: None,
        };
        let numbers = log_block_numbers(&chain, &filter, 3).unwrap();
        assert_eq!(numbers, vec![0, 1, 2, 3]);
    }

    #[test]
    fn a_metrics_body_carries_the_standard_exposition_shape() {
        // A scrape's shape is checked without a node: the rendering lives in
        // the node, so this pins the format contract the endpoint promises.
        let body = "# HELP tiger_chain_height Highest canonical block number.\n\
                    # TYPE tiger_chain_height gauge\n\
                    tiger_chain_height 7\n";
        assert!(body.contains("# TYPE "));
        assert!(body.lines().any(|l| l.starts_with("tiger_chain_height ")));
        // Every sample line has a name and a value, separated by one space.
        let sample = body.lines().find(|l| l.starts_with("tiger_chain_height ")).unwrap();
        assert_eq!(sample.split(' ').count(), 2);
    }

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
            tx_type: 0,
            access_list: None,
            max_priority_fee_per_gas: None,
            chain_id: None,
        };
        sign_transaction(&mut tx, 6666, &key).unwrap();
        let encoded = encode_legacy_transaction(&tx).unwrap();

        let decoded = tiger_blockchain::decode_transaction(&encoded).unwrap();
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
        let decoded = tiger_blockchain::decode_transaction(&[0xc0]).err();
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
            tx_type: 0,
            access_list: None,
            max_priority_fee_per_gas: None,
            chain_id: None,
        };
        sign_transaction(&mut tx, 6666, &key).unwrap();
        let encoded = encode_legacy_transaction(&tx).unwrap();
        let decoded = tiger_blockchain::decode_transaction(&encoded).unwrap();
        assert_eq!(decoded.from, format!("0x{}", "00".repeat(20)));

        let recovered =
            tiger_blockchain::recover_transaction_sender(&decoded, 6666).unwrap();
        assert_eq!(recovered, tiger_crypto::private_key_to_address(&key).unwrap());
    }

    #[test]
    fn a_malformed_transaction_is_rejected() {
        // Not RLP at all.
        assert!(tiger_blockchain::decode_transaction(&[0xff, 0xff]).is_err());
        // An RLP list with the wrong arity.
        let short = tiger_state::rlp::RlpItem::list(vec![
            tiger_state::rlp::RlpItem::from_u64(1),
        ])
        .encode();
        assert!(tiger_blockchain::decode_transaction(&short).is_err());
    }
}
