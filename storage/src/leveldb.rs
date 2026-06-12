//! LevelDB

use super::types::*;

// =============================================================================
// LEVELDB
// =============================================================================

/// LevelDB
pub struct LevelDB {
    db: std::collections::HashMap<Vec<u8>, Vec<u8>>,
}

impl LevelDB {
    pub fn new(_path: &str) -> Self {
        Self {
            db: std::collections::HashMap::new(),
        }
    }
}

impl Storage for LevelDB {
    fn get(&self, key: &[u8]) -> Option<Vec<u8>> {
        self.db.get(key).cloned()
    }

    fn put(&mut self, key: Vec<u8>, value: Vec<u8>) {
        self.db.insert(key, value);
    }

    fn delete(&mut self, key: &[u8]) -> bool {
        self.db.remove(key).is_some()
    }
}