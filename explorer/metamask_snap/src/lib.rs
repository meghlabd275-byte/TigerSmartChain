//! MetaMask Snap Service
//! Snap for wallet integration with TigerScan

use serde::{Deserialize, Serialize};
use sha2::{Sha256, Digest};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SnapConfig {
    pub snap_id: String,
    pub version: String,
    pub rpc_url: String,
    pub chain_id: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SnapManifest {
    pub version: String,
    pub name: String,
    pub description: String,
    pub proposed_name: String,
    pub source: SnapSource,
    pub initialPermissions: Permissions,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SnapSource {
    pub shasum: String,
    pub url: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Permissions {
    pub eth_accounts: PermissionObject,
    pub eth_blockchain: PermissionObject,
    pub eth_call: PermissionObject,
    pub eth_sendTransaction: PermissionObject,
    pub eth_signTypedData_v4: PermissionObject,
    pub personal_sign: PermissionObject,
    pub net_version: PermissionObject,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PermissionObject {
    #[serde(rename = "enabled")]
    pub enabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SnapRPCRequest {
    pub method: String,
    pub params: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SnapRPCResponse {
    pub result: Option<serde_json::Value>,
    pub error: Option<String>,
}

pub struct MetaMaskSnap {
    config: SnapConfig,
}

impl MetaMaskSnap {
    pub fn new(config: SnapConfig) -> Self {
        Self { config }
    }

    /// Generate manifest.json
    pub fn generate_manifest(&self) -> SnapManifest {
        SnapManifest {
            version: self.config.version.clone(),
            name: "TigerScan".to_string(),
            description: "TigerScan Blockchain Explorer Snap".to_string(),
            proposed_name: "tigerscan".to_string(),
            source: SnapSource {
                shasum: "abc123".to_string(),
                url: "https://tigerscan.io/snap/index.js".to_string(),
            },
            initialPermissions: Permissions {
                eth_accounts: PermissionObject { enabled: true },
                eth_blockchain: PermissionObject { enabled: true },
                eth_call: PermissionObject { enabled: true },
                eth_sendTransaction: PermissionObject { enabled: true },
                eth_signTypedData_v4: PermissionObject { enabled: true },
                personal_sign: PermissionObject { enabled: true },
                net_version: PermissionObject { enabled: true },
            },
        }
    }

    /// Handle RPC request
    pub fn handle_rpc(&self, request: SnapRPCRequest) -> SnapRPCResponse {
        match request.method.as_str() {
            "tigerscan_getAddressBalance" => {
                let result = self.get_balance(&request.params);
                SnapRPCResponse { result: Some(result), error: None }
            }
            "tigerscan_getTransactions" => {
                let result = self.get_transactions(&request.params);
                SnapRPCResponse { result: Some(result), error: None }
            }
            "tigerscan_getTokenBalance" => {
                let result = self.get_token_balance(&request.params);
                SnapRPCResponse { result: Some(result), error: None }
            }
            "tigerscan_getTokenTransfers" => {
                let result = self.get_token_transfers(&request.params);
                SnapRPCResponse { result: Some(result), error: None }
            }
            "tigerscan_getNFTs" => {
                let result = self.get_nfts(&request.params);
                SnapRPCResponse { result: Some(result), error: None }
            }
            "tigerscan_simulateTransaction" => {
                let result = self.simulate_tx(&request.params);
                SnapRPCResponse { result: Some(result), error: None }
            }
            "tigerscan_checkPhishing" => {
                let result = self.check_phishing(&request.params);
                SnapRPCResponse { result: Some(result), error: None }
            }
            "tigerscan_getGasPrice" => {
                let result = self.get_gas_price();
                SnapRPCResponse { result: Some(result), error: None }
            }
            _ => SnapRPCResponse { result: None, error: Some("Method not found".to_string()) },
        }
    }

    fn get_balance(&self, params: &serde_json::Value) -> serde_json::Value {
        let address = params.get("address").and_then(|v| v.as_str()).unwrap_or("0x0000000000000000000000000000000000000000");
        serde_json::json!({ "address": address, "balance": "0" })
    }

    fn get_transactions(&self, params: &serde_json::Value) -> serde_json::Value {
        let address = params.get("address").and_then(|v| v.as_str()).unwrap_or("");
        let limit = params.get("limit").and_then(|v| v.as_u64()).unwrap_or(10) as usize;
        serde_json::json!({ "transactions": [], "address": address, "count": limit })
    }

    fn get_token_balance(&self, params: &serde_json::Value) -> serde_json::Value {
        let address = params.get("address").and_then(|v| v.as_str()).unwrap_or("");
        let token = params.get("token").and_then(|v| v.as_str()).unwrap_or("");
        serde_json::json!({ "address": address, "token": token, "balance": "0" })
    }

    fn get_token_transfers(&self, params: &serde_json::Value) -> serde_json::Value {
        let address = params.get("address").and_then(|v| v.as_str()).unwrap_or("");
        serde_json::json!({ "transfers": [], "address": address })
    }

    fn get_nfts(&self, params: &serde_json::Value) -> serde_json::Value {
        let address = params.get("address").and_then(|v| v.as_str()).unwrap_or("");
        serde_json::json!({ "nfts": [], "address": address })
    }

    fn simulate_tx(&self, params: &serde_json::Value) -> serde_json::Value {
        let tx = params.get("transaction").cloned();
        serde_json::json!({
            "success": true,
            "gasUsed": "21000",
            "stateChanges": [],
            "transaction": tx
        })
    }

    fn check_phishing(&self, params: &serde_json::Value) -> serde_json::Value {
        let address = params.get("address").and_then(|v| v.as_str()).unwrap_or("");
        serde_json::json!({
            "address": address,
            "isMalicious": false,
            "riskScore": 0
        })
    }

    fn get_gas_price(&self) -> serde_json::Value {
        serde_json::json!({
            "slow": "10000000000",
            "average": "20000000000",
            "fast": "50000000000"
        })
    }
}

// Snap API exposed methods
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SnapMethod {
    pub name: String,
    pub description: String,
    pub parameters: Vec<Parameter>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Parameter {
    pub name: String,
    pub param_type: String,
    pub required: bool,
}

pub fn get_snap_methods() -> Vec<SnapMethod> {
    vec![
        SnapMethod { name: "tigerscan_getAddressBalance".to_string(), description: "Get ETH balance for address".to_string(), parameters: vec![Parameter { name: "address".to_string(), param_type: "string".to_string(), required: true }] },
        SnapMethod { name: "tigerscan_getTransactions".to_string(), description: "Get transaction history".to_string(), parameters: vec![Parameter { name: "address".to_string(), param_type: "string".to_string(), required: true }] },
        SnapMethod { name: "tigerscan_getTokenBalance".to_string(), description: "Get ERC-20 token balance".to_string(), parameters: vec![Parameter { name: "address".to_string(), param_type: "string".to_string(), required: true }, Parameter { name: "token".to_string(), param_type: "string".to_string(), required: true }] },
        SnapMethod { name: "tigerscan_getTokenTransfers".to_string(), description: "Get token transfer history".to_string(), parameters: vec![Parameter { name: "address".to_string(), param_type: "string".to_string(), required: true }] },
        SnapMethod { name: "tigerscan_getNFTs".to_string(), description: "Get NFTs owned by address".to_string(), parameters: vec![Parameter { name: "address".to_string(), param_type: "string".to_string(), required: true }] },
        SnapMethod { name: "tigerscan_simulateTransaction".to_string(), description: "Simulate a transaction before sending".to_string(), parameters: vec![Parameter { name: "transaction".to_string(), param_type: "object".to_string(), required: true }] },
        SnapMethod { name: "tigerscan_checkPhishing".to_string(), description: "Check if address is malicious".to_string(), parameters: vec![Parameter { name: "address".to_string(), param_type: "string".to_string(), required: true }] },
        SnapMethod { name: "tigerscan_getGasPrice".to_string(), description: "Get current gas prices".to_string(), parameters: vec![] },
    ]
}