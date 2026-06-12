//! Approval Service Types

use serde::{Deserialize, Serialize};

// =============================================================================
// APPROVAL SERVICE
// =============================================================================

/// Approval
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Approval {
    pub owner: String,
    pub spender: String,
    pub token: String,
    pub amount: u64,
    pub block: u64,
}

/// Token Allowance
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenAllowance {
    pub token: String,
    pub owner: String,
    pub spender: String,
    pub allowance: u64,
    pub updated_at: u64,
}

/// Approval Service
pub struct Service {
    approvals: std::collections::HashMap<String, Approval>,
}

impl Service {
    pub fn new() -> Self {
        Self {
            approvals: std::collections::HashMap::new(),
        }
    }

    /// Add approval
    pub fn add_approval(&mut self, key: String, approval: Approval) {
        self.approvals.insert(key, approval);
    }

    /// Get approval
    pub fn get_approval(&self, key: &str) -> Option<&Approval> {
        self.approvals.get(key)
    }
}

impl Default for Service {
    fn default() -> Self {
        Self::new()
    }
}