//! WebSocket Server Implementation

use crate::messages::*;
use crate::subscriptions::*;
use anyhow::Result;
use futures_util::{SinkExt, StreamExt};
use parking_lot::RwLock;
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use thiserror::Error;
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::broadcast;
use tokio_tungstenite::tungstenite::Message;
use tracing::{error, info, warn};

/// WebSocket server errors
#[derive(Error, Debug)]
pub enum WsError {
    #[error("Connection error: {0}")]
    Connection(String),
    #[error("Message error: {0}")]
    Message(String),
    #[error("Subscription error: {0}")]
    Subscription(String),
}

/// WebSocket message handler
pub struct WsServer {
    addr: SocketAddr,
    subscribers: Arc<RwLock<HashMap<String, Vec<Subscription>>>>,
    /// Broadcast channels for each event type
    block_tx: broadcast::Sender<BlockEvent>,
    tx_tx: broadcast::Sender<TxEvent>,
    pending_tx_tx: broadcast::Sender<PendingTxEvent>,
    log_tx: broadcast::Sender<LogEvent>,
    gas_tx: broadcast::Sender<GasEvent>,
}

impl WsServer {
    /// Create new WebSocket server
    pub fn new(addr: SocketAddr) -> Self {
        let (block_tx, _) = broadcast::channel(1000);
        let (tx_tx, _) = broadcast::channel(1000);
        let (pending_tx_tx, _) = broadcast::channel(1000);
        let (log_tx, _) = broadcast::channel(1000);
        let (gas_tx, _) = broadcast::channel(1000);

        Self {
            addr,
            subscribers: Arc::new(RwLock::new(HashMap::new())),
            block_tx,
            tx_tx,
            pending_tx_tx,
            log_tx,
            gas_tx,
        }
    }

    /// Run the server
    pub async fn run(&self) -> Result<()> {
        let listener = TcpListener::bind(self.addr).await?;
        info!("WebSocket server listening on {}", self.addr);

        loop {
            match listener.accept().await {
                Ok((stream, addr)) => {
                    let subscribers = self.subscribers.clone();
                    let block_rx = self.block_tx.subscribe();
                    let tx_rx = self.tx_tx.subscribe();
                    let pending_rx = self.pending_tx_tx.subscribe();
                    let log_rx = self.log_tx.subscribe();
                    let gas_rx = self.gas_tx.subscribe();

                    tokio::spawn(async move {
                        if let Err(e) = self.handle_connection(
                            stream,
                            addr,
                            subscribers,
                            block_rx,
                            tx_rx,
                            pending_rx,
                            log_rx,
                            gas_rx,
                        ).await {
                            error!("Connection error: {}", e);
                        }
                    });
                }
                Err(e) => {
                    warn!("Failed to accept connection: {}", e);
                }
            }
        }
    }

