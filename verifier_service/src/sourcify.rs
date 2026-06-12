//! Sourcify Integration

use super::types::*;

// =============================================================================
// SOURCIFY
// =============================================================================

/// Sourcify Client
pub struct Client {
    url: String,
}

impl Client {
    pub fn new() -> Self {
        Self {
            url: "https://sourcify.dev".to_string(),
        }
    }

    /// Verify contract
    pub fn verify(&self, _request: &VerificationRequest) -> Result<VerifiedContract, String> {
        Err("Not implemented".to_string())
    }

    /// Check if verified
    pub fn is_verified(&self, _address: &str) -> bool {
        false
    }
}

impl Default for Client {
    fn default() -> Self {
        Self::new()
    }
}