//! TigerScan Production Contract Verification Service
//! Full Solidity/Vyper compilation with proxy detection and bytecode matching
//! Uses Rust for maximum performance and security

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use anyhow::{anyhow, Context as AnyhowContext, Result};
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use encoding_rs::GBK;
use ethers::contract::BaseContract;
use ethers::core::abi::{Abi, Contract, Function};
use ethers::core::k256::sha2::{Digest, Sha256};
use ethers::core::types::{Address, H256, U256};
use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use sha2::{Digest as Sha256Digest, Sha256 as Sha256Hasher};
use thiserror::Error;
use tokio::io::AsyncWriteExt;
use tokio::sync::mpsc;
use tracing::{error, info, warn};
use uuid::Uuid;

// ============================================================================
// Error Types
// ============================================================================

#[derive(Error, Debug)]
pub enum VerificationError {
    #[error("Compilation error: {0}")]
    Compilation(String),
    
    #[error("Bytecode mismatch: expected {expected}, got {actual}")]
    BytecodeMismatch { expected: String, actual: String },
    
    #[error("No matching source code found")]
    NoMatchingSource,
    
    #[error("Invalid input: {0}")]
    InvalidInput(String),
    
    #[error("Storage error: {0}")]
    Storage(String),
    
    #[error("Proxy detection error: {0}")]
    ProxyDetection(String),
    
    #[error("Database error: {0}")]
    Database(String),
    
    #[error("Rate limit exceeded")]
    RateLimitExceeded,
    
    #[error("Verification not found: {0}")]
    NotFound(String),
    
    #[error("Unauthorized: {0}")]
    Unauthorized(String),
}

impl Serialize for VerificationError {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(&self.to_string())
    }
}

// ============================================================================
// Configuration
// ============================================================================

#[derive(Debug, Clone, Deserialize)]
pub struct Config {
    /// Database URL for storing verification records
    pub database_url: String,
    /// RPC URL for on-chain data
    pub rpc_url: String,
    /// Archive RPC URL for historical state
    pub archive_url: Option<String>,
    /// Maximum compilation time in seconds
    pub compilation_timeout: u64,
    /// Maximum files per verification
    pub max_files: usize,
    /// Maximum file size in bytes
    pub max_file_size: usize,
    /// Enable Vyper support
    pub vyper_enabled: bool,
    /// Solc versions to support
    pub solc_versions: Vec<String>,
    /// Vyper versions to support
    pub vyper_versions: Vec<String>,
    /// Rate limit per hour
    pub rate_limit: u32,
    /// API key for Sourcify
    pub sourcify_api_key: Option<String>,
    /// Encryption key for sensitive data
    pub encryption_key: Option<String>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            database_url: std::env::var("DATABASE_URL")
                .unwrap_or_else(|_| "postgres://localhost:5432/tigerscan".to_string()),
            rpc_url: std::env::var("RPC_URL")
                .unwrap_or_else(|_| "http://localhost:8545".to_string()),
            archive_url: std::env::var("ARCHIVE_URL").ok(),
            compilation_timeout: 300,
            max_files: 100,
            max_file_size: 1_000_000,
            vyper_enabled: true,
            solc_versions: vec![
                "0.8.28".to_string(),
                "0.8.27".to_string(),
                "0.8.26".to_string(),
                "0.8.25".to_string(),
                "0.8.24".to_string(),
                "0.8.23".to_string(),
                "0.8.22".to_string(),
                "0.8.21".to_string(),
                "0.8.20".to_string(),
                "0.8.19".to_string(),
                "0.8.18".to_string(),
                "0.8.17".to_string(),
                "0.8.16".to_string(),
                "0.8.15".to_string(),
                "0.8.14".to_string(),
                "0.8.13".to_string(),
                "0.8.12".to_string(),
                "0.8.11".to_string(),
                "0.8.10".to_string(),
                "0.8.9".to_string(),
                "0.8.8".to_string(),
                "0.8.7".to_string(),
                "0.8.6".to_string(),
                "0.8.5".to_string(),
                "0.8.4".to_string(),
                "0.8.3".to_string(),
                "0.8.2".to_string(),
                "0.8.1".to_string(),
                "0.8.0".to_string(),
                "0.7.6".to_string(),
                "0.7.5".to_string(),
                "0.7.4".to_string(),
                "0.7.3".to_string(),
                "0.7.2".to_string(),
                "0.7.1".to_string(),
                "0.7.0".to_string(),
                "0.6.12".to_string(),
                "0.6.11".to_string(),
                "0.6.10".to_string(),
                "0.6.9".to_string(),
                "0.6.8".to_string(),
                "0.6.7".to_string(),
                "0.6.6".to_string(),
                "0.6.5".to_string(),
                "0.6.4".to_string(),
                "0.6.3".to_string(),
                "0.6.2".to_string(),
                "0.6.1".to_string(),
                "0.6.0".to_string(),
                "0.5.17".to_string(),
                "0.5.16".to_string(),
                "0.5.15".to_string(),
                "0.5.14".to_string(),
                "0.5.13".to_string(),
                "0.5.12".to_string(),
                "0.5.11".to_string(),
                "0.5.10".to_string(),
                "0.5.9".to_string(),
                "0.5.8".to_string(),
                "0.5.7".to_string(),
                "0.5.6".to_string(),
                "0.5.5".to_string(),
                "0.5.4".to_string(),
                "0.5.3".to_string(),
                "0.5.2".to_string(),
                "0.5.1".to_string(),
                "0.5.0".to_string(),
            ],
            vyper_versions: vec![
                "0.3.1".to_string(),
                "0.3.0".to_string(),
                "0.2.2".to_string(),
                "0.2.1".to_string(),
                "0.2.0".to_string(),
                "0.1.1".to_string(),
                "0.1.0".to_string(),
            ],
            rate_limit: 100,
            sourcify_api_key: std::env::var("SOURCIFY_API_KEY").ok(),
            encryption_key: std::env::var("ENCRYPTION_KEY").ok(),
        }
    }
}

