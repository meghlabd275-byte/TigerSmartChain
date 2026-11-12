//! ENS registry and resolver interaction.
//!
//! Resolution follows EIP-137 exactly:
//!
//! 1. `registry.resolver(node)` yields the resolver contract for the name.
//! 2. `resolver.addr(node)` yields the address the name points at.
//!
//! A name with no resolver, or a resolver that returns the zero address, is
//! unresolved. Nothing is invented: when the node cannot be reached the caller
//! receives an error.

use std::sync::Arc;

use ethers::core::types::Address;

use crate::cache::ENSCache;
use crate::chain::{decode_string, word_to_address, word_to_hex, Chain};
use crate::crypto::{namehash, validate_name};
use crate::database::Database;
use crate::errors::{Error, Result};
use crate::types::{ENSDomain, ENSRecord};

/// The registry and resolver calls this service uses.
const RESOLVER_OF: &str = "resolver(bytes32)";
const OWNER_OF: &str = "owner(bytes32)";
const ADDR_OF: &str = "addr(bytes32)";
const NAME_OF: &str = "name(bytes32)";
const TEXT_OF: &str = "text(bytes32,string)";
const CONTENTHASH_OF: &str = "contenthash(bytes32)";

pub struct ENSRegistry {
    chain: Chain,
    database: Arc<Database>,
    cache: Arc<ENSCache>,
    /// Address of the registry contract for this chain.
    registry_address: Address,
}

impl ENSRegistry {
    /// Build a registry reader.
    ///
    /// `registry_address` comes from configuration, because the deployment
    /// address differs per chain and guessing one would silently resolve
    /// against the wrong contract.
    pub fn new(
        rpc_url: &str,
        registry_address: &str,
        database: Arc<Database>,
        cache: Arc<ENSCache>,
    ) -> Result<Self> {
        let registry_address = registry_address.parse().map_err(|error| {
            Error::registry(format!("invalid registry address {registry_address}: {error}"))
        })?;

        Ok(Self {
            chain: Chain::new(rpc_url)?,
            database,
            cache,
            registry_address,
        })
    }

    /// The address of the registry contract in use.
    pub fn registry_address(&self) -> Address {
        self.registry_address
    }

    /// The resolver contract responsible for `name`, if any.
    pub async fn resolver_for(&self, name: &str) -> Result<Option<Address>> {
        let node = namehash(name);

        let raw = self
            .chain
            .call_raw(self.registry_address, RESOLVER_OF, (node,))
            .await?;

        Ok(word_to_address(&raw).and_then(|address| address.parse().ok()))
    }

    /// The owner of `name`, if the registry knows one.
    pub async fn owner_of(&self, name: &str) -> Result<Option<String>> {
        let node = namehash(name);

        let raw = self
            .chain
            .call_raw(self.registry_address, OWNER_OF, (node,))
            .await?;

        Ok(word_to_address(&raw))
    }

    /// Resolve `name` to a full record.
    ///
    /// The cache and database are consulted first so a hot lookup is served
    /// without a round trip, but a record that is not stored anywhere is always
    /// fetched from the chain.
    pub async fn resolve(&self, name: &str) -> Result<ENSRecord> {
        if !validate_name(name) {
            return Err(Error::invalid_name(name));
        }

        if let Some(record) = self.cache.get(name) {
            return Ok(record);
        }

        let node = namehash(name);
        let resolver = self.resolver_for(name).await?;

        let mut record = match self.database.get_record(name).await? {
            Some(record) => record,
            None => ENSRecord::new(name.to_string()),
        };

        record.resolver = resolver.map(|address| format!("{address:?}"));

        if let Some(resolver) = resolver {
            record.address = self.resolve_address(resolver, node).await?;
            record.owner = self.owner_of(name).await?;
        }

        self.cache.set(record.clone());

        // A record read from the chain is worth persisting; a failure to write
        // must not fail the lookup.
        let _ = self.database.save_record(&record).await;

        Ok(record)
    }

    /// `resolver.addr(node)` for an already-known resolver.
    async fn resolve_address(&self, resolver: Address, node: [u8; 32]) -> Result<Option<String>> {
        let raw = self.chain.call_raw(resolver, ADDR_OF, (node,)).await?;
        Ok(word_to_address(&raw))
    }

    /// Read a text record, for example `avatar` or `email`.
    pub async fn text_record(&self, name: &str, key: &str) -> Result<Option<String>> {
        let node = namehash(name);

        let Some(resolver) = self.resolver_for(name).await? else {
            return Ok(None);
        };

        let raw = self
            .chain
            .call_raw(resolver, TEXT_OF, (node, key.to_string()))
            .await?;

        Ok(decode_string(&raw))
    }

    /// Read the content hash (IPFS or Swarm) of a name.
    pub async fn content_hash(&self, name: &str) -> Result<Option<String>> {
        let node = namehash(name);

        let Some(resolver) = self.resolver_for(name).await? else {
            return Ok(None);
        };

        let raw = self
            .chain
            .call_raw(resolver, CONTENTHASH_OF, (node,))
            .await?;

        if raw.iter().all(|byte| *byte == 0) {
            return Ok(None);
        }

        Ok(Some(word_to_hex(&raw)))
    }

    /// Domain metadata, assembled from the registry and the database.
    pub async fn get_domain(&self, name: &str) -> Result<Option<ENSDomain>> {
        if !validate_name(name) {
            return Err(Error::invalid_name(name));
        }

        let mut domain = match self.database.get_domain(name).await? {
            Some(domain) => domain,
            None => ENSDomain {
                name: name.to_string(),
                label_hash: None,
                name_hash: None,
                owner: None,
                resolver: None,
                ttl: None,
                is_eth_2ld: name.split('.').count() == 2,
                registration_date: None,
                expiry_date: None,
                is_available: true,
            },
        };

        let node = namehash(name);
        domain.name_hash = Some(word_to_hex(&node));
        domain.label_hash = name
            .split('.')
            .next()
            .map(|label| word_to_hex(&crate::crypto::label_hash(label)));

        let resolver = self.resolver_for(name).await?;
        domain.resolver = resolver.map(|address| format!("{address:?}"));

        domain.owner = self.owner_of(name).await?;
        domain.is_available = domain.owner.is_none();

        Ok(Some(domain))
    }

    /// Whether `name` has no owner and can therefore be registered.
    pub async fn is_available(&self, name: &str) -> Result<bool> {
        if !validate_name(name) {
            return Err(Error::invalid_name(name));
        }

        Ok(self.owner_of(name).await?.is_none())
    }

    /// `resolver.name(node)`, used by the reverse path.
    pub async fn name_of(&self, resolver: Address, node: [u8; 32]) -> Result<Option<String>> {
        let raw = self.chain.call_raw(resolver, NAME_OF, (node,)).await?;
        Ok(decode_string(&raw))
    }

    /// True when the configured registry answers calls.
    pub async fn is_reachable(&self) -> bool {
        self.chain.is_reachable().await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_abis_are_the_published_ones() {
        assert_eq!(Chain::selector(RESOLVER_OF), [0x01, 0x78, 0xb8, 0xbf]);
        assert_eq!(Chain::selector(OWNER_OF), [0x02, 0x57, 0x1b, 0xe3]);
        assert_eq!(Chain::selector(ADDR_OF), [0x3b, 0x3b, 0x57, 0xde]);
        assert_eq!(Chain::selector(NAME_OF), [0x69, 0x1f, 0x34, 0x31]);
    }
}
