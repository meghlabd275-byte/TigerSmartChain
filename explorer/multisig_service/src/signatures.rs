//! Multisig Signatures

use std::collections::HashMap;

/// Known multisig function signatures
pub struct Signatures;

impl Signatures {
    pub fn get() -> HashMap<String, String> {
        let mut map = HashMap::new();
        map.insert("dafecc80".to_string(), "execTransaction(address,uint256,bytes,uint8)".to_string());
        map.insert("ce11ed6f".to_string(), "setup(address[],uint256,bytes,address,address,uint256,address)".to_string());
        map.insert("b63f8a2b".to_string(), "signMessage(bytes)".to_string());
        map.insert("8b80e700".to_string(), "approveHash(bytes32)".to_string());
        map
    }
}