//! The node's WebSocket JSON-RPC endpoint.
//!
//! This is where `eth_subscribe` lives. A dApp opens one socket, subscribes to
//! `newHeads`, `logs`, `newPendingTransactions` or `syncing`, and the node
//! pushes each event as it happens; polling `eth_getFilterChanges` is the
//! fallback for clients that cannot hold a socket.
//!
//! The node publishes into one broadcast channel and each connection filters
//! what it wants, so a slow client's backlog is its own and cannot stall block
//! import or another client. Everything the socket reports is read from the
//! chain and the pool as they are at that moment; nothing is cached or replayed
//! from a fixture.
//!
//! The socket is reachable by anyone who can open it, so it is bound by the
//! same per-address rate and concurrency limits as the HTTP RPC, and the number
//! of subscriptions one connection may hold is capped.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use futures::{SinkExt, StreamExt};
use serde_json::{json, Value};
use tokio::net::TcpListener;
use tokio::sync::broadcast;
use tokio_tungstenite::tungstenite::Message;

use crate::limits::PerIpLimiter;
use crate::node::TigerNode;
use crate::rpc_server::{dispatch, RpcRequest, RpcResponse};

/// How many events are buffered for a connection before the oldest are dropped.
///
/// A client that stops reading must not be able to stall the node, so the
/// channel drops rather than blocks. A client that falls further behind is told
/// so it can resynchronise instead of silently missing heads.
const EVENT_BUFFER: usize = 512;

/// The most subscriptions one connection may hold at once.
const MAX_SUBSCRIPTIONS_PER_CONNECTION: usize = 64;

/// One event the node publishes to subscribers.
#[derive(Debug, Clone)]
pub enum ChainEvent {
    /// A new canonical head, already rendered as the block object `newHeads`
    /// reports.
    Head(Value),
    /// Logs from a block, each with its block and transaction context.
    Logs(Vec<Value>),
    /// A transaction accepted into the pool, by hash.
    PendingTransaction(String),
    /// The sync state changed.
    Syncing(Value),
}

/// The node's event bus.
///
/// Publishing is non-blocking: a connection that has fallen behind loses the
/// oldest events rather than delaying the publisher, which is what keeps block
/// import independent of client speed.
pub struct EventBus {
    sender: broadcast::Sender<ChainEvent>,
    /// How many events were dropped because every receiver was behind.
    dropped: AtomicU64,
}

impl Default for EventBus {
    fn default() -> Self {
        Self::new()
    }
}

impl EventBus {
    pub fn new() -> Self {
        let (sender, _) = broadcast::channel(EVENT_BUFFER);
        Self {
            sender,
            dropped: AtomicU64::new(0),
        }
    }

    /// Subscribe to the event stream.
    pub fn subscribe(&self) -> broadcast::Receiver<ChainEvent> {
        self.sender.subscribe()
    }

    /// Publish an event to every subscriber.
    pub fn publish(&self, event: ChainEvent) {
        // A send error only means there are no subscribers right now, which is
        // the normal state of a node with no WebSocket clients.
        if self.sender.send(event).is_err() {
            self.dropped.fetch_add(1, Ordering::Relaxed);
        }
    }

    /// How many events could not be delivered. Exposed for metrics.
    pub fn dropped(&self) -> u64 {
        self.dropped.load(Ordering::Relaxed)
    }
}

/// One subscription on one connection.
struct Subscription {
    /// `newHeads`, `logs`, `newPendingTransactions` or `syncing`.
    kind: String,
    /// For `logs`, the filter the subscription was created with.
    filter: Option<crate::filters::LogFilter>,
}

/// Serve the WebSocket endpoint on `addr` until the node stops.
pub async fn serve(node: Arc<TigerNode>, addr: String) -> std::io::Result<()> {
    let listener = TcpListener::bind(&addr).await?;
    log::info!("WebSocket JSON-RPC listening on {addr}");
    serve_listener(listener, node).await
}

