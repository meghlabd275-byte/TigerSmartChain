//! ENS service entry point.
//!
//! The server owns the registry reader, the resolver reader and the reverse
//! reader, and exposes the operations the JSON-RPC layer calls.

use std::sync::Arc;

use ethers::core::types::Address;

use crate::cache::ENSCache;
use crate::crypto::validate_address;
use crate::errors::{Error, Result};
use crate::registry::ENSRegistry;
use crate::resolver::Resolver;
use crate::reverse::ReverseLookup;
use crate::types::{ENSDomain, ENSQuery, ENSRecord};

/// Text record keys read when the caller asks for text records.
const DEFAULT_TEXT_KEYS: [&str; 6] = ["avatar", "email", "url", "description", "notice", "keywords"];

/// Coin types read when the caller asks for multi-chain addresses: Ethereum,
/// Bitcoin, Litecoin, Dogecoin and a TGR placeholder for chain-specific records.
const DEFAULT_COIN_TYPES: [u32; 5] = [60, 0, 2, 3, 9666];

pub struct Server {
    registry: Arc<ENSRegistry>,
    resolver: Arc<Resolver>,
    reverse: Arc<ReverseLookup>,
    cache: Arc<ENSCache>,
}

impl Server {
    pub fn new(
        registry: Arc<ENSRegistry>,
        resolver: Arc<Resolver>,
        reverse: Arc<ReverseLookup>,
        cache: Arc<ENSCache>,
    ) -> Self {
        Self {
            registry,
            resolver,
            reverse,
            cache,
        }
    }

    /// Resolve a name, optionally filling in text records and coin addresses.
    pub async fn resolve(&self, query: ENSQuery) -> Result<ENSRecord> {
        let mut record = self.registry.resolve(&query.name).await?;

        let wants_text = query.include_text.unwrap_or(false);
        let wants_coins = query.include_coins.unwrap_or(false);

        if !wants_text && !wants_coins {
            return Ok(record);
        }

        // Both extra record kinds live on the resolver, so a name without one
        // is returned as-is rather than failing.
        let Some(resolver) = self.registry.resolver_for(&query.name).await? else {
            return Ok(record);
        };

        if wants_text {
            let keys: Vec<String> = DEFAULT_TEXT_KEYS
                .iter()
                .map(|key| (*key).to_string())
                .collect();

            record.text_records = Some(
                self.resolver
                    .get_text_records(resolver, &query.name, &keys)
                    .await?,
            );
        }

        if wants_coins {
            record.coin_addresses = Some(
                self.resolver
                    .get_coin_addresses(resolver, &query.name, &DEFAULT_COIN_TYPES)
                    .await?,
            );
        }

        Ok(record)
    }

    /// Reverse lookup, with the forward confirmation required by EIP-181.
    pub async fn reverse_lookup(&self, address: &str) -> Result<Option<String>> {
        if !validate_address(address) {
            return Err(Error::invalid_address(address));
        }

        let address: Address = address
            .parse()
            .map_err(|_| Error::invalid_address(address))?;

        self.reverse.lookup(address).await
    }

    /// Domain metadata.
    pub async fn get_domain(&self, name: &str) -> Result<Option<ENSDomain>> {
        self.registry.get_domain(name).await
    }

    /// Whether a name can be registered.
    pub async fn is_available(&self, name: &str) -> Result<bool> {
        self.registry.is_available(name).await
    }

    /// The resolver contract for a name, for callers that want to read records
    /// directly.
    pub async fn resolver_for(&self, name: &str) -> Result<Option<Address>> {
        self.registry.resolver_for(name).await
    }

    /// The cache, so the JSON-RPC layer can invalidate an entry after a write.
    pub fn cache(&self) -> &ENSCache {
        &self.cache
    }
}
