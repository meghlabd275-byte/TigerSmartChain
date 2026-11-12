//! TigerScan Trace Debug Service Main
//! High-performance API server for transaction tracing

#![forbid(unsafe_code)]

use std::net::SocketAddr;
use std::sync::Arc;

use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use tiny_http::{Response, Server, StatusCode};
use tracing::{error, info, Level};
use tracing_subscriber::FmtSubscriber;

use trace_debug_service::{
    Config, GasOptimizer, MemoryInspector, StackInspector, StorageInspector,
    TraceApiRequest, TraceApiResponse, TraceRequest, TraceResult, TraceService,
};

// ============================================================================
// API Types
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiRequest {
    pub jsonrpc: String,
    pub method: String,
    pub params: Option<serde_json::Value>,
    pub id: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiResponse {
    pub jsonrpc: String,
    pub result: Option<serde_json::Value>,
    pub error: Option<ApiError>,
    pub id: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiError {
    pub code: i32,
    pub message: String,
    pub data: Option<serde_json::Value>,
}

// ============================================================================
// Rate Limiter
// ============================================================================

#[derive(Debug)]
pub struct RateLimiter {
    requests: Arc<RwLock<RateLimitStore>>,
    config: RateLimitConfig,
}

#[derive(Debug, Default)]
pub struct RateLimitStore {
    pub requests: std::collections::HashMap<String, Vec<std::time::Instant>>,
}

#[derive(Debug, Clone)]
pub struct RateLimitConfig {
    pub requests_per_minute: u32,
    pub burst_size: u32,
}

impl RateLimiter {
    pub fn new() -> Self {
        Self {
            requests: Arc::new(RwLock::new(RateLimitStore::default())),
            config: RateLimitConfig {
                requests_per_minute: 60,
                burst_size: 5,
            },
        }
    }

    pub fn check(&self, identifier: &str) -> Result<(), String> {
        let now = std::time::Instant::now();
        let store = self.requests.read();
        
        let requests = store.requests.get(identifier)
            .cloned()
            .unwrap_or_default();
        
        let last_minute: Vec<_> = requests.iter()
            .filter(|t| now.duration_since(**t).as_secs() < 60)
            .collect();
        
        let count = last_minute.len() as u32;
        if count >= self.config.burst_size {
            return Err("Rate limit exceeded - burst".to_string());
        }
        
        if count >= self.config.requests_per_minute {
            return Err("Rate limit exceeded - per minute".to_string());
        }
        
        Ok(())
    }

    pub fn record(&self, identifier: &str) {
        let now = std::time::Instant::now();
        let mut store = self.requests.write();
        
        let requests = store.requests.entry(identifier.to_string())
            .or_insert_with(Vec::new);
        
        requests.push(now);
        
        // Clean old entries
        let cutoff = now.checked_sub(std::time::Duration::from_secs(60))
            .unwrap_or(now);
        
        requests.retain(|t| *t > cutoff);
    }
}

// ============================================================================
// Request Handler
// ============================================================================

pub struct Handler {
    service: TraceService,
    rate_limiter: RateLimiter,
    gas_optimizer: GasOptimizer,
}

impl Handler {
    pub fn new(service: TraceService) -> Self {
        Self {
            service,
            rate_limiter: RateLimiter::new(),
            gas_optimizer: GasOptimizer::new(),
        }
    }

    pub fn handle(&self, body: &[u8], ip: &str) -> Response<std::io::Cursor<Vec<u8>>> {
        // Check rate limit
        if let Err(e) = self.rate_limiter.check(ip) {
            let response = ApiResponse {
                jsonrpc: "2.0".to_string(),
                result: None,
                error: Some(ApiError {
                    code: -32000,
                    message: e,
                    data: None,
                }),
                id: 0,
            };
            return Response::from_string(serde_json::to_string(&response).unwrap_or_default())
                .with_status_code(StatusCode::from(429));
        }
        
        self.rate_limiter.record(ip);
        
        // Parse request
        let request: ApiRequest = match serde_json::from_slice(body) {
            Ok(r) => r,
            Err(e) => {
                let response = ApiResponse {
                    jsonrpc: "2.0".to_string(),
                    result: None,
                    error: Some(ApiError {
                        code: -32700,
                        message: format!("Parse error: {}", e),
                        data: None,
                    }),
                    id: 0,
                };
                return Response::from_string(serde_json::to_string(&response).unwrap_or_default())
                    .with_status_code(StatusCode::from(400));
            }
        };
        
        // The handlers are async and this is called from the synchronous HTTP
        // loop, so a runtime has to drive them here. Reusing the ambient
        // runtime keeps a request from nesting one inside another, which would
        // panic; a fresh runtime is only built when there is none.
        let result = match tokio::runtime::Handle::try_current() {
            Ok(handle) => handle.block_on(self.dispatch(&request.method, request.params)),
            Err(_) => match tokio::runtime::Runtime::new() {
                Ok(runtime) => runtime.block_on(self.dispatch(&request.method, request.params)),
                Err(e) => Err(format!("cannot start a runtime to handle the request: {e}")),
            },
        };
        
        let response = match result {
            Ok(result) => ApiResponse {
                jsonrpc: "2.0".to_string(),
                result: Some(serde_json::to_value(result).unwrap_or_default()),
                error: None,
                id: request.id,
            },
            Err(e) => ApiResponse {
                jsonrpc: "2.0".to_string(),
                result: None,
                error: Some(ApiError {
                    code: -32000,
                    message: e,
                    data: None,
                }),
                id: request.id,
            },
        };
        
        Response::from_string(serde_json::to_string(&response).unwrap_or_default())
            .with_status_code(StatusCode::from(200))
    }

    /// Route a JSON-RPC method to its handler.
    ///
    /// Every handler is normalised to a `serde_json::Value` so the response
    /// builder has one shape to serialise, rather than a `to_value` at each
    /// call site that could silently fall back to null.
    async fn dispatch(
        &self,
        method: &str,
        params: Option<serde_json::Value>,
    ) -> Result<serde_json::Value, String> {
        match method {
            "debug_traceTransaction" => self
                .handle_trace(params)
                .await
                .and_then(|r| serde_json::to_value(r).map_err(|e| e.to_string())),
            "debug_traceCall" => self.handle_trace_call(params).await,
            "trace_analyzeGas" => self.handle_gas_analysis(params).await,
            "trace_getStateDiff" => self.handle_state_diff(params).await,
            "trace_getCallTree" => self.handle_call_tree(params).await,
            "trace_estimateGas" => self
                .handle_estimate_gas(params)
                .await
                .map(|gas| serde_json::json!(gas)),
            _ => Err(format!("Unknown method: {method}")),
        }
    }

    async fn handle_trace(&self, params: Option<serde_json::Value>) -> Result<TraceResult, String> {
        let params: TraceApiRequest = params
            .and_then(|p| serde_json::from_value(p).ok())
            .ok_or("Invalid params")?;
        
        let request = TraceRequest {
            transaction_hash: params.tx_hash,
            block_number: params.block,
            trace_types: vec![],
            enable_state_diff: params.enable_state_diff.unwrap_or(true),
            enable_gas_profiling: params.enable_gas_profiling.unwrap_or(true),
        };
        
        self.service.trace_transaction(request).await
            .map_err(|e| e.to_string())
    }

    async fn handle_trace_call(
        &self,
        params: Option<serde_json::Value>,
    ) -> Result<serde_json::Value, String> {
        let params = params.ok_or("Missing params")?;
        // The first parameter is the call object, the second the block tag.
        let call = if params.is_array() {
            params
                .get(0)
                .cloned()
                .ok_or("trace_call requires a call object")?
        } else {
            params.get("call").cloned().unwrap_or(params.clone())
        };
        if !call.is_object() {
            return Err("trace_call requires a call object as its first parameter".to_string());
        }
        let block = params
            .get(1)
            .or_else(|| params.get("block"))
            .and_then(|b| b.as_str())
            .unwrap_or("latest")
            .to_string();

        let frames = self
            .service
            .trace_call(call, &block)
            .await
            .map_err(|e| e.to_string())?;
        Ok(serde_json::json!({ "calls": frames, "block": block }))
    }

    async fn handle_gas_analysis(&self, params: Option<serde_json::Value>) -> Result<serde_json::Value, String> {
        let params: TraceApiRequest = params
            .and_then(|p| serde_json::from_value(p).ok())
            .ok_or("Invalid params")?;
        
        let request = TraceRequest {
            transaction_hash: params.tx_hash,
            block_number: params.block,
            trace_types: vec![],
            enable_state_diff: false,
            enable_gas_profiling: true,
        };
        
        let result = self.service.trace_transaction(request).await
            .map_err(|e| e.to_string())?;
        
        let optimizations = self.gas_optimizer.analyze(&result);
        
        Ok(serde_json::json!({
            "total_gas": result.gas_used,
            "optimizations": optimizations,
            "suggestions": result.gas_profiling.map(|gp| gp.optimization_suggestions)
        }))
    }

    async fn handle_state_diff(&self, params: Option<serde_json::Value>) -> Result<serde_json::Value, String> {
        let params: TraceApiRequest = params
            .and_then(|p| serde_json::from_value(p).ok())
            .ok_or("Invalid params")?;
        
        let request = TraceRequest {
            transaction_hash: params.tx_hash,
            block_number: params.block,
            trace_types: vec![],
            enable_state_diff: true,
            enable_gas_profiling: false,
        };
        
        let result = self.service.trace_transaction(request).await
            .map_err(|e| e.to_string())?;
        
        Ok(serde_json::json!({
            "state_diff": result.state_diff
        }))
    }

    async fn handle_call_tree(&self, params: Option<serde_json::Value>) -> Result<serde_json::Value, String> {
        let params: TraceApiRequest = params
            .and_then(|p| serde_json::from_value(p).ok())
            .ok_or("Invalid params")?;
        
        let request = TraceRequest {
            transaction_hash: params.tx_hash,
            block_number: params.block,
            trace_types: vec![],
            enable_state_diff: false,
            enable_gas_profiling: false,
        };
        
        let result = self.service.trace_transaction(request).await
            .map_err(|e| e.to_string())?;
        
        Ok(serde_json::json!({
            "calls": result.traces,
            "internal_transactions": result.internal_txs
        }))
    }

    async fn handle_estimate_gas(&self, params: Option<serde_json::Value>) -> Result<u64, String> {
        let params = params.ok_or("Missing params")?;
        // `estimate_gas` takes the call object directly, or a wrapper around
        // it, so both spellings are accepted.
        let call = if params.is_array() {
            params
                .get(0)
                .cloned()
                .ok_or("estimate_gas requires a call object")?
        } else {
            params.get("call").cloned().unwrap_or(params.clone())
        };
        if !call.is_object() {
            return Err("estimate_gas requires a call object".to_string());
        }
        let request: ethers::types::TransactionRequest = serde_json::from_value(call)
            .map_err(|e| format!("could not read the call: {e}"))?;
        self.service
            .estimate_gas(&request)
            .await
            .map_err(|e| e.to_string())
    }
}

// ============================================================================
// Main
// ============================================================================

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize logging
    let subscriber = FmtSubscriber::builder()
        .with_max_level(Level::INFO)
        .with_target(false)
        .init();
    
    info!("Starting Trace Debug Service");
    
    // Load configuration
    let config = Config::default();
    
    // Create service
    let service = TraceService::new(config).await?;
    
    // Create handler
    let handler = Arc::new(Handler::new(service));
    
    // Start HTTP server. The address comes from TRACE_RPC_ADDR when set, so a
    // deployment can move the port without a rebuild.
    let addr: SocketAddr = std::env::var("TRACE_RPC_ADDR")
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or_else(|| SocketAddr::from(([0, 0, 0, 0], 8546)));
    info!("Server listening on {}", addr);

    tigerscan_service_http::serve_with_ip(addr, move |body, ip| handler.handle(body, ip))
        .map_err(|e| -> Box<dyn std::error::Error> { e.into() })?;

    Ok(())
}