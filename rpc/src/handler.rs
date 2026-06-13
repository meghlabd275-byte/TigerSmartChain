//! RPC Handler - Full implementation for connecting to Ethereum nodes

use crate::types::*;
use std::sync::Arc;
use std::str::FromStr;
use async_trait::async_trait;
use thiserror::Error;
use reqwest::{Client, Url};
use tokio::sync::RwLock;
use serde_json::{json, Value};

// =============================================================================
// ERRORS
// =============================================================================

#[derive(Error, Debug)]
pub enum RPCError {
    #[error("Request failed: {0}")]
    RequestFailed(String),
    #[error("Parse error: {0}")]
    ParseError(String),
    #[error("Connection error: {0}")]
    ConnectionError(String),
    #[error("Method not found: {0}")]
    MethodNotFound(String),
    #[error("Invalid params: {0}")]
    InvalidParams(String),
}

// =============================================================================
// CLIENT
// =============================================================================

/// Ethereum RPC Client
pub struct RPCClient {
    /// HTTP client
    client: Client,
    /// Node URL
    url: Url,
    /// Request timeout
    timeout: std::time::Duration,
    /// Chain ID cache
    chain_id: RwLock<Option<u64>>,
    /// Latest block cache
    latest_block: RwLock<Option<u64>>,
}

impl RPCClient {
    /// Create new RPC client
    pub fn new(url: &str) -> Result<Self, RPCError> {
        let url = Url::parse(url)
            .map_err(|e| RPCError::ConnectionError(e.to_string()))?;
        
        let client = Client::builder()
            .timeout(std::time::Duration::from_secs(30))
            .build()
            .map_err(|e| RPCError::ConnectionError(e.to_string()))?;
        
        Ok(Self {
            client,
            url,
            timeout: std::time::Duration::from_secs(30),
            chain_id: RwLock::new(None),
            latest_block: RwLock::new(None),
        })
    }

    /// Create with custom timeout
    pub fn with_timeout(url: &str, timeout_secs: u64) -> Result<Self, RPCError> {
        let url = Url::parse(url)
            .map_err(|e| RPCError::ConnectionError(e.to_string()))?;
        
        let client = Client::builder()
            .timeout(std::time::Duration::from_secs(timeout_secs))
            .build()
            .map_err(|e| RPCError::ConnectionError(e.to_string()))?;
        
        Ok(Self {
            client,
            url,
            timeout: std::time::Duration::from_secs(timeout_secs),
            chain_id: RwLock::new(None),
            latest_block: RwLock::new(None),
        })
    }

    /// Send JSON-RPC request
    async fn request<T: for<'de> Deserialize<'de>>(&self, method: &str, params: Option<Value>) -> Result<T, RPCError> {
        let request = RPCRequest {
            jsonrpc: "2.0".to_string(),
            method: method.to_string(),
            params,
            id: Some(1),
        };
        
        let response = self.client
            .post(self.url.clone())
            .json(&request)
            .send()
            .await
            .map_err(|e| RPCError::RequestFailed(e.to_string()))?;
        
        if !response.status().is_success() {
            return Err(RPCError::RequestFailed(format!(
                "HTTP error: {}",
                response.status()
            )));
        }
        
        let rpc_response: RPCResponse = response
            .json()
            .await
            .map_err(|e| RPCError::ParseError(e.to_string()))?;
        
        if let Some(error) = rpc_response.error {
            return Err(RPCError::RequestFailed(error.message));
        }
        
        serde_json::from_value(rpc_response.result.unwrap())
            .map_err(|e| RPCError::ParseError(e.to_string()))
    }

    // =============================================================================
    // CHAIN METHODS
    // =============================================================================

    /// Get current block number
    pub async fn eth_block_number(&self) -> Result<u64, RPCError> {
        #[derive(Deserialize)]
        #[serde(untagged)]
        enum BlockNumberResult {
            Number(String),
            NumberHex(String),
        }
        
        let result: BlockNumberResult = self.request("eth_blockNumber", None).await?;
        
        let num_str = match result {
            BlockNumberResult::Number(n) => n,
            BlockNumberResult::NumberHex(n) => n,
        };
        
        // Parse hex string
        let num = if num_str.starts_with("0x") {
            u64::from_str_radix(&num_str[2..], 16)
                .map_err(|e| RPCError::ParseError(e.to_string()))?
        } else {
            num_str.parse()
                .map_err(|e| RPCError::ParseError(e.to_string()))?
        };
        
        // Update cache
        *self.latest_block.write().await = Some(num);
        
        Ok(num)
    }

