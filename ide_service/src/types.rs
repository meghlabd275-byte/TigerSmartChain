//! IDE Service Types

use serde::{Deserialize, Serialize};

// =============================================================================
// IDE SERVICE
// =============================================================================

/// Contract Project
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContractProject {
    pub id: String,
    pub name: String,
    pub contracts: Vec<ContractFile>,
    pub compiler: String,
    pub version: String,
}

/// Contract File
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContractFile {
    pub name: String,
    pub content: String,
    pub language: String,
}

/// Compilation Result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompilationResult {
    pub success: bool,
    pub bytecode: String,
    pub abi: String,
    pub errors: Vec<String>,
    pub warnings: Vec<String>,
}

/// Deployment
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Deployment {
    pub contract: String,
    pub address: String,
    pub deployer: String,
    pub tx_hash: String,
    pub block: u64,
}

/// IDE Service
pub struct Service {
    projects: std::collections::HashMap<String, ContractProject>,
}

impl Service {
    pub fn new() -> Self {
        Self {
            projects: std::collections::HashMap::new(),
        }
    }

    /// Add project
    pub fn add_project(&mut self, project: ContractProject) {
        self.projects.insert(project.id.clone(), project);
    }

    /// Get project
    pub fn get_project(&self, id: &str) -> Option<&ContractProject> {
        self.projects.get(id)
    }
}

impl Default for Service {
    fn default() -> Self {
        Self::new()
    }
}