//! Token Revoker Types

use serde::{Deserialize, Serialize};

// =============================================================================
// TOKEN REVOKER
// =============================================================================

/// Revocation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Revocation {
    pub token: String,
    pub holder: String,
    pub amount: u64,
    pub reason: String,
    pub timestamp: u64,
}

/// Token Revoker
pub struct Revoker {
    revocations: std::collections::HashMap<String, Revocation>,
}

impl Revoker {
    pub fn new() -> Self {
        Self {
            revocations: std::collections::HashMap::new(),
        }
    }

    /// Revoke
    pub fn revoke(&mut self, key: String, revocation: Revocation) {
        self.revocations.insert(key, revocation);
    }

    /// Check revocation
    pub fn is_revoked(&self, key: &str) -> bool {
        self.revocations.contains_key(key)
    }
}

impl Default for Revoker {
    fn default() -> Self {
        Self::new()
    }
}