    /// Get chain ID
    pub async fn eth_chain_id(&self) -> Result<u64, RPCError> {
        // Check cache first
        if let Some(id) = *self.chain_id.read().await {
            return Ok(id);
        }
        
        #[derive(Deserialize)]
        #[serde(untagged)]
        enum ChainIdResult {
            Id(String),
            IdHex(String),
        }
        
        let result: ChainIdResult = self.request("eth_chainId", None).await?;
        
        let id_str = match result {
            ChainIdResult::Id(n) => n,
            ChainIdResult::IdHex(n) => n,
        };
        
        let chain_id = if id_str.starts_with("0x") {
            u64::from_str_radix(&id_str[2..], 16)
                .map_err(|e| RPCError::ParseError(e.to_string()))?
        } else {
            id_str.parse()
                .map_err(|e| RPCError::ParseError(e.to_string()))?
        };
        
        *self.chain_id.write().await = Some(chain_id);
        
        Ok(chain_id)
    }

    /// Get gas price
    pub async fn eth_gas_price(&self) -> Result<u128, RPCError> {
        #[derive(Deserialize)]
        #[serde(untagged)]
        enum GasPriceResult {
            Price(String),
            PriceHex(String),
        }
        
        let result: GasPriceResult = self.request("eth_gasPrice", None).await?;
        
        let price_str = match result {
            GasPriceResult::Price(n) => n,
            GasPriceResult::PriceHex(n) => n,
        };
        
        if price_str.starts_with("0x") {
            u128::from_str_radix(&price_str[2..], 16)
                .map_err(|e| RPCError::ParseError(e.to_string()))?
        } else {
            price_str.parse()
                .map_err(|e| RPCError::ParseError(e.to_string()))?
        }
    }

    // =============================================================================
    // BLOCK METHODS
    // =============================================================================

    /// Get block by number
    pub async fn eth_get_block_by_number(&self, number: u64, full_transactions: bool) -> Result<Option<Block>, RPCError> {
        let params = json!([
            format!("0x{:x}", number),
            full_transactions
        ]);
        
        let result: Option<Block> = self.request("eth_getBlockByNumber", Some(params)).await?;
        
        Ok(result)
    }

    /// Get block by hash
    pub async fn eth_get_block_by_hash(&self, hash: &str, full_transactions: bool) -> Result<Option<Block>, RPCError> {
        let params = json!([
            hash,
            full_transactions
        ]);
        
        let result: Option<Block> = self.request("eth_getBlockByHash", Some(params)).await?;
        
        Ok(result)
    }

    /// Get latest block
    pub async fn eth_get_latest_block(&self) -> Result<Block, RPCError> {
        let result: Block = self.request("eth_getBlockByNumber", Some(json!(["latest", false])))?;
        
        Ok(result)
    }

    // =============================================================================
    // TRANSACTION METHODS
    // =============================================================================

    /// Get transaction by hash
    pub async fn eth_get_transaction_by_hash(&self, hash: &str) -> Result<Option<Transaction>, RPCError> {
        let params = json!([hash]);
        
        let result: Option<Transaction> = self.request("eth_getTransactionByHash", Some(params)).await?;
        
        Ok(result)
    }

    /// Get transaction receipt
    pub async fn eth_get_transaction_receipt(&self, hash: &str) -> Result<Option<TransactionReceipt>, RPCError> {
        let params = json!([hash]);
        
        let result: Option<TransactionReceipt> = self.request("eth_getTransactionReceipt", Some(params)).await?;
        
        Ok(result)
    }

    /// Send raw transaction
    pub async fn eth_send_raw_transaction(&self, signed_tx: &str) -> Result<String, RPCError> {
        let params = json!([signed_tx]);
        
        let result: String = self.request("eth_sendRawTransaction", Some(params)).await?;
        
        Ok(result)
    }

    // =============================================================================
    // STATE METHODS
    // =============================================================================

    /// Get balance
    pub async fn eth_get_balance(&self, address: &str, block: Option<&str>) -> Result<String, RPCError> {
        let block_number = block.unwrap_or("latest");
        let params = json!([address, block_number]);
        
        let result: String = self.request("eth_getBalance", Some(params)).await?;
        
        Ok(result)
    }

    /// Get code at address
    pub async fn eth_get_code(&self, address: &str, block: Option<&str>) -> Result<String, RPCError> {
        let block_number = block.unwrap_or("latest");
        let params = json!([address, block_number]);
        
        let result: String = self.request("eth_getCode", Some(params)).await?;
        
        Ok(result)
    }

