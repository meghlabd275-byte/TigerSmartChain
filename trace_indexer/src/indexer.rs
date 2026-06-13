//! Trace Indexer Implementation for TigerScan
//! 
//! Full implementation for indexing internal transactions through trace methods.

use crate::types::*;
use chrono::Utc;
use std::sync::Arc;
use std::str::FromStr;
use tokio::sync::RwLock;
use tokio::time::{interval, Duration};
use thiserror::Error;
use reqwest::Client;
use serde::{Serialize, Deserialize};
use serde_json::{json, Value};

// =============================================================================
// ERRORS
// =============================================================================

#[derive(Error, Debug)]
pub enum TraceIndexerError {
    #[error("RPC error: {0}")]
    RPCError(String),
    #[error("Parse error: {0}")]
    ParseError(String),
    #[error("Not running")]
    NotRunning,
    #[error("Storage error: {0}")]
    StorageError(String),
}

// =============================================================================
// TRACE INDEXER
// =============================================================================

/// Trace Indexer - Full implementation for internal transaction tracking
pub struct TraceIndexer {
    config: TraceIndexerConfig,
    stats: Arc<RwLock<TraceStats>>,
    running: Arc<RwLock<bool>>,
    current_block: Arc<RwLock<u64>>,
    rpc_url: String,
    client: Client,
}

impl TraceIndexer {
    /// Create new trace indexer
    pub fn new(rpc_url: &str) -> Self {
        let config = TraceIndexerConfig {
            rpc_url: rpc_url.to_string(),
            ..Default::default()
        };
        
        let client = Client::builder()
            .timeout(Duration::from_secs(config.timeout_secs))
            .build()
            .unwrap_or_else(|_| Client::new());
        
        Self {
            config: config.clone(),
            stats: Arc::new(RwLock::new(TraceStats::default())),
            running: Arc::new(RwLock::new(false)),
            current_block: Arc::new(RwLock::new(config.start_block)),
            rpc_url: rpc_url.to_string(),
            client,
        }
    }

    /// Create with config
    pub fn with_config(config: TraceIndexerConfig) -> Self {
        let client = Client::builder()
            .timeout(Duration::from_secs(config.timeout_secs))
            .build()
            .unwrap_or_else(|_| Client::new());
        
        Self {
            config: config.clone(),
            stats: Arc::new(RwLock::new(TraceStats::default())),
            running: Arc::new(RwLock::new(false)),
            current_block: Arc::new(RwLock::new(config.start_block)),
            rpc_url: config.rpc_url.clone(),
            client,
        }
    }

    /// Start indexing traces
    pub async fn start(&self) -> Result<(), TraceIndexerError> {
        *self.running.write().await = true;
        
        tracing::info!("Starting trace indexer from block {}", self.config.start_block);
        
        self.process_traces_loop().await
    }

    /// Stop indexing
    pub async fn stop(&self) {
        *self.running.write().await = false;
        tracing::info!("Trace indexer stopped");
    }

    /// Is running
    pub async fn is_running(&self) -> bool {
        *self.running.read().await
    }

    /// Get statistics
    pub async fn get_stats(&self) -> TraceStats {
        self.stats.read().await.clone()
    }

    /// Get current block
    pub async fn get_current_block(&self) -> u64 {
        *self.current_block.read().await
    }

    /// Process traces in a loop
    async fn process_traces_loop(&self) -> Result<(), TraceIndexerError> {
        let mut current = self.config.start_block;
        let mut poll_interval = interval(Duration::from_secs(12));

        while !*self.running.read().await {
            return Err(TraceIndexerError::NotRunning);
        }

        while self.is_running().await {
            poll_interval.tick().await;

            // Get latest block from RPC
            let latest = match self.fetch_latest_block().await {
                Ok(n) => n,
                Err(e) => {
                    tracing::warn!("Failed to fetch latest block: {}", e);
                    continue;
                }
            };

            // Process new blocks
            if current > latest {
                continue;
            }

            let batch_size = self.config.batch_size as u64;
            let end = std::cmp::min(current + batch_size - 1, latest);

            tracing::info!("Processing traces for blocks {} to {}", current, end);

            for block_num in current..=end {
                match self.process_block_traces(block_num).await {
                    Ok(_) => {
                        *self.current_block.write().await = block_num + 1;
                    }
                    Err(e) => {
                        tracing::error!("Error processing traces for block {}: {}", block_num, e);
                    }
                }
            }

            current = end + 1;
        }

        Ok(())
    }

