//! WalletConnect v2 Service
//! Web3 wallet integration for dApps

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use sha2::{Sha256, Digest};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WalletConnectConfig {
    pub project_id: String,
    pub relay_url: String,
    pub metadata: ClientMetadata,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClientMetadata {
    pub name: String,
    pub description: String,
    pub url: String,
    pub icons: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Session {
    pub topic: String,
    pub relay: String,
    pub peer_meta: Option<PeerMetadata>,
    pub accounts: Vec<String>,
    pub chain_id: u32,
    pub state: SessionState,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PeerMetadata {
    pub name: String,
    pub url: String,
    pub icons: Vec<String>,
    pub description: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionState {
    pub accounts: Vec<String>,
    pub chain_id: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConnectRequest {
    pub topic: Option<String>,
    pub params: ConnectParams,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConnectParams {
    pub peer_meta: ClientMetadata,
    pub chains: Option<Vec<u32>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Request {
    pub request: RequestMethod,
    pub chain_id: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "method")]
pub enum RequestMethod {
    #[serde(rename = "eth_requestAccounts")]
    RequestAccounts,
    #[serde(rename = "eth_accounts")]
    Accounts,
    #[serde(rename = "eth_chainId")]
    ChainId,
    #[serde(rename = "net_version")]
    NetVersion,
    #[serde(rename = "eth_blockNumber")]
    BlockNumber,
    #[serde(rename = "eth_getBalance")]
    GetBalance { params: Vec<String> },
    #[serde(rename = "eth_call")]
    Call { params: CallParams },
    #[serde(rename = "eth_sendTransaction")]
    SendTransaction { params: TransactionParams },
    #[serde(rename = "personal_sign")]
    PersonalSign { params: Vec<String> },
    #[serde(rename = "eth_signTypedData_v4")]
    SignTypedData { params: Vec<String> },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CallParams {
    pub to: String,
    pub data: Option<String>,
    pub value: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TransactionParams {
    pub from: String,
    pub to: String,
    pub value: Option<String>,
    pub data: Option<String>,
    pub gas: Option<String>,
    pub gas_price: Option<String>,
}

pub struct WalletConnectService {
    config: WalletConnectConfig,
    sessions: HashMap<String, Session>,
    pending_requests: HashMap<String, Vec<PendingRequest>>,
}

#[derive(Debug, Clone)]
pub struct PendingRequest {
    pub topic: String,
    pub method: String,
    pub params: serde_json::Value,
    pub created_at: i64,
}

impl WalletConnectService {
    pub fn new(config: WalletConnectConfig) -> Self {
        Self {
            config,
            sessions: HashMap::new(),
            pending_requests: HashMap::new(),
        }
    }

    /// Create new pairing proposal
    pub fn create_proposal(&self) -> String {
        let topic = self.generate_topic();
        let uri = format!("wc:{}@2?relayUrl={}&projectId={}", 
            topic, 
            self.config.relay_url,
            self.config.project_id
        );
        uri
    }

    /// Approve session
    pub fn approve_session(&mut self, topic: &str, accounts: Vec<String>, chain_id: u32) -> Result<Session, String> {
        let session = Session {
            topic: topic.to_string(),
            relay: self.config.relay_url.clone(),
            peer_meta: None,
            accounts: accounts.clone(),
            chain_id,
            state: SessionState { accounts, chain_id },
        };
        
        self.sessions.insert(topic.to_string(), session.clone());
        Ok(session)
    }

    /// Reject session
    pub fn reject_session(&mut self, topic: &str) {
        self.sessions.remove(topic);
    }

    /// Get session
    pub fn get_session(&self, topic: &str) -> Option<&Session> {
        self.sessions.get(topic)
    }

    /// Update session
    pub fn update_session(&mut self, topic: &str, accounts: Vec<String>, chain_id: u32) -> Result<Session, String> {
        let session = self.sessions.get_mut(topic)
            .ok_or("Session not found")?;
        
        session.accounts = accounts;
        session.chain_id = chain_id;
        session.state = SessionState { accounts: accounts.clone(), chain_id };
        
        Ok(session.clone())
    }

    /// Handle request
    pub fn handle_request(&mut self, topic: &str, request: Request) -> Result<serde_json::Value, String> {
        let session = self.sessions.get(topic)
            .ok_or("Session not found")?;
        
        match request.request {
            RequestMethod::RequestAccounts => {
                Ok(serde_json::json!(session.accounts))
            }
            RequestMethod::Accounts => {
                Ok(serde_json::json!(session.accounts))
            }
            RequestMethod::ChainId => {
                Ok(serde_json::json!(format!("0x{:x}", session.chain_id)))
            }
            RequestMethod::NetVersion => {
                Ok(serde_json::json!(session.chain_id.to_string()))
            }
            RequestMethod::BlockNumber => {
                // Would query from node
                Ok(serde_json::json!("0x12345"))
            }
            RequestMethod::GetBalance { params } => {
                // Would query from node
                Ok(serde_json::json!("0x0"))
            }
            RequestMethod::Call { params: _ } => {
                // Would execute on node
                Ok(serde_json::json!("0x"))
            }
            RequestMethod::SendTransaction { params: _ } => {
                // Would send via node
                Ok(serde_json::json!("0xtxhash"))
            }
            RequestMethod::PersonalSign { params } => {
                // Would sign message
                Ok(serde_json::json!("0xsignature"))
            }
            RequestMethod::SignTypedData { params } => {
                // Would sign typed data
                Ok(serde_json::json!("0xsignature"))
            }
        }
    }

    /// Generate topic
    fn generate_topic(&self) -> String {
        let mut hasher = Sha256::new();
        hasher.update(chrono::Utc::now().timestamp_nanos_opt().unwrap_or(0).to_string().as_bytes());
        hex::encode(hasher.finalize())[..32].to_string()
    }

    /// Get active sessions
    pub fn get_sessions(&self) -> Vec<&Session> {
        self.sessions.values().collect()
    }

    /// Delete session
    pub fn delete_session(&mut self, topic: &str) {
        self.sessions.remove(topic);
    }
}

// Event types
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionEvent {
    pub event: String,
    pub chain_id: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionUpdate {
    pub accounts: Vec<String>,
    pub chain_id: u32,
}

pub fn format_uri(config: &WalletConnectConfig, topic: &str) -> String {
    format!("wc:{}@2?relayUrl={}&projectId={}", 
        topic, 
        config.relay_url,
        config.project_id
    )
}

pub fn parse_uri(uri: &str) -> Option<(String, String, String)> {
    let uri = uri.strip_prefix("wc:")?;
    let parts: Vec<&str> = uri.split('@').collect();
    if parts.len() != 2 { return None; }
    let topic = parts[0].to_string();
    
    let query_part = parts[1];
    let mut relay_url = String::new();
    let mut project_id = String::new();
    
    for param in query_part.split('?').nth(1).unwrap_or("").split('&') {
        let kv: Vec<&str> = param.split('=').collect();
        if kv.len() != 2 { continue; }
        match kv[0] {
            "relayUrl" => relay_url = kv[1].to_string(),
            "projectId" => project_id = kv[1].to_string(),
            _ => {}
        }
    }
    
    Some((topic, relay_url, project_id))
}