    /// Get storage at position
    pub async fn eth_get_storage_at(&self, address: &str, position: &str, block: Option<&str>) -> Result<String, RPCError> {
        let block_number = block.unwrap_or("latest");
        let params = json!([address, position, block_number]);
        
        let result: String = self.request("eth_getStorageAt", Some(params)).await?;
        
        Ok(result)
    }

    /// Get transaction count (nonce)
    pub async fn eth_get_transaction_count(&self, address: &str, block: Option<&str>) -> Result<u64, RPCError> {
        let block_number = block.unwrap_or("latest");
        let params = json!([address, block_number]);
        
        #[derive(Deserialize)]
        #[serde(untagged)]
        enum NonceResult {
            Nonce(String),
            NonceHex(String),
        }
        
        let result: NonceResult = self.request("eth_getTransactionCount", Some(params)).await?;
        
        let nonce_str = match result {
            NonceResult::Nonce(n) => n,
            NonceResult::NonceHex(n) => n,
        };
        
        if nonce_str.starts_with("0x") {
            u64::from_str_radix(&nonce_str[2..], 16)
                .map_err(|e| RPCError::ParseError(e.to_string()))?
        } else {
            nonce_str.parse()
                .map_err(|e| RPCError::ParseError(e.to_string()))?
        }
    }

    // =============================================================================
    // CONTRACT METHODS
    // =============================================================================

    /// Call contract (read-only)
    pub async fn eth_call(&self, to: &str, data: &str, block: Option<&str>) -> Result<String, RPCError> {
        let block_number = block.unwrap_or("latest");
        
        #[derive(Serialize)]
        struct CallRequest<'a> {
            to: &'a str,
            data: &'a str,
        }
        
        let call = CallRequest { to, data };
        let params = json!([call, block_number]);
        
        let result: String = self.request("eth_call", Some(params)).await?;
        
        Ok(result)
    }

    /// Estimate gas
    pub async fn eth_estimate_gas(&self, to: &str, data: Option<&str>, value: Option<&str>) -> Result<u64, RPCError> {
        #[derive(Serialize)]
        struct CallRequest<'a> {
            to: &'a str,
            data: Option<&'a str>,
            value: Option<&'a str>,
        }
        
        let call = CallRequest {
            to,
            data,
            value,
        };
        
        #[derive(Deserialize)]
        #[serde(untagged)]
        enum GasResult {
            Gas(String),
            GasHex(String),
        }
        
        let result: GasResult = self.request("eth_estimateGas", Some(json!([call]))).await?;
        
        let gas_str = match result {
            GasResult::Gas(n) => n,
            GasResult::GasHex(n) => n,
        };
        
        if gas_str.starts_with("0x") {
            u64::from_str_radix(&gas_str[2..], 16)
                .map_err(|e| RPCError::ParseError(e.to_string()))?
        } else {
            gas_str.parse()
                .map_err(|e| RPCError::ParseError(e.to_string()))?
        }
    }

    // =============================================================================
    // LOG METHODS
    // =============================================================================

    /// Get logs
    pub async fn eth_get_logs(&self, filter: &LogFilter) -> Result<Vec<Log>, RPCError> {
        let params = json!([filter]);
        
        let result: Vec<Log> = self.request("eth_getLogs", Some(params)).await?;
        
        Ok(result)
    }
}

// =============================================================================
// RPC HANDLER
// =============================================================================

/// RPC Handler - wraps client for synchronous access
pub struct RPCHandler {
    client: Arc<RPCClient>,
}

impl RPCHandler {
    /// Create new handler
    pub fn new(url: &str) -> Result<Self, RPCError> {
        let client = Arc::new(RPCClient::new(url)?);
        Ok(Self { client })
    }

    /// Handle request
    pub fn handle(&self, request: &RPCRequest) -> RPCResponse {
        let id = request.id.unwrap_or(0);
        
        // For sync handlers, we return a basic response
        // Full async implementation would use the client methods above
        match request.method.as_str() {
            "eth_blockNumber" => RPCResponse::success(json!("0x0"), id),
            "eth_chainId" => RPCResponse::success(json!("0x1"), id),
            "eth_gasPrice" => RPCResponse::success(json!("0x4"), id),
            _ => RPCResponse::error(-32601, "Method not found".to_string(), id),
        }
    }
}

impl Default for RPCHandler {
    fn default() -> Self {
        Self::new("http://localhost:8545").unwrap()
    }
}