/// Serve the WebSocket endpoint on an already-bound listener.
pub async fn serve_listener(listener: TcpListener, node: Arc<TigerNode>) -> std::io::Result<()> {
    // The same shape as the HTTP RPC's limit: one second's worth of the
    // configured rate as the burst, and that many concurrent connections per
    // address. A socket is long-lived, so the held count is what matters most.
    let rate = f64::from(node.config().rpc_rate_limit);
    let limiter = Arc::new(PerIpLimiter::new(rate, rate, rate.max(1.0) as usize));

    loop {
        let (stream, peer) = match listener.accept().await {
            Ok(connection) => connection,
            Err(e) => {
                log::warn!("WebSocket accept failed: {e}");
                continue;
            }
        };
        if !limiter.admit(peer.ip()) {
            log::debug!("refusing WebSocket from {peer}: rate or connection limit");
            drop(stream);
            continue;
        }
        let node = Arc::clone(&node);
        let limiter = Arc::clone(&limiter);
        tokio::spawn(async move {
            let result = handle_connection(stream, Arc::clone(&node), peer).await;
            limiter.release(peer.ip());
            if let Err(e) = result {
                log::debug!("WebSocket connection from {peer} ended: {e}");
            }
        });
    }
}

/// Drive one WebSocket connection: handshake, then requests and events.
async fn handle_connection(
    stream: tokio::net::TcpStream,
    node: Arc<TigerNode>,
    peer: std::net::SocketAddr,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let ws = tokio_tungstenite::accept_async(stream).await?;
    let (mut sink, mut source) = ws.split();
    let mut events = node.events().subscribe();
    let mut subscriptions: HashMap<String, Subscription> = HashMap::new();
    let mut next_id: u64 = 1;

    loop {
        tokio::select! {
            incoming = source.next() => {
                let message = match incoming {
                    Some(Ok(message)) => message,
                    Some(Err(e)) => {
                        log::debug!("WebSocket from {peer} closed: {e}");
                        break;
                    }
                    None => break,
                };
                let text = match message {
                    Message::Text(text) => text,
                    Message::Binary(bytes) => match String::from_utf8(bytes) {
                        Ok(text) => text,
                        Err(_) => {
                            let response = RpcResponse::error(
                                -32700,
                                "binary frames must be UTF-8 JSON",
                                Value::Null,
                            );
                            send_json(&mut sink, &serde_json::to_value(response)?).await?;
                            continue;
                        }
                    },
                    Message::Ping(payload) => {
                        sink.send(Message::Pong(payload)).await?;
                        continue;
                    }
                    Message::Close(_) => break,
                    // Pong and raw frames carry nothing to act on.
                    _ => continue,
                };

                let response = handle_text(&node, &text, &mut subscriptions, &mut next_id).await;
                if !response.is_null() {
                    send_json(&mut sink, &response).await?;
                }
            }
            event = events.recv() => {
                match event {
                    Ok(event) => {
                        let mut closed = false;
                        for notification in notifications_for(&event, &subscriptions) {
                            // A closed sink means the client went away; the
                            // next read returns `None` and the loop ends.
                            if send_json(&mut sink, &notification).await.is_err() {
                                closed = true;
                                break;
                            }
                        }
                        if closed {
                            break;
                        }
                    }
                    Err(broadcast::error::RecvError::Lagged(missed)) => {
                        // The client fell behind. Tell it rather than letting it
                        // silently miss heads, so it can resynchronise.
                        let notice = json!({
                            "jsonrpc": "2.0",
                            "method": "tiger_subscriptionLagged",
                            "params": { "missed": missed },
                        });
                        if send_json(&mut sink, &notice).await.is_err() {
                            break;
                        }
                    }
                    Err(broadcast::error::RecvError::Closed) => break,
                }
            }
        }
    }

    Ok(())
}