    /// Handle a single connection
    async fn handle_connection(
        &self,
        stream: TcpStream,
        addr: SocketAddr,
        subscribers: Arc<RwLock<HashMap<String, Vec<Subscription>>>>,
        mut block_rx: broadcast::Receiver<BlockEvent>,
        mut tx_rx: broadcast::Receiver<TxEvent>,
        mut pending_rx: broadcast::Receiver<PendingTxEvent>,
        mut log_rx: broadcast::Receiver<LogEvent>,
        mut gas_rx: broadcast::Receiver<GasEvent>,
    ) -> Result<(), WsError> {
        let ws_stream = tokio_tungstenite::accept_async(stream)
            .await
            .map_err(|e| WsError::Connection(e.to_string()))?;

        let (mut write, mut read) = ws_stream.split();
        let client_id = uuid::Uuid::new_v4().to_string();

        info!("New WebSocket client: {}", client_id);

        // Send welcome message
        let welcome = WsMessage::Welcome(WelcomeMessage {
            client_id: client_id.clone(),
            message: "Connected to TigerScan WebSocket API".to_string(),
        });
        write.send(Message::Text(serde_json::to_string(&welcome)?))
            .await
            .map_err(|e| WsError::Message(e.to_string()))?;

        // Handle messages and events
        loop {
            tokio::select! {
                // Handle incoming messages from client
                msg = read.next() => {
                    match msg {
                        Some(Ok(Message::Text(text))) => {
                            if let Err(e) = self.handle_message(&client_id, &text, &subscribers).await {
                                error!("Message error: {}", e);
                            }
                        }
                        Some(Ok(Message::Close(_))) | None => {
                            info!("Client {} disconnected", client_id);
                            break;
                        }
                        Some(Err(e)) => {
                            warn!("Read error: {}", e);
                        }
                        _ => {}
                    }
                }

                // Handle block events
                Ok(event) = block_rx.recv() => {
                    let msg = WsMessage::Block(event);
                    if let Ok(text) = serde_json::to_string(&msg) {
                        let _ = write.send(Message::Text(text)).await;
                    }
                }

                // Handle transaction events
                Ok(event) = tx_rx.recv() => {
                    let msg = WsMessage::Transaction(event);
                    if let Ok(text) = serde_json::to_string(&msg) {
                        let _ = write.send(Message::Text(text)).await;
                    }
                }

                // Handle pending transaction events
                Ok(event) = pending_rx.recv() => {
                    let msg = WsMessage::PendingTransaction(event);
                    if let Ok(text) = serde_json::to_string(&msg) {
                        let _ = write.send(Message::Text(text)).await;
                    }
                }

                // Handle log events
                Ok(event) = log_rx.recv() => {
                    let msg = WsMessage::Log(event);
                    if let Ok(text) = serde_json::to_string(&msg) {
                        let _ = write.send(Message::Text(text)).await;
                    }
                }

                // Handle gas price events
                Ok(event) = gas_rx.recv() => {
                    let msg = WsMessage::GasPrice(event);
                    if let Ok(text) = serde_json::to_string(&msg) {
                        let _ = write.send(Message::Text(text)).await;
                    }
                }
            }
        }

        // Cleanup subscriptions
        let mut subs = subscribers.write();
        subs.remove(&client_id);

        Ok(())
    }

    /// Handle incoming message
    async fn handle_message(
        &self,
        client_id: &str,
        text: &str,
        subscribers: &Arc<RwLock<HashMap<String, Vec<Subscription>>>>,
    ) -> Result<(), WsError> {
        let msg: ClientMessage = serde_json::from_str(text)
            .map_err(|e| WsError::Message(e.to_string()))?;

        match msg {
            ClientMessage::Subscribe { channel, params } => {
                let sub = Subscription {
                    id: uuid::Uuid::new_v4().to_string(),
                    client_id: client_id.to_string(),
                    channel: channel.clone(),
                    params,
                };

                let mut subs = subscribers.write();
                subs.entry(client_id.to_string())
                    .or_insert_with(Vec::new)
                    .push(sub);

                info!("Client {} subscribed to {}", client_id, channel);
            }

            ClientMessage::Unsubscribe { channel } => {
                let mut subs = subscribers.write();
                if let Some(client_subs) = subs.get_mut(client_id) {
                    client_subs.retain(|s| s.channel != channel);
                    info!("Client {} unsubscribed from {}", client_id, channel);
                }
            }

            ClientMessage::Ping => {
                // Handle ping
            }
        }

        Ok(())
    }

    /// Broadcast a block event
    pub fn broadcast_block(&self, event: BlockEvent) {
        let _ = self.block_tx.send(event);
    }

    /// Broadcast a transaction event
    pub fn broadcast_transaction(&self, event: TxEvent) {
        let _ = self.tx_tx.send(event);
    }

    /// Broadcast a pending transaction event
    pub fn broadcast_pending_tx(&self, event: PendingTxEvent) {
        let _ = self.pending_tx_tx.send(event);
    }

    /// Broadcast a log event
    pub fn broadcast_log(&self, event: LogEvent) {
        let _ = self.log_tx.send(event);
    }

    /// Broadcast a gas price event
    pub fn broadcast_gas(&self, event: GasEvent) {
        let _ = self.gas_tx.send(event);
    }
}