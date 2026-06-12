//! Verifier Service Types

use serde::{Deserialize, Serialize};

// =============================================================================
// VERIFIER SERVICE
// =============================================================================

/// Verification Request
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VerificationRequest {
    pub address: String,
    pub source_code: String,
    pub compiler_version: String,
    pub abi: String,
}

/// Verified Contract
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VerifiedContract {
    pub address: String,
    pub source_code: String,
    pub compiler_version: String,
    pub abi: String,
    pub bytecode: String,
    pub verified_at: u64,
}

/// Verifier Service
pub struct Service {
    verified: std::collections::HashMap<String, VerifiedContract>,
}

impl Service {
    pub fn new() -> Self {
        Self {
            verified: std::collections::HashMap::new(),
        }
    }

    /// Verify
    pub fn verify(&mut self, address: String, verified: VerifiedContract) {
        self.verified.insert(address, verified);
    }

    /// Is verified
    pub fn is_verified(&self, address: &str) -> bool {
        self.verified.contains_key(address)
    }
}

impl Default for Service {
    fn default() -> Self {
        Self::new()
    }
}