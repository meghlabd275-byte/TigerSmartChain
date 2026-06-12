//! Validator Service Types

use serde::{Deserialize, Serialize};

// =============================================================================
// VALIDATOR SERVICE
// =============================================================================

/// Validator
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Validator {
    pub address: String,
    pub name: String,
    pub stake: u64,
    pub commission: u8,
    pub uptime: f64,
    pub status: String,
}

/// Validator Metrics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidatorMetrics {
    pub address: String,
    pub blocks_proposed: u64,
    pub blocks_missed: u64,
    pub uptime: f64,
    pub last_proposed: u64,
}

/// Validator Service
pub struct Service {
    validators: std::collections::HashMap<String, Validator>,
}

impl Service {
    pub fn new() -> Self {
        Self {
            validators: std::collections::HashMap::new(),
        }
    }

    /// Add validator
    pub fn add_validator(&mut self, validator: Validator) {
        self.validators.insert(validator.address.clone(), validator);
    }

    /// Get validator
    pub fn get_validator(&self, address: &str) -> Option<&Validator> {
        self.validators.get(address)
    }
}

impl Default for Service {
    fn default() -> Self {
        Self::new()
    }
}