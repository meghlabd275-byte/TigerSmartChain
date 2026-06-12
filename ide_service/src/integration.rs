//! IDE Integration

use super::types::*;

// =============================================================================
// IDE INTEGRATION
// =============================================================================

/// Remix Integration
pub struct RemixIntegration {
    url: String,
}

impl RemixIntegration {
    pub fn new() -> Self {
        Self {
            url: "https://remix.ethereum.org".to_string(),
        }
    }

    /// Open in Remix
    pub fn open_in_remix(&self, project: &ContractProject) -> String {
        format!("{}/#{}", self.url, project.id)
    }

    /// Compile in Remix
    pub fn compile(&self, _project: &ContractProject) -> CompilationResult {
        CompilationResult {
            success: true,
            bytecode: "0x".to_string(),
            abi: "[]".to_string(),
            errors: vec![],
            warnings: vec![],
        }
    }
}

impl Default for RemixIntegration {
    fn default() -> Self {
        Self::new()
    }
}