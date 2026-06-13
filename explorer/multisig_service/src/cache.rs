//! Multisig Cache

use std::collections::HashMap;
use std::sync::Arc;

use parking_lot::RwLock;

use crate::types::MultisigInfo;

pub struct Cache {
    entries: RwLock<HashMap<String, MultisigInfo>>,
}

impl Cache {
    pub fn new() -> Self {
        Self {
            entries: RwLock::new(HashMap::new()),
        }
    }
    
    pub fn get(&self, address: &str) -> Option<MultisigInfo> {
        self.entries.read().get(address).cloned()
    }
    
    pub fn set(&self, info: MultisigInfo) {
        self.entries.write().insert(info.address.clone(), info);
    }
}