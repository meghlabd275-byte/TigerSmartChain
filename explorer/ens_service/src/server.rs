//! ENS Server
//! HTTP API server for ENS resolution

use std::sync::Arc;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;

use crate::cache::ENSCache;
use crate::database::Database;
use crate::errors::Result;
use crate::registry::ENSRegistry;
use crate::resolver::Resolver;
use crate::reverse::ReverseLookup;
use crate::types::ENSQuery;

pub struct Server {
    registry: Arc<ENSRegistry>,
    resolver: Arc<Resolver>,
    reverse: Arc<ReverseLookup>,
    cache: Arc<ENSCache>,
}

impl Server {
    pub fn new(
        registry: ENSRegistry,
        resolver: Resolver,
        reverse: ReverseLookup,
        cache: ENSCache,
    ) -> Self {
        Self {
            registry: Arc::new(registry),
            resolver: Arc::new(resolver),
            reverse: Arc::new(reverse),
            cache: Arc::new(cache),
        }
    }
    
    /// Resolve ENS name
    pub async fn resolve(&self, query: ENSQuery) -> Result<crate::types::ENSRecord> {
        self.registry.resolve(&query.name).await
    }
    
    /// Reverse lookup
    pub async fn reverse_lookup(&self, address: &str) -> Result<Option<String>> {
        let addr: ethers::core::types::Address = address.parse()
            .map_err(|_| crate::errors::Error::invalid_address(address))?;
        self.reverse.lookup(addr).await
    }
    
    /// Get domain info
    pub async fn get_domain(&self, name: &str) -> Result<Option<crate::types::ENSDomain>> {
        self.registry.get_domain(name).await
    }
    
    /// Check availability
    pub async fn is_available(&self, name: &str) -> Result<bool> {
        self.registry.is_available(name).await
    }
}