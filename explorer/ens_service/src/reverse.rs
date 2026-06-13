//! ENS Reverse Lookup
//! Reverse resolution from address to name

use std::sync::Arc;

use ethers::core::types::Address;
use ethers::providers::{Http, Provider};

use crate::database::Database;
use crate::errors::Result;

pub struct ReverseLookup {
    provider: Provider<Http>,
    database: Arc<Database>,
}

impl ReverseLookup {
    pub fn new(rpc_url: String, database: Arc<Database>) -> Self {
        let provider = Provider::Http(rpc_url.parse().unwrap());
        Self { provider, database }
    }
    
    /// Perform reverse lookup
    pub async fn lookup(&self, address: Address) -> Result<Option<String>> {
        // Construct reverse name
        let reversed = format!("{:x}.addr.reverse", address);
        
        // Query for reverse record
        if let Some(record) = self.database.get_record(&reversed).await? {
            return Ok(Some(record.name));
        }
        
        Ok(None)
    }
    
    /// Get all reverse records for address
    pub async fn get_all_records(&self, address: Address) -> Result<Vec<String>> {
        let name = format!("{:x}.addr.reverse", address);
        
        // In production, would query database for all reverse records
        Ok(vec![name])
    }
}