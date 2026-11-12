//! Decompiler service: fetches runtime bytecode over JSON-RPC and analyses it.

use ethers::providers::{Http, Middleware, Provider};
use ethers::types::Address;
use serde::{Deserialize, Serialize};
use tracing::{info, warn};

use crate::analysis::{decode_bytecode, Analyzer};
use crate::patterns::PatternDetector;
use crate::types::{Config, DecompiledContract};

/// A JSON-RPC request from the API surface.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiRequest {
    pub method: String,
    pub params: Option<serde_json::Value>,
}

/// A JSON-RPC response.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiResponse {
    pub success: bool,
    pub result: Option<serde_json::Value>,
    pub error: Option<String>,
}

impl ApiResponse {
    pub fn ok(result: serde_json::Value) -> Self {
        Self {
            success: true,
            result: Some(result),
            error: None,
        }
    }

    pub fn error(message: impl Into<String>) -> Self {
        Self {
            success: false,
            result: None,
            error: Some(message.into()),
        }
    }
}

/// The decompiler service.
pub struct Server {
    config: Config,
    provider: Provider<Http>,
}

impl Server {
    /// Build a server bound to the configured RPC endpoint.
    pub fn new(config: Config) -> Result<Self, String> {
        let provider = Provider::<Http>::try_from(config.rpc_url.as_str())
            .map_err(|error| format!("invalid RPC URL {}: {error}", config.rpc_url))?;

        Ok(Self { config, provider })
    }

    /// The active configuration.
    pub fn config(&self) -> &Config {
        &self.config
    }

    /// Fetch the deployed runtime bytecode for an address.
    pub async fn fetch_code(&self, address: &str) -> Result<String, String> {
        let address: Address = address
            .parse()
            .map_err(|error| format!("invalid address {address}: {error}"))?;

        let code = self
            .provider
            .get_code(address, None)
            .await
            .map_err(|error| format!("eth_getCode failed: {error}"))?;

        if code.is_empty() {
            return Err(format!("no contract deployed at {address:?}"));
        }

        Ok(format!("0x{}", hex::encode(code.as_ref())))
    }

    /// Fetch and decompile a deployed contract.
    pub async fn decompile(&self, address: &str) -> Result<DecompiledContract, String> {
        let bytecode = self.fetch_code(address).await?;

        Ok(self.decompile_bytecode(address, &bytecode))
    }

    /// Analyse a bytecode blob that the caller already holds.
    pub fn decompile_bytecode(&self, address: &str, bytecode: &str) -> DecompiledContract {
        let analysis = Analyzer::analyze(bytecode);
        let contract_type = PatternDetector::detect(bytecode);

        info!(
            address,
            functions = analysis.functions.len(),
            slots = analysis.variables.len(),
            events = analysis.events.len(),
            contract_type = contract_type.as_str(),
            "decompiled contract"
        );

        DecompiledContract {
            address: address.to_string(),
            bytecode: bytecode.to_string(),
            contract_type: contract_type.as_str().to_string(),
            functions: analysis.functions,
            variables: analysis.variables,
            events: analysis.events,
            is_minimal_proxy: analysis.is_minimal_proxy,
            is_proxy: analysis.is_proxy,
            has_metadata: analysis.has_metadata,
            runtime_size: analysis.runtime_size,
        }
    }

    /// Detect the contract type of a bytecode blob.
    pub fn detect_type(&self, bytecode: &str) -> String {
        PatternDetector::detect(bytecode).as_str().to_string()
    }

    /// Handle one API request.
    pub async fn handle(&self, body: &[u8]) -> ApiResponse {
        let request: ApiRequest = match serde_json::from_slice(body) {
            Ok(request) => request,
            Err(error) => return ApiResponse::error(format!("parse error: {error}")),
        };

        match request.method.as_str() {
            "decompile" => match self.handle_decompile(request.params).await {
                Ok(value) => ApiResponse::ok(value),
                Err(error) => ApiResponse::error(error),
            },
            "detectType" => match self.handle_detect_type(request.params) {
                Ok(value) => ApiResponse::ok(value),
                Err(error) => ApiResponse::error(error),
            },
            other => ApiResponse::error(format!("unknown method: {other}")),
        }
    }