/// Handle one text frame: either a JSON-RPC request or a subscription control
/// message.
async fn handle_text(
    node: &Arc<TigerNode>,
    text: &str,
    subscriptions: &mut HashMap<String, Subscription>,
    next_id: &mut u64,
) -> Value {
    // A batch is an array of requests; the response is an array in the same
    // order, with notifications (no id) omitted as the specification requires.
    let parsed: Value = match serde_json::from_str(text) {
        Ok(value) => value,
        Err(e) => {
            return error_value(-32700, format!("parse error: {e}"), Value::Null);
        }
    };

    if let Value::Array(requests) = parsed {
        if requests.is_empty() {
            return error_value(-32600, "an empty batch is not a valid request".to_string(), Value::Null);
        }
        let mut responses = Vec::with_capacity(requests.len());
        for request in requests {
            if request.get("id").is_none() {
                // A notification gets no response.
                let _ = handle_request(node, request, subscriptions, next_id).await;
                continue;
            }
            responses.push(handle_request(node, request, subscriptions, next_id).await);
        }
        if responses.is_empty() {
            return Value::Null;
        }
        return Value::Array(responses);
    }

    handle_request(node, parsed, subscriptions, next_id).await
}

/// Route one request, intercepting the subscription methods that only exist on
/// a WebSocket, and delegating everything else to the shared RPC dispatcher.
async fn handle_request(
    node: &Arc<TigerNode>,
    request: Value,
    subscriptions: &mut HashMap<String, Subscription>,
    next_id: &mut u64,
) -> Value {
    let id = request.get("id").cloned().unwrap_or(Value::Null);
    let method = request
        .get("method")
        .and_then(|m| m.as_str())
        .unwrap_or("")
        .to_string();
    let params = request.get("params").cloned().unwrap_or(Value::Null);

    match method.as_str() {
        "eth_subscribe" => {
            if subscriptions.len() >= MAX_SUBSCRIPTIONS_PER_CONNECTION {
                return error_value(
                    -32603,
                    format!(
                        "too many subscriptions on this connection; at most \
                         {MAX_SUBSCRIPTIONS_PER_CONNECTION} are allowed"
                    ),
                    id,
                );
            }
            match create_subscription(node, &params, subscriptions, next_id).await {
                Ok(sub_id) => success_value(Value::String(sub_id), id),
                Err(e) => error_value(-32602, e, id),
            }
        }
        "eth_unsubscribe" => {
            let sub_id = params.get(0).and_then(|v| v.as_str()).unwrap_or("");
            let removed = subscriptions.remove(sub_id).is_some();
            success_value(Value::Bool(removed), id)
        }
        _ => {
            // Everything else is the same code path the HTTP RPC uses, so the
            // two endpoints cannot answer differently.
            let rpc_request = RpcRequest {
                jsonrpc: request
                    .get("jsonrpc")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string()),
                method,
                params,
                id: request.get("id").cloned(),
            };
            match serde_json::to_value(dispatch(node, rpc_request).await) {
                Ok(value) => value,
                Err(e) => error_value(-32603, format!("cannot serialise the response: {e}"), id),
            }
        }
    }
}

/// Validate and record a subscription.
async fn create_subscription(
    node: &Arc<TigerNode>,
    params: &Value,
    subscriptions: &mut HashMap<String, Subscription>,
    next_id: &mut u64,
) -> Result<String, String> {
    let kind = params
        .get(0)
        .and_then(|v| v.as_str())
        .ok_or_else(|| "missing subscription type".to_string())?;

    let filter = match kind {
        "newHeads" | "newPendingTransactions" | "syncing" => None,
        "logs" => {
            let filter_json = params.get(1).cloned().unwrap_or_else(|| json!({}));
            let height = {
                let cm = node.get_chain_manager().await;
                cm.chain().height()
            };
            Some(crate::rpc_server::parse_log_filter(
                &filter_json,
                height,
                node.finalized_height().await,
            )?)
        }
        other => {
            return Err(format!(
                "unknown subscription type {other:?}; expected newHeads, logs, \
                 newPendingTransactions or syncing"
            ))
        }
    };

    // Subscription ids are unique per connection; the counter is the
    // connection's own, so two clients never collide.
    let id = format!("0x{:x}", *next_id);
    *next_id += 1;
    subscriptions.insert(
        id.clone(),
        Subscription {
            kind: kind.to_string(),
            filter,
        },
    );
    Ok(id)
}

