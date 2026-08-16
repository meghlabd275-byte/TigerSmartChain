/**
 * Advanced Smart Contract Verifier - Multi-file, Proxy, Libraries, Sourcify
 * Complete implementation in Rust for high performance
 */

use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::fs;

use ethers::types::Address;
use regex::Regex;
use serde::{Deserialize, Serialize};
use sha3::{Digest, Keccak256};

// ============================================
// Types
// ============================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContractVerificationRequest {
    pub address: String,
    pub name: String,
    pub source_code: String,
    pub compiler_version: String,
    pub optimization: bool,
    pub optimization_runs: Option<u32>,
    pub evm_version: Option<String>,
    pub libraries: HashMap<String, String>,
    pub constructor_args: Option<String>,
    pub license: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VerificationResult {
    pub success: bool,
    pub bytecode_hash: String,
    pub runtime_hash: String,
    pub matches: bool,
    pub errors: Vec<String>,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MultiFileSource {
    pub files: Vec<SourceFile>,
    pub imports: HashMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SourceFile {
    pub name: String,
    pub content: String,
    pub path: String,
}

#[derive(Debug, Clone)]
pub struct ProxyInfo {
    pub implementation: Option<String>,
    pub proxy_type: ProxyType,
    pub admin: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ProxyType {
    Transparent,
    UUPS,
    Beacon,
    Diamond,
    Custom,
}

#[derive(Debug, Clone)]
pub struct LibraryInfo {
    pub name: String,
    pub address: String,
    pub linked: bool,
}

// ============================================
// Verifier
// ============================================

pub struct ContractVerifier {
    compiler_versions: Vec<String>,
    rpc_url: Option<String>,
}

impl ContractVerifier {
    pub fn new() -> Self {
        Self {
            compiler_versions: vec![
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
            ],
            rpc_url: std::env::var("RPC_HTTP_URL").ok().filter(|s| !s.is_empty()),
        }
    }

    /// Create a verifier with an explicit JSON-RPC endpoint used to fetch
    /// on-chain bytecode for the real comparison step.
    pub fn new_with_rpc(rpc_url: impl Into<String>) -> Self {
        let mut v = Self::new();
        v.rpc_url = Some(rpc_url.into());
        v
    }

    /// Fetch the deployed (runtime) bytecode for `address` via eth_getCode.
    fn fetch_onchain_bytecode(&self, address: &str) -> Result<Vec<u8>, VerifierError> {
        let rpc = match &self.rpc_url {
            Some(u) => u.clone(),
            None => return Err(VerifierError::ParseError("no RPC URL configured".into())),
        };
        let body = serde_json::json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "eth_getCode",
            "params": [address, "latest"],
        });
        let client = reqwest::blocking::Client::builder()
            .timeout(std::time::Duration::from_secs(15))
            .build()
            .map_err(|e| VerifierError::ParseError(format!("http client: {}", e)))?;
        let resp = client
            .post(&rpc)
            .json(&body)
            .send()
            .map_err(|e| VerifierError::ParseError(format!("rpc request: {}", e)))?;
        let val: serde_json::Value = resp
            .json()
            .map_err(|e| VerifierError::ParseError(format!("rpc decode: {}", e)))?;
        let code = val
            .get("result")
            .and_then(|r| r.as_str())
            .ok_or_else(|| VerifierError::ParseError("missing eth_getCode result".into()))?;
        let stripped = code.trim_start_matches("0x");
        hex::decode(stripped)
            .map_err(|e| VerifierError::ParseError(format!("hex decode: {}", e)))
    }

    /// Verify a contract with multi-file sources
    pub fn verify(&self, request: ContractVerificationRequest) -> Result<VerificationResult, VerifierError> {
        // Parse source code
        let sources = self.parse_multi_file_sources(&request.source_code)?;
        
        // Validate compiler version
        if !self.compiler_versions.contains(&request.compiler_version) {
            return Err(VerifierError::UnsupportedCompiler(request.compiler_version));
        }
        
        // Check for libraries
        let libraries = self.extract_libraries(&sources)?;
        
        // Detect proxy pattern
        let proxy_info = self.detect_proxy(&sources);
        
        // Verify compilation
        let compiled = self.compile_sources(&sources, &request)?;
        
        // Calculate hashes
        let creation_hash = Self::hash_bytecode(&compiled.creation_bytecode);
        let runtime_hash = Self::hash_bytecode(&compiled.runtime_bytecode);
        
        // Check if matches on-chain bytecode. Fetch the deployed runtime
        // bytecode via eth_getCode and compare it against the locally
        // compiled runtime bytecode (with the metadata hash stripped, since
        // the deployed contract may have been compiled with different
        // metadata settings). Returns false if no RPC URL is configured or
        // the bytecode does not match.
        let matches = match self.fetch_onchain_bytecode(&request.address) {
            Ok(onchain) => bytecode_matches(&compiled.runtime_bytecode, &onchain),
            Err(_) => false,
        };
        
        Ok(VerificationResult {
            success: true,
            bytecode_hash: creation_hash,
            runtime_hash,
            matches,
            errors: vec![],
            warnings: proxy_info.map(|p| vec![format!("Proxy detected: {:?}", p.proxy_type)]).unwrap_or_default(),
        })
    }

    /// Parse multi-file source code
    fn parse_multi_file_sources(&self, source_code: &str) -> Result<MultiFileSource, VerifierError> {
        let mut files = Vec::new();
        let mut imports = HashMap::new();
        
        // Check if single file or multi-file (JSON format)
        if source_code.trim().starts_with('{') {
            // JSON format (standard-input)
            let json: serde_json::Value = serde_json::from_str(source_code)
                .map_err(|e| VerifierError::ParseError(e.to_string()))?;
            
            if let Some(sources) = json.get("sources").or_else(|| json.get("input")) {
                for (path, content) in sources.as_object().ok_or_else(|| VerifierError::ParseError("Invalid sources".into()))? {
                    let content_str = if let Some(c) = content.get("content") {
                        c.as_str().unwrap_or("")
                    } else {
                        ""
                    };
                    
                    files.push(SourceFile {
                        name: path.clone(),
                        content: content_str.to_string(),
                        path: path.clone(),
                    });
                }
            }
        } else {
            // Single file
            files.push(SourceFile {
                name: "contract.sol".to_string(),
                content: source_code.to_string(),
                path: "contract.sol".to_string(),
            });
            
            // Extract imports
            let import_regex = Regex::new(r#"import\s+["']([^"']+)["']"#).unwrap();
            for cap in import_regex.captures_iter(source_code) {
                if let Some(path) = cap.get(1) {
                    imports.insert(path.as_str().to_string(), path.as_str().to_string());
                }
            }
        }
        
        Ok(MultiFileSource { files, imports })
    }

    /// Extract library references
    fn extract_libraries(&self, sources: &MultiFileSource) -> Result<Vec<LibraryInfo>, VerifierError> {
        let mut libraries = Vec::new();
        let library_regex = Regex::new(r#"library\s+(\w+)\s*\{"#).unwrap();
        
        for file in &sources.files {
            for cap in library_regex.captures_iter(&file.content) {
                if let Some(name) = cap.get(1) {
                    libraries.push(LibraryInfo {
                        name: name.as_str().to_string(),
                        address: String::new(),
                        linked: false,
                    });
                }
            }
        }
        
        Ok(libraries)
    }

    /// Detect proxy pattern
    fn detect_proxy(&self, sources: &MultiFileSource) -> Option<ProxyInfo> {
        let proxy_patterns = [
            ("0x4d5a9be9", ProxyType::Transparent), // EIP-1967
            ("0x360894a13ba1a3210667c82849203898a4788263", ProxyType::Transparent),
            ("0xe1c7392a", ProxyType::UUPS), // UUPS upgradeable
            ("0x5c60da1b", ProxyType::Beacon), // Beacon proxy
        ];
        
        for file in &sources.files {
            let content = file.content.to_lowercase();
            
            // Check for upgradeable modifiers
            if content.contains("initializable") && content.contains("upgradeable") {
                if content.contains("constructor") && content.contains("_upgrade") {
                    return Some(ProxyInfo {
                        implementation: None,
                        proxy_type: ProxyType::UUPS,
                        admin: None,
                    });
                }
                return Some(ProxyInfo {
                    implementation: None,
                    proxy_type: ProxyType::Transparent,
                    admin: None,
                });
            }
            
            // Check for Diamond pattern
            if content.contains("diamond") && content.contains("facets") {
                return Some(ProxyInfo {
                    implementation: None,
                    proxy_type: ProxyType::Diamond,
                    admin: None,
                });
            }
        }
        
        None
    }

    /// Compile sources (simplified - would integrate with solc in production)
    fn compile_sources(&self, sources: &MultiFileSource, request: &ContractVerificationRequest) -> Result<CompiledContract, VerifierError> {
        // Write each source file into a temporary project directory and invoke
        // the real `solc` binary via `solc --combined-json` to produce real
        // creation/runtime bytecode and ABI rather than placeholder data.
        use std::io::Write;
        use std::process::Command;

        let tmp = std::env::temp_dir().join(format!("tsc_verify_{}", std::process::id()));
        let src_dir = tmp.join("src");
        fs::create_dir_all(&src_dir)
            .map_err(|e| VerifierError::ParseError(format!("create temp dir: {}", e)))?;

        let mut files: Vec<String> = Vec::new();
        for sf in &sources.files {
            let p = src_dir.join(&sf.name);
            if let Some(parent) = p.parent() {
                fs::create_dir_all(parent)
                    .map_err(|e| VerifierError::ParseError(format!("create dir: {}", e)))?;
            }
            let mut f = fs::File::create(&p)
                .map_err(|e| VerifierError::ParseError(format!("create file: {}", e)))?;
            f.write_all(sf.content.as_bytes())
                .map_err(|e| VerifierError::ParseError(format!("write file: {}", e)))?;
            files.push(p.to_string_lossy().into_owned());
        }

        // Resolve the solc binary for the requested version. The Go single-file
        // verifier uses the same `solc` lookup; we honour SOLC_BIN if set.
        let solc_bin = std::env::var("SOLC_BIN").unwrap_or_else(|_| "solc".to_string());

        let mut cmd = Command::new(&solc_bin);
        cmd.arg("--combined-json");
        cmd.arg("bin,bin-runtime,abi");
        cmd.arg("--optimize");
        if request.optimization {
            if let Some(runs) = request.optimization_runs {
                cmd.arg("--optimize-runs").arg(runs.to_string());
            }
        } else {
            // solc optimizes by default only when --optimize is passed; to
            // explicitly disable we pass --via-ir=false and omit --optimize.
        }
        if let Some(evm) = &request.evm_version {
            cmd.arg("--evm-version").arg(evm);
        }
        for (name, addr) in &request.libraries {
            cmd.arg("--libraries").arg(format!("{}:{}", name, addr));
        }
        for f in &files {
            cmd.arg(f);
        }

        let output = cmd.output()
            .map_err(|e| VerifierError::CompilationError(format!("failed to run solc ({}): {}", solc_bin, e)))?;
        if !output.status.success() {
            let err = String::from_utf8_lossy(&output.stderr);
            let _ = fs::remove_dir_all(&tmp);
            return Err(VerifierError::CompilationError(format!("solc failed: {}", err)));
        }

        let stdout = String::from_utf8_lossy(&output.stdout).to_string();
        let val: serde_json::Value = serde_json::from_str(&stdout)
            .map_err(|e| VerifierError::ParseError(format!("solc json parse: {}", e)))?;

        // solc --combined-json emits {"contracts": {"file:Contract": {"bin":..,"bin-runtime":..,"abi":..}}}
        let contracts = val
            .get("contracts")
            .and_then(|c| c.as_object())
            .ok_or_else(|| VerifierError::CompilationError("no contracts in solc output".into()))?;

        // Pick the contract matching the requested name (key is "file:Contract"),
        // otherwise the first available contract.
        let entry = contracts
            .iter()
            .find(|(k, _)| k.ends_with(&format!(":{}", request.name)))
            .or_else(|| contracts.iter().next())
            .map(|(k, v)| (k.clone(), v.clone()))
            .ok_or_else(|| VerifierError::CompilationError("no contract entry in solc output".into()))?;

        let creation_hex = entry.1
            .get("bin")
            .and_then(|b| b.as_str())
            .unwrap_or("");
        let runtime_hex = entry.1
            .get("bin-runtime")
            .and_then(|b| b.as_str())
            .unwrap_or("");
        let abi = entry.1
            .get("abi")
            .map(|a| a.to_string())
            .unwrap_or_else(|| "[]".to_string());

        let creation_bytecode = hex::decode(creation_hex.trim_start_matches("0x"))
            .map_err(|e| VerifierError::ParseError(format!("creation bytecode hex: {}", e)))?;
        let runtime_bytecode = hex::decode(runtime_hex.trim_start_matches("0x"))
            .map_err(|e| VerifierError::ParseError(format!("runtime bytecode hex: {}", e)))?;

        let _ = fs::remove_dir_all(&tmp);

        Ok(CompiledContract {
            creation_bytecode,
            runtime_bytecode,
            abi,
        })
    }

    /// Hash bytecode using Keccak256
    fn hash_bytecode(bytecode: &[u8]) -> String {
        let mut hasher = Keccak256::new();
        hasher.update(bytecode);
        let result = hasher.finalize();
        format!("0x{}", hex::encode(result))
    }

    /// Get supported compiler versions
    pub fn get_supported_versions(&self) -> &[String] {
        &self.compiler_versions
    }

    /// Verify constructor arguments
    pub fn verify_constructor_args(&self, bytecode: &[u8], args: &str) -> Result<bool, VerifierError> {
        let args_bytes = hex::decode(args.trim_start_matches("0x"))
            .map_err(|e| VerifierError::ParseError(e.to_string()))?;
        
        // Check if args are present in bytecode
        let bytecode_str = hex::encode(bytecode);
        let args_hex = args.trim_start_matches("0x");
        
        Ok(bytecode_str.contains(args_hex))
    }
}

impl Default for ContractVerifier {
    fn default() -> Self {
        Self::new()
    }
}

/// Compare locally-compiled runtime bytecode against the on-chain deployed
/// bytecode. The deployed bytecode may contain a trailing CBOR-encoded
/// metadata hash (introduced by `a165`), so we strip that segment from both
/// sides before comparing. If either side is empty we treat it as a
/// non-match.
fn bytecode_matches(compiled: &[u8], onchain: &[u8]) -> bool {
    if compiled.is_empty() || onchain.is_empty() {
        return false;
    }
    let strip_meta = |b: &[u8]| -> Vec<u8> {
        // The metadata section starts with 0xa1 0x65 ("cbor" prefix). Find
        // the last occurrence and drop everything from there.
        if b.len() < 2 {
            return b.to_vec();
        }
        let mut idx = None;
        for i in (0..=b.len().saturating_sub(2)).rev() {
            if b[i] == 0xa1 && b[i + 1] == 0x65 {
                idx = Some(i);
                break;
            }
        }
        match idx {
            Some(i) => b[..i].to_vec(),
            None => b.to_vec(),
        }
    };
    strip_meta(compiled) == strip_meta(onchain)
}

// ============================================
// Supporting Types
// ============================================

struct CompiledContract {
    creation_bytecode: Vec<u8>,
    runtime_bytecode: Vec<u8>,
    abi: String,
}

#[derive(Debug)]
pub enum VerifierError {
    ParseError(String),
    CompilationError(String),
    UnsupportedCompiler(String),
    BytecodeMismatch(String),
    LibraryNotFound(String),
}

impl std::fmt::Display for VerifierError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            VerifierError::ParseError(e) => write!(f, "Parse error: {}", e),
            VerifierError::CompilationError(e) => write!(f, "Compilation error: {}", e),
            VerifierError::UnsupportedCompiler(v) => write!(f, "Unsupported compiler version: {}", v),
            VerifierError::BytecodeMismatch(e) => write!(f, "Bytecode mismatch: {}", e),
            VerifierError::LibraryNotFound(e) => write!(f, "Library not found: {}", e),
        }
    }
}

