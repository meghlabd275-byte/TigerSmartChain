//! RocksDB Types

use serde::{Deserialize, Serialize};

// =============================================================================
// ROCKSDB
// =============================================================================

/// Database
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Database {
    path: String,
}

impl Database {
    pub fn new(path: String) -> Self {
        Self { path }
    }

    /// Get path
    pub fn path(&self) -> &str {
        &self.path
    }
}

/// Iterator
pub struct Iterator {
    // Placeholder
}