    /// Fetch latest block number from RPC
    async fn fetch_latest_block(&self) -> Result<u64, TraceIndexerError> {
        let request = json!({
            "jsonrpc": "2.0",
            "method": "eth_blockNumber",
            "params": [],
            "id": 1
        });

        let response = self.client
            .post(&self.rpc_url)
            .json(&request)
            .send()
            .await
            .map_err(|e| TraceIndexerError::RPCError(e.to_string()))?
            .json::<Value>()
            .await
            .map_err(|e| TraceIndexerError::RPCError(e.to_string()))?;

        let result = response.get("result")
            .ok_or_else(|| TraceIndexerError::RPCError("No result in response".to_string()))?;

        let hex = result.as_str()
            .ok_or_else(|| TraceIndexerError::RPCError("Invalid block number".to_string()))?;

        let num = u64::from_str_radix(&hex[2..], 16)
            .map_err(|e| TraceIndexerError::ParseError(e.to_string()))?;

        Ok(num)
    }

    /// Process traces for a single block
    async fn process_block_traces(&self, block_number: u64) -> Result<BlockTraceResult, TraceIndexerError> {
        let start_time = std::time::Instant::now();

        // Fetch trace data using trace_block
        let traces = if self.config.enable_traces {
            self.fetch_block_traces(block_number).await?
        } else {
            Vec::new()
        };

        // Fetch state diffs
        let state_diffs = if self.config.enable_state_diffs {
            self.fetch_state_diffs(block_number).await?
        } else {
            Vec::new()
        };

        // Extract contract creations from traces
        let creations = if self.config.enable_creations {
            self.extract_creations(&traces, block_number)
        } else {
            Vec::new()
        };

        // Extract self-destructs from traces
        let selfdestructs = if self.config.enable_selfdestructs {
            self.extract_selfdestructs(&traces, block_number)
        } else {
            Vec::new()
        };

        // Update statistics
        let mut stats = self.stats.write().await;
        stats.current_block = block_number;
        stats.indexed_traces += traces.len() as u64;
        stats.indexed_state_diffs += state_diffs.len() as u64;
        stats.indexed_creations += creations.len() as u64;
        stats.indexed_selfdestructs += selfdestructs.len() as u64;
        stats.last_update = Utc::now().timestamp();
        
        let elapsed = start_time.elapsed().as_secs_f64();
        if elapsed > 0.0 {
            stats.processing_rate = (traces.len() as f64 + state_diffs.len() as f64) / elapsed;
        }

        Ok(BlockTraceResult {
            block_number,
            traces,
            state_diffs,
            creations,
            selfdestructs,
        })
    }

    /// Fetch block traces from RPC
    async fn fetch_block_traces(&self, block_number: u64) -> Result<Vec<IndexedTrace>, TraceIndexerError> {
        let request = json!({
            "jsonrpc": "2.0",
            "method": "trace_block",
            "params": [format!("0x{:x}", block_number), ["trace"]],
            "id": 1
        });

        let response = self.client
            .post(&self.rpc_url)
            .json(&request)
            .send()
            .await
            .map_err(|e| TraceIndexerError::RPCError(e.to_string()))?
            .json::<Value>()
            .await
            .map_err(|e| TraceIndexerError::RPCError(e.to_string()))?;

        let result = response.get("result")
            .ok_or_else(|| TraceIndexerError::RPCError("No result in response".to_string()))?;

        let traces_array = result.as_array()
            .ok_or_else(|| TraceIndexerError::RPCError("Invalid trace result".to_string()))?;

        let mut traces = Vec::new();
        let mut subtrace_index = 0u32;

        for trace_value in traces_array {
            if let Some(trace_obj) = trace_value.as_object() {
                // Parse trace action
                let action = trace_obj.get("action");
                let trace_type = trace_obj.get("type")
                    .and_then(|v| v.as_str())
                    .unwrap_or("call")
                    .to_string();

                let (from, to, value, call_type, input) = if let Some(action_obj) = action.and_then(|a| a.as_object()) {
                    (
                        action_obj.get("from").and_then(|v| v.as_str()).unwrap_or("0x").to_string(),
                        action_obj.get("to").and_then(|v| v.as_str()).unwrap_or("0x").to_string(),
                        action_obj.get("value").and_then(|v| v.as_str()).unwrap_or("0x0").to_string(),
                        action_obj.get("callType").and_then(|v| v.as_str()).unwrap_or("call").to_string(),
                        action_obj.get("input").and_then(|v| v.as_str()).unwrap_or("").to_string(),
                    )
                } else {
                    ("0x".to_string(), "0x".to_string(), "0x0".to_string(), "call".to_string(), String::new())
                };

                let mut trace = IndexedTrace::new(
                    String::new(), // tx_hash will be set below
                    block_number,
                    subtrace_index,
                    call_type.clone(),
                    from.clone(),
                    to.clone(),
                    value.clone(),
                );
                
                trace.input = input;
                trace.trace_type = trace_type.clone();
                trace.tx_hash = trace_obj.get("txHash")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string())
                    .unwrap_or_default();

                // Get gas info from result
                if let Some(result_obj) = trace_obj.get("result").and_then(|r| r.as_object()) {
                    trace.gas_used = result_obj.get("gas").and_then(|v| v.as_str()).map(|s| s.to_string());
                    trace.output = result_obj.get("returnValue").and_then(|v| v.as_str()).map(|s| s.to_string());
                }

                // Get error
                trace.error = trace_obj.get("error").and_then(|v| v.as_str()).map(|s| s.to_string());

                // Get subtraces for depth
                trace.depth = trace_obj.get("subtraces")
                    .and_then(|v| v.as_u64())
                    .map(|n| n as u32)
                    .unwrap_or(0);

                traces.push(trace);
                subtrace_index += 1;
            }
        }

