//! JSON-RPC client used by the API handlers.
//!
//! Every response the API serves is produced by a call to the node over
//! JSON-RPC. Nothing is synthesised: when the node cannot answer, the handler
//! returns an error rather than a plausible-looking default.

use crate::types::APIError;
use serde::de::DeserializeOwned;
use serde_json::{json, Value};
use std::time::Duration;

/// Transport failures and JSON-RPC errors.
#[derive(Debug)]
pub enum RpcError {
    /// The request never reached the node.
    Transport(String),
    /// The node replied with a JSON-RPC error object.
    Node { code: i64, message: String },
    /// The reply did not have the shape the caller expected.
    Decode(String),
    /// No endpoint has been configured.
    NotConfigured,
}

impl std::fmt::Display for RpcError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RpcError::Transport(message) => write!(f, "node unreachable: {message}"),
            RpcError::Node { code, message } => write!(f, "node error {code}: {message}"),
            RpcError::Decode(message) => write!(f, "unexpected reply: {message}"),
            RpcError::NotConfigured => write!(f, "no RPC endpoint configured"),
        }
    }
}

impl std::error::Error for RpcError {}

impl From<RpcError> for APIError {
    fn from(error: RpcError) -> Self {
        let code = match &error {
            RpcError::Node { code, .. } => *code as i32,
            RpcError::NotConfigured => -32000,
            RpcError::Transport(_) => -32001,
            RpcError::Decode(_) => -32603,
        };
        APIError {
            code,
            message: error.to_string(),
        }
    }
}

/// A JSON-RPC client for the TigerSmartChain node.
#[derive(Clone)]
pub struct RpcClient {
    url: String,
    client: reqwest::Client,
}

impl RpcClient {
    /// Build a client for `url`.
    pub fn new(url: impl Into<String>) -> Result<Self, RpcError> {
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(30))
            .build()
            .map_err(|e| RpcError::Transport(e.to_string()))?;

        Ok(Self {
            url: url.into(),
            client,
        })
    }

    /// Issue a JSON-RPC call and decode the `result` field into `T`.
    pub async fn call<T: DeserializeOwned>(
        &self,
        method: &str,
        params: Value,
    ) -> Result<T, RpcError> {
        let value = self.call_raw(method, params).await?;

        serde_json::from_value(value).map_err(|e| RpcError::Decode(e.to_string()))
    }

    /// Issue a JSON-RPC call and return the raw `result` value.
    pub async fn call_raw(&self, method: &str, params: Value) -> Result<Value, RpcError> {
        let body = json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": method,
            "params": params,
        });

        let response = self
            .client
            .post(&self.url)
            .json(&body)
            .send()
            .await
            .map_err(|e| RpcError::Transport(e.to_string()))?;

        let payload: Value = response
            .json()
            .await
            .map_err(|e| RpcError::Decode(e.to_string()))?;

        if let Some(error) = payload.get("error") {
            return Err(RpcError::Node {
                code: error.get("code").and_then(Value::as_i64).unwrap_or(-32603),
                message: error
                    .get("message")
                    .and_then(Value::as_str)
                    .unwrap_or("unknown error")
                    .to_string(),
            });
        }

        payload
            .get("result")
            .cloned()
            .ok_or_else(|| RpcError::Decode("reply had neither result nor error".to_string()))
    }

    /// The endpoint this client talks to.
    pub fn url(&self) -> &str {
        &self.url
    }
}
