//! TigerScan State DB Module

pub mod types;

pub use types::*;

use rocksdb::{Options, DB};
use std::path::Path;

impl StateDB {
    /// Create a new StateDB with persistent storage
    pub fn with_path<P: AsRef<Path>>(path: P) -> Self {
        let mut opts = Options::default();
        opts.create_if_missing(true);
        let db = DB::open(&opts, path).ok();

        Self {
            db,
            accounts: std::collections::HashMap::new(),
            code: std::collections::HashMap::new(),
            storage: std::collections::HashMap::new(),
        }
    }

    /// Get account data
    pub fn get_account_persistent(&self, address: &str) -> Option<Vec<u8>> {
        if let Some(db) = &self.db {
            db.get(address).ok().flatten()
        } else {
            self.get_account(address).cloned()
        }
    }

    /// Set account data
    pub fn set_account_persistent(&mut self, address: String, data: Vec<u8>) {
        if let Some(db) = &self.db {
            let _ = db.put(&address, &data);
        } else {
            self.set_account(address, data);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_persistent_statedb() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("statedb");

        {
            let mut db = StateDB::with_path(&path);
            db.set_account_persistent("0x123".to_string(), vec![1, 2, 3]);
        }

        {
            let db = StateDB::with_path(&path);
            let data = db.get_account_persistent("0x123");
            assert_eq!(data, Some(vec![1, 2, 3]));
        }
    }
}