/// The notifications an event produces, one per subscription that wants it.
///
/// A client may hold several subscriptions on one connection, so one event can
/// produce more than one notification. Each is a separate JSON-RPC message,
/// because a notification is a single object rather than an array.
fn notifications_for(
    event: &ChainEvent,
    subscriptions: &HashMap<String, Subscription>,
) -> Vec<Value> {
    let mut out = Vec::new();

    for (id, subscription) in subscriptions {
        let result = match (event, subscription.kind.as_str()) {
            (ChainEvent::Head(head), "newHeads") => Some(head.clone()),
            (ChainEvent::Logs(logs), "logs") => {
                let filter = subscription.filter.as_ref();
                let matched: Vec<Value> = logs
                    .iter()
                    .filter(|log| log_matches(log, filter))
                    .cloned()
                    .collect();
                if matched.is_empty() {
                    None
                } else {
                    Some(Value::Array(matched))
                }
            }
            (ChainEvent::PendingTransaction(hash), "newPendingTransactions") => {
                Some(Value::String(hash.clone()))
            }
            (ChainEvent::Syncing(status), "syncing") => Some(status.clone()),
            _ => None,
        };

        if let Some(result) = result {
            out.push(json!({
                "jsonrpc": "2.0",
                "method": "eth_subscription",
                "params": { "subscription": id, "result": result },
            }));
        }
    }

    out
}

/// Whether a rendered log satisfies a subscription filter.
///
/// The log arrives already rendered, so the comparison is on the rendered
/// address and topics, which is exactly what the client will match on.
fn log_matches(log: &Value, filter: Option<&crate::filters::LogFilter>) -> bool {
    let Some(filter) = filter else {
        return true;
    };

    if let Some(addresses) = &filter.address {
        let matches = log
            .get("address")
            .and_then(|v| v.as_str())
            .map(|address| {
                let address = address.trim_start_matches("0x").to_ascii_lowercase();
                addresses
                    .iter()
                    .any(|a| tiger_blockchain::hex_encode(a).to_ascii_lowercase() == address)
            })
            .unwrap_or(false);
        if !matches {
            return false;
        }
    }

    if let Some(topic_filter) = &filter.topics {
        let log_topics: Vec<[u8; 32]> = log
            .get("topics")
            .and_then(|v| v.as_array())
            .map(|topics| {
                topics
                    .iter()
                    .filter_map(|t| t.as_str())
                    .filter_map(|t| {
                        let raw = t.trim_start_matches("0x");
                        let bytes = decode_hex(raw)?;
                        if bytes.len() == 32 {
                            let mut out = [0u8; 32];
                            out.copy_from_slice(&bytes);
                            Some(out)
                        } else {
                            None
                        }
                    })
                    .collect()
            })
            .unwrap_or_default();
        if !crate::rpc_server::topics_match(topic_filter, &log_topics) {
            return false;
        }
    }

    true
}

