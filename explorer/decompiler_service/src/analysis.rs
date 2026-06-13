//! Bytecode Analysis
//! Advanced bytecode analysis for decompilation

use crate::types::{DecompiledContract, DecompiledEvent, DecompiledFunction, StorageVariable};

pub struct Analyzer;

impl Analyzer {
    /// Analyze bytecode and extract functions
    pub fn analyze(bytecode: &str) -> (Vec<DecompiledFunction>, Vec<StorageVariable>, Vec<DecompiledEvent>) {
        let bytes = match hex::decode(bytecode.trim_start_matches("0x")) {
            Ok(b) => b,
            Err(_) => return (vec![], vec![], vec![]),
        };
        
        let mut functions = Vec::new();
        let mut variables = Vec::new();
        
        // Function selector analysis
        let known_selectors = Self::known_selectors();
        for i in 0..bytes.len().saturating_sub(4) {
            if i + 4 < bytes.len() {
                let selector = format!(
                    "{:02x}{:02x}{:02x}{:02x}",
                    bytes[i], bytes[i+1], bytes[i+2], bytes[i+3]
                );
                if let Some(sig) = known_selectors.get(&selector) {
                    let name = sig.split('(').next().unwrap_or("unknown").to_string();
                    functions.push(DecompiledFunction {
                        name,
                        signature: sig.clone(),
                        selector: selector.clone(),
                        visibility: "external".to_string(),
                        inputs: vec![],
                    });
                }
            }
        }
        
        // Storage variable detection
        for (i, &byte) in bytes.iter().enumerate() {
            if byte == 0x54 && i + 32 <= bytes.len() {
                let slot = i / 32;
                variables.push(StorageVariable {
                    name: format!("storage_slot_{}", slot),
                    param_type: "uint256".to_string(),
                    slot,
                });
            }
        }
        
        // Event detection
        let event_selectors = Self::event_selectors();
        let mut events = Vec::new();
        for (selector, name) in event_selectors {
            events.push(DecompiledEvent {
                name: name.to_string(),
                signature: selector.to_string(),
                topic: selector.to_string(),
            });
        }
        
        (functions, variables, events)
    }
    
    fn known_selectors() -> std::collections::HashMap<String, String> {
        let mut map = std::collections::HashMap::new();
        map.insert("a9059cbb".to_string(), "transfer(address,uint256)".to_string());
        map.insert("095ea7b3".to_string(), "approve(address,uint256)".to_string());
        map.insert("23b872dd".to_string(), "transferFrom(address,address,uint256)".to_string());
        map.insert("8c5be1e5".to_string(), "Approval(address,address,uint256)".to_string());
        map.insert("a22cb465".to_string(), "setApprovalForAll(address,bool)".to_string());
        map.insert("42842e0e".to_string(), "safeTransferFrom(address,address,uint256)".to_string());
        map.insert("b88d4fde".to_string(), "safeTransferFrom(address,address,uint256,bytes)".to_string());
        map.insert("2e1a7d4d".to_string(), "withdraw(uint256)".to_string());
        map.insert("4e71d92d".to_string(), "execute(address,uint256,bytes)".to_string());
        map.insert("0c4a6c7d".to_string(), "execute(bytes)".to_string());
        map
    }
    
    fn event_selectors() -> Vec<(String, String)> {
        vec![
            ("Transfer(address,address,uint256)".to_string(), "Transfer".to_string()),
            ("Approval(address,address,uint256)".to_string(), "Approval".to_string()),
            ("TransferSingle(address,address,address,uint256,uint256)".to_string(), "TransferSingle".to_string()),
            ("TransferBatch(address,address,address,uint256[],uint256[])", "TransferBatch".to_string()),
        ]
    }
}