    async fn handle_decompile(
        &self,
        params: Option<serde_json::Value>,
    ) -> Result<serde_json::Value, String> {
        let params = params.ok_or("missing params")?;

        let address = params
            .get("address")
            .and_then(|value| value.as_str())
            .ok_or("missing address")?;

        let contract = match params.get("bytecode").and_then(|value| value.as_str()) {
            Some(bytecode) => self.decompile_bytecode(address, bytecode),
            None => self.decompile(address).await?,
        };

        serde_json::to_value(contract).map_err(|error| error.to_string())
    }

    fn handle_detect_type(
        &self,
        params: Option<serde_json::Value>,
    ) -> Result<serde_json::Value, String> {
        let params = params.ok_or("missing params")?;

        let bytecode = params
            .get("bytecode")
            .and_then(|value| value.as_str())
            .ok_or("missing bytecode")?;

        // A malformed blob must not be reported as a contract of unknown type.
        if decode_bytecode(bytecode).is_none() {
            warn!("rejected bytecode that is not valid hex");

            return Err("bytecode is not valid hex".to_string());
        }

        Ok(serde_json::json!({ "contractType": self.detect_type(bytecode) }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn server() -> Server {
        Server::new(Config {
            rpc_url: "http://localhost:8545".to_string(),
            max_iterations: 10000,
            timeout_ms: 30000,
        })
        .expect("a local URL is valid")
    }

    #[tokio::test]
    async fn decompile_uses_the_supplied_bytecode() {
        let server = server();
        // PUSH4 a9059cbb ; EQ ; PUSH2 0x0100 ; JUMPI ; STOP
        let contract = server.decompile_bytecode(
            "0x0000000000000000000000000000000000000001",
            "0x63a9059cbb146101005700",
        );

        assert_eq!(contract.functions.len(), 1);
        assert_eq!(contract.functions[0].signature, "transfer(address,uint256)");
        assert_eq!(contract.runtime_size, 11);
    }

    #[tokio::test]
    async fn an_unknown_method_is_reported_as_an_error() {
        let server = server();
        let body = br#"{"method":"nope"}"#;

        let response = server.handle(body).await;

        assert!(!response.success);
        assert!(response.error.unwrap().contains("unknown method"));
    }

    #[tokio::test]
    async fn malformed_json_is_rejected() {
        let server = server();

        let response = server.handle(b"not json").await;

        assert!(!response.success);
        assert!(response.error.unwrap().contains("parse error"));
    }

    #[tokio::test]
    async fn non_hex_bytecode_is_rejected_by_detect_type() {
        let server = server();
        let body = br#"{"method":"detectType","params":{"bytecode":"0xzz"}}"#;

        let response = server.handle(body).await;

        assert!(!response.success);
        assert!(response.error.unwrap().contains("valid hex"));
    }

    #[tokio::test]
    async fn detect_type_reports_an_erc20() {
        let server = server();
        let body = br#"{"method":"detectType","params":{"bytecode":"63a9059cbb14610100570063 23b872dd 14610100570063095ea7b31461010057006370a0823114610100570063dd62ed3e1461010057006318160ddd146101005700"}}"#;

        let response = server.handle(body).await;

        assert!(response.success);
        assert_eq!(
            response.result.unwrap()["contractType"],
            "ERC-20 Token"
        );
    }

    #[tokio::test]
    async fn a_bad_rpc_url_is_rejected_at_construction() {
        let result = Server::new(Config {
            rpc_url: "not a url".to_string(),
            max_iterations: 10,
            timeout_ms: 10,
        });

        assert!(result.is_err());
    }

    #[tokio::test]
    async fn an_invalid_address_is_rejected_before_the_rpc_call() {
        let server = server();

        let error = server.fetch_code("0xnothex").await.unwrap_err();

        assert!(error.contains("invalid address"));
    }
}