async fn send_json<S>(
    sink: &mut S,
    value: &Value,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>>
where
    S: SinkExt<Message> + Unpin,
    S::Error: std::error::Error + Send + Sync + 'static,
{
    let text = serde_json::to_string(value)?;
    sink.send(Message::Text(text)).await?;
    Ok(())
}

/// Decode a hex string, tolerating an odd length by rejecting it.
fn decode_hex(raw: &str) -> Option<Vec<u8>> {
    if raw.len() % 2 != 0 {
        return None;
    }
    let bytes = raw.as_bytes();
    let mut out = Vec::with_capacity(raw.len() / 2);
    for pair in bytes.chunks(2) {
        let hi = (pair[0] as char).to_digit(16)?;
        let lo = (pair[1] as char).to_digit(16)?;
        out.push(((hi << 4) | lo) as u8);
    }
    Some(out)
}

fn success_value(result: Value, id: Value) -> Value {
    json!({ "jsonrpc": "2.0", "id": id, "result": result })
}

fn error_value(code: i64, message: String, id: Value) -> Value {
    json!({
        "jsonrpc": "2.0",
        "id": id,
        "error": { "code": code, "message": message },
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Config;
    use crate::node::create_genesis_block;
    use futures::{SinkExt, StreamExt};
    use serde_json::json;
    use std::sync::Arc;
    use tokio_tungstenite::tungstenite::Message;

    /// A node with the built-in listeners off, so a test binds its own ports.
    async fn test_node() -> Arc<TigerNode> {
        let data_dir = std::env::temp_dir().join(format!(
            "tiger-ws-test-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        let _ = std::fs::remove_dir_all(&data_dir);
        let config = Config {
            chain_id: 6666,
            rpc_enabled: false,
            metrics_enabled: false,
            ws_enabled: false,
            data_dir: data_dir.to_string_lossy().into_owned(),
            ..Config::default_mainnet()
        };
        let node = Arc::new(
            TigerNode::from_genesis(config, create_genesis_block(6666))
                .await
                .expect("node must initialize"),
        );
        node.start().await.expect("node must start");
        node
    }

    /// Connect a client and return the split sink and source.
    async fn connect(
        addr: std::net::SocketAddr,
    ) -> (
        futures::stream::SplitSink<
            tokio_tungstenite::WebSocketStream<
                tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>,
            >,
            Message,
        >,
        futures::stream::SplitStream<
            tokio_tungstenite::WebSocketStream<
                tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>,
            >,
        >,
    ) {
        let url = format!("ws://{addr}/");
        let (ws, _) = tokio_tungstenite::connect_async(url)
            .await
            .expect("the WebSocket handshake must succeed");
        ws.split()
    }

    /// Send one text frame and read the next text frame back.
    async fn round_trip<S, R>(sink: &mut S, source: &mut R, request: Value) -> Value
    where
        S: SinkExt<Message> + Unpin,
        S::Error: std::fmt::Debug,
        R: StreamExt<Item = Result<Message, tokio_tungstenite::tungstenite::Error>> + Unpin,
    {
        sink.send(Message::Text(request.to_string()))
            .await
            .expect("send must succeed");
        loop {
            match source.next().await {
                Some(Ok(Message::Text(text))) => {
                    return serde_json::from_str(&text).expect("a JSON response");
                }
                Some(Ok(_)) => continue,
                other => panic!("expected a text frame, got {other:?}"),
            }
        }
    }

    #[tokio::test]
    async fn a_subscription_is_accepted_and_an_unknown_type_is_refused() {
        let node = test_node().await;
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let server = tokio::spawn(serve_listener(listener, Arc::clone(&node)));
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;

        let (mut sink, mut source) = connect(addr).await;

        let reply = round_trip(
            &mut sink,
            &mut source,
            json!({"jsonrpc":"2.0","id":1,"method":"eth_subscribe","params":["newHeads"]}),
        )
        .await;
        let sub_id = reply
            .get("result")
            .and_then(|v| v.as_str())
            .expect("a subscription id must be returned")
            .to_string();

        // A second subscription gets a different id.
        let reply = round_trip(
            &mut sink,
            &mut source,
            json!({"jsonrpc":"2.0","id":2,"method":"eth_subscribe","params":["logs",{}]}),
        )
        .await;
        let other = reply.get("result").and_then(|v| v.as_str()).unwrap();
        assert_ne!(sub_id, other, "each subscription must get its own id");

        // An unknown type is refused rather than silently accepted.
        let reply = round_trip(
            &mut sink,
            &mut source,
            json!({"jsonrpc":"2.0","id":3,"method":"eth_subscribe","params":["blockHeaders"]}),
        )
        .await;
        assert!(reply.get("error").is_some(), "got: {reply}");

        // Unsubscribing reports true once and false thereafter.
        let reply = round_trip(
            &mut sink,
            &mut source,
            json!({"jsonrpc":"2.0","id":4,"method":"eth_unsubscribe","params":[sub_id]}),
        )
        .await;
        assert_eq!(reply.get("result"), Some(&Value::Bool(true)));
        let reply = round_trip(
            &mut sink,
            &mut source,
            json!({"jsonrpc":"2.0","id":5,"method":"eth_unsubscribe","params":[sub_id]}),
        )
        .await;
        assert_eq!(reply.get("result"), Some(&Value::Bool(false)));

        server.abort();
    }

    #[tokio::test]
    async fn a_non_subscription_method_is_served_by_the_same_dispatcher() {
        let node = test_node().await;
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let server = tokio::spawn(serve_listener(listener, Arc::clone(&node)));
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;

        let (mut sink, mut source) = connect(addr).await;
        let reply = round_trip(
            &mut sink,
            &mut source,
            json!({"jsonrpc":"2.0","id":1,"method":"eth_chainId"}),
        )
        .await;
        // The WebSocket must answer exactly as the HTTP RPC does.
        assert_eq!(reply.get("result"), Some(&json!("0x1a0a")), "got: {reply}");

        // A batch is answered with an array in the same order.
        let reply = round_trip(
            &mut sink,
            &mut source,
            json!([
                {"jsonrpc":"2.0","id":1,"method":"eth_chainId"},
                {"jsonrpc":"2.0","id":2,"method":"eth_blockNumber"},
            ]),
        )
        .await;
        let array = reply.as_array().expect("a batch must answer with an array");
        assert_eq!(array.len(), 2);

        server.abort();
    }

    #[tokio::test]
    async fn a_new_head_is_pushed_to_a_subscriber() {
        let node = test_node().await;
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let server = tokio::spawn(serve_listener(listener, Arc::clone(&node)));
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;

        let (mut sink, mut source) = connect(addr).await;
        let reply = round_trip(
            &mut sink,
            &mut source,
            json!({"jsonrpc":"2.0","id":1,"method":"eth_subscribe","params":["newHeads"]}),
        )
        .await;
        let sub_id = reply.get("result").and_then(|v| v.as_str()).unwrap().to_string();

        // Publish a head. The node renders the block it is given, so the
        // notification must carry that block's number.
        let block = node
            .get_chain_manager()
            .await
            .chain()
            .get_block(0)
            .cloned()
            .expect("genesis must be present");
        node.publish_block_events(&block).await;

        let notification = loop {
            match source.next().await {
                Some(Ok(Message::Text(text))) => {
                    let value: Value = serde_json::from_str(&text).unwrap();
                    if value.get("method") == Some(&json!("eth_subscription")) {
                        break value;
                    }
                }
                Some(Ok(_)) => continue,
                other => panic!("expected a notification, got {other:?}"),
            }
        };
        assert_eq!(
            notification.pointer("/params/subscription"),
            Some(&json!(sub_id))
        );
        assert_eq!(
            notification.pointer("/params/result/number"),
            Some(&json!("0x0")),
            "the pushed head must be the block that was published: {notification}"
        );

        server.abort();
    }

    #[tokio::test]
    async fn a_pending_transaction_is_pushed_to_a_subscriber() {
        let node = test_node().await;
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let server = tokio::spawn(serve_listener(listener, Arc::clone(&node)));
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;

        let (mut sink, mut source) = connect(addr).await;
        round_trip(
            &mut sink,
            &mut source,
            json!({"jsonrpc":"2.0","id":1,"method":"eth_subscribe","params":["newPendingTransactions"]}),
        )
        .await;

        let hash = format!("0x{}", "ab".repeat(32));
        node.publish_pending_transaction(&hash);

        let notification = loop {
            match source.next().await {
                Some(Ok(Message::Text(text))) => {
                    let value: Value = serde_json::from_str(&text).unwrap();
                    if value.get("method") == Some(&json!("eth_subscription")) {
                        break value;
                    }
                }
                Some(Ok(_)) => continue,
                other => panic!("expected a notification, got {other:?}"),
            }
        };
        assert_eq!(
            notification.pointer("/params/result"),
            Some(&json!(hash)),
            "the pushed transaction must be the one published: {notification}"
        );

        server.abort();
    }

    #[tokio::test]
    async fn a_subscription_the_event_does_not_match_gets_nothing() {
        let node = test_node().await;
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let server = tokio::spawn(serve_listener(listener, Arc::clone(&node)));
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;

        let (mut sink, mut source) = connect(addr).await;
        // A `logs` subscription, then a head event, which it does not want.
        round_trip(
            &mut sink,
            &mut source,
            json!({"jsonrpc":"2.0","id":1,"method":"eth_subscribe","params":["logs",{}]}),
        )
        .await;

        let block = node
            .get_chain_manager()
            .await
            .chain()
            .get_block(0)
            .cloned()
            .expect("genesis must be present");
        node.publish_block_events(&block).await;

        // The chain id request that follows must be answered without an
        // interleaved log notification, which is only true if the head event
        // produced nothing for the log subscription.
        let reply = round_trip(
            &mut sink,
            &mut source,
            json!({"jsonrpc":"2.0","id":2,"method":"eth_chainId"}),
        )
        .await;
        assert_eq!(reply.get("id"), Some(&json!(2)), "got: {reply}");
        assert_eq!(reply.get("result"), Some(&json!("0x1a0a")), "got: {reply}");

        server.abort();
    }

    #[tokio::test]
    async fn a_log_subscription_matches_only_its_address() {
        let node = test_node().await;
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let server = tokio::spawn(serve_listener(listener, Arc::clone(&node)));
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;

        let (mut sink, mut source) = connect(addr).await;
        let wanted = format!("0x{}", "11".repeat(20));
        round_trip(
            &mut sink,
            &mut source,
            json!({"jsonrpc":"2.0","id":1,"method":"eth_subscribe","params":["logs",{"address": wanted}]}),
        )
        .await;

        // A log from a different address must not be delivered.
        let other_log = json!({
            "address": format!("0x{}", "22".repeat(20)),
            "topics": [],
            "data": "0x",
            "blockNumber": "0x1",
            "blockHash": format!("0x{}", "33".repeat(32)),
            "transactionHash": format!("0x{}", "44".repeat(32)),
            "transactionIndex": "0x0",
            "logIndex": "0x0",
            "removed": false,
        });
        node.events().publish(ChainEvent::Logs(vec![other_log]));

        let reply = round_trip(
            &mut sink,
            &mut source,
            json!({"jsonrpc":"2.0","id":2,"method":"eth_chainId"}),
        )
        .await;
        assert_eq!(reply.get("id"), Some(&json!(2)), "got: {reply}");

        // A log from the wanted address is delivered.
        let wanted_log = json!({
            "address": wanted,
            "topics": [],
            "data": "0x",
            "blockNumber": "0x1",
            "blockHash": format!("0x{}", "33".repeat(32)),
            "transactionHash": format!("0x{}", "44".repeat(32)),
            "transactionIndex": "0x0",
            "logIndex": "0x0",
            "removed": false,
        });
        node.events().publish(ChainEvent::Logs(vec![wanted_log]));

        let notification = loop {
            match source.next().await {
                Some(Ok(Message::Text(text))) => {
                    let value: Value = serde_json::from_str(&text).unwrap();
                    if value.get("method") == Some(&json!("eth_subscription")) {
                        break value;
                    }
                }
                Some(Ok(_)) => continue,
                other => panic!("expected a notification, got {other:?}"),
            }
        };
        assert_eq!(
            notification.pointer("/params/result/0/address"),
            Some(&json!(wanted)),
            "only the matching log must be delivered: {notification}"
        );

        server.abort();
    }

    #[tokio::test]
    async fn the_subscription_count_per_connection_is_capped() {
        let node = test_node().await;
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let server = tokio::spawn(serve_listener(listener, Arc::clone(&node)));
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;

        let (mut sink, mut source) = connect(addr).await;
        for i in 0..MAX_SUBSCRIPTIONS_PER_CONNECTION {
            let reply = round_trip(
                &mut sink,
                &mut source,
                json!({"jsonrpc":"2.0","id":i,"method":"eth_subscribe","params":["newHeads"]}),
            )
            .await;
            assert!(reply.get("result").is_some(), "subscription {i} must be accepted: {reply}");
        }
        // The one past the cap is refused rather than accepted.
        let reply = round_trip(
            &mut sink,
            &mut source,
            json!({"jsonrpc":"2.0","id":999,"method":"eth_subscribe","params":["newHeads"]}),
        )
        .await;
        assert!(reply.get("error").is_some(), "got: {reply}");

        server.abort();
    }
}
