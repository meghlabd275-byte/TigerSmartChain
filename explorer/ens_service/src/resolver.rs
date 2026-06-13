//! ENS Resolver
//! Contract resolver implementation

use std::sync::Arc;

use ethers::core::types::Address;
use ethers::providers::{Http, Provider};

use crate::database::Database;
use crate::errors::Result;
use crate::types::ENSRecord;

pub struct Resolver {
    provider: Provider<Http>,
    database: Arc<Database>,
}

impl Resolver {
    pub fn new(rpc_url: String, database: Arc<Database>) -> Self {
        let provider = Provider::Http(rpc_url.parse().unwrap());
        Self { provider, database }
    }
    
    /// Resolve address from name
    pub async fn resolve_address(&self, name: &str) -> Result<Option<Address>> {
        Ok(None)
    }
    
    /// Resolve content hash
    pub async fn resolve_content_hash(&self, name: &str) -> Result<Option<String>> {
        Ok(None)
    }
    
    /// Get text records
    pub async fn get_text_records(&self, name: &str) -> Result<Vec<(String, String)>> {
        Ok(vec![])
    }
    
    /// Get coin addresses
    pub async fn get_coin_addresses(&self, name: &str) -> Result<Vec<(u32, String)>> {
        Ok(vec![])
    }
    
    /// Set record
    pub async fn set_record(&self, record: &ENSRecord) -> Result<()> {
        self.database.save_record(record).await
    }
}