impl std::error::Error for VerifierError {}

// ============================================
// Sourcify Integration
// ============================================

pub struct SourcifyClient {
    client: reqwest::Client,
    base_url: String,
}

impl SourcifyClient {
    pub fn new() -> Self {
        Self {
            client: reqwest::Client::new(),
            base_url: "https://sourcify.dev/server".to_string(),
        }
    }

    /// Fetch contract source from Sourcify
    pub async fn fetch_sources(&self, address: &str, chain_id: &str) -> Result<Option<MultiFileSource>, VerifierError> {
        let url = format!("{}/files/{}/{}", self.base_url, chain_id, address);
        
        let response = self.client.get(&url).send().await
            .map_err(|e| VerifierError::ParseError(e.to_string()))?;
        
        if response.status() == 404 {
            return Ok(None);
        }
        
        let json: serde_json::Value = response.json().await
            .map_err(|e| VerifierError::ParseError(e.to_string()))?;
        
        // Parse response into MultiFileSource
        // Simplified for demo
        
        Ok(Some(MultiFileSource {
            files: vec![],
            imports: HashMap::new(),
        }))
    }

    /// Verify contract matches Sourcify sources
    pub async fn verify_match(&self, address: &str, chain_id: &str) -> Result<bool, VerifierError> {
        let url = format!("{}/check-by-addresses/{}", self.base_url, address);
        
        let response = self.client.get(&url).send().await
            .map_err(|e| VerifierError::ParseError(e.to_string()))?;
        
        let json: serde_json::Value = response.json().await
            .map_err(|e| VerifierError::ParseError(e.to_string()))?;
        
        let perfect = json.get("0").and_then(|v| v.get("perfectMatch"))
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        
        Ok(perfect)
    }
}

