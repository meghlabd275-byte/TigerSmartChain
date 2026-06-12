//! Input Validation Module for TigerScan
//! Comprehensive input validation and sanitization

use regex::Regex;
use serde::{Deserialize, Serialize};
use std::net::IpAddr;

// =============================================================================
// VALIDATOR
// =============================================================================

/// Input Validator
pub struct Validator {
    address_regex: Regex,
    tx_hash_regex: Regex,
    block_hash_regex: Regex,
    block_number_regex: Regex,
    ens_regex: Regex,
    token_id_regex: Regex,
    signature_regex: Regex,
    email_regex: Regex,
    url_regex: Regex,
    safe_string_regex: Regex,
}

impl Default for Validator {
    fn default() -> Self {
        Self::new()
    }
}

impl Validator {
    /// Create a new validator
    pub fn new() -> Self {
        Self {
            address_regex: Regex::new(r"^0x[a-fA-F0-9]{40}$").unwrap(),
            tx_hash_regex: Regex::new(r"^0x[a-fA-F0-9]{64}$").unwrap(),
            block_hash_regex: Regex::new(r"^0x[a-fA-F0-9]{64}$").unwrap(),
            block_number_regex: Regex::new(r"^\d+$").unwrap(),
            ens_regex: Regex::new(r"^[a-zA-Z0-9][a-zA-Z0-9-]{0,62}\.[a-zA-Z0-9-]{0,63}\.(eth|xyz|luxe)$").unwrap(),
            token_id_regex: Regex::new(r"^\d+$|^0x[a-fA-F0-9]+$").unwrap(),
            signature_regex: Regex::new(r"^0x[a-fA-F0-9]{130}$|^[a-fA-F0-9]{130}$").unwrap(),
            email_regex: Regex::new(r"^[a-zA-Z0-9._%+-]+@[a-zA-Z0-9.-]+\.[a-zA-Z]{2,}$").unwrap(),
            url_regex: Regex::new(r"^https?://[a-zA-Z0-9.-]+(/[a-zA-Z0-9./_-]*)?$").unwrap(),
            safe_string_regex: Regex::new(r"^[a-zA-Z0-9_.-]+$").unwrap(),
        }
    }

    // =============================================================================
    // ETHEREUM VALIDATION
    // =============================================================================

    /// Validate Ethereum address
    pub fn validate_address(&self, addr: &str) -> bool {
        if addr.is_empty() || addr.len() != 42 {
            return false;
        }
        self.address_regex.is_match(addr)
    }

    /// Validate transaction hash
    pub fn validate_tx_hash(&self, hash: &str) -> bool {
        if hash.is_empty() || hash.len() != 66 {
            return false;
        }
        self.tx_hash_regex.is_match(hash)
    }

    /// Validate block hash
    pub fn validate_block_hash(&self, hash: &str) -> bool {
        if hash.is_empty() || hash.len() != 66 {
            return false;
        }
        self.block_hash_regex.is_match(hash)
    }

    /// Validate block number
    pub fn validate_block_number(&self, num: &str) -> bool {
        if num.is_empty() {
            return false;
        }
        if !self.block_number_regex.is_match(num) {
            return false;
        }
        // Reasonable block number range
        if let Ok(n) = num.parse::<u64>() {
            return n < 200_000_000;
        }
        false
    }

    /// Validate ENS domain
    pub fn validate_ens(&self, ens: &str) -> bool {
        if ens.is_empty() || ens.len() > 100 {
            return false;
        }
        self.ens_regex.is_match(ens)
    }

    /// Validate token ID
    pub fn validate_token_id(&self, token_id: &str) -> bool {
        if token_id.is_empty() {
            return false;
        }
        self.token_id_regex.is_match(token_id)
    }

    /// Validate signature
    pub fn validate_signature(&self, sig: &str) -> bool {
        if sig.is_empty() {
            return false;
        }
        self.signature_regex.is_match(sig)
    }

    // =============================================================================
    // GENERAL VALIDATION
    // =============================================================================

