//! Entry Point Interface

use crate::types::{AccountAbstractionInfo, Config};

pub struct EntryPoint {
    config: Config,
}

impl EntryPoint {
    pub fn new(config: Config) -> Self {
        Self { config }
    }
    
    /// Get entry point address
    pub fn address(&self) -> &str {
        &self.config.entry_point
    }
    
    /// Validate user operation
    pub async fn validate_user_op(
        &self,
        _user_op: &crate::user_op::UserOperation,
    ) -> Result<crate::validation::ValidationResult, crate::errors::Error> {
        Ok(crate::validation::ValidationResult {
            sender: _user_op.sender.clone(),
            nonce: _user_op.nonce.clone(),
            valid_after: 0,
            valid_until: u64::MAX,
        })
    }
    
    /// Simulate validation
    pub async fn simulate_validation(
        &self,
        _user_op: &crate::user_op::UserOperation,
    ) -> Result<crate::validation::ValidationResult, crate::errors::Error> {
        self.validate_user_op(_user_op).await
    }
}