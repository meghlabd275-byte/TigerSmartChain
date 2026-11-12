//! API Types for TigerScan

use serde::{Deserialize, Serialize};

// =============================================================================
// REQUEST TYPES
// =============================================================================

/// API Request
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct APIRequest {
    pub method: String,
    pub params: Option<serde_json::Value>,
    pub id: Option<i64>,
}

/// Block Request
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BlockRequest {
    pub block_number: Option<u64>,
    pub block_hash: Option<String>,
    pub include_txs: Option<bool>,
}

impl BlockRequest {
    /// Reject a request that names neither a number nor a hash.
    pub fn validate(&self) -> Result<(), APIError> {
        if self.block_number.is_none() && self.block_hash.is_none() {
            return Err(APIError::invalid_params(
                "a block request needs either block_number or block_hash",
            ));
        }

        Ok(())
    }
}

/// Transaction Request
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TransactionRequest {
    pub tx_hash: String,
}

impl TransactionRequest {
    /// Reject anything that is not a 32-byte transaction hash.
    pub fn validate(&self) -> Result<(), APIError> {
        if !crate::handlers::is_hash(&self.tx_hash) {
            return Err(APIError::invalid_params(
                "tx_hash must be 32 bytes of hex",
            ));
        }

        Ok(())
    }
}

/// Address Request
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AddressRequest {
    pub address: String,
    pub page: Option<usize>,
    pub limit: Option<usize>,
}

impl AddressRequest {
    /// Reject anything that is not a 20-byte address.
    pub fn validate(&self) -> Result<(), APIError> {
        if !crate::handlers::is_address(&self.address) {
            return Err(APIError::invalid_params(
                "address must be 20 bytes of hex",
            ));
        }

        Ok(())
    }
}

/// Token Request
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenRequest {
    pub page: Option<usize>,
    pub limit: Option<usize>,
    pub search: Option<String>,
}

/// Search Request
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchRequest {
    pub query: String,
    pub limit: Option<usize>,
}

impl SearchRequest {
    /// Reject an empty query or an out-of-range page size.
    pub fn validate(&self) -> Result<(), APIError> {
        if self.query.trim().is_empty() {
            return Err(APIError::invalid_params("query must not be empty"));
        }

        if let Some(limit) = self.limit {
            if limit == 0 || limit > 100 {
                return Err(APIError::invalid_params(
                    "limit must be between 1 and 100",
                ));
            }
        }

        Ok(())
    }
}

// =============================================================================
// RESPONSE TYPES
// =============================================================================

/// API Response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct APIResponse<T> {
    pub jsonrpc: String,
    pub id: Option<i64>,
    pub result: Option<T>,
    pub error: Option<APIError>,
}

impl<T> APIResponse<T> {
    pub fn ok(result: T) -> Self {
        Self {
            jsonrpc: "2.0".to_string(),
            id: None,
            result: Some(result),
            error: None,
        }
    }

    pub fn err(code: i32, message: String) -> Self {
        Self {
            jsonrpc: "2.0".to_string(),
            id: None,
            result: None,
            error: Some(APIError { code, message }),
        }
    }
}

/// API Error
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct APIError {
    pub code: i32,
    pub message: String,
}

impl APIError {
    /// A JSON-RPC invalid-params error (-32602).
    pub fn invalid_params(message: impl Into<String>) -> Self {
        Self {
            code: -32602,
            message: message.into(),
        }
    }

    /// A JSON-RPC internal error (-32603).
    pub fn internal(message: impl Into<String>) -> Self {
        Self {
            code: -32603,
            message: message.into(),
        }
    }

    /// A "not found" error in the server-defined range.
    pub fn not_found(message: impl Into<String>) -> Self {
        Self {
            code: -32000,
            message: message.into(),
        }
    }
}

impl std::fmt::Display for APIError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} (code {})", self.message, self.code)
    }
}

impl std::error::Error for APIError {}

impl axum::response::IntoResponse for APIError {
    fn into_response(self) -> axum::response::Response {
        let status = match self.code {
            -32602 => axum::http::StatusCode::BAD_REQUEST,
            -32000 => axum::http::StatusCode::NOT_FOUND,
            _ => axum::http::StatusCode::INTERNAL_SERVER_ERROR,
        };

        (status, axum::Json(self)).into_response()
    }
}

/// Block Response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BlockResponse {
    pub number: u64,
    pub hash: String,
    pub parent_hash: String,
    pub timestamp: i64,
    pub transactions: Vec<String>,
    pub gas_used: u64,
    pub gas_limit: u64,
    pub miner: String,
    pub difficulty: String,
    pub total_difficulty: String,
    pub size: u64,
    pub nonce: String,
    pub mix_hash: String,
    pub extra_data: String,
}

/// Transaction Response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TransactionResponse {
    pub hash: String,
    pub block_number: Option<u64>,
    pub block_hash: Option<String>,
    pub from: String,
    pub to: Option<String>,
    pub value: String,
    pub gas_price: u64,
    pub gas_used: Option<u64>,
    pub nonce: u64,
    pub input: String,
    pub status: Option<String>,
}

/// Address Response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AddressResponse {
    pub address: String,
    pub balance: String,
    pub tx_count: i64,
    pub transactions: Vec<TransactionResponse>,
}

/// Token Response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenResponse {
    pub address: String,
    pub name: String,
    pub symbol: String,
    pub decimals: u8,
    pub total_supply: String,
    pub holders: i64,
    pub transfers: i64,
    pub price: Option<f64>,
    pub volume_24h: Option<f64>,
}

/// NFT Response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NFTResponse {
    pub address: String,
    pub token_id: String,
    pub owner: String,
    pub uri: String,
    pub metadata: Option<serde_json::Value>,
}

// =============================================================================
// LIST RESPONSES
// =============================================================================

/// Paginated Response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PaginatedResponse<T> {
    pub items: Vec<T>,
    pub total: usize,
    pub page: usize,
    pub limit: usize,
    pub has_more: bool,
}

impl<T> PaginatedResponse<T> {
    pub fn new(items: Vec<T>, total: usize, page: usize, limit: usize) -> Self {
        let has_more = (page * limit) < total;
        Self { items, total, page, limit, has_more }
    }
}

// =============================================================================
// STATS
// =============================================================================

/// API Stats
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct APIStats {
    pub uptime: i64,
    pub requests_total: i64,
    pub requests_by_endpoint: std::collections::HashMap<String, i64>,
    pub avg_response_time: f64,
    pub active_connections: i64,
}

// =============================================================================
// HELPERS
// =============================================================================

/// Parse pagination
pub fn parse_pagination(page: Option<usize>, limit: Option<usize>) -> (usize, usize) {
    let page = page.unwrap_or(0).max(0);
    let limit = limit.unwrap_or(50).min(100).max(1);
    (page, limit)
}

// =============================================================================
// TESTS
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pagination() {
        let (page, limit) = parse_pagination(Some(1), Some(50));
        assert_eq!(page, 1);
        assert_eq!(limit, 50);
    }

    #[test]
    fn test_response() {
        let resp: APIResponse<String> = APIResponse::ok("test".to_string());
        assert!(resp.result.is_some());
        assert!(resp.error.is_none());
    }
}