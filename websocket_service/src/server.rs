//! WebSocket Server - Production-grade async server
//! High-performance connection handling

use std::net::SocketAddr;
use std::sync::Arc;

use futures_util::{SinkExt, StreamExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::{broadcast, mpsc};
use tokio_tungstenite::{accept_async, tungstenite::Message};

use crate::config::Config;
use crate::connection::{Connection, ConnectionManager};
use crate::error::{Error, Result};
use crate::events::Event;
use crate::handler::Handler;
use crate::heartbeat::Heartbeat;
use crate::metrics::Metrics;

pub struct Server {
    config: Config,
    listener: Option<TcpListener>,
    shutdown_tx: broadcast::Sender<()>,
    event_tx: broadcast::Sender<Event>,
    connection_manager: ConnectionManager,
    handler: Handler,
    heartbeat: Heartbeat,
    metrics: Metrics,
}

impl Server {
    /// Create new server
    pub fn new(config: Config) -> Self {
        let connection_manager = ConnectionManager::new(config.clone());
        let handler = Handler::new(config.clone());
        let heartbeat = Heartbeat::new(config.clone());
        let metrics = Metrics::new();
        
        Self {
            config: config.clone(),
            listener: None,
            shutdown_tx: broadcast::channel().0,
            event_tx: broadcast::channel().0,
            connection_manager,
            handler,
            heartbeat,
            metrics,
        }
    }
    
    /// Start server
    pub async fn start(&mut self) -> Result<()> {
        let addr = SocketAddr::from(([0, 0, 0, 0], self.config.port));
        
        let listener = TcpListener::bind(addr)
            .await
            .map_err(|e| Error::connection(format!("Failed to bind: {}", e)))?;
        
        self.listener = Some(listener);
        
        log::info!("WebSocket server listening on {}", addr);
        
        // Start heartbeat task
        self.heartbeat.start(self.connection_manager.clone()).await;
        
        // Accept connections
        self.accept_loop().await;
        
        Ok(())
    }
    
    /// Accept loop
    async fn accept_loop(&mut self) {
        let listener = match &self.listener {
            Some(l) => l,
            None => return,
        };
        
        loop {
            tokio::select! {
                _ = self.shutdown_tx.recv() => {
                    log::info!("Shutdown signal received");
                    break;
                }
                result = listener.accept() => {
                    match result {
                        Ok((stream, remote_addr)) => {
                            let server = self.clone_for_handler();
                            tokio::spawn(async move {
                                if let Err(e) = server.handle_connection(stream, remote_addr).await {
                                    log::error!("Connection error: {}", e);
                                }
                            });
                        }
                        Err(e) => {
                            log::error!("Accept error: {}", e);
                        }
                    }
                }
            }
        }
    }
    
    /// Handle individual connection
    async fn handle_connection(&self, stream: TcpStream, remote_addr: SocketAddr) -> Result<()> {
        // Create connection
        let connection = self.connection_manager.create_connection(
            remote_addr.to_string()
        )?;
        
        // Accept WebSocket
        let ws_stream = accept_async(stream)
            .await
            .map_err(|e| Error::connection(format!("WebSocket handshake failed: {}", e)))?;
        
        let (mut writer, mut reader) = ws_stream.split();
        
        // Set connection state to connected
        connection.set_state(crate::connection::ConnectionState::Connected);
        
        // Create message channel
        let (tx, mut rx) = mpsc::channel::<Vec<u8>>(self.config.message_queue_size);
        connection.set_sender(tx);
        
        // Handle messages
        loop {
            tokio::select! {
                _ = self.shutdown_tx.recv() => {
                    break;
                }
                msg = reader.next() => {
                    match msg {
                        Some(Ok(Message::Text(text))) => {
                            // Handle text message
                            if let Err(e) = self.handler.handle_message(
                                &connection,
                                text.as_bytes(),
                            ).await {
                                log::error!("Message error: {}", e);
                            }
                        }
                        Some(Ok(Message::Binary(data))) => {
                            // Handle binary message
                            connection.record_received(data.len());
                            if let Err(e) = self.handler.handle_message(
                                &connection,
                                &data,
                            ).await {
                                log::error!("Message error: {}", e);
                            }
                        }
                        Some(Ok(Message::Close(_))) | None => {
                            break;
                        }
                        Some(Err(e)) => {
                            log::error!("WebSocket error: {}", e);
                            break;
                        }
                        _ => {}
                    }
                }
                msg = rx.recv() => {
                    if let Some(data) = msg {
                        if let Err(e) = writer.send(Message::Binary(data)).await {
                            log::error!("Send error: {}", e);
                            break;
                        }
                    } else {
                        break;
                    }
                }
            }
        }
        
        // Cleanup
        self.connection_manager.remove_connection(connection.id());
        
        Ok(())
    }
    
    /// Clone for handler task
    fn clone_for_handler(&self) -> Self {
        Self {
            config: self.config.clone(),
            listener: None,
            shutdown_tx: self.shutdown_tx.clone(),
            event_tx: self.event_tx.clone(),
            connection_manager: ConnectionManager::new(self.config.clone()),
            handler: self.handler.clone(),
            heartbeat: self.heartbeat.clone(),
            metrics: self.metrics.clone(),
        }
    }
    
    /// Broadcast event to all subscribers
    pub async fn broadcast(&self, event: Event) -> Result<()> {
        self.event_tx.send(event).map_err(|e| Error::internal(format!("Broadcast error: {}", e)))?;
        self.metrics.record_event_sent();
        Ok(())
    }
    
    /// Get metrics
    pub fn metrics(&self) -> &Metrics {
        &self.metrics
    }
    
    /// Get connection count
    pub fn connection_count(&self) -> usize {
        self.connection_manager.connection_count()
    }
}

impl Clone for Server {
    fn clone(&self) -> Self {
        Self {
            config: self.config.clone(),
            listener: None,
            shutdown_tx: self.shutdown_tx.clone(),
            event_tx: self.event_tx.clone(),
            connection_manager: ConnectionManager::new(self.config.clone()),
            handler: self.handler.clone(),
            heartbeat: self.heartbeat.clone(),
            metrics: self.metrics.clone(),
        }
    }
}