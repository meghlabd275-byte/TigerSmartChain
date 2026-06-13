//! TigerScan Contract Verification Service Main
//! High-performance API server with rate limiting and encryption

#![forbid(unsafe_code)]

use std::net::SocketAddr;
use std::sync::Arc;

use contract_verification_service::{
    BatchVerifyRequest, BatchVerifyResponse, CompilerSettings, Config,
    SourceFile, VerifyRequest, VerifyResponse, VerificationResult,
    VerificationService, VerificationStatus,
};
use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use tiny_http::{Header, Response, Server, StatusCode};
use tracing::{error, info, Level};
use tracing_subscriber::FmtSubscriber;

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
    pub ips: std::collections::HashMap<String, Vec<std::time::Instant>>,
    pub keys: std::collections::HashMap<String, Vec<std::time::Instant>>,
}

#[derive(Debug, Clone)]
pub struct RateLimitConfig {
    pub requests_per_minute: u32,
    pub requests_per_hour: u32,
    pub burst_size: u32,
}

impl RateLimiter {
    pub fn new() -> Self {
        Self {
            requests: Arc::new(RwLock::new(RateLimitStore::default())),
            config: RateLimitConfig {
                requests_per_minute: 60,
                requests_per_hour: 1000,
                burst_size: 10,
            },
        }
    }

    pub fn check(&self, identifier: &str) -> Result<(), String> {
        let now = std::time::Instant::now();
        let store = self.requests.read();
        
        // Get existing requests
        let requests = store.ips.get(identifier)
            .cloned()
            .unwrap_or_default();
        
        // Filter to last minute
        let last_minute: Vec<_> = requests.iter()
            .filter(|t| now.duration_since(**t).as_secs() < 60)
            .collect();
        
        // Check burst
        let recent_count = last_minute.len() as u32;
        if recent_count >= self.config.burst_size {
            return Err("Rate limit exceeded - burst".to_string());
        }
        
        // Check per minute
        if recent_count >= self.config.requests_per_minute {
            return Err("Rate limit exceeded - per minute".to_string());
        }
        
        // Check per hour
        let last_hour: Vec<_> = requests.iter()
            .filter(|t| now.duration_since(**t).as_secs() < 3600)
            .collect();
        
        if last_hour.len() as u32 >= self.config.requests_per_hour {
            return Err("Rate limit exceeded - per hour".to_string());
        }
        
        Ok(())
    }

    pub fn record(&self, identifier: &str) {
        let now = std::time::Instant::now();
        let mut store = self.requests.write();
        
        let requests = store.ips.entry(identifier.to_string())
            .or_insert_with(Vec::new);
        
        requests.push(now);
        
        // Clean old entries
        let cutoff = std::time::Instant::now()
            .checked_sub(std::time::Duration::from_secs(3600))
            .unwrap_or(now);
        
        requests.retain(|t| *t > cutoff);
    }
}

// ============================================================================
// Encryption
// ============================================================================

pub struct Encryptor {
    key: [u8; 32],
}

impl Encryptor {
    pub fn new(key: Option<String>) -> Result<Self, String> {
        let key = key.unwrap_or_else(|| {
            // Generate random key if not provided
            use rand::RngCore;
            let mut key = [0u8; 32];
            rand::thread_rng().fill_bytes(&mut key);
            hex::encode(key)
        });
        
        let key = if key.len() == 64 {
            hex::decode(&key).map_err(|e| e.to_string())?
        } else {
            // Hash the key to get 32 bytes
            use sha2::{Sha256, Digest};
            let mut hasher = Sha256::new();
            hasher.update(key.as_bytes());
            hasher.finalize().to_vec()
        };
        
        let mut key_arr = [0u8; 32];
        key_arr.copy_from_slice(&key[..32]);
        
        Ok(Self { key: key_arr })
    }

    pub fn encrypt(&self, plaintext: &str) -> Result<String, String> {
        use aes_gcm::{
            aead::{Aead, KeyInit},
            Aes256Gcm, Nonce,
        };
        use rand::RngCore;
        
        let cipher = Aes256Gcm::new(&self.key.into());
        
        let mut nonce_bytes = [0u8; 12];
        rand::thread_rng().fill_bytes(&mut nonce_bytes);
        let nonce = Nonce::from_slice(&nonce_bytes);
        
        let ciphertext = cipher.encrypt(nonce, plaintext.as_bytes())
            .map_err(|e| e.to_string())?;
        
        let mut result = nonce_bytes.to_vec();
        result.extend(ciphertext);
        
        Ok(base64::encode(&result))
    }

    pub fn decrypt(&self, encrypted: &str) -> Result<String, String> {
        use aes_gcm::{
            aead::{Aead, KeyInit},
            Aes256Gcm, Nonce,
        };
        
        let data = base64::decode(encrypted).map_err(|e| e.to_string())?;
        
        if data.len() < 12 {
            return Err("Invalid encrypted data".to_string());
        }
        
        let (nonce_bytes, ciphertext) = data.split_at(12);
        let nonce = Nonce::from_slice(nonce_bytes);
        
        let cipher = Aes256Gcm::new(&self.key.into());
        
        let plaintext = cipher.decrypt(nonce, ciphertext)
            .map_err(|e| e.to_string())?;
        
        String::from_utf8(plaintext).map_err(|e| e.to_string())
    }
}

// ============================================================================
// Request Handler
// ============================================================================

pub struct Handler {
    service: VerificationService,
    rate_limiter: RateLimiter,
    encryptor: Option<Encryptor>,
}

