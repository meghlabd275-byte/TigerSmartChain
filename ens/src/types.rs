//! ENS Types

use serde::{Deserialize, Serialize};

// =============================================================================
// ENS
// =============================================================================

/// ENS Record
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Record {
    pub name: String,
    pub owner: String,
    pub resolver: String,
    pub ttl: u64,
    pub address: Option<String>,
}

/// ENS Registry
pub struct Registry {
    records: std::collections::HashMap<String, Record>,
}

impl Registry {
    pub fn new() -> Self {
        Self {
            records: std::collections::HashMap::new(),
        }
    }

    /// Set record
    pub fn set_record(&mut self, name: String, record: Record) {
        self.records.insert(name, record);
    }

    /// Get record
    pub fn get_record(&self, name: &str) -> Option<&Record> {
        self.records.get(name)
    }
}

impl Default for Registry {
    fn default() -> Self {
        Self::new()
    }
}