//! TigerScan Decompiler Service - Bytecode to source analysis

use std::collections::HashMap;
use std::sync::Arc;
use anyhow::Result;
use serde::{Deserialize, Serialize};
use tracing::info;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DecompiledContract {
    pub address: String,
    pub bytecode: String,
    pub functions: Vec<DecompiledFunction>,
    pub variables: Vec<StorageVariable>,
    pub events: Vec<DecompiledEvent>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DecompiledFunction {
    pub name: String,
    pub signature: String,
    pub selector: String,
    pub visibility: String,
    pub inputs: Vec<Parameter>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Parameter {
    pub name: String,
    pub param_type: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StorageVariable {
    pub name: String,
    pub param_type: String,
    pub slot: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DecompiledEvent {
    pub name: String,
    pub signature: String,
    pub topic: String,
}

pub struct Decompiler {
    function_sigs: HashMap<String, String>,
}

impl Decompiler {
    pub fn new() -> Self {
        let mut function_sigs = HashMap::new();
        function_sigs.insert("0xa9059cbb".to_string(), "transfer(address,uint256)".to_string());
        function_sigs.insert("0x095ea7b3".to_string(), "approve(address,uint256)".to_string());
        function_sigs.insert("0x23b872dd".to_string(), "transferFrom(address,address,uint256)".to_string());
        function_sigs.insert("0x8c5be1e5".to_string(), "Transfer(address,address,uint256)".to_string());
        function_sigs.insert("0xa22cb465".to_string(), "SetApprovalForAll(address,bool)".to_string());
        Self { function_sigs }
    }

    pub fn decompile(&self, bytecode: &str) -> Result<DecompiledContract> {
        info!("Decompiling bytecode: {} bytes", bytecode.len());
        let bytecode = bytecode.trim_start_matches("0x");
        let bytes = hex::decode(bytecode)?;
        
        let mut functions = Vec::new();
        let mut events = Vec::new();
        
        // Find function selectors
        for i in 0..bytes.len().saturating_sub(4) {
            if bytes[i] == 0x60 && i + 4 < bytes.len() {
                let selector = format!("0x{:02x}{:02x}{:02x}{:02x}", bytes[i+1], bytes[i+2], bytes[i+3], bytes[i+4]);
                if let Some(sig) = self.function_sigs.get(&selector) {
                    let name = sig.split('(').next().unwrap_or("unknown").to_string();
                    functions.push(DecompiledFunction {
                        name: name.clone(),
                        signature: sig.clone(),
                        selector: selector[2..10].to_string(),
                        visibility: "external".to_string(),
                        inputs: vec![],
                    });
                }
            }
        }
        
        // Find storage operations
        let mut variables = Vec::new();
        for (i, &byte) in bytes.iter().enumerate() {
            if byte == 0x54 { // SLOAD
                let slot = i / 32;
                variables.push(StorageVariable {
                    name: format!("slot_{}", slot),
                    param_type: "uint256".to_string(),
                    slot,
                });
            }
        }
        
        Ok(DecompiledContract {
            address: String::new(),
            bytecode: bytecode.to_string(),
            functions,
            variables,
            events,
        })
    }
}

impl Default for Decompiler {
    fn default() -> Self { Self::new() }
}