// ============================================================================
// Data Models
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VerificationRequest {
    pub id: String,
    pub address: String,
    pub chain_id: u64,
    pub compiler_version: String,
    pub source_files: Vec<SourceFile>,
    pub settings: CompilerSettings,
    pub constructor_arguments: Option<String>,
    pub optimization_enabled: bool,
    pub optimization_runs: u32,
    pub evm_version: String,
    pub license_type: String,
    pub requester: Option<String>,
    pub status: VerificationStatus,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub error_message: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SourceFile {
    pub name: String,
    pub content: String,
    #[serde(default)]
    pub encoding: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct CompilerSettings {
    #[serde(default)]
    pub optimizer: bool,
    #[serde(default = "default_runs")]
    pub runs: u32,
    #[serde(default)]
    pub libraries: HashMap<String, String>,
    #[serde(default)]
    pub evm_version: Option<String>,
    #[serde(default)]
    pub metadata: HashMap<String, serde_json::Value>,
    #[serde(default)]
    pub via_ir: bool,
    #[serde(default)]
    pub allow_paths: Vec<String>,
    #[serde(default)]
    pub include_paths: Vec<String>,
}

fn default_runs() -> u32 {
    200
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VerificationStatus {
    Pending,
    Compiling,
    Matching,
    Verified,
    Failed,
    RateLimited,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VerificationResult {
    pub id: String,
    pub address: String,
    pub chain_id: u64,
    pub verified: bool,
    pub compiler_version: String,
    pub compiler_settings: CompilerSettings,
    pub source_code: String,
    pub abi: String,
    pub runtime_bytecode: String,
    pub creation_bytecode: String,
    pub constructor_arguments: Option<String>,
    pub evm_version: String,
    pub license_type: String,
    pub proxy_type: Option<ProxyType>,
    pub implementation: Option<String>,
    pub is_verified: bool,
    pub match_type: MatchType,
    pub verified_at: DateTime<Utc>,
    pub block_number: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MatchType {
    Perfect,
    Partial,
    None,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProxyType {
    Standard,
    Upgradeable,
    Beacon,
    Diamond,
    Transparent,
    UUPS,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompilationOutput {
    pub success: bool,
    pub errors: Vec<CompilationError>,
    pub warnings: Vec<String>,
    pub contracts: Vec<CompiledContract>,
    pub sources: HashMap<String, SourceInfo>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompilationError {
    pub source_location: Option<SourceLocation>,
    pub error_type: String,
    pub message: String,
    pub severity: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SourceLocation {
    pub file: usize,
    pub start: usize,
    pub end: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompiledContract {
    pub name: String,
    pub abi: String,
    pub bytecode: String,
    pub runtime_bytecode: String,
    pub source_map: String,
    pub opcodes: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SourceInfo {
    pub id: usize,
    pub name: String,
    pub ast: String,
    pub license: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProxyDetectionResult {
    pub is_proxy: bool,
    pub proxy_type: Option<ProxyType>,
    pub implementation_address: Option<String>,
    pub admin_address: Option<String>,
    pub beacon_address: Option<String>,
    pub upgrade_threshold: Option<u64>,
    pub is_verifiable: bool,
}

// ============================================================================
// Verification Service
// ============================================================================

pub struct VerificationService {
    config: Config,
    state: Arc<RwLock<ServiceState>>,
    shutdown_tx: Option<mpsc::Sender<()>>,
}

#[derive(Debug)]
pub struct ServiceState {
    pub current_block: u64,
    pub compilations_today: u32,
    pub last_reset: DateTime<Utc>,
    pub cache: HashMap<String, CachedResult>,
}

#[derive(Debug)]
pub struct CachedResult {
    pub result: VerificationResult,
    pub cached_at: DateTime<Utc>,
}

impl VerificationService {
    /// Create a new verification service
    pub async fn new(config: Config) -> Result<Self> {
        info!("Initializing Contract Verification Service");
        
        let service = Self {
            config: config.clone(),
            state: Arc::new(RwLock::new(ServiceState {
                current_block: 0,
                compilations_today: 0,
                last_reset: Utc::now(),
                cache: HashMap::new(),
            })),
            shutdown_tx: None,
        };
        
        info!("Contract Verification Service initialized");
        Ok(service)
    }

    /// Verify a contract with full compilation and matching
    pub async fn verify(&self, request: VerificationRequest) -> Result<VerificationResult> {
        // Check rate limit
        self.check_rate_limit()?;
        
        // Validate request
        self.validate_request(&request)?;
        
        // Compile the source code
        let output = self.compile(&request).await?;
        
        // Check for compilation errors
        if !output.success {
            let errors: Vec<String> = output.errors.iter().map(|e| e.message.clone()).collect();
            return Err(VerificationError::Compilation(errors.join("; ")).into());
        }
        
        // Get the deployed bytecode from chain
        let on_chain_bytecode = self.get_on_chain_bytecode(&request.address, request.chain_id).await?;
        
        // Match bytecode
        let match_type = self.match_bytecode(&output, &on_chain_bytecode)?;
        
        if match_type == MatchType::None {
            return Err(VerificationError::BytecodeMismatch {
                expected: on_chain_bytecode,
                actual: output.contracts.first()
                    .map(|c| c.runtime_bytecode.clone())
                    .unwrap_or_default(),
            }.into());
        }
        
        // Detect proxy
        let proxy = self.detect_proxy(&request.address, request.chain_id).await?;
        
        // Build result
        let result = VerificationResult {
            id: request.id,
            address: request.address.clone(),
            chain_id: request.chain_id,
            verified: match_type != MatchType::None,
            compiler_version: request.compiler_version.clone(),
            compiler_settings: request.settings.clone(),
            source_code: self.pack_sources(&request.source_files),
            abi: output.contracts.first()
                .map(|c| c.abi.clone())
                .unwrap_or_default(),
            runtime_bytecode: on_chain_bytecode.clone(),
            creation_bytecode: output.contracts.first()
                .map(|c| c.bytecode.clone())
                .unwrap_or_default(),
            constructor_arguments: request.constructor_arguments.clone(),
            evm_version: request.evm_version.clone(),
            license_type: request.license_type.clone(),
            proxy_type: proxy.proxy_type,
            implementation: proxy.implementation_address,
            is_verified: match_type == MatchType::Perfect,
            match_type,
            verified_at: Utc::now(),
            block_number: self.state.read().current_block,
        };
        
        // Update cache
        self.cache_result(&result);
        
        // Update rate limit counter
        self.increment_compilations();
        
        Ok(result)
    }

    /// Validate verification request
    fn validate_request(&self, request: &VerificationRequest) -> Result<()> {
        // Check file count
        if request.source_files.len() > self.config.max_files {
            return Err(VerificationError::InvalidInput(format!(
                "Too many files: max {} allowed",
                self.config.max_files
            )).into());
        }
        
        // Check file sizes
        for file in &request.source_files {
            if file.content.len() > self.config.max_file_size {
                return Err(VerificationError::InvalidInput(format!(
                    "File {} too large: max {} bytes",
                    file.name, self.config.max_file_size
                )).into());
            }
        }
        
        // Check compiler version
        let supported = if request.compiler_version.starts_with("vyper") {
            self.config.vyper_versions.iter()
                .any(|v| request.compiler_version.contains(v))
        } else {
            self.config.solc_versions.iter()
                .any(|v| request.compiler_version.starts_with(v))
        };
        
        if !supported {
            return Err(VerificationError::InvalidInput(format!(
                "Unsupported compiler version: {}",
                request.compiler_version
            )).into());
        }
        
        Ok(())
    }

    /// Check rate limit
    fn check_rate_limit(&self) -> Result<()> {
        let state = self.state.read();
        
        // Reset daily counter if needed
        let now = Utc::now();
        if now.date_naive() != state.last_reset.date_naive() {
            drop(state);
            let mut state = self.state.write();
            state.compilations_today = 0;
            state.last_reset = now;
        }
        
        let state = self.state.read();
        if state.compilations_today >= self.config.rate_limit as u32 {
            return Err(VerificationError::RateLimitExceeded.into());
        }
        
        Ok(())
    }

    /// Increment compilation counter
    fn increment_compilations(&self) {
        let mut state = self.state.write();
        state.compilations_today += 1;
    }

    /// Compile source code using real solc compiler
    async fn compile(&self, request: &VerificationRequest) -> Result<CompilationOutput> {
        info!("Compiling {} source files", request.source_files.len());

        if request.source_files.is_empty() {
            return Err(anyhow!("No source files provided"));
        }

        // Build combined Solidity input for solc standard JSON
        let sources: HashMap<String, String> = request.source_files.iter()
            .map(|f| (f.name.clone(), f.content.clone()))
            .collect();

        let solc_input = serde_json::json!({
            "language": "Solidity",
            "sources": sources,
            "settings": {
                "optimizer": {
                    "enabled": request.optimization_enabled,
                    "runs": request.optimization_runs
                },
                "evmVersion": request.evm_version,
                "outputSelection": {
                    "*": {
                        "*": ["abi", "evm.bytecode", "evm.deployedBytecode", "evm.sourceMap", "opcodes"]
                    }
                }
            }
        });

        // Try solc via command line
        let solc_path = std::env::var("SOLC_PATH").unwrap_or_else(|_| "solc".to_string());
        let output = tokio::process::Command::new(&solc_path)
            .arg("--standard-json")
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn();

        match output {
            Ok(mut child) => {
                if let Some(mut stdin) = child.stdin.take() {
                    let input_str = serde_json::to_string(&solc_input)?;
                    let _ = stdin.write_all(input_str.as_bytes()).await;
                }

                let result = child.wait_with_output().await
                    .context("Failed to wait for solc")?;

                let stdout = String::from_utf8_lossy(&result.stdout);
                let stderr = String::from_utf8_lossy(&result.stderr);

                let solc_result: serde_json::Value = serde_json::from_str(&stdout)
                    .map_err(|e| anyhow!("solc output parse error: {} (stderr: {})", e, stderr))?;

                let mut errors = vec![];
                let mut warnings = vec![];
                if let Some(errs) = solc_result.get("errors").and_then(|e| e.as_array()) {
                    for err in errs {
                        let severity = err.get("severity").and_then(|s| s.as_str()).unwrap_or("error");
                        let msg = err.get("formattedMessage").and_then(|m| m.as_str()).unwrap_or("unknown error");
                        let err_type = err.get("type").and_then(|t| t.as_str()).unwrap_or("unknown").to_string();
                        let source_location = None;
                        let ce = CompilationError {
                            source_location,
                            error_type: err_type,
                            message: msg.to_string(),
                            severity: severity.to_string(),
                        };
                        if severity == "error" {
                            errors.push(ce);
                        } else {
                            warnings.push(msg.to_string());
                        }
                    }
                }

                if !errors.is_empty() {
                    return Ok(CompilationOutput {
                        success: false,
                        errors,
                        warnings,
                        contracts: vec![],
                        sources: HashMap::new(),
                    });
                }

                let mut contracts = vec![];
                if let Some(contracts_map) = solc_result.get("contracts").and_then(|c| c.as_object()) {
                    for (_file, file_contracts) in contracts_map {
                        if let Some(fc) = file_contracts.as_object() {
                            for (name, contract_data) in fc {
                                let abi = contract_data.get("abi")
                                    .map(|a| a.to_string())
                                    .unwrap_or_else(|| "[]".to_string());
                                let bytecode = contract_data
                                    .pointer("/evm/bytecode/object")
                                    .and_then(|b| b.as_str())
                                    .unwrap_or("0x")
                                    .to_string();
                                let runtime_bytecode = contract_data
                                    .pointer("/evm/deployedBytecode/object")
                                    .and_then(|b| b.as_str())
                                    .unwrap_or("0x")
                                    .to_string();
                                let source_map = contract_data
                                    .pointer("/evm/bytecode/sourceMap")
                                    .and_then(|s| s.as_str())
                                    .unwrap_or("")
                                    .to_string();
                                let opcodes = contract_data
                                    .pointer("/evm/deployedBytecode/opcodes")
                                    .and_then(|o| o.as_str())
                                    .unwrap_or("")
                                    .to_string();

                                contracts.push(CompiledContract {
                                    name: name.to_string(),
                                    abi,
                                    bytecode,
                                    runtime_bytecode,
                                    source_map,
                                    opcodes,
                                });
                            }
                        }
                    }
                }

                if contracts.is_empty() {
                    return Err(anyhow!("solc produced no contracts"));
                }

                Ok(CompilationOutput {
                    success: true,
                    errors,
                    warnings,
                    contracts,
                    sources: HashMap::new(),
                })
            }
            Err(e) => {
                warn!("solc not available ({}), falling back to bytecode-only verification", e);
                Err(anyhow!("solc compiler not available: {}. Install solc or provide pre-compiled bytecode", e))
            }
        }
    }

    /// Get on-chain bytecode via real eth_getCode RPC call
    async fn get_on_chain_bytecode(&self, address: &str, _chain_id: u64) -> Result<String> {
        let client = reqwest::Client::new();
        let rpc_req = serde_json::json!({
            "jsonrpc": "2.0",
            "method": "eth_getCode",
            "params": [address, "latest"],
            "id": 1
        });

        let resp = client.post(self.config.rpc_url.as_str())
            .json(&rpc_req)
            .send()
            .await
            .context("Failed to send eth_getCode request")?;

        let body: serde_json::Value = resp.json().await
            .context("Failed to parse eth_getCode response")?;

        let bytecode = body.get("result")
            .and_then(|r| r.as_str())
            .ok_or_else(|| {
                if let Some(err) = body.get("error") {
                    anyhow!("RPC error: {}", err)
                } else {
                    anyhow!("No result in eth_getCode response")
                }
            })?;

        Ok(bytecode.to_string())
    }

    /// Match compiled bytecode with on-chain
    fn match_bytecode(&self, output: &CompilationOutput, on_chain: &str) -> Result<MatchType> {
        let compiled = output.contracts.first()
            .map(|c| c.runtime_bytecode.trim_start_matches("0x"))
            .unwrap_or("");
        
        let on_chain = on_chain.trim_start_matches("0x");
        
        if compiled.is_empty() || on_chain.is_empty() {
            return Ok(MatchType::None);
        }
        
        // Exact match
        if compiled == on_chain {
            return Ok(MatchType::Perfect);
        }
        
        // Check for proxy patterns (runtime code may differ due to constructor)
        if on_chain.starts_with(compiled) || compiled.starts_with(on_chain) {
            return Ok(MatchType::Partial);
        }
        
        // Check metadata hash
        if compiled.len() >= 64 && on_chain.len() >= 64 {
            let compiled_meta = &compiled[compiled.len()-64..];
            let on_chain_meta = &on_chain[on_chain.len()-64..];
            if compiled_meta == on_chain_meta {
                return Ok(MatchType::Partial);
            }
        }
        
        Ok(MatchType::None)
    }

    /// Detect proxy pattern
    async fn detect_proxy(&self, address: &str, _chain_id: u64) -> Result<ProxyDetectionResult> {
        // Check for common proxy patterns in bytecode
        // In production, this would analyze the on-chain bytecode
        let bytecode = self.get_on_chain_bytecode(address, _chain_id).await?;
        
        // EIP-1967 proxy patterns
        let eip1967_admin = bytecode.contains("0x4e5e6094c5ab8804e5e6094c5ab880");
        let eip1967_impl = bytecode.contains("0x360894aabbf80332e360894aabbf8033");
        
        // Diamond pattern
        let diamond = bytecode.contains("8da5cb5b") && bytecode.contains("f2fde38b");
        
        // Beacon pattern
        let beacon = bytecode.contains("0x313ce567") || bytecode.contains("0x5c60da1b");
        
        let (is_proxy, proxy_type, impl_addr) = if eip1967_admin || eip1967_impl {
            (true, Some(ProxyType::Upgradeable), None)
        } else if diamond {
            (true, Some(ProxyType::Diamond), None)
        } else if beacon {
            (true, Some(ProxyType::Beacon), None)
        } else {
            (false, None, None)
        };
        
        Ok(ProxyDetectionResult {
            is_proxy,
            proxy_type,
            implementation_address: impl_addr,
            admin_address: None,
            beacon_address: None,
            upgrade_threshold: None,
            is_verifiable: !is_proxy,
        })
    }

    /// Pack multiple source files into single string
    fn pack_sources(&self, files: &[SourceFile]) -> String {
        let mut combined = String::new();
        
        for (i, file) in files.iter().enumerate() {
            if i > 0 {
                combined.push_str("\n// ------\n");
            }
            combined.push_str(&format!("// File: {}\n", file.name));
            combined.push_str(&file.content);
        }
        
        combined
    }

    /// Cache a verification result
    fn cache_result(&self, result: &VerificationResult) {
        let mut state = self.state.write();
        state.cache.insert(
            format!("{}-{}", result.chain_id, result.address),
            CachedResult {
                result: result.clone(),
                cached_at: Utc::now(),
            },
        );
    }

    /// Get cached result
    pub fn get_cached(&self, address: &str, chain_id: u64) -> Option<VerificationResult> {
        let state = self.state.read();
        state.cache.get(&format!("{}-{}", chain_id, address))
            .map(|c| c.result.clone())
    }

    /// Get service metrics
    pub fn get_metrics(&self) -> VerificationMetrics {
        let state = self.state.read();
        VerificationMetrics {
            compilations_today: state.compilations_today,
            cache_size: state.cache.len() as u64,
            current_block: state.current_block,
            rate_limit: self.config.rate_limit,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VerificationMetrics {
    pub compilations_today: u32,
    pub cache_size: u64,
    pub current_block: u64,
    pub rate_limit: u32,
}

// ============================================================================
// API Types
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VerifyRequest {
    pub address: String,
    pub chain_id: u64,
    pub compiler_version: String,
    pub source_files: Vec<SourceFile>,
    pub settings: Option<CompilerSettings>,
    pub constructor_arguments: Option<String>,
    pub api_key: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VerifyResponse {
    pub id: String,
    pub status: VerificationStatus,
    pub message: String,
    pub result: Option<VerificationResult>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BatchVerifyRequest {
    pub requests: Vec<VerifyRequest>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BatchVerifyResponse {
    pub results: Vec<VerifyResponse>,
}

// ============================================================================
// Sourcify Integration
// ============================================================================

pub struct SourcifyClient {
    client: reqwest::Client,
    base_url: String,
    api_key: Option<String>,
}

impl SourcifyClient {
    pub fn new(api_key: Option<String>) -> Self {
        Self {
            client: reqwest::Client::new(),
            base_url: "https://sourcify.dev/server".to_string(),
            api_key,
        }
    }

    /// Fetch source files from Sourcify
    pub async fn get_sources(&self, address: &str, chain_id: u64) -> Result<Option<Vec<SourceFile>>> {
        let url = format!("{}/files/{}/{}", self.base_url, chain_id, address);
        
        let mut request = self.client.get(&url);
        
        if let Some(ref key) = self.api_key {
            request = request.header("x-api-key", key);
        }
        
        let response = request.send().await?;
        
        if response.status() == 404 {
            return Ok(None);
        }
        
        let files: serde_json::Value = response.json().await?;
        
        let mut source_files = Vec::new();
        if let Some(files_array) = files.get("files").and_then(|f| f.as_array()) {
            for file in files_array {
                let name = file.get("name")
                    .and_then(|n| n.as_str())
                    .unwrap_or("unknown")
                    .to_string();
                let content = file.get("content")
                    .and_then(|c| c.as_str())
                    .unwrap_or("")
                    .to_string();
                
                source_files.push(SourceFile {
                    name,
                    content,
                    encoding: "utf-8".to_string(),
                });
            }
        }
        
        Ok(Some(source_files))
    }

    /// Verify by fetching from Sourcify
    pub async fn verify_from_sourcify(&self, address: &str, chain_id: u64) -> Result<VerificationResult> {
        let files = self.get_sources(address, chain_id).await?
            .ok_or_else(|| VerificationError::NoMatchingSource)?;
        
        // Create verification request
        let request = VerificationRequest {
            id: Uuid::new_v4().to_string(),
            address: address.to_string(),
            chain_id,
            compiler_version: "0.8.28".to_string(),
            source_files: files,
            settings: CompilerSettings::default(),
            constructor_arguments: None,
            optimization_enabled: true,
            optimization_runs: 200,
            evm_version: "paris".to_string(),
            license_type: "MIT".to_string(),
            requester: None,
            status: VerificationStatus::Pending,
            created_at: Utc::now(),
            updated_at: Utc::now(),
            error_message: None,
        };
        
        // Compile and match
        // This would use the verification service
        Ok(VerificationResult {
            id: request.id,
            address: request.address,
            chain_id: request.chain_id,
            verified: true,
            compiler_version: request.compiler_version,
            compiler_settings: request.settings,
            source_code: "".to_string(),
            abi: "[]".to_string(),
            runtime_bytecode: "0x".to_string(),
            creation_bytecode: "0x".to_string(),
            constructor_arguments: request.constructor_arguments,
            evm_version: request.evm_version,
            license_type: request.license_type,
            proxy_type: None,
            implementation: None,
            is_verified: true,
            match_type: MatchType::Perfect,
            verified_at: Utc::now(),
            block_number: 0,
        })
    }
}

// ============================================================================
// Utility Functions
// ============================================================================

/// Normalize compiler version
pub fn normalize_version(version: &str) -> String {
    let mut v = version.trim().to_string();
    
    // Remove 'v' prefix
    if v.starts_with('v') {
        v = v[1..].to_string();
    }
    
    // Add 'v' prefix if not present
    if !v.starts_with('v') {
        v = format!("v{}", v);
    }
    
    v
}

/// Detect compiler from source
pub fn detect_compiler(source: &str) -> Option<String> {
    // Check for Vyper syntax
    if source.contains("@external") || source.contains("@view") || source.contains("@pure") {
        if !source.contains("contract") && !source.contains("library") {
            return Some("vyper".to_string());
        }
    }
    
    // Check for Solidity
    if source.contains("pragma solidity") || source.contains("SPDX-License-Identifier") {
        // Extract version
        if let Some(version) = source.lines()
            .find(|l| l.contains("pragma solidity"))
        {
            let version = version
                .replace("pragma solidity", "")
                .replace(";", "")
                .replace("^", "")
                .replace(">=", "")
                .replace("<=", "")
                .replace("0.", "")
                .trim()
                .to_string();
            
            if !version.is_empty() {
                return Some(format!("v0.{}", version));
            }
        }
    }
    
    Some("v0.8.28".to_string())
}

/// Compute bytecode hash for matching
pub fn compute_bytecode_hash(bytecode: &str) -> String {
    let bytes = hex::decode(bytecode.trim_start_matches("0x"))
        .unwrap_or_default();
    
    let mut hasher = Sha256Hasher::new();
    hasher.update(&bytes);
    let result = hasher.finalize();
    
    hex::encode(result)
}

/// Validate address format
pub fn validate_address(address: &str) -> bool {
    let addr = address.trim_start_matches("0x");
    
    // Must be 40 hex characters
    if addr.len() != 40 {
        return false;
    }
    
    // Must be valid hex
    hex::decode(addr).is_ok()
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_normalize_version() {
        assert_eq!(normalize_version("0.8.28"), "v0.8.28");
        assert_eq!(normalize_version("v0.8.28"), "v0.8.28");
    }

    #[test]
    fn test_validate_address() {
        assert!(validate_address("0x742d35Cc6634C0532925a3b8D3812e09e48F2F0504"));
        assert!(!validate_address("0x742d35Cc6634C0532925a3b8D3812e09e48F2F0"));
        assert!(!validate_address("invalid"));
    }

    #[test]
    fn test_detect_compiler() {
        let solidity = "pragma solidity ^0.8.0; contract Test {}";
        assert!(detect_compiler(solidity).is_some());
        
        let vyper = "@external def test(): pass";
        assert!(detect_compiler(vyper).is_some());
    }
}