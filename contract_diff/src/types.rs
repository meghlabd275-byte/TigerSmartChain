//! Contract Diff Types

use serde::{Deserialize, Serialize};

// =============================================================================
// CONTRACT DIFF
// =============================================================================

/// Contract Version
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContractVersion {
    pub address: String,
    pub bytecode: String,
    pub block: u64,
}

/// Diff
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Diff {
    pub address: String,
    pub old_version: u64,
    pub new_version: u64,
    pub changes: Vec<String>,
}

/// Contract Diff
pub struct ContractDiff {
    versions: std::collections::HashMap<String, Vec<ContractVersion>>,
}

impl ContractDiff {
    pub fn new() -> Self {
        Self {
            versions: std::collections::HashMap::new(),
        }
    }

    /// Add version
    pub fn add_version(&mut self, address: String, version: ContractVersion) {
        self.versions.entry(address).or_insert_with(Vec::new).push(version);
    }

    /// Get latest version
    pub fn get_latest(&self, address: &str) -> Option<&ContractVersion> {
        self.versions.get(address).and_then(|v| v.last())
    }
}

impl Default for ContractDiff {
    fn default() -> Self {
        Self::new()
    }
}