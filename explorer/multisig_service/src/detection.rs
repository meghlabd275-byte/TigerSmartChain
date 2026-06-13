//! Multisig Detection

use crate::types::{Config, MultisigInfo};

pub struct Detector {
    config: Config,
}

impl Detector {
    pub fn new(config: Config) -> Self {
        Self { config }
    }
    
    /// Detect if address is multisig
    pub fn detect(&self, address: &str, bytecode: &str) -> MultisigInfo {
        let is_gnosis = bytecode.contains("dafecc80") || bytecode.contains("ce11ed6f");
        let is_safe = bytecode.contains("a6b71e26");
        
        MultisigInfo {
            address: address.to_string(),
            threshold: if is_gnosis || is_safe { 2 } else { 1 },
            owners: vec![],
            implementation: if is_gnosis { "Gnosis Safe".to_string() } else { "Unknown".to_string() },
            is_gnosis_safe: is_gnosis,
            is_multisig: is_gnosis || is_safe,
        }
    }
}