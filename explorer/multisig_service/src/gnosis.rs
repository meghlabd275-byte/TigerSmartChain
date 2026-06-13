//! Gnosis Safe Interface

use crate::types::MultisigInfo;

pub struct GnosisSafe {
    master_copy: String,
}

impl GnosisSafe {
    pub fn new(master_copy: String) -> Self {
        Self { master_copy }
    }
    
    pub fn master_copy(&self) -> &str {
        &self.master_copy
    }
    
    pub fn is_compatible(&self, bytecode: &str) -> bool {
        bytecode.contains("dafecc80")
    }
}