impl Handler {
    pub fn new(service: VerificationService, encryptor: Option<Encryptor>) -> Self {
        Self {
            service,
            rate_limiter: RateLimiter::new(),
            encryptor,
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
        
        // Handle method
        let result = match request.method.as_str() {
            "eth_verifyContract" => self.handle_verify(request.params).await,
            "eth_verifyBatch" => self.handle_batch_verify(request.params).await,
            "eth_getVerificationStatus" => self.handle_get_status(request.params).await,
            "eth_getSource" => self.handle_get_source(request.params).await,
            _ => {
                Err(format!("Unknown method: {}", request.method))
            }
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

    async fn handle_verify(&self, params: Option<serde_json::Value>) -> Result<VerifyResponse, String> {
        let params: VerifyRequest = params
            .and_then(|p| serde_json::from_value(p).ok())
            .ok_or("Invalid params")?;
        
        let settings = params.settings.unwrap_or_default();
        
        let request = contract_verification_service::VerificationRequest {
            id: uuid::Uuid::new_v4().to_string(),
            address: params.address.clone(),
            chain_id: params.chain_id,
            compiler_version: params.compiler_version.clone(),
            source_files: params.source_files.clone(),
            settings,
            constructor_arguments: params.constructor_arguments,
            optimization_enabled: true,
            optimization_runs: 200,
            evm_version: "paris".to_string(),
            license_type: "MIT".to_string(),
            requester: None,
            status: VerificationStatus::Pending,
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
            error_message: None,
        };
        
        // Verify
        let result = self.service.verify(request).await;
        
        match result {
            Ok(result) => Ok(VerifyResponse {
                id: result.id.clone(),
                status: VerificationStatus::Verified,
                message: "Contract verified successfully".to_string(),
                result: Some(result),
            }),
            Err(e) => Ok(VerifyResponse {
                id: params.address.clone(),
                status: VerificationStatus::Failed,
                message: e.to_string(),
                result: None,
            }),
        }
    }

    async fn handle_batch_verify(&self, params: Option<serde_json::Value>) -> Result<BatchVerifyResponse, String> {
        let params: BatchVerifyRequest = params
            .and_then(|p| serde_json::from_value(p).ok())
            .ok_or("Invalid params")?;
        
        let mut results = Vec::new();
        
        for request in params.requests {
            let result = self.handle_verify(Some(serde_json::to_value(request).unwrap())).await;
            results.push(result.unwrap_or_else(|e| VerifyResponse {
                id: "".to_string(),
                status: VerificationStatus::Failed,
                message: e,
                result: None,
            }));
        }
        
        Ok(BatchVerifyResponse { results })
    }

    async fn handle_get_status(&self, params: Option<serde_json::Value>) -> Result<VerificationStatus, String> {
        let params = params.ok_or("Missing params")?;
        
        let address = params.get("address")
            .and_then(|a| a.as_str())
            .ok_or("Missing address")?;
        
        let chain_id = params.get("chain_id")
            .and_then(|c| c.as_u64())
            .unwrap_or(1);
        
        // Check cache
        if let Some(result) = self.service.get_cached(address, chain_id) {
            return Ok(if result.is_verified {
                VerificationStatus::Verified
            } else {
                VerificationStatus::Failed
            });
        }
        
        Ok(VerificationStatus::Pending)
    }

    async fn handle_get_source(&self, params: Option<serde_json::Value>) -> Result<Vec<SourceFile>, String> {
        let params = params.ok_or("Missing params")?;
        
        let address = params.get("address")
            .and_then(|a| a.as_str())
            .ok_or("Missing address")?;
        
        let chain_id = params.get("chain_id")
            .and_then(|c| c.as_u64())
            .unwrap_or(1);
        
        // Try Sourcify
        let client = contract_verification_service::SourcifyClient::new(None);
        client.get_sources(address, chain_id).await?
            .ok_or("Source not found")
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
    
    info!("Starting Contract Verification Service");
    
    // Load configuration
    let config = Config::default();
    
    // Initialize encryptor
    let encryptor = match &config.encryption_key {
        Some(key) => Some(Encryptor::new(Some(key.clone()))?),
        None => None,
    };
    
    // Create service
    let service = VerificationService::new(config).await?;
    
    // Create handler
    let handler = Arc::new(Handler::new(service, encryptor));
    
    // Start HTTP server
    let addr = SocketAddr::from(([0, 0, 0, 0], 8545));
    let server = Server::http(addr)?;
    
    info!("Server listening on {}", addr);
    
    for request in server.incoming_requests() {
        let handler = handler.clone();
        
        tokio::spawn(async move {
            let ip = request.remote_addr()
                .map(|a| a.ip().to_string())
                .unwrap_or_else(|| "unknown".to_string());
            
            let body = request.as_vec();
            
            let response = handler.handle(&body, &ip);
            
            let mut resp = request.respond(response)?;
            
            // Add headers
            let headers = vec![
                Header::from_bytes(&b"Content-Type"[..], &b"application/json"[..]).unwrap(),
                Header::from_bytes(&b"X-Content-Type-Options"[..], &b"nosniff"[..]).unwrap(),
                Header::from_bytes(&b"X-Frame-Options"[..], &b"DENY"[..]).unwrap(),
                Header::from_bytes(&b"X-XSS-Protection"[..], &b"1; mode=block"[..]).unwrap(),
                Header::from_bytes(&b"Strict-Transport-Security"[..], &b"max-age=31536000; includeSubDomains"[..]).unwrap(),
            ];
            
            for header in headers {
                resp.add_header(header);
            }
            
            Ok(())
        });
    }
    
    Ok(())
}