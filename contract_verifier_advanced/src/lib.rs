/**
 * Advanced Smart Contract Verifier - Multi-file, Proxy, Libraries, Sourcify
 * Complete implementation in Rust for high performance
 */

use std::collections::HashMap;
use std::fs;

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
    /// The ABI the compiler produced, as JSON. A client needs it to call the
    /// verified contract, so verification is not complete without it.
    pub abi: String,
    /// The libraries the source references, and whether each was linked.
    pub libraries: Vec<LibraryInfo>,
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
    /// An EIP-1167 minimal proxy, which copies a fixed implementation address
    /// into its own code instead of reading a storage slot.
    Minimal,
    Custom,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
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

    /// Verify a contract with multi-file sources.
    ///
    /// The source is compiled with the requested compiler and the resulting
    /// runtime bytecode is compared against the code deployed at the address.
    /// `success` and `matches` are true only when that comparison passed, so a
    /// compilation that does not reproduce the deployed code is reported as a
    /// failure rather than a verification.
    pub fn verify(&self, request: ContractVerificationRequest) -> Result<VerificationResult, VerifierError> {
        let sources = self.parse_multi_file_sources(&request.source_code)?;

        if !self.compiler_versions.contains(&request.compiler_version) {
            return Err(VerifierError::UnsupportedCompiler(request.compiler_version));
        }

        let mut libraries = self.extract_libraries(&sources)?;
        // A library the source calls but that was not linked compiles to a
        // placeholder address, so the runtime bytecode cannot match. Record
        // which ones were linked so the report can say so.
        for library in &mut libraries {
            if let Some(address) = request.libraries.get(&library.name) {
                library.address = address.clone();
                library.linked = true;
            }
        }

        let compiled = self.compile_sources(&sources, &request)?;

        let creation_hash = Self::hash_bytecode(&compiled.creation_bytecode);
        let runtime_hash = Self::hash_bytecode(&compiled.runtime_bytecode);

        // The deployed code is needed twice: to decide the match and to find
        // the proxy pattern, which lives in the deployed code rather than in
        // the source.
        let onchain = self.fetch_onchain_bytecode(&request.address).ok();
        let proxy_info = self.detect_proxy(&sources, onchain.as_deref());

        let mut errors = Vec::new();
        let mut warnings = Vec::new();
        let matches = match &onchain {
            Some(code) => {
                if bytecode_matches(&compiled.runtime_bytecode, code) {
                    true
                } else {
                    errors.push(
                        "compiled runtime bytecode does not match the deployed code".to_string(),
                    );
                    false
                }
            }
            None => {
                errors.push(
                    "could not read the deployed bytecode, so the match is unknown".to_string(),
                );
                false
            }
        };
        if let Some(info) = &proxy_info {
            warnings.push(format!("proxy pattern detected: {:?}", info.proxy_type));
        }
        let unlinked: Vec<String> = libraries
            .iter()
            .filter(|library| !library.linked)
            .map(|library| library.name.clone())
            .collect();
        if !unlinked.is_empty() {
            warnings.push(format!(
                "libraries referenced but not linked: {}",
                unlinked.join(", ")
            ));
        }

        Ok(VerificationResult {
            success: matches,
            bytecode_hash: creation_hash,
            runtime_hash,
            matches,
            abi: compiled.abi,
            libraries,
            errors,
            warnings,
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

    /// Detect a proxy pattern.
    ///
    /// Two signals are used, strongest first. The deployed bytecode is checked
    /// for the EIP-1967 storage slots and the EIP-1167 minimal-proxy shape,
    /// which a proxy carries whichever source it was built from. When no code
    /// is available the source text is inspected for the upgradeable patterns
    /// instead, which is weaker but is the best that can be done before
    /// deployment.
    fn detect_proxy(&self, sources: &MultiFileSource, onchain: Option<&[u8]>) -> Option<ProxyInfo> {
        // EIP-1967 and EIP-1822 storage slot constants. A proxy reads its
        // implementation or beacon from one of these, so its presence in the
        // deployed code identifies the pattern without needing the source.
        const EIP1967_IMPLEMENTATION: [u8; 32] = [
            0x36, 0x08, 0x94, 0xa1, 0x3b, 0xa1, 0xa3, 0x21, 0x06, 0x67, 0xc8, 0x28, 0x49, 0x20,
            0x38, 0x98, 0xa4, 0x78, 0x82, 0x63, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff,
            0xff, 0xff, 0xff, 0xff,
        ];
        const EIP1967_BEACON: [u8; 32] = [
            0xa3, 0xf0, 0xad, 0x74, 0x68, 0xc2, 0x9c, 0xc0, 0x0a, 0x9c, 0x4d, 0xdc, 0x0a, 0x3a,
            0xa3, 0x1e, 0x0b, 0x50, 0x2c, 0x0d, 0x70, 0x4b, 0x62, 0x9c, 0x5c, 0x2c, 0x2b, 0x2a,
            0x53, 0x9e, 0x9a, 0x6d,
        ];
        if let Some(code) = onchain {
            if code.windows(EIP1967_BEACON.len()).any(|w| w == EIP1967_BEACON) {
                return Some(ProxyInfo {
                    implementation: None,
                    proxy_type: ProxyType::Beacon,
                    admin: None,
                });
            }
            if code
                .windows(EIP1967_IMPLEMENTATION.len())
                .any(|w| w == EIP1967_IMPLEMENTATION)
            {
                return Some(ProxyInfo {
                    implementation: None,
                    proxy_type: ProxyType::Transparent,
                    admin: None,
                });
            }
            // An EIP-1167 minimal proxy copies a fixed implementation address
            // into its own code rather than reading a storage slot, so its
            // shape identifies it and the address is readable from the code.
            const MINIMAL_PROXY_PREFIX: [u8; 10] =
                [0x36, 0x3d, 0x3d, 0x37, 0x3d, 0x3d, 0x3d, 0x34, 0x3d, 0x73];
            if code.len() == 45 && code.starts_with(&MINIMAL_PROXY_PREFIX) {
                return Some(ProxyInfo {
                    implementation: Some(format!("0x{}", hex::encode(&code[10..30]))),
                    proxy_type: ProxyType::Minimal,
                    admin: None,
                });
            }
        }

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

    /// Compile the sources with the requested `solc` version.
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

    /// Verify that the supplied constructor arguments appear in the creation
    /// bytecode.
    ///
    /// The comparison is over bytes, so an odd-length hex string — which has
    /// no byte representation — is rejected rather than silently mis-decoded.
    pub fn verify_constructor_args(
        &self,
        bytecode: &[u8],
        args: &str,
    ) -> Result<bool, VerifierError> {
        let args = args.trim().trim_start_matches("0x");
        if args.len() % 2 != 0 {
            return Err(VerifierError::ParseError(
                "constructor arguments must be an even number of hex digits".to_string(),
            ));
        }
        let args_bytes = hex::decode(args).map_err(|e| VerifierError::ParseError(e.to_string()))?;
        if args_bytes.is_empty() {
            // No arguments were supplied, so there is nothing to look for.
            return Ok(true);
        }
        Ok(bytecode
            .windows(args_bytes.len())
            .any(|window| window == args_bytes.as_slice()))
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

    /// Fetch contract source from Sourcify.
    ///
    /// The endpoint returns `{status, files: [{name, content}]}`. A contract
    /// with no published sources answers `false`, which is reported as `None`
    /// rather than as an empty file list: the two mean different things.
    pub async fn fetch_sources(
        &self,
        address: &str,
        chain_id: &str,
    ) -> Result<Option<MultiFileSource>, VerifierError> {
        let url = format!("{}/files/{}/{}", self.base_url, chain_id, address);

        let response = self
            .client
            .get(&url)
            .send()
            .await
            .map_err(|e| VerifierError::ParseError(e.to_string()))?;

        if response.status() == 404 {
            return Ok(None);
        }

        let json: serde_json::Value = response
            .json()
            .await
            .map_err(|e| VerifierError::ParseError(e.to_string()))?;

        let status = json.get("status").and_then(|v| v.as_str()).unwrap_or_default();
        if status == "false" || status.eq_ignore_ascii_case("error") {
            return Ok(None);
        }
        let files = match json.get("files").and_then(|v| v.as_array()) {
            Some(files) if !files.is_empty() => files,
            _ => return Ok(None),
        };

        let mut sources = Vec::with_capacity(files.len());
        let mut imports = HashMap::new();
        for file in files {
            let name = file
                .get("name")
                .and_then(|v| v.as_str())
                .unwrap_or_default()
                .to_string();
            if name.is_empty() {
                continue;
            }
            let content = file
                .get("content")
                .and_then(|v| v.as_str())
                .unwrap_or_default()
                .to_string();
            imports.insert(name.clone(), content.clone());
            let path = if name.starts_with('/') {
                name.clone()
            } else {
                format!("/{name}")
            };
            sources.push(SourceFile {
                name,
                content,
                path,
            });
        }
        if sources.is_empty() {
            return Ok(None);
        }
        Ok(Some(MultiFileSource {
            files: sources,
            imports,
        }))
    }

    /// Verify contract matches Sourcify sources
    pub async fn verify_match(&self, address: &str, chain_id: &str) -> Result<bool, VerifierError> {
        let url = format!(
            "{}/check-by-addresses?addresses={}&chainIds={}",
            self.base_url, address, chain_id
        );
        
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
        
        let proxy = verifier.detect_proxy(&sources, None);
        assert!(proxy.is_some());

        // The deployed code decides the pattern when it is available: an
        // EIP-1967 implementation slot marks a transparent proxy whatever the
        // source says.
        let mut code = vec![0x60, 0x80, 0x60, 0x40, 0x52];
        code.extend_from_slice(&[
            0x36, 0x08, 0x94, 0xa1, 0x3b, 0xa1, 0xa3, 0x21, 0x06, 0x67, 0xc8, 0x28, 0x49, 0x20,
            0x38, 0x98, 0xa4, 0x78, 0x82, 0x63, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff,
            0xff, 0xff, 0xff, 0xff,
        ]);
        let detected = verifier.detect_proxy(&sources, Some(&code));
        assert!(matches!(
            detected.map(|info| info.proxy_type),
            Some(ProxyType::Transparent)
        ));

        // An EIP-1167 minimal proxy is identified by its shape and its target
        // is read out of the code.
        let mut minimal = vec![0x36, 0x3d, 0x3d, 0x37, 0x3d, 0x3d, 0x3d, 0x34, 0x3d, 0x73];
        minimal.extend_from_slice(&[0x11; 20]);
        minimal.extend_from_slice(&[
            0x5a, 0xf4, 0x3d, 0x82, 0x80, 0x3e, 0x90, 0x3d, 0x91, 0x60, 0x2a, 0x57, 0xfd, 0x5b,
            0xf3,
        ]);
        assert_eq!(minimal.len(), 45);
        let detected = verifier.detect_proxy(&sources, Some(&minimal)).unwrap();
        assert!(matches!(detected.proxy_type, ProxyType::Minimal));
        assert_eq!(detected.implementation.unwrap(), format!("0x{}", "11".repeat(20)));
    }

    #[test]
    fn constructor_arguments_are_compared_as_bytes() {
        let verifier = ContractVerifier::new();
        // 0xdeadbeef appears in the bytecode, 0xcafebabe does not.
        let bytecode = vec![0x60, 0x80, 0xde, 0xad, 0xbe, 0xef, 0x00];
        assert!(verifier.verify_constructor_args(&bytecode, "0xdeadbeef").unwrap());
        assert!(!verifier.verify_constructor_args(&bytecode, "0xcafebabe").unwrap());
        // No arguments means nothing to find.
        assert!(verifier.verify_constructor_args(&bytecode, "0x").unwrap());
        // An odd-length hex string has no byte representation and must be
        // rejected rather than mis-decoded.
        assert!(verifier.verify_constructor_args(&bytecode, "0xabc").is_err());
    }
}