    /// Validate email
    pub fn validate_email(&self, email: &str) -> bool {
        if email.is_empty() || email.len() > 254 {
            return false;
        }
        self.email_regex.is_match(email)
    }

    /// Validate URL
    pub fn validate_url(&self, url: &str) -> bool {
        if url.is_empty() || url.len() > 2048 {
            return false;
        }
        self.url_regex.is_match(url)
    }

    /// Validate safe string
    pub fn validate_safe_string(&self, s: &str) -> bool {
        if s.is_empty() || s.len() > 256 {
            return false;
        }
        self.safe_string_regex.is_match(s)
    }

    /// Validate IP address
    pub fn validate_ip(&self, ip: &str) -> bool {
        ip.parse::<IpAddr>().is_ok()
    }

    // =============================================================================
    // SANITIZATION
    // =============================================================================

    /// Sanitize address (add 0x prefix)
    pub fn sanitize_address(&self, addr: &str) -> String {
        let addr = addr.trim().to_lowercase();
        if !addr.starts_with("0x") {
            format!("0x{}", addr)
        } else {
            addr
        }
    }

    /// Sanitize hex string
    pub fn sanitize_hex(&self, hex: &str) -> String {
        let hex = hex.trim().to_lowercase();
        hex.trim_start_matches("0x").to_string()
    }

    /// Sanitize URL
    pub fn sanitize_url(&self, url: &str) -> Option<String> {
        // Basic URL sanitization - remove dangerous schemes
        if url.contains("javascript:") || url.contains("data:") {
            return None;
        }
        Some(url.to_string())
    }

    /// Sanitize HTML (remove dangerous tags)
    pub fn sanitize_html(&self, html: &str) -> String {
        let mut result = html.to_string();
        
        // Remove script tags
        let script_re = Regex::new(r"(?i)<script[^>]*>.*?</script>").unwrap();
        result = script_re.replace_all(&result, "").to_string();
        
        // Remove object tags
        let object_re = Regex::new(r"(?i)<object[^>]*>.*?</object>").unwrap();
        result = object_re.replace_all(&result, "").to_string();
        
        // Remove iframe
        let iframe_re = Regex::new(r"(?i)<iframe[^>]*>.*?</iframe>").unwrap();
        result = iframe_re.replace_all(&result, "").to_string();
        
        // Remove event handlers
        let event_re = Regex::new(r"(?i)\s+on[a-z]+=\"[^\"]*\"").unwrap();
        result = event_re.replace_all(&result, "").to_string();
        
        // Remove javascript: URLs
        let js_re = Regex::new(r"(?i)javascript:").unwrap();
        result = js_re.replace_all(&result, "").to_string();
        
        result
    }

    /// Sanitize SQL (remove dangerous keywords)
    pub fn sanitize_sql(&self, input: &str) -> String {
        let mut result = input.to_string();
        
        let keywords = [
            "UNION", "SELECT", "DROP", "DELETE", "INSERT", "UPDATE",
            "EXEC", "EXECUTE", "XP_", "SP_", "ALTER", "CREATE",
        ];
        
        for keyword in keywords {
            let re = Regex::new(&format!(r"(?i)\b{}\b", keyword)).unwrap();
            result = re.replace_all(&result, "").to_string();
        }
        
        result
    }

    /// Sanitize path (prevent traversal)
    pub fn sanitize_path(&self, path: &str) -> String {
        let mut result = path.to_string();
        
        // Remove path traversal
        result = result.replace("../", "");
        result = result.replace("..\\", "");
        result = result.replace("%2e%2e", "");
        result = result.replace("%252e", "");
        
        // Remove null bytes
        result = result.replace("\x00", "");
        
        // Normalize slashes
        result = result.replace('\\', "/");
        
        result
    }
}

// =============================================================================
// VALIDATION RESULT
// =============================================================================

/// Validation result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidationResult {
    pub valid: bool,
    pub errors: Vec<String>,
}

