//! User Operation Types

use serde::{Deserialize, Serialize};

/// User Operation (ERC-4337)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserOperation {
    pub sender: String,
    pub nonce: String,
    pub init_code: String,
    pub call_data: String,
    pub call_gas_limit: String,
    pub verification_gas_limit: String,
    pub pre_verification_gas: String,
    pub max_fee_per_gas: String,
    pub max_priority_fee_per_gas: String,
    pub signature: String,
}

impl UserOperation {
    /// Get sender address
    pub fn sender(&self) -> &str {
        &self.sender
    }
    
    /// Get nonce
    pub fn nonce(&self) -> &str {
        &self.nonce
    }
    
    /// Encode user operation for signing
    pub fn encode(&self) -> Vec<u8> {
        // In production, would use proper ABI encoding
        let mut encoded = Vec::new();
        encoded.extend_from_slice(self.sender.as_bytes());
        encoded.extend_from_slice(self.nonce.as_bytes());
        encoded
    }
}