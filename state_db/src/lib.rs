//! TigerSmartChain State DB Module
//!
//! Provides persistent storage for accounts, contract code, and storage slots using RocksDB.

pub mod types;

pub use types::*;

use rocksdb::{Options, DB, ColumnFamilyDescriptor};
use std::path::Path;

pub const CF_ACCOUNTS: &str = "accounts";
pub const CF_CODE: &str = "code";
pub const CF_STORAGE: &str = "storage";

impl StateDB {
    /// Create a new StateDB with persistent storage using column families
    pub fn with_path<P: AsRef<Path>>(path: P) -> Self {
        let mut opts = Options::default();
        opts.create_if_missing(true);
        opts.create_missing_column_families(true);

        let cf_accounts = ColumnFamilyDescriptor::new(CF_ACCOUNTS, Options::default());
        let cf_code = ColumnFamilyDescriptor::new(CF_CODE, Options::default());
        let cf_storage = ColumnFamilyDescriptor::new(CF_STORAGE, Options::default());

        let db = DB::open_cf_descriptors(&opts, path, vec![cf_accounts, cf_code, cf_storage]).ok();

        Self {
            db,
            accounts: std::collections::HashMap::new(),
            code: std::collections::HashMap::new(),
            storage: std::collections::HashMap::new(),
        }
    }

    /// Get account data from persistent storage
    pub fn get_account_persistent(&self, address: &str) -> Option<Vec<u8>> {
        if let Some(db) = &self.db {
            let cf = db.cf_handle(CF_ACCOUNTS)?;
            db.get_cf(cf, address).ok().flatten()
        } else {
            self.get_account(address).cloned()
        }
    }

    /// Set account data in persistent storage
    pub fn set_account_persistent(&mut self, address: String, data: Vec<u8>) {
        if let Some(db) = &self.db {
            if let Some(cf) = db.cf_handle(CF_ACCOUNTS) {
                let _ = db.put_cf(cf, &address, &data);
            }
        }
        self.set_account(address, data);
    }

    /// Get contract code from persistent storage
    pub fn get_code_persistent(&self, address: &str) -> Option<Vec<u8>> {
        if let Some(db) = &self.db {
            let cf = db.cf_handle(CF_CODE)?;
            db.get_cf(cf, address).ok().flatten()
        } else {
            self.get_code(address).cloned()
        }
    }

    /// Set contract code in persistent storage
    pub fn set_code_persistent(&mut self, address: String, code: Vec<u8>) {
        if let Some(db) = &self.db {
            if let Some(cf) = db.cf_handle(CF_CODE) {
                let _ = db.put_cf(cf, &address, &code);
            }
        }
        self.set_code(address, code);
    }

    /// Get storage value from persistent storage
    pub fn get_storage_persistent(&self, address: &str, key: &[u8]) -> Option<Vec<u8>> {
        if let Some(db) = &self.db {
            let cf = db.cf_handle(CF_STORAGE)?;
            let storage_key = self.make_storage_key(address, key);
            db.get_cf(cf, storage_key).ok().flatten()
        } else {
            self.get_storage(address, key).cloned()
        }
    }

    /// Set storage value in persistent storage
    pub fn set_storage_persistent(&mut self, address: String, key: Vec<u8>, value: Vec<u8>) {
        if let Some(db) = &self.db {
            if let Some(cf) = db.cf_handle(CF_STORAGE) {
                let storage_key = self.make_storage_key(&address, &key);
                let _ = db.put_cf(cf, storage_key, &value);
            }
        }
        self.set_storage(address, key, value);
    }

    fn make_storage_key(&self, address: &str, key: &[u8]) -> Vec<u8> {
        let mut combined = address.as_bytes().to_vec();
        combined.extend_from_slice(key);
        combined
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_persistent_statedb_accounts() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("statedb_acc");

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

    #[test]
    fn test_persistent_statedb_code() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("statedb_code");

        {
            let mut db = StateDB::with_path(&path);
            db.set_code_persistent("0x456".to_string(), vec![4, 5, 6]);
        }

        {
            let db = StateDB::with_path(&path);
            let data = db.get_code_persistent("0x456");
            assert_eq!(data, Some(vec![4, 5, 6]));
        }
    }

    #[test]
    fn test_persistent_statedb_storage() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("statedb_storage");

        {
            let mut db = StateDB::with_path(&path);
            db.set_storage_persistent("0x789".to_string(), vec![0, 1], vec![7, 8, 9]);
        }

        {
            let db = StateDB::with_path(&path);
            let data = db.get_storage_persistent("0x789", &[0, 1]);
            assert_eq!(data, Some(vec![7, 8, 9]));
        }
    }
}
