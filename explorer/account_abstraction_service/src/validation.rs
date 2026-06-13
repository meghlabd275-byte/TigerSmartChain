//! AA Validation

use crate::user_op::UserOperation;
use crate::validation::ValidationResult;
use crate::errors::Result;

pub struct Validator;

impl Validator {
    /// Validate user operation
    pub fn validate(&self, user_op: &UserOperation) -> Result<ValidationResult> {
        // Basic validation
        if user_op.sender.is_empty() {
            return Err(crate::errors::Error::validation("Sender is required"));
        }
        
        if !user_op.sender.starts_with("0x") {
            return Err(crate::errors::Error::validation("Invalid sender format"));
        }
        
        Ok(ValidationResult {
            sender: user_op.sender.clone(),
            nonce: user_op.nonce.clone(),
            valid_after: 0,
            valid_until: u64::MAX,
        })
    }
}