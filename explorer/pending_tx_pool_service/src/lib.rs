//! TigerScan Real-time Pending Transaction Pool Service
//! Live transaction monitoring with WebSocket updates
//! Uses Rust for maximum performance and low latency

use std::collections::{BinaryHeap, HashMap, HashSet};
use std::sync::Arc;
use std::cmp::Ordering;

use anyhow::Result;
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use ethers::core::types::{Address, H256, U256};
use ethers::providers::{Http, Provider, StreamExt, Ws};
use ethers::types::{Block, Transaction, Eip1559TransactionRequest};
use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use thiserror::Error;
use tokio::sync::broadcast;
use tracing::{error, info, warn};

// ============================================================================
// Error Types
// ============================================================================

#[derive(Error, Debug)]
pub enum MempoolError {
    #[error("RPC error: {0}")]
    Rpc(String),
    
    #[error("Transaction not found: {0}")]
    NotFound(String),
    
    #[error("Pool error: {0}")]
    Pool(String),
    
    #[error("WebSocket error: {0}")]
    WebSocket(String),
    
    #[error("Rate limit exceeded")]
    RateLimitExceeded,
}

// ============================================================================
// Configuration
// ============================================================================

#[derive(Debug, Clone, Deserialize)]
pub struct Config {
    /// RPC HTTP endpoint
    pub rpc_url: String,
    /// WebSocket endpoint for real-time
    pub ws_url: String,
    /// Maximum pending transactions
    pub max_pending_txs: usize,
    /// Minimum gas price in Gwei
    pub min_gas_price: f64,
    /// Maximum gas price in Gwei
    pub max_gas_price: f64,
    /// Update interval in milliseconds
    pub update_interval: u64,
    /// Enable duplicate filtering
    pub filter_duplicates: bool,
    /// Enable nonce tracking
    pub track_nonce: bool,
    /// Broadcast channel size
    pub broadcast_size: usize,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            rpc_url: std::env::var("RPC_URL")
                .unwrap_or_else(|_| "http://localhost:8545".to_string()),
            ws_url: std::env::var("WS_URL")
                .unwrap_or_else(|_| "ws://localhost:8546".to_string()),
            max_pending_txs: 10000,
            min_gas_price: 0.001,
            max_gas_price: 100.0,
            update_interval: 100,
            filter_duplicates: true,
            track_nonce: true,
            broadcast_size: 10000,
        }
    }
}

