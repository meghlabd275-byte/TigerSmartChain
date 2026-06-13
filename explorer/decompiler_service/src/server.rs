//! Decompiler Server

use crate::analysis::Analyzer;
use crate::patterns::PatternDetector;
use crate::types::{Config, DecompiledContract};

pub struct Server {
    config: Config,
}

impl Server {
    pub fn new(config: Config) -> Self {
        Self { config }
    }
    
    /// Decompile contract bytecode
    pub fn decompile(&self, address: &str, bytecode: &str) -> DecompiledContract {
        let (functions, variables, events) = Analyzer::analyze(bytecode);
        let contract_type = PatternDetector::detect(bytecode);
        
        DecompiledContract {
            address: address.to_string(),
            bytecode: bytecode.to_string(),
            functions,
            variables,
            events,
        }
    }
    
    /// Detect contract type
    pub fn detect_type(&self, bytecode: &str) -> String {
        let contract_type = PatternDetector::detect(bytecode);
        contract_type.as_str().to_string()
    }
}