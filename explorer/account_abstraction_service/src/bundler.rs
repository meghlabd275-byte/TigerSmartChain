//! AA Bundler

use crate::user_op::UserOperation;
use crate::errors::{Error, Result};

pub struct Bundler {
    entry_point: String,
    bundler_url: String,
}

impl Bundler {
    pub fn new(entry_point: String, bundler_url: String) -> Self {
        Self {
            entry_point,
            bundler_url,
        }
    }
    
    /// Send user operation
    pub async fn send_user_op(
        &self,
        user_op: UserOperation,
    ) -> Result<String> {
        // In production, would send to bundler
        Ok(format!("0x{:x}", rand::random::<u256>()))
    }
    
    /// Estimate gas
    pub async fn estimate_gas(
        &self,
        user_op: &UserOperation,
    ) -> Result<u64> {
        Ok(21000)
    }
}