// ============================================================================
// Data Types
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PendingTransaction {
    pub hash: String,
    pub from: String,
    pub to: Option<String>,
    pub value: String,
    pub gas_price: String,
    pub gas_limit: u64,
    pub nonce: u64,
    pub data: String,
    pub chain_id: u64,
    pub timestamp: i64,
    pub first_seen: i64,
    pub tx_type: TxType,
    pub size: usize,
    pub propagated: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TxType {
    Legacy,
    Eip2930,
    Eip1559,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PoolStats {
    pub total_pending: usize,
    pub total_by_type: HashMap<String, usize>,
    pub avg_gas_price: f64,
    pub avg_nonce: f64,
    pub top_senders: Vec<SenderStats>,
    pub gas_distribution: HashMap<String, usize>,
    pub last_block: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SenderStats {
    pub address: String,
    pub tx_count: usize,
    pub total_gas: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MempoolUpdate {
    pub update_type: UpdateType,
    pub transactions: Vec<PendingTransaction>,
    pub removed: Vec<String>,
    pub timestamp: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UpdateType {
    Added,
    Removed,
    Replaced,
    Confirmed,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GasPriceOracle {
    pub slow: f64,
    pub average: f64,
    pub fast: f64,
    pub base_fee: f64,
    pub last_update: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BlockInfo {
    pub number: u64,
    pub timestamp: i64,
    pub base_fee_per_gas: u64,
    pub gas_limit: u64,
    pub gas_used: u64,
}

// ============================================================================
// Priority Queue for Gas Price
// ============================================================================

impl Ord for PendingTransaction {
    fn cmp(&self, other: &Self) -> Ordering {
        // Compare by gas price (higher first)
        let self_gas = U256::from_dec_str(&self.gas_price).unwrap_or_default();
        let other_gas = U256::from_dec_str(&other.gas_price).unwrap_or_default();
        
        other_gas.cmp(&self_gas)
    }
}

impl PartialOrd for PendingTransaction {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Eq for PendingTransaction {}

// ============================================================================
// Mempool Service
// ============================================================================

pub struct MempoolService {
    config: Config,
    rpc: Provider<Http>,
    ws: Provider<Ws>,
    state: Arc<RwLock<MempoolState>>,
    broadcast_tx: broadcast::Sender<MempoolUpdate>,
    shutdown_tx: Option<tokio::sync::oneshot::Sender<()>>,
}

#[derive(Debug)]
pub struct MempoolState {
    pub pending: HashMap<String, PendingTransaction>,
    pub by_sender: HashMap<String, HashMap<u64, String>>, // sender -> nonce -> hash
    pub by_nonce: HashMap<String, HashMap<u64, String>>, // sender -> nonce -> hash
    pub heap: BinaryHeap<PendingTransaction>,
    pub nonces: HashMap<String, u64>,
    pub base_fee: u64,
    pub last_block: u64,
    pub last_update: i64,
}

impl MempoolService {
    pub async fn new(config: Config) -> Result<Self> {
        info!("Initializing Pending Transaction Pool Service");
        
        let rpc = Provider::<Http>::try_from(config.rpc_url.clone())?;
        
        let ws = Provider::<Ws>::connect(&config.ws_url).await?;
        
        let (broadcast_tx, _) = broadcast::channel(config.broadcast_size);
        
        let service = Self {
            config: config.clone(),
            rpc,
            ws,
            state: Arc::new(RwLock::new(MempoolState {
                pending: HashMap::new(),
                by_sender: HashMap::new(),
                by_nonce: HashMap::new(),
                heap: BinaryHeap::new(),
                nonces: HashMap::new(),
                base_fee: 0,
                last_block: 0,
                last_update: Utc::now().timestamp(),
            })),
            broadcast_tx,
            shutdown_tx: None,
        };
        
        info!("Pending Transaction Pool Service initialized");
        Ok(service)
    }

    /// Start the mempool service
    pub fn start(&mut self) {
        info!("Starting mempool service");
        
        let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel();
        self.shutdown_tx = Some(shutdown_tx);
        
        let state = self.state.clone();
        let config = self.config.clone();
        let broadcast_tx = self.broadcast_tx.clone();
        
        tokio::spawn(async move {
            Self::run_loop(state, config, broadcast_tx, shutdown_rx).await;
        });
    }

    /// Stop the mempool service
    pub fn stop(&mut self) {
        if let Some(tx) = self.shutdown_tx.take() {
            let _ = tx.send(());
        }
    }

    /// Run the main loop
    async fn run_loop(
        state: Arc<RwLock<MempoolState>>,
        config: Config,
        broadcast_tx: broadcast::Sender<MempoolUpdate>,
        shutdown_rx: tokio::sync::oneshot::Receiver<()>,
    ) {
        let mut ws = match Provider::<Ws>::connect(&config.ws_url).await {
            Ok(ws) => ws,
            Err(e) => {
                error!("Failed to connect to WebSocket: {}", e);
                return;
            }
        };
        
        // Subscribe to pending transactions
        let mut pending_stream = match ws.subscribe_pending_txs().await {
            Ok(stream) => stream,
            Err(e) => {
                error!("Failed to subscribe to pending txs: {}", e);
                return;
            }
        };
        
        info!("Mempool service started, listening for pending transactions");
        
        tokio::select! {
            _ = shutdown_rx => {
                info!("Mempool service stopped");
            }
            _ = async {
                while let Some(tx_hash) = pending_stream.next().await {
                    let state = state.clone();
                    
                    // Fetch transaction details
                    if let Ok(Some(tx)) = state.read().rpc.get_transaction(&tx_hash).await {
                        let pending = Self::convert_tx(tx);
                        
                        // Add to mempool
                        let mut mempool_state = state.write();
                        
                        // Check duplicate
                        if config.filter_duplicates && mempool_state.pending.contains_key(&pending.hash) {
                            continue;
                        }
                        
                        // Check size limit
                        if mempool_state.pending.len() >= config.max_pending_txs {
                            // Remove lowest gas price tx
                            if let Some(lowest) = mempool_state.heap.pop() {
                                mempool_state.pending.remove(&lowest.hash);
                            }
                        }
                        
                        // Add transaction
                        mempool_state.pending.insert(pending.hash.clone(), pending.clone());
                        mempool_state.heap.push(pending.clone());
                        
                        // Track by sender
                        mempool_state.by_sender
                            .entry(pending.from.clone())
                            .or_insert_with(HashMap::new)
                            .insert(pending.nonce, pending.hash.clone());
                        
                        // Broadcast update
                        let update = MempoolUpdate {
                            update_type: UpdateType::Added,
                            transactions: vec![pending],
                            removed: vec![],
                            timestamp: Utc::now().timestamp(),
                        };
                        
                        let _ = broadcast_tx.send(update);
                    }
                }
            } => {}
        }
    }

    /// Convert transaction to pending format
    fn convert_tx(tx: Transaction) -> PendingTransaction {
        let tx_type = if tx.max_fee_per_gas.is_some() {
            TxType::Eip1559
        } else if tx.access_list.is_some() {
            TxType::Eip2930
        } else {
            TxType::Legacy
        };
        
        let data = format!("{:?}", tx.input);
        let size = 4 + 4 + 32 + 32 + 32 + 4 + 32 + 32 + data.len() / 2;
        
        PendingTransaction {
            hash: format!("{:?}", tx.hash()),
            from: format!("{:?}", tx.from),
            to: tx.to.map(|a| format!("{:?}", a)),
            value: format!("{}", tx.value),
            gas_price: format!("{}", tx.gas_price.unwrap_or_default()),
            gas_limit: tx.gas.unwrap_or_default().as_u64(),
            nonce: tx.nonce.as_u64(),
            data: hex::encode(&tx.input),
            chain_id: tx.chain_id.unwrap_or_default().as_u64(),
            timestamp: Utc::now().timestamp(),
            first_seen: Utc::now().timestamp(),
            tx_type,
            size,
            propagated: false,
        }
    }

    /// Add a transaction to the mempool
    pub fn add_transaction(&self, tx: PendingTransaction) {
        let mut state = self.state.write();
        
        // Check duplicate
        if self.config.filter_duplicates && state.pending.contains_key(&tx.hash) {
            return;
        }
        
        // Add to mempool
        state.pending.insert(tx.hash.clone(), tx.clone());
        state.heap.push(tx);
        
        // Track by sender
        state.by_sender
            .entry(tx.from.clone())
            .or_insert_with(HashMap::new)
            .insert(tx.nonce, tx.hash.clone());
        
        // Broadcast update
        let update = MempoolUpdate {
            update_type: UpdateType::Added,
            transactions: vec![tx],
            removed: vec![],
            timestamp: Utc::now().timestamp(),
        };
        
        let _ = self.broadcast_tx.send(update);
    }

    /// Remove a transaction from the mempool
    pub fn remove_transaction(&self, hash: &str) -> Option<PendingTransaction> {
        let mut state = self.state.write();
        
        if let Some(tx) = state.pending.remove(hash) {
            // Remove from heap (need to rebuild)
            let mut new_heap = BinaryHeap::new();
            for t in state.heap.iter() {
                if t.hash != hash {
                    new_heap.push(t.clone());
                }
            }
            state.heap = new_heap;
            
            // Remove from sender tracking
            if let Some(nonces) = state.by_sender.get_mut(&tx.from) {
                nonces.remove(&tx.nonce);
            }
            
            // Broadcast update
            let update = MempoolUpdate {
                update_type: UpdateType::Removed,
                transactions: vec![],
                removed: vec![hash.to_string()],
                timestamp: Utc::now().timestamp(),
            };
            
            let _ = self.broadcast_tx.send(update);
            
            return Some(tx);
        }
        
        None
    }

    /// Get pending transactions
    pub fn get_pending(&self, limit: Option<usize>, offset: Option<usize>) -> Vec<PendingTransaction> {
        let state = self.state.read();
        
        let limit = limit.unwrap_or(100);
        let offset = offset.unwrap_or(0);
        
        let mut txs: Vec<_> = state.pending.values()
            .cloned()
            .collect();
        
        // Sort by gas price
        txs.sort_by(|a, b| {
            let a_gas = U256::from_dec_str(&a.gas_price).unwrap_or_default();
            let b_gas = U256::from_dec_str(&b.gas_price).unwrap_or_default();
            b_gas.cmp(&a_gas)
        });
        
        txs.into_iter()
            .skip(offset)
            .take(limit)
            .collect()
    }

    /// Get transactions by sender
    pub fn get_by_sender(&self, sender: &str) -> Vec<PendingTransaction> {
        let state = self.state.read();
        
        state.by_sender
            .get(sender)
            .map(|nonces| {
                nonces.values()
                    .filter_map(|hash| state.pending.get(hash).cloned())
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Get mempool statistics
    pub fn get_stats(&self) -> PoolStats {
        let state = self.state.read();
        
        let total = state.pending.len();
        
        // Count by type
        let mut by_type: HashMap<String, usize> = HashMap::new();
        for tx in state.pending.values() {
            *by_type.entry(format!("{:?}", tx.tx_type)).or_insert(0) += 1;
        }
        
        // Calculate average gas price
        let mut total_gas = U256::zero();
        let mut count = 0;
        for tx in state.pending.values() {
            if let Ok(gas) = U256::from_dec_str(&tx.gas_price) {
                total_gas += gas;
                count += 1;
            }
        }
        let avg_gas_price = if count > 0 {
            (total_gas / count).as_u64() as f64 / 1e18
        } else {
            0.0
        };
        
        // Calculate average nonce
        let total_nonce: u64 = state.pending.values()
            .map(|tx| tx.nonce)
            .sum();
        let avg_nonce = if total > 0 {
            total_nonce as f64 / total as f64
        } else {
            0.0
        };
        
        // Top senders
        let mut sender_counts: HashMap<String, (usize, u64)> = HashMap::new();
        for tx in state.pending.values() {
            let entry = sender_counts.entry(tx.from.clone()).or_insert((0, 0));
            entry.0 += 1;
            entry.1 += tx.gas_limit;
        }
        
        let mut top_senders: Vec<_> = sender_counts.into_iter()
            .map(|(addr, (count, gas))| SenderStats {
                address: addr,
                tx_count: count,
                total_gas: gas,
            })
            .collect();
        
        top_senders.sort_by(|a, b| b.tx_count.cmp(&a.tx_count));
        top_senders.truncate(10);
        
        // Gas distribution
        let mut gas_dist: HashMap<String, usize> = HashMap::new();
        for tx in state.pending.values() {
            let gas_price = if let Ok(gas) = U256::from_dec_str(&tx.gas_price) {
                gas.as_u64() as f64 / 1e9
            } else {
                0.0
            };
            
            let bucket = if gas_price < 1.0 {
                "< 1 Gwei".to_string()
            } else if gas_price < 5.0 {
                "1-5 Gwei".to_string()
            } else if gas_price < 10.0 {
                "5-10 Gwei".to_string()
            } else if gas_price < 50.0 {
                "10-50 Gwei".to_string()
            } else {
                "> 50 Gwei".to_string()
            };
            
            *gas_dist.entry(bucket).or_insert(0) += 1;
        }
        
        PoolStats {
            total_pending: total,
            total_by_type: by_type,
            avg_gas_price,
            avg_nonce,
            top_senders,
            gas_distribution: gas_dist,
            last_block: state.last_block,
        }
    }

    /// Get gas price oracle
    pub async fn get_gas_oracle(&self) -> Result<GasPriceOracle> {
        let state = self.state.read();
        
        let base_fee = state.base_fee;
        
        // Get pending transactions sorted by gas price
        let mut txs: Vec<_> = state.pending.values()
            .cloned()
            .collect();
        
        txs.sort_by(|a, b| {
            let a_gas = U256::from_dec_str(&a.gas_price).unwrap_or_default();
            let b_gas = U256::from_dec_str(&b.gas_price).unwrap_or_default();
            b_gas.cmp(&a_gas)
        });
        
        let count = txs.len();
        
        let slow = if count > 0 {
            let idx = (count * 20 / 100).max(1);
            let tx = &txs[idx.saturating_sub(1)];
            U256::from_dec_str(&tx.gas_price).unwrap_or_default().as_u64() as f64 / 1e9
        } else {
            base_fee as f64 / 1e9
        };
        
        let average = if count > 0 {
            let idx = count * 50 / 100;
            let tx = &txs[idx.saturating_sub(1)];
            U256::from_dec_str(&tx.gas_price).unwrap_or_default().as_u64() as f64 / 1e9
        } else {
            base_fee as f64 / 1e9
        };
        
        let fast = if count > 0 {
            let idx = (count * 80 / 100).min(count - 1);
            let tx = &txs[idx];
            U256::from_dec_str(&tx.gas_price).unwrap_or_default().as_u64() as f64 / 1e9
        } else {
            base_fee as f64 / 1e9 + 2.0
        };
        
        Ok(GasPriceOracle {
            slow,
            average,
            fast,
            base_fee: base_fee as f64 / 1e9,
            last_update: Utc::now().timestamp(),
        })
    }

    /// Subscribe to mempool updates
    pub fn subscribe(&self) -> broadcast::Receiver<MempoolUpdate> {
        self.broadcast_tx.subscribe()
    }

    /// Get transaction by hash
    pub fn get_transaction(&self, hash: &str) -> Option<PendingTransaction> {
        let state = self.state.read();
        state.pending.get(hash).cloned()
    }

    /// Check if transaction is in mempool
    pub fn contains(&self, hash: &str) -> bool {
        let state = self.state.read();
        state.pending.contains_key(hash)
    }

    /// Get next nonce for sender
    pub fn get_next_nonce(&self, sender: &str) -> Option<u64> {
        let state = self.state.read();
        
        if let Some(nonces) = state.by_sender.get(sender) {
            let max_nonce = nonces.keys().max().copied();
            max_nonce.map(|n| n + 1)
        } else {
            state.nonces.get(sender).copied()
        }
    }
}

// ============================================================================
// API Types
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MempoolApiRequest {
    pub limit: Option<usize>,
    pub offset: Option<usize>,
    pub sender: Option<String>,
    pub min_gas: Option<f64>,
    pub max_gas: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MempoolApiResponse {
    pub success: bool,
    pub result: Option<serde_json::Value>,
    pub error: Option<String>,
}

// ============================================================================
// Utility Functions
// ============================================================================

/// Format gas price to Gwei
pub fn to_gwei(wei: &str) -> f64 {
    U256::from_dec_str(wei).unwrap_or_default().as_u64() as f64 / 1e9
}

/// Format from Gwei to Wei
pub fn from_gwei(gwei: f64) -> String {
    format!("{}", U256::from((gwei * 1e9) as u64))
}

/// Estimate confirmation time
pub fn estimate_confirmation(gas_price: f64, base_fee: f64) -> u32 {
    let priority_fee = gas_price - base_fee;
    
    if priority_fee < 1.0 {
        // > 5 minutes
        300
    } else if priority_fee < 5.0 {
        // 1-5 minutes
        180
    } else if priority_fee < 20.0 {
        // 15-60 seconds
        30
    } else if priority_fee < 50.0 {
        // < 15 seconds
        10
    } else {
        // < 5 seconds
        3
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_gas_conversion() {
        assert!((to_gwei("1000000000") - 1.0).abs() < 0.001);
        assert_eq!(from_gwei(1.0), "1000000000");
    }

    #[test]
    fn test_confirmation_estimate() {
        assert_eq!(estimate_confirmation(0.5, 0.1), 300);
        assert_eq!(estimate_confirmation(100.0, 0.1), 3);
    }
}