        Ok(traces)
    }

    /// Fetch state diffs from RPC
    async fn fetch_state_diffs(&self, block_number: u64) -> Result<Vec<IndexedStateDiff>, TraceIndexerError> {
        let request = json!({
            "jsonrpc": "2.0",
            "method": "trace_getStateDiffs",
            "params": [format!("0x{:x}", block_number), "stateDiff"],
            "id": 1
        });

        let response = self.client
            .post(&self.rpc_url)
            .json(&request)
            .send()
            .await
            .map_err(|e| TraceIndexerError::RPCError(e.to_string()))?
            .json::<Value>()
            .await
            .map_err(|e| TraceIndexerError::RPCError(e.to_string()))?;

        let result = response.get("result")
            .ok_or_else(|| TraceIndexerError::RPCError("No result in response".to_string()))?;

        let diffs_array = result.as_array()
            .ok_or_else(|| TraceIndexerError::RPCError("Invalid state diff result".to_string()))?;

        let mut state_diffs = Vec::new();

        for diff_value in diffs_array {
            if let Some(diff_obj) = diff_value.as_object() {
                let address = diff_obj.get("address")
                    .and_then(|v| v.as_str())
                    .unwrap_or("0x")
                    .to_string();

                let (previous, current) = if let Some(changes) = diff_obj.get("0").and_then(|c| c.as_object()) {
                    (
                        changes.get("*").and_then(|v| v.as_str()).unwrap_or("0x0").to_string(),
                        changes.get("+").and_then(|v| v.as_str()).unwrap_or("0x0").to_string(),
                    )
                } else {
                    ("0x0".to_string(), "0x0".to_string())
                };

                let diff = IndexedStateDiff {
                    id: format!("{}-{}-{}", address, block_number, state_diffs.len()),
                    transaction_hash: String::new(),
                    block_number,
                    address: address.clone(),
                    storage_key: None,
                    previous,
                    current,
                    diff_type: StateDiffType::Balance,
                };

                state_diffs.push(diff);
            }
        }

        Ok(state_diffs)
    }

    /// Extract contract creations from traces
    fn extract_creations(&self, traces: &[IndexedTrace], block_number: u64) -> Vec<ContractCreation> {
        traces
            .iter()
            .filter(|t| t.trace_type == "create" || t.trace_type == "create2")
            .filter_map(|t| {
                let address = t.output.as_ref()?;
                Some(ContractCreation::new(
                    t.transaction_hash.clone(),
                    block_number,
                    address.clone(),
                    t.from.clone(),
                ))
            })
            .collect()
    }

    /// Extract self-destructs from traces
    fn extract_selfdestructs(&self, traces: &[IndexedTrace], block_number: u64) -> Vec<SelfDestruct> {
        traces
            .iter()
            .filter(|t| t.trace_type == "suicide")
            .filter_map(|t| {
                Some(SelfDestruct {
                    id: format!("{}-{}", t.transaction_hash, t.to),
                    transaction_hash: t.transaction_hash.clone(),
                    block_number,
                    address: t.from.clone(),
                    refund_address: t.to.clone(),
                    balance: t.value.clone(),
                })
            })
            .collect()
    }

    /// Get traces for a specific transaction
    pub async fn get_transaction_traces(&self, tx_hash: &str) -> Result<TransactionTraceResult, TraceIndexerError> {
        let request = json!({
            "jsonrpc": "2.0",
            "method": "trace_transaction",
            "params": [tx_hash],
            "id": 1
        });

        let response = self.client
            .post(&self.rpc_url)
            .json(&request)
            .send()
            .await
            .map_err(|e| TraceIndexerError::RPCError(e.to_string()))?
            .json::<Value>()
            .await
            .map_err(|e| TraceIndexerError::RPCError(e.to_string()))?;

        let result = response.get("result")
            .ok_or_else(|| TraceIndexerError::RPCError("No result in response".to_string()))?;

        let traces_array = result.as_array()
            .ok_or_else(|| TraceIndexerError::RPCError("Invalid trace result".to_string()))?;

        let mut traces = Vec::new();
        let mut subtrace_index = 0u32;

        for trace_value in traces_array {
            if let Some(trace_obj) = trace_value.as_object() {
                let action = trace_obj.get("action");
                let trace_type = trace_obj.get("type")
                    .and_then(|v| v.as_str())
                    .unwrap_or("call")
                    .to_string();

                let (from, to, value, call_type, input) = if let Some(action_obj) = action.and_then(|a| a.as_object()) {
                    (
                        action_obj.get("from").and_then(|v| v.as_str()).unwrap_or("0x").to_string(),
                        action_obj.get("to").and_then(|v| v.as_str()).unwrap_or("0x").to_string(),
                        action_obj.get("value").and_then(|v| v.as_str()).unwrap_or("0x0").to_string(),
                        action_obj.get("callType").and_then(|v| v.as_str()).unwrap_or("call").to_string(),
                        action_obj.get("input").and_then(|v| v.as_str()).unwrap_or("").to_string(),
                    )
                } else {
                    ("0x".to_string(), "0x".to_string(), "0x0".to_string(), "call".to_string(), String::new())
                };

                let mut trace = IndexedTrace::new(
                    tx_hash.to_string(),
                    0,
                    subtrace_index,
                    call_type.clone(),
                    from.clone(),
                    to.clone(),
                    value.clone(),
                );
                
                trace.input = input;
                trace.trace_type = trace_type;

                if let Some(result_obj) = trace_obj.get("result").and_then(|r| r.as_object()) {
                    trace.gas_used = result_obj.get("gas").and_then(|v| v.as_str()).map(|s| s.to_string());
                    trace.output = result_obj.get("returnValue").and_then(|v| v.as_str()).map(|s| s.to_string());
                }

                trace.error = trace_obj.get("error").and_then(|v| v.as_str()).map(|s| s.to_string());

                traces.push(trace);
                subtrace_index += 1;
            }
        }

        // Build call tree
        let call_tree = self.build_call_tree(&traces);

        Ok(TransactionTraceResult {
            transaction_hash: tx_hash.to_string(),
            block_number: 0,
            traces,
            state_diffs: Vec::new(),
            creations: Vec::new(),
            selfdestructs: Vec::new(),
            call_tree,
        })
    }

    /// Build call tree from traces
    fn build_call_tree(&self, traces: &[IndexedTrace]) -> Vec<CallTreeNode> {
        let mut nodes = Vec::new();
        
        for (i, trace) in traces.iter().enumerate() {
            let children: Vec<CallTreeNode> = traces
                .iter()
                .skip(i + 1)
                .take(trace.depth as usize)
                .map(|t| CallTreeNode {
                    index: t.subtrace_index,
                    call_type: t.call_type.clone(),
                    from: t.from.clone(),
                    to: t.to.clone(),
                    value: t.value.clone(),
                    gas: t.gas.clone(),
                    input: t.input.clone(),
                    output: t.output.clone().unwrap_or_default(),
                    children: Vec::new(),
                    error: t.error.clone(),
                })
                .collect();

            nodes.push(CallTreeNode {
                index: trace.subtrace_index,
                call_type: trace.call_type.clone(),
                from: trace.from.clone(),
                to: trace.to.clone(),
                value: trace.value.clone(),
                gas: trace.gas.clone(),
                input: trace.input.clone(),
                output: trace.output.clone().unwrap_or_default(),
                children,
                error: trace.error.clone(),
            });
        }

        nodes
    }

    /// Replay transaction for simulation
    pub async fn replay_transaction(&self, tx_hash: &str, trace_types: &[&str]) -> Result<TraceReplayResult, TraceIndexerError> {
        let request = json!({
            "jsonrpc": "2.0",
            "method": "trace_replayTransaction",
            "params": [tx_hash, trace_types],
            "id": 1
        });

        let response = self.client
            .post(&self.rpc_url)
            .json(&request)
            .send()
            .await
            .map_err(|e| TraceIndexerError::RPCError(e.to_string()))?
            .json::<Value>()
            .await
            .map_err(|e| TraceIndexerError::RPCError(e.to_string()))?;

        let result = response.get("result")
            .ok_or_else(|| TraceIndexerError::RPCError("No result in response".to_string()))?
            .clone();

        Ok(TraceReplayResult {
            tx_hash: tx_hash.to_string(),
            trace: result,
        })
    }
}

// =============================================================================
// HELPER STRUCTS
// =============================================================================

/// Trace Replay Result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TraceReplayResult {
    pub tx_hash: String,
    pub trace: Value,
}

// Extend IndexedTrace to add tx_hash field
trait IndexedTraceExt {
    fn tx_hash(&mut self, hash: String);
}

impl IndexedTraceExt for IndexedTrace {
    fn tx_hash(&mut self, hash: String) {
        self.id = format!("{}-{}-{}", hash, self.block_number, self.subtrace_index);
    }
}