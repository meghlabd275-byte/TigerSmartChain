//! WebSocket Client for TigerScan

use crate::messages::*;
use anyhow::Result;
use futures_util::{SinkExt, StreamExt};
use std::sync::Arc;
use tokio::sync::{broadcast, RwLock};
use tokio_tungstenite::{connect_async, tungstenite::Message};

/// WebSocket client for TigerScan API
pub struct WsClient {
    url: String,
    subscriptions: Arc<RwLock<Vec<Subscription>>>,
    /// Event broadcast channels
    block_tx: broadcast::Sender<BlockEvent>,
    tx_tx: broadcast::Sender<TxEvent>,
    pending_tx_tx: broadcast::Sender<PendingTxEvent>,
    log_tx: broadcast::Sender<LogEvent>,
    gas_tx: broadcast::Sender<GasEvent>,
}

impl WsClient {
    /// Create new WebSocket client
    pub fn new(url: &str) -> Self {
        let (block_tx, _) = broadcast::channel(1000);
        let (tx_tx, _) = broadcast::channel(1000);
        let (pending_tx_tx, _) = broadcast::channel(1000);
        let (log_tx, _) = broadcast::channel(1000);
        let (gas_tx, _) = broadcast::channel(1000);

        Self {
            url: url.to_string(),
            subscriptions: Arc::new(RwLock::new(Vec::new())),
            block_tx,
            tx_tx,
            pending_tx_tx,
            log_tx,
            gas_tx,
        }
    }

    /// Connect and run the client
    pub async fn run(&self) -> Result<()> {
        let (ws_stream, _) = connect_async(&self.url).await?;
        let (mut write, mut read) = ws_stream.split();

        // Process messages
        while let Some(msg) = read.next().await {
            match msg {
                Ok(Message::Text(text)) => {
                    self.handle_message(&text).await?;
                }
                Ok(Message::Close(_)) => break,
                Err(e) => {
                    anyhow::bail!("WebSocket error: {}", e);
                }
                _ => {}
            }
        }

        Ok(())
    }

    /// Handle incoming message
    async fn handle_message(&self, text: &str) -> Result<()> {
        let msg: WsMessage = serde_json::from_str(text)?;

        match msg {
            WsMessage::Welcome(w) => {
                println!("Connected: {}", w.message);
            }
            WsMessage::Block(e) => {
                let _ = self.block_tx.send(e);
            }
            WsMessage::Transaction(e) => {
                let _ = self.tx_tx.send(e);
            }
            WsMessage::PendingTransaction(e) => {
                let _ = self.pending_tx_tx.send(e);
            }
            WsMessage::Log(e) => {
                let _ = self.log_tx.send(e);
            }
            WsMessage::GasPrice(e) => {
                let _ = self.gas_tx.send(e);
            }
            WsMessage::Error(e) => {
                println!("Error: {} - {}", e.code, e.message);
            }
            WsMessage::Pong => {}
        }

        Ok(())
    }

    /// Subscribe to a channel
    pub async fn subscribe(&self, channel: &str, params: Option<SubscribeParams>) -> Result<()> {
        let sub = Subscription {
            id: uuid::Uuid::new_v4().to_string(),
            client_id: String::new(),
            channel: channel.to_string(),
            params,
        };

        self.subscriptions.write().await.push(sub);
        Ok(())
    }

    /// Unsubscribe from a channel
    pub async fn unsubscribe(&self, channel: &str) -> Result<()> {
        self.subscriptions.write().await.retain(|s| s.channel != channel);
        Ok(())
    }

    /// Subscribe to block events
    pub fn subscribe_blocks(&self) -> broadcast::Receiver<BlockEvent> {
        self.block_tx.subscribe()
    }

    /// Subscribe to transaction events
    pub fn subscribe_transactions(&self) -> broadcast::Receiver<TxEvent> {
        self.tx_tx.subscribe()
    }

    /// Subscribe to pending transaction events
    pub fn subscribe_pending(&self) -> broadcast::Receiver<PendingTxEvent> {
        self.pending_tx_tx.subscribe()
    }

    /// Subscribe to log events
    pub fn subscribe_logs(&self) -> broadcast::Receiver<LogEvent> {
        self.log_tx.subscribe()
    }

    /// Subscribe to gas price events
    pub fn subscribe_gas(&self) -> broadcast::Receiver<GasEvent> {
        self.gas_tx.subscribe()
    }
}