//! WebSocket pub/sub for TigerScan.
//!
//! A working server and client over `tokio-tungstenite`:
//!
//! * [`WSServer::serve`] accepts connections on a TCP listener and speaks the
//!   standard Ethereum pub/sub dialect (`eth_subscribe` / `eth_unsubscribe`).
//! * [`WSClientConnection`] connects, subscribes and receives real frames.
//!
//! Subscriptions are keyed by id, so a client can hold several at once and
//! unsubscribe from one without disturbing the others. Broadcasts are only sent
//! to clients that subscribed to the matching channel.

use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use futures::{SinkExt, StreamExt};
use serde_json::{json, Value};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::{mpsc, Mutex, RwLock};
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::{accept_async, connect_async};

pub mod types;

pub use types::{Channel, MessageType, Subscription, WSClient, WSConfig, WSEvent, WSMessage};

// =============================================================================
// ERRORS
// =============================================================================

#[derive(Debug, thiserror::Error)]
pub enum WSError {
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),

    #[error("websocket error: {0}")]
    WebSocket(#[from] tokio_tungstenite::tungstenite::Error),

    #[error("json error: {0}")]
    Json(#[from] serde_json::Error),

    #[error("not connected")]
    NotConnected,

    #[error("connection limit reached")]
    TooManyConnections,

    #[error("invalid subscription: {0}")]
    InvalidSubscription(String),

    #[error("invalid listen address: {0}")]
    InvalidAddress(String),
}

// =============================================================================
// SERVER
// =============================================================================

/// One connected client: its metadata, its subscriptions, and its outbound queue.
struct ConnectedClient {
    info: WSClient,
    /// Subscription id -> channel.
    subscriptions: HashMap<String, Channel>,
    outbound: mpsc::Sender<Message>,
}

/// A subscription id -> client id index.
type SubscriptionIndex = HashMap<String, String>;

#[derive(Default)]
struct ServerState {
    clients: HashMap<String, ConnectedClient>,
    index: SubscriptionIndex,
}

/// WebSocket pub/sub server.
pub struct WSServer {
    config: WSConfig,
    state: Arc<RwLock<ServerState>>,
    next_client_id: AtomicU64,
    next_subscription_id: AtomicU64,
    running: Arc<RwLock<bool>>,
}

impl WSServer {
    pub fn new(config: WSConfig) -> Self {
        Self {
            config,
            state: Arc::new(RwLock::new(ServerState::default())),
            next_client_id: AtomicU64::new(1),
            next_subscription_id: AtomicU64::new(1),
            running: Arc::new(RwLock::new(false)),
        }
    }

    /// Bind and serve until the listener stops accepting.
    ///
    /// Each accepted connection gets its own task. A client whose outbound
    /// queue is full has frames dropped rather than being allowed to block
    /// every other subscriber.
    pub async fn serve(&self) -> Result<(), WSError> {
        let addr: SocketAddr = format!("{}:{}", self.config.host, self.config.port)
            .parse()
            .map_err(|e| WSError::InvalidAddress(format!("{e}")))?;
        let listener = TcpListener::bind(addr).await?;
        *self.running.write().await = true;

        log::info!("WebSocket server listening on {addr}");

        while let Ok((stream, peer)) = listener.accept().await {
            {
                let state = self.state.read().await;
                if state.clients.len() >= self.config.max_connections {
                    log::warn!("rejecting {peer}: connection limit reached");
                    continue;
                }
            }

            let state = Arc::clone(&self.state);
            let config = self.config.clone();
            let client_id = format!("c{}", self.next_client_id.fetch_add(1, Ordering::Relaxed));
            let next_sub = Arc::new(AtomicU64::new(
                self.next_subscription_id
                    .fetch_add(1_000_000, Ordering::Relaxed),
            ));

            tokio::spawn(async move {
                let result = Self::serve_client(
                    stream,
                    peer,
                    client_id.clone(),
                    state.clone(),
                    config,
                    next_sub,
                )
                .await;

                if let Err(e) = result {
                    log::debug!("client {client_id} ended: {e}");
                }
                Self::remove_client(&state, &client_id).await;
            });
        }

        *self.running.write().await = false;
        Ok(())
    }

    async fn serve_client(
        stream: TcpStream,
        peer: SocketAddr,
        client_id: String,
        state: Arc<RwLock<ServerState>>,
        config: WSConfig,
        next_sub: Arc<AtomicU64>,
    ) -> Result<(), WSError> {
        let ws = accept_async(stream).await?;
        let (mut sink, mut source) = ws.split();

        let (tx, mut rx) = mpsc::channel::<Message>(config.message_limit);

        state.write().await.clients.insert(
            client_id.clone(),
            ConnectedClient {
                info: WSClient {
                    id: client_id.clone(),
                    subscriptions: Vec::new(),
                    connected_at: now_secs(),
                },
                subscriptions: HashMap::new(),
                outbound: tx,
            },
        );

        log::debug!("client {client_id} connected from {peer}");

        // Pump queued outbound frames to the socket.
        let writer = tokio::spawn(async move {
            while let Some(message) = rx.recv().await {
                if sink.send(message).await.is_err() {
                    break;
                }
            }
        });

        while let Some(frame) = source.next().await {
            match frame? {
                Message::Text(text) => {
                    if let Some(reply) =
                        Self::handle_request(&state, &client_id, &text, &next_sub).await
                    {
                        let payload = serde_json::to_string(&reply)?;
                        Self::send_to(&state, &client_id, Message::Text(payload)).await;
                    }
                }
                Message::Ping(payload) => {
                    Self::send_to(&state, &client_id, Message::Pong(payload)).await;
                }
                Message::Close(_) => break,
                _ => {}
            }
        }

        writer.abort();
        Ok(())
    }

    /// Handle one client request, returning the reply to send.
    async fn handle_request(
        state: &Arc<RwLock<ServerState>>,
        client_id: &str,
        text: &str,
        next_sub: &AtomicU64,
    ) -> Option<Value> {
        let request: Value = match serde_json::from_str(text) {
            Ok(v) => v,
            Err(e) => {
                return Some(json!({
                    "jsonrpc": "2.0",
                    "id": Value::Null,
                    "error": { "code": -32700, "message": format!("parse error: {e}") }
                }))
            }
        };

        let id = request.get("id").cloned().unwrap_or(Value::Null);
        let method = request.get("method").and_then(|m| m.as_str()).unwrap_or("");

        match method {
            "eth_subscribe" => {
                let params = request
                    .get("params")
                    .and_then(|p| p.as_array())
                    .cloned()
                    .unwrap_or_default();

                let channel = match Self::parse_channel(&params) {
                    Ok(c) => c,
                    Err(e) => {
                        return Some(json!({
                            "jsonrpc": "2.0", "id": id,
                            "error": { "code": -32602, "message": e }
                        }))
                    }
                };

                let sub_id = format!("0x{:x}", next_sub.fetch_add(1, Ordering::Relaxed));
                let mut guard = state.write().await;
                if let Some(client) = guard.clients.get_mut(client_id) {
                    client.subscriptions.insert(sub_id.clone(), channel);
                    client.info.subscriptions.push(sub_id.clone());
                }
                guard.index.insert(sub_id.clone(), client_id.to_string());

                Some(json!({ "jsonrpc": "2.0", "id": id, "result": sub_id }))
            }
            "eth_unsubscribe" => {
                let sub_id = request
                    .get("params")
                    .and_then(|p| p.as_array())
                    .and_then(|a| a.first())
                    .and_then(|v| v.as_str())
                    .unwrap_or_default()
                    .to_string();

                let mut guard = state.write().await;
                let mut removed = false;
                if let Some(client) = guard.clients.get_mut(client_id) {
                    removed = client.subscriptions.remove(&sub_id).is_some();
                    client.info.subscriptions.retain(|s| s != &sub_id);
                }
                if removed {
                    guard.index.remove(&sub_id);
                }

                Some(json!({ "jsonrpc": "2.0", "id": id, "result": removed }))
            }
            other => Some(json!({
                "jsonrpc": "2.0",
                "id": id,
                "error": { "code": -32601, "message": format!("method not found: {other}") }
            })),
        }
    }

    /// Build a [`Channel`] from `eth_subscribe` params.
    fn parse_channel(params: &[Value]) -> Result<Channel, String> {
        let name = params
            .first()
            .and_then(|v| v.as_str())
            .ok_or_else(|| "eth_subscribe requires a subscription name".to_string())?;

        Ok(match name {
            "newHeads" | "new_heads" => Channel::NewBlocks,
            "newTransactions" | "new_transactions" => Channel::NewTransactions,
            "newPendingTransactions" | "new_pending_transactions" => Channel::PendingTransactions,
            "logs" => {
                let addresses = params
                    .get(1)
                    .and_then(|v| v.get("address"))
                    .map(extract_addresses)
                    .unwrap_or_default();
                Channel::Logs { addresses }
            }
            "addresses" => {
                Channel::Addresses(params.get(1).map(extract_addresses).unwrap_or_default())
            }
            "contracts" => {
                Channel::Contracts(params.get(1).map(extract_addresses).unwrap_or_default())
            }
            "tokens" => Channel::Tokens(params.get(1).map(extract_addresses).unwrap_or_default()),
            other => return Err(format!("unknown subscription: {other}")),
        })
    }

    async fn send_to(state: &Arc<RwLock<ServerState>>, client_id: &str, message: Message) {
        let guard = state.read().await;
        if let Some(client) = guard.clients.get(client_id) {
            if client.outbound.try_send(message).is_err() {
                log::debug!("dropping frame for {client_id}: outbound queue full");
            }
        }
    }

    async fn remove_client(state: &Arc<RwLock<ServerState>>, client_id: &str) {
        let mut guard = state.write().await;
        if let Some(client) = guard.clients.remove(client_id) {
            for sub_id in client.subscriptions.keys() {
                guard.index.remove(sub_id);
            }
            log::debug!("client {client_id} disconnected");
        }
    }

    /// Deliver `event` to every subscription matching `channel`.
    ///
    /// Each frame is a JSON-RPC notification carrying the subscription id the
    /// client was given, which is what clients key their handlers on. Returns
    /// the number of frames queued.
    pub async fn broadcast(&self, channel: &Channel, event: &WSEvent) -> usize {
        let mut delivered = 0;
        let guard = self.state.read().await;

        for (client_id, client) in &guard.clients {
            for (sub_id, subscribed) in &client.subscriptions {
                if !subscribed.matches(channel) {
                    continue;
                }

                let notification = json!({
                    "jsonrpc": "2.0",
                    "method": "eth_subscription",
                    "params": { "subscription": sub_id, "result": event },
                });

                let payload = match serde_json::to_string(&notification) {
                    Ok(p) => p,
                    Err(e) => {
                        log::error!("failed to serialise notification: {e}");
                        continue;
                    }
                };

                if client.outbound.try_send(Message::Text(payload)).is_ok() {
                    delivered += 1;
                    log::trace!("broadcast to {client_id} on {sub_id}");
                }
            }
        }

        delivered
    }

    /// Number of connected clients.
    pub async fn client_count(&self) -> usize {
        self.state.read().await.clients.len()
    }

    /// Subscription ids held by one client.
    pub async fn subscriptions_of(&self, client_id: &str) -> Vec<String> {
        self.state
            .read()
            .await
            .clients
            .get(client_id)
            .map(|c| c.subscriptions.keys().cloned().collect())
            .unwrap_or_default()
    }

    /// Address the server is configured to listen on.
    pub fn address(&self) -> String {
        format!("{}:{}", self.config.host, self.config.port)
    }

    pub async fn is_running(&self) -> bool {
        *self.running.read().await
    }

    /// Test hook: first connected client id, in insertion-independent order.
    #[cfg(test)]
    async fn first_client_id(&self) -> Option<String> {
        self.state.read().await.clients.keys().next().cloned()
    }
}

// =============================================================================
// CLIENT
// =============================================================================

type MaybeTls = tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>;
type WsStream = tokio_tungstenite::WebSocketStream<MaybeTls>;
type WsSink = futures::stream::SplitSink<WsStream, Message>;
type WsSource = futures::stream::SplitStream<WsStream>;

/// A connected WebSocket client.
pub struct WSClientConnection {
    url: String,
    sink: Mutex<Option<WsSink>>,
    source: Mutex<Option<WsSource>>,
    subscriptions: RwLock<Vec<String>>,
    next_request_id: AtomicU64,
}

impl WSClientConnection {
    pub fn new(url: &str) -> Self {
        Self {
            url: url.to_string(),
            sink: Mutex::new(None),
            source: Mutex::new(None),
            subscriptions: RwLock::new(Vec::new()),
            next_request_id: AtomicU64::new(1),
        }
    }

    /// Open the connection.
    pub async fn connect(&self) -> Result<(), WSError> {
        let (ws, _) = connect_async(&self.url).await?;
        let (sink, source) = ws.split();
        *self.sink.lock().await = Some(sink);
        *self.source.lock().await = Some(source);
        Ok(())
    }

    /// Close the connection.
    pub async fn disconnect(&self) {
        if let Some(mut sink) = self.sink.lock().await.take() {
            let _ = sink.close().await;
        }
        *self.source.lock().await = None;
        self.subscriptions.write().await.clear();
    }

    pub async fn is_connected(&self) -> bool {
        self.sink.lock().await.is_some()
    }

    /// Subscribe to `channel`, returning the subscription id.
    pub async fn subscribe(&self, channel: Channel) -> Result<String, WSError> {
        let params = channel.subscribe_params();
        let result = self.request("eth_subscribe", params).await?;
        let sub_id = result
            .as_str()
            .ok_or_else(|| WSError::InvalidSubscription("node returned no id".into()))?
            .to_string();

        self.subscriptions.write().await.push(sub_id.clone());
        Ok(sub_id)
    }

    /// Cancel a subscription.
    pub async fn unsubscribe(&self, subscription_id: &str) -> Result<bool, WSError> {
        let result = self
            .request("eth_unsubscribe", json!([subscription_id]))
            .await?;
        let removed = result.as_bool().unwrap_or(false);

        if removed {
            self.subscriptions
                .write()
                .await
                .retain(|s| s != subscription_id);
        }
        Ok(removed)
    }

    /// Send a request and wait for the matching response.
    ///
    /// Notifications (frames without a matching `id`) are skipped.
    pub async fn request(&self, method: &str, params: Value) -> Result<Value, WSError> {
        let id = self.next_request_id.fetch_add(1, Ordering::Relaxed);
        let frame = json!({ "jsonrpc": "2.0", "id": id, "method": method, "params": params });

        {
            let mut guard = self.sink.lock().await;
            let sink = guard.as_mut().ok_or(WSError::NotConnected)?;
            sink.send(Message::Text(serde_json::to_string(&frame)?))
                .await?;
        }

        loop {
            let frame = self.receive_raw().await?.ok_or(WSError::NotConnected)?;

            if frame.get("id").and_then(|v| v.as_u64()) != Some(id) {
                continue;
            }

            if let Some(error) = frame.get("error") {
                return Err(WSError::InvalidSubscription(error.to_string()));
            }

            return Ok(frame.get("result").cloned().unwrap_or(Value::Null));
        }
    }

    /// Next notification as a [`WSMessage`], or `None` once the stream ends.
    pub async fn receive(&self) -> Option<WSMessage> {
        let frame = self.receive_raw().await.ok()??;
        let channel = frame
            .get("params")
            .and_then(|p| p.get("subscription"))
            .and_then(|s| s.as_str())
            .unwrap_or("unknown")
            .to_string();
        let data = frame
            .get("params")
            .and_then(|p| p.get("result"))
            .cloned()
            .unwrap_or(Value::Null);

        Some(WSMessage {
            r#type: MessageType::Event,
            payload: data.to_string(),
            timestamp: now_secs(),
            channel: Some(channel),
        })
    }

    /// Receive the next frame, answering pings as they arrive.
    pub async fn receive_raw(&self) -> Result<Option<Value>, WSError> {
        loop {
            let next = {
                let mut guard = self.source.lock().await;
                let source = guard.as_mut().ok_or(WSError::NotConnected)?;
                source.next().await
            };

            let Some(frame) = next else {
                return Ok(None);
            };

            match frame? {
                Message::Text(text) => return Ok(Some(serde_json::from_str(&text)?)),
                Message::Ping(payload) => {
                    let mut guard = self.sink.lock().await;
                    if let Some(sink) = guard.as_mut() {
                        let _ = sink.send(Message::Pong(payload)).await;
                    }
                }
                Message::Close(_) => return Ok(None),
                _ => continue,
            }
        }
    }

    /// Subscription ids currently held.
    pub async fn subscriptions(&self) -> Vec<String> {
        self.subscriptions.read().await.clone()
    }

    pub fn url(&self) -> &str {
        &self.url
    }
}

// =============================================================================
// BUILDER
// =============================================================================

pub struct WSServerBuilder {
    config: WSConfig,
}

impl WSServerBuilder {
    pub fn new() -> Self {
        Self {
            config: WSConfig::default(),
        }
    }

    pub fn host(mut self, host: impl Into<String>) -> Self {
        self.config.host = host.into();
        self
    }

    pub fn port(mut self, port: u16) -> Self {
        self.config.port = port;
        self
    }

    pub fn max_connections(mut self, max: usize) -> Self {
        self.config.max_connections = max;
        self
    }

    pub fn build(self) -> WSServer {
        WSServer::new(self.config)
    }
}

impl Default for WSServerBuilder {
    fn default() -> Self {
        Self::new()
    }
}

// =============================================================================
// HELPERS
// =============================================================================

fn extract_addresses(value: &Value) -> Vec<String> {
    match value {
        Value::String(s) => vec![s.to_lowercase()],
        Value::Array(items) => items
            .iter()
            .filter_map(|v| v.as_str())
            .map(|s| s.to_lowercase())
            .collect(),
        _ => Vec::new(),
    }
}

fn now_secs() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Start a server on an ephemeral port and return it with its URL.
    async fn start_server() -> (Arc<WSServer>, String) {
        let port = {
            let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
            listener.local_addr().unwrap().port()
        };

        let server = Arc::new(WSServer::new(WSConfig {
            host: "127.0.0.1".into(),
            port,
            max_connections: 16,
            ping_interval: 30,
            message_limit: 64,
        }));

        let handle = Arc::clone(&server);
        tokio::spawn(async move {
            let _ = handle.serve().await;
        });

        let url = format!("ws://127.0.0.1:{port}");
        for _ in 0..200 {
            if TcpStream::connect(("127.0.0.1", port)).await.is_ok() {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }

        (server, url)
    }

    #[tokio::test]
    async fn client_subscribes_and_unsubscribes() {
        let (server, url) = start_server().await;

        let client = WSClientConnection::new(&url);
        client.connect().await.unwrap();
        assert!(client.is_connected().await);

        let sub_id = client.subscribe(Channel::NewBlocks).await.unwrap();
        assert!(sub_id.starts_with("0x"));
        assert_eq!(client.subscriptions().await, vec![sub_id.clone()]);

        let client_id = server.first_client_id().await.expect("client registered");
        assert_eq!(server.subscriptions_of(&client_id).await.len(), 1);

        assert!(client.unsubscribe(&sub_id).await.unwrap());
        assert!(client.subscriptions().await.is_empty());
        assert!(server.subscriptions_of(&client_id).await.is_empty());

        client.disconnect().await;
    }

    #[tokio::test]
    async fn broadcasts_reach_only_subscribed_clients() {
        let (server, url) = start_server().await;

        let blocks_client = WSClientConnection::new(&url);
        blocks_client.connect().await.unwrap();
        let sub_id = blocks_client.subscribe(Channel::NewBlocks).await.unwrap();

        let event = WSEvent {
            channel: "newHeads".into(),
            data: json!({ "number": "0x10" }),
            timestamp: now_secs(),
        };

        assert_eq!(server.broadcast(&Channel::NewBlocks, &event).await, 1);
        // A channel nobody subscribed to reaches nobody.
        assert_eq!(server.broadcast(&Channel::PendingTransactions, &event).await, 0);

        let received = tokio::time::timeout(
            std::time::Duration::from_secs(5),
            blocks_client.receive(),
        )
        .await
        .expect("frame arrived")
        .expect("frame parsed");

        assert_eq!(received.channel.as_deref(), Some(sub_id.as_str()));
        assert!(received.payload.contains("0x10"));

        blocks_client.disconnect().await;
    }

    #[tokio::test]
    async fn unknown_methods_and_channels_are_rejected() {
        let (_server, url) = start_server().await;
        let client = WSClientConnection::new(&url);
        client.connect().await.unwrap();

        assert!(client.request("eth_bogus", json!([])).await.is_err());

        // An unsupported subscription name must not create a subscription.
        let err = client
            .request("eth_subscribe", json!(["notAChannel"]))
            .await
            .unwrap_err();
        assert!(err.to_string().contains("unknown subscription"));
        assert!(client.subscriptions().await.is_empty());

        client.disconnect().await;
    }

    #[tokio::test]
    async fn a_malformed_frame_is_reported_and_the_connection_survives() {
        let (server, url) = start_server().await;
        let client = WSClientConnection::new(&url);
        client.connect().await.unwrap();

        // A raw frame that is not JSON.
        {
            let mut sink = tokio_tungstenite::connect_async(&url).await.unwrap().0;
            sink.send(Message::Text("{not json".into())).await.unwrap();
            let reply = sink.next().await.unwrap().unwrap();
            let text = reply.into_text().unwrap();
            assert!(text.contains("-32700"), "expected a parse error, got {text}");
        }

        // The server is still serving.
        assert_eq!(server.client_count().await, 2);
        let sub_id = client.subscribe(Channel::NewBlocks).await.unwrap();
        assert!(!sub_id.is_empty());

        client.disconnect().await;
    }

    #[test]
    fn channel_matching_honours_address_filters() {
        let all = Channel::Logs {
            addresses: Vec::new(),
        };
        let filtered = Channel::Logs {
            addresses: vec!["0xabc".into()],
        };

        assert!(all.matches(&Channel::Logs { addresses: vec![] }));
        assert!(all.matches(&filtered));
        assert!(filtered.matches(&filtered));
        assert!(!filtered.matches(&Channel::Logs {
            addresses: vec!["0xdef".into()]
        }));
        assert!(!filtered.matches(&Channel::NewBlocks));
    }

    #[tokio::test]
    async fn a_disconnected_client_cannot_send() {
        let client = WSClientConnection::new("ws://127.0.0.1:1");
        assert!(!client.is_connected().await);
        assert!(matches!(
            client.subscribe(Channel::NewBlocks).await,
            Err(WSError::NotConnected)
        ));
        assert!(client.receive().await.is_none());
    }

    #[test]
    fn subscription_params_are_well_formed() {
        assert_eq!(Channel::NewBlocks.subscribe_params(), json!(["newHeads"]));
        assert_eq!(
            Channel::Logs {
                addresses: vec!["0xabc".into()]
            }
            .subscribe_params(),
            json!(["logs", { "address": ["0xabc"] }])
        );
        assert_eq!(
            Channel::Addresses(vec!["0x1".into()]).subscribe_params(),
            json!(["addresses", ["0x1"]])
        );
    }
}