impl ValidationResult {
    pub fn valid() -> Self {
        Self {
            valid: true,
            errors: Vec::new(),
        }
    }

    pub fn invalid(errors: Vec<String>) -> Self {
        Self {
            valid: false,
            errors,
        }
    }
}

// =============================================================================
// VALIDATION HELPERS
// =============================================================================

/// Validate token transfer request
pub fn validate_token_transfer(
    from: &str,
    to: &str,
    value: &str,
    token: &str,
) -> ValidationResult {
    let validator = Validator::new();
    let mut errors = Vec::new();

    if !validator.validate_address(from) {
        errors.push("Invalid from address".to_string());
    }
    if !validator.validate_address(to) {
        errors.push("Invalid to address".to_string());
    }
    if !validator.validate_address(token) {
        errors.push("Invalid token address".to_string());
    }
    if value.is_empty() {
        errors.push("Invalid value".to_string());
    }

    if errors.is_empty() {
        ValidationResult::valid()
    } else {
        ValidationResult::invalid(errors)
    }
}

/// Validate NFT transfer request
pub fn validate_nft_transfer(
    from: &str,
    to: &str,
    token: &str,
    token_id: &str,
) -> ValidationResult {
    let validator = Validator::new();
    let mut errors = Vec::new();

    if !validator.validate_address(from) {
        errors.push("Invalid from address".to_string());
    }
    if !validator.validate_address(to) {
        errors.push("Invalid to address".to_string());
    }
    if !validator.validate_address(token) {
        errors.push("Invalid token address".to_string());
    }
    if !validator.validate_token_id(token_id) {
        errors.push("Invalid token ID".to_string());
    }

    if errors.is_empty() {
        ValidationResult::valid()
    } else {
        ValidationResult::invalid(errors)
    }
}

/// Validate contract call
pub fn validate_contract_call(to: &str, data: &str) -> ValidationResult {
    let validator = Validator::new();
    let mut errors = Vec::new();

    if !validator.validate_address(to) {
        errors.push("Invalid contract address".to_string());
    }
    if data.is_empty() {
        errors.push("Invalid data".to_string());
    }
    if !data.starts_with("0x") {
        errors.push("Data must start with 0x".to_string());
    }

    if errors.is_empty() {
        ValidationResult::valid()
    } else {
        ValidationResult::invalid(errors)
    }
}

// =============================================================================
// TESTS
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_address() {
        let v = Validator::new();
        
        assert!(v.validate_address("0x742d35Cc6634C0532925a3b844Bc454e4438f44e"));
        assert!(!v.validate_address("0x742d35Cc6634C0532925a3b844Bc454e4438f44"));
        assert!(!v.validate_address("invalid"));
    }

    #[test]
    fn test_tx_hash() {
        let v = Validator::new();
        
        let hash = "0x5c504ed1a82b1c5d8d9a3b0c9f0c5a1d6e3a1b2c3d4e5f6a7b8c9d0e1f2a3b4c5d6e7f8a9b0c1d2e3f4a5b6c7d8e9f0a1b2c3d4e5f6a7b8c9d0e1f2a3b4c5";
        assert!(v.validate_tx_hash(hash));
    }

    #[test]
    fn test_block_number() {
        let v = Validator::new();
        
        assert!(v.validate_block_number("15000000"));
        assert!(!v.validate_block_number("999999999"));
        assert!(!v.validate_block_number("abc"));
    }

    #[test]
    fn test_email() {
        let v = Validator::new();
        
        assert!(v.validate_email("test@example.com"));
        assert!(!v.validate_email("invalid"));
    }

    #[test]
    fn test_sanitize_html() {
        let v = Validator::new();
        
        let result = v.sanitize_html("<script>alert(1)</script>test");
        
        assert!(!result.contains("<script>"));
    }

    #[test]
    fn test_sanitize_path() {
        let v = Validator::new();
        
        let result = v.sanitize_path("../etc/passwd");
        
        assert!(!result.contains(".."));
    }
}