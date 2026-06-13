//! TigerScan Contract Diff Viewer Service
//! Compare contract versions and source code

use std::collections::HashMap;
use std::sync::Arc;

use anyhow::Result;
use chrono::{DateTime, Utc};
use ethers::core::types::{Address, H256};
use ethers::providers::{Http, Provider};
use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use thiserror::Error;
use tracing::{error, info, warn};

// ============================================================================
// Error Types
// ============================================================================

#[derive(Error, Debug)]
pub enum DiffError {
    #[error("RPC error: {0}")]
    Rpc(String),
    
    #[error("Not found: {0}")]
    NotFound(String),
    
    #[error("Parse error: {0}")]
    Parse(String),
}

// ============================================================================
// Configuration
// ============================================================================

#[derive(Debug, Clone, Deserialize)]
pub struct Config {
    pub rpc_url: String,
    pub max_versions: usize,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            rpc_url: std::env::var("RPC_URL")
                .unwrap_or_else(|_| "http://localhost:8545".to_string()),
            max_versions: 10,
        }
    }
}

// ============================================================================
// Data Types
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContractVersion {
    pub version: u32,
    pub address: String,
    pub bytecode: String,
    pub source_code: Option<String>,
    pub compiler: String,
    pub timestamp: i64,
    pub block_number: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContractDiff {
    pub address: String,
    pub from_version: u32,
    pub to_version: u32,
    pub additions: Vec<DiffLine>,
    pub deletions: Vec<DiffLine>,
    pub changes: Vec<DiffChange>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiffLine {
    pub line_number: usize,
    pub content: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiffChange {
    pub line_from: usize,
    pub line_to: usize,
    pub change_type: DiffChangeType,
    pub content: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DiffChangeType {
    Added,
    Removed,
    Modified,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VersionHistory {
    pub address: String,
    pub versions: Vec<ContractVersion>,
    pub bytecode_changes: Vec<BytecodeChange>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BytecodeChange {
    pub version: u32,
    pub bytecode: String,
    pub block_number: u64,
    pub timestamp: i64,
}

// ============================================================================
// Diff Service
// ============================================================================

pub struct ContractDiffService {
    config: Config,
    rpc: Provider<Http>,
    state: Arc<RwLock<DiffState>>,
}

#[derive(Debug)]
pub struct DiffState {
    pub versions: HashMap<String, Vec<ContractVersion>>,
    pub bytecode_history: HashMap<String, Vec<BytecodeChange>>,
}

impl ContractDiffService {
    pub async fn new(config: Config) -> Result<Self> {
        info!("Initializing Contract Diff Service");
        
        let rpc = Provider::<Http>::try_from(config.rpc_url.clone())?;
        
        let service = Self {
            config: config.clone(),
            rpc,
            state: Arc::new(RwLock::new(DiffState {
                versions: HashMap::new(),
                bytecode_history: HashMap::new(),
            })),
        };
        
        info!("Contract Diff Service initialized");
        Ok(service)
    }

    /// Add a new version
    pub fn add_version(&self, address: &str, version: ContractVersion) {
        let mut state = self.state.write();
        
        let versions = state.versions
            .entry(address.to_string())
            .or_insert_with(Vec::new);
        
        versions.push(version);
        
        // Keep only max versions
        if versions.len() > self.config.max_versions {
            versions.drain(0..versions.len() - self.config.max_versions);
        }
    }

    /// Get version history
    pub fn get_history(&self, address: &str) -> Option<VersionHistory> {
        let state = self.state.read();
        
        let versions = state.versions.get(address)?.clone();
        
        let bytecode_history = state.bytecode_history.get(address)
            .map(|v| v.clone())
            .unwrap_or_default();
        
        Some(VersionHistory {
            address: address.to_string(),
            versions,
            bytecode_changes: bytecode_history,
        })
    }

    /// Compare two versions
    pub fn compare(&self, address: &str, from_version: u32, to_version: u32) -> Option<ContractDiff> {
        let state = self.state.read();
        
        let versions = state.versions.get(address)?;
        
        let from = versions.iter().find(|v| v.version == from_version)?;
        let to = versions.iter().find(|v| v.version == to_version)?;
        
        // Compute line-by-line diff
        let from_lines: Vec<&str> = from.source_code
            .as_ref()
            .map(|s| s.lines().collect())
            .unwrap_or_default();
        
        let to_lines: Vec<&str> = to.source_code
            .as_ref()
            .map(|s| s.lines().collect())
            .unwrap_or_default();
        
        let mut additions = Vec::new();
        let mut deletions = Vec::new();
        let mut changes = Vec::new();
        
        // Simple diff algorithm
        let max_lines = from_lines.len().max(to_lines.len());
        
        for i in 0..max_lines {
            let from_line = from_lines.get(i).map(|s| s.to_string());
            let to_line = to_lines.get(i).map(|s| s.to_string());
            
            match (&from_line, &to_line) {
                (Some(f), Some(t)) if f != t => {
                    changes.push(DiffChange {
                        line_from: i + 1,
                        line_to: i + 1,
                        change_type: DiffChangeType::Modified,
                        content: format!("- {}\n+ {}", f, t),
                    });
                }
                (None, Some(t)) => {
                    additions.push(DiffLine {
                        line_number: i + 1,
                        content: t.to_string(),
                    });
                }
                (Some(f), None) => {
                    deletions.push(DiffLine {
                        line_number: i + 1,
                        content: f.to_string(),
                    });
                }
                _ => {}
            }
        }
        
        Some(ContractDiff {
            address: address.to_string(),
            from_version,
            to_version,
            additions,
            deletions,
            changes,
        })
    }

    /// Detect bytecode changes
    pub fn detect_bytecode_changes(&self, address: &str) -> Vec<BytecodeChange> {
        let state = self.state.read();
        
        let history = state.bytecode_history.get(address);
        
        if let Some(history) = history {
            let mut changes = Vec::new();
            let mut prev_bytecode = String::new();
            
            for (i, bc) in history.iter().enumerate() {
                if !prev_bytecode.is_empty() && prev_bytecode != bc.bytecode {
                    changes.push(BytecodeChange {
                        version: i as u32,
                        bytecode: bc.bytecode.clone(),
                        block_number: bc.block_number,
                        timestamp: bc.timestamp,
                    });
                }
                prev_bytecode = bc.bytecode.clone();
            }
            
            changes
        } else {
            vec![]
        }
    }
}

// ============================================================================
// API Types
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiffApiRequest {
    pub address: Option<String>,
    pub from_version: Option<u32>,
    pub to_version: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiffApiResponse {
    pub success: bool,
    pub result: Option<serde_json::Value>,
    pub error: Option<String>,
}

// ============================================================================
// Utility Functions
// ============================================================================

/// Simple line-by-line diff
pub fn compute_diff(old: &str, new: &str) -> (Vec<String>, Vec<String>) {
    let old_lines: Vec<&str> = old.lines().collect();
    let new_lines: Vec<&str> = new.lines().collect();
    
    let mut additions = Vec::new();
    let mut deletions = Vec::new();
    
    // Simple diff
    let max = old_lines.len().max(new_lines.len());
    
    for i in 0..max {
        let old_line = old_lines.get(i);
        let new_line = new_lines.get(i);
        
        if old_line.is_none() && new_line.is_some() {
            additions.push(new_line.unwrap().to_string());
        } else if old_line.is_some() && new_line.is_none() {
            deletions.push(old_line.unwrap().to_string());
        } else if old_line != new_line {
            if let Some(n) = new_line {
                additions.push(n.to_string());
            }
            if let Some(o) = old_line {
                deletions.push(o.to_string());
            }
        }
    }
    
    (additions, deletions)
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_compute_diff() {
        let old = "line1\nline2\nline3";
        let new = "line1\nline2 modified\nline3";
        
        let (add, del) = compute_diff(old, new);
        
        assert!(!add.is_empty() || !del.is_empty());
    }
}