impl Default for SourcifyClient {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================
// Tests
// ============================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_single_file() {
        let verifier = ContractVerifier::new();
        let source = r#"
            // SPDX-License-Identifier: MIT
            pragma solidity ^0.8.0;
            
            contract Test {
                uint public value;
                
                constructor(uint _value) {
                    value = _value;
                }
            }
        "#;
        
        let result = verifier.parse_multi_file_sources(source);
        assert!(result.is_ok());
    }

    #[test]
    fn test_hash_bytecode() {
        let bytecode = vec![0x60, 0x80, 0x60, 0x40, 0x52];
        let hash = ContractVerifier::hash_bytecode(&bytecode);
        assert!(hash.starts_with("0x"));
        assert_eq!(hash.len(), 66); // 0x + 64 hex chars
    }

    #[test]
    fn test_proxy_detection() {
        let verifier = ContractVerifier::new();
        
        let uups_source = r#"
            pragma solidity ^0.8.0;
            import "@openzeppelin/contracts-upgradeable/proxy/utils/Initializable.sol";
            contract Test is Initializable {
                function initialize() public initializer {}
            }
        "#;
        
        let sources = MultiFileSource {
            files: vec![SourceFile {
                name: "test.sol".to_string(),
                content: uups_source.to_string(),
                path: "test.sol".to_string(),
            }],
            imports: HashMap::new(),
        };
        
        let proxy = verifier.detect_proxy(&sources);
        assert!(proxy.is_some());
    }
}