//! Debug/Trace API Service
//!
//! Provides EVM debugging and tracing capabilities:
//! - VM tracer with step-by-step execution
//! - State diff tracing
//! - Call frame tracing
//! - Memory and storage inspection

use serde::{Deserialize, Serialize};
use std::collections::{HashMap, VecDeque};
use std::time::{SystemTime, UNIX_EPOCH};

// =============================================================================
// ERRORS
// =============================================================================

/// Trace Errors
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum TraceError {
    #[serde(rename = "execution_failed")]
    ExecutionFailed(String),
    #[serde(rename = "invalid_tx")]
    InvalidTransaction(String),
    #[serde(rename = "rpc_error")]
    RpcError(String),
    #[serde(rename = "timeout")]
    Timeout(String),
}

// =============================================================================
// TRACE CONFIGURATION
// =============================================================================

/// Trace configuration options
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TraceConfig {
    /// Enable call tracing
    pub enable_call_tracing: bool,
    /// Enable state diffs
    pub enable_state_diffs: bool,
    /// Enable memory tracing
    pub enable_memory: bool,
    /// Enable stack tracing
    pub enable_stack: bool,
    /// Enable storage tracing
    pub enable_storage: bool,
    /// Enable revert reason
    pub enable_revert_reason: bool,
    /// Maximum trace size
    pub max_trace_size: usize,
    /// Timeout in seconds
    pub timeout_seconds: u64,
}

impl Default for TraceConfig {
    fn default() -> Self {
        Self {
            enable_call_tracing: true,
            enable_state_diffs: true,
            enable_memory: true,
            enable_stack: true,
            enable_storage: true,
            enable_revert_reason: true,
            max_trace_size: 100000,
            timeout_seconds: 30,
        }
    }
}

// =============================================================================
// TRACE STRUCTURES
// =============================================================================

/// VM step
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VMState {
    pub pc: u64,
    pub op: u8,
    pub op_name: String,
    pub gas: String,
    pub gas_cost: String,
    pub memory: Option<String>,
    pub stack: Vec<String>,
    pub storage: Option<HashMap<String, String>>,
    pub depth: u32,
    pub error: Option<String>,
}

/// Call frame
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CallFrame {
    pub call_type: String,
    pub from: String,
    pub to: String,
    pub value: String,
    pub gas: String,
    pub input: String,
    pub output: String,
    pub revert_reason: Option<String>,
    pub depth: u32,
    pub calls: Vec<CallFrame>,
}

/// State change
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StateChange {
    pub address: String,
    pub key: String,
    pub old_value: String,
    pub new_value: String,
    pub change_type: StateChangeType,
}

/// State change type
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum StateChangeType {
    #[serde(rename = "storage")]
    Storage,
    #[serde(rename = "balance")]
    Balance,
    #[serde(rename = "nonce")]
    Nonce,
    #[serde(rename = "code")]
    Code,
}

/// Complete trace result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TraceResult {
    pub tx_hash: String,
    pub gas_used: String,
    pub failed: bool,
    pub revert_reason: Option<String>,
    pub steps: Vec<VMState>,
    pub calls: Vec<CallFrame>,
    pub state_changes: Vec<StateChange>,
    pub output: String,
    pub execution_time_ms: u64,
}

// =============================================================================
// DEBUGGER
// =============================================================================

/// Debugger with VM tracing
pub struct Debugger {
    /// RPC endpoint
    rpc_url: String,
    /// Default config
    config: TraceConfig,
    /// Trace cache
    trace_cache: HashMap<String, TraceResult>,
    /// Maximum cache size
    max_cache_size: usize,
    /// Statistics
    stats: DebuggerStats,
}

/// Debugger statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DebuggerStats {
    pub total_traces: u64,
    pub cache_hits: u64,
    pub cache_misses: u64,
    pub errors: u64,
}

impl Default for DebuggerStats {
    fn default() -> Self {
        Self {
            total_traces: 0,
            cache_hits: 0,
            cache_misses: 0,
            errors: 0,
        }
    }
}

impl Debugger {
    /// Create new debugger
    pub fn new(rpc_url: String) -> Self {
        Self {
            rpc_url,
            config: TraceConfig::default(),
            trace_cache: HashMap::new(),
            max_cache_size: 1000,
            stats: DebuggerStats::default(),
        }
    }

    /// Trace transaction
    pub fn trace_transaction(&mut self, tx_hash: &str, config: Option<TraceConfig>) -> Result<TraceResult, TraceError> {
        self.stats.total_traces += 1;
        
        let cfg = config.unwrap_or(self.config.clone());
        
        // Check cache
        if let Some(cached) = self.trace_cache.get(tx_hash) {
            self.stats.cache_hits += 1;
            return Ok(cached.clone());
        }
        
        self.stats.cache_misses += 1;
        
        // In production, call debug_traceTransaction RPC
        // For now, create mock trace
        let result = self.create_mock_trace(tx_hash, &cfg);
        
        // Cache result
        if self.trace_cache.len() >= self.max_cache_size {
            if let Some(first) = self.trace_cache.keys().next().cloned() {
                self.trace_cache.remove(&first);
            }
        }
        self.trace_cache.insert(tx_hash.to_string(), result.clone());
        
        Ok(result)
    }

    /// Trace call (simulate without executing on chain)
    pub fn trace_call(&mut self, from: &str, to: &str, value: &str, data: &str, config: Option<TraceConfig>) -> Result<TraceResult, TraceError> {
        let cfg = config.unwrap_or(self.config.clone());
        
        // In production, call debug_traceCall RPC
        let mut result = TraceResult {
            tx_hash: format!("call_{}", now_unix()),
            gas_used: "0x5208".to_string(), // 21000 gas
            failed: false,
            revert_reason: None,
            steps: vec![],
            calls: vec![],
            state_changes: vec![],
            output: "0x".to_string(),
            execution_time_ms: 0,
        };
        
        // Add call frame
        if cfg.enable_call_tracing {
            result.calls.push(CallFrame {
                call_type: "CALL".to_string(),
                from: from.to_string(),
                to: to.to_string(),
                value: value.to_string(),
                gas: "0x0".to_string(),
                input: data.to_string(),
                output: "0x".to_string(),
                revert_reason: None,
                depth: 1,
                calls: vec![],
            });
        }
        
        Ok(result)
    }

    /// Trace raw VM execution
    pub fn trace_vm(&mut self, code: &str, input: &str, config: Option<TraceConfig>) -> Result<Vec<VMState>, TraceError> {
        let cfg = config.unwrap_or(self.config.clone());
        
        // In production, trace each VM step
        let mut steps = vec![];
        
        // Mock steps for demonstration
        steps.push(VMState {
            pc: 0,
            op: 0x60, // PUSH1
            op_name: "PUSH1".to_string(),
            gas: "0x1b1".to_string(),
            gas_cost: "0x3".to_string(),
            memory: None,
            stack: vec![],
            storage: None,
            depth: 1,
            error: None,
        });
        
        Ok(steps)
    }

    /// Get state diffs for transaction
    pub fn get_state_diffs(&self, tx_hash: &str) -> Result<Vec<StateChange>, TraceError> {
        // Query from database or cache
        Ok(vec![])
    }

    /// Generate call tree
    pub fn get_call_tree(&self, tx_hash: &str) -> Option<CallFrame> {
        None
    }

    /// Set config
    pub fn set_config(&mut self, config: TraceConfig) {
        self.config = config;
    }

    /// Get statistics
    pub fn stats(&self) -> &DebuggerStats {
        &self.stats
    }

    /// Clear cache
    pub fn clear_cache(&mut self) {
        self.trace_cache.clear();
    }

    // Helper: create mock trace
    fn create_mock_trace(&self, tx_hash: &str, cfg: &TraceConfig) -> TraceResult {
        let mut result = TraceResult {
            tx_hash: tx_hash.to_string(),
            gas_used: "0x8610".to_string(), // 34320 gas
            failed: false,
            revert_reason: None,
            steps: vec![],
            calls: vec![],
            state_changes: vec![],
            output: "0x".to_string(),
            execution_time_ms: 0,
        };

        // Add mock steps
        if cfg.enable_memory || cfg.enable_stack {
            result.steps.push(VMState {
                pc: 0,
                op: 0x60,
                op_name: "PUSH1".to_string(),
                gas: "0x1b1".to_string(),
                gas_cost: "0x3".to_string(),
                memory: if cfg.enable_memory { Some("0x".to_string()) } else { None },
                stack: if cfg.enable_stack { vec!["0x00".to_string()] } else { vec![] },
                storage: None,
                depth: 1,
                error: None,
            });
        }

        // Add mock call frames
        if cfg.enable_call_tracing {
            result.calls.push(CallFrame {
                call_type: "CALL".to_string(),
                from: "0x0000000000000000000000000000000000000000".to_string(),
                to: tx_hash.to_string(),
                value: "0x0".to_string(),
                gas: "0x0".to_string(),
                input: "0x".to_string(),
                output: "0x".to_string(),
                revert_reason: None,
                depth: 1,
                calls: vec![],
            });
        }

        // Add mock state changes
        if cfg.enable_state_diffs {
            result.state_changes.push(StateChange {
                address: "0x0000000000000000000000000000000000000000".to_string(),
                key: "balance".to_string(),
                old_value: "0x0".to_string(),
                new_value: "0x0".to_string(),
                change_type: StateChangeType::Balance,
            });
        }

        result
    }
}

// =============================================================================
// TRACE API SERVICE
// =============================================================================

/// Trace API service
pub struct TraceAPI {
    debugger: Debugger,
}

impl TraceAPI {
    /// Create new trace API
    pub fn new(rpc_url: String) -> Self {
        Self {
            debugger: Debugger::new(rpc_url),
        }
    }

    /// Trace transaction
    pub fn trace_tx(&mut self, tx_hash: &str, config: Option<TraceConfig>) -> Result<TraceResult, TraceError> {
        self.debugger.trace_transaction(tx_hash, config)
    }

    /// Trace call
    pub fn trace_call(&mut self, from: &str, to: &str, value: &str, data: &str, config: Option<TraceConfig>) -> Result<TraceResult, TraceError> {
        self.debugger.trace_call(from, to, value, data, config)
    }

    /// Get state diffs
    pub fn state_diffs(&self, tx_hash: &str) -> Result<Vec<StateChange>, TraceError> {
        self.debugger.get_state_diffs(tx_hash)
    }

    /// Get call tree
    pub fn call_tree(&self, tx_hash: &str) -> Option<CallFrame> {
        self.debugger.get_call_tree(tx_hash)
    }
}

// =============================================================================
// HELPER FUNCTIONS
// =============================================================================

/// Get current Unix timestamp
fn now_unix() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs()
}