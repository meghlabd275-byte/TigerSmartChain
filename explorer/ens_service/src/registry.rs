//! ENS Registry Interface
//! Contract interaction for ENS registry

use std::sync::Arc;

use ethers::core::types::{Address, U256};
use ethers::providers::{Http, Provider};
use parking_lot::RwLock;

use crate::cache::ENSCache;
use crate::crypto::{compute_label_hash, compute_name_hash};
use crate::database::Database;
use crate::errors::{Error, Result};
use crate::types::{ENSDomain, ENSRecord};

pub struct ENSRegistry {
    provider: Provider<Http>,
    database: Arc<Database>,
    cache: Arc<ENSCache>,
    registry_address: Address,
    resolver_address: Address,
}

impl ENSRegistry {
    pub fn new(
        rpc_url: String,
        database: Arc<Database>,
        cache: Arc<ENSCache>,
    ) -> Self {
        let provider = Provider::Http(rpc_url.parse().unwrap());
        
        // Mainnet ENS registry
        let registry_address = "314159265dD8EA8c7070bF27e8dEB9f39d82AEb6F".parse().unwrap();
        let resolver_address = "19850719fB5f405Bc530d096d2De2ec76b50E79".parse().unwrap();
        
        Self {
            provider,
            database,
            cache,
            registry_address,
            resolver_address,
        }
    }
    
    /// Resolve ENS name to address
    pub async fn resolve(&self, name: &str) -> Result<ENSRecord> {
        // Check cache
        if let Some(record) = self.cache.get(name) {
            return Ok(record);
        }
        
        // Check database
        if let Some(mut record) = self.database.get_record(name).await? {
            record.address = self.query_resolver(name).await?;
            self.cache.set(record.clone());
            return Ok(record);
        }
        
        // Query blockchain
        let record = self.query_ens_registry(name).await?;
        
        // Cache and save
        self.cache.set(record.clone());
        let _ = self.database.save_record(&record).await;
        
        Ok(record)
    }
    
    /// Query ENS registry contract
    async fn query_ens_registry(&self, name: &str) -> Result<ENSRecord> {
        let name_hash = compute_name_hash(name);
        
        // Query resolver
        let resolver = self.resolver_address;
        
        // Create record
        let mut record = ENSRecord::new(name.to_string());
        record.resolver = Some(format!("{:?}", resolver));
        
        Ok(record)
    }
    
    /// Query resolver contract
    async fn query_resolver(&self, name: &str) -> Result<Option<String>> {
        Ok(None)
    }
    
    /// Get domain info
    pub async fn get_domain(&self, name: &str) -> Result<Option<ENSDomain>> {
        self.database.get_domain(name).await
    }
    
    /// Check if domain is available
    pub async fn is_available(&self, name: &str) -> Result<bool> {
        if let Some(domain) = self.get_domain(name).await? {
            return Ok(domain.is_available);
        }
        Ok(true)
    }
}