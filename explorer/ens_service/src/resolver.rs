//! ENS resolver reader.
//!
//! The resolver contract holds the records for a name: the address, the content
//! hash, the text records and the per-coin addresses. Each method here performs
//! the corresponding read-only call and returns what the chain reports.

use std::sync::Arc;

use ethers::core::types::Address;

use crate::chain::{decode_string, word_to_address, word_to_hex, Chain};
use crate::crypto::{namehash, validate_name};
use crate::database::Database;
use crate::errors::{Error, Result};
use crate::types::{CoinAddress, ENSRecord, TextRecord};

const ADDR_OF: &str = "addr(bytes32)";
const TEXT_OF: &str = "text(bytes32,string)";
const CONTENTHASH_OF: &str = "contenthash(bytes32)";
const COIN_ADDR_OF: &str = "addr(bytes32,uint256)";

pub struct Resolver {
    chain: Chain,
    database: Arc<Database>,
}

impl Resolver {
    /// Build a resolver reader.
    pub fn new(rpc_url: &str, database: Arc<Database>) -> Result<Self> {
        Ok(Self {
            chain: Chain::new(rpc_url)?,
            database,
        })
    }

    /// `resolver.addr(node)`.
    pub async fn resolve_address(&self, resolver: Address, name: &str) -> Result<Option<Address>> {
        if !validate_name(name) {
            return Err(Error::invalid_name(name));
        }

        let node = namehash(name);
        let raw = self.chain.call_raw(resolver, ADDR_OF, (node,)).await?;

        Ok(word_to_address(&raw).and_then(|address| address.parse().ok()))
    }

    /// `resolver.contenthash(node)`.
    pub async fn resolve_content_hash(
        &self,
        resolver: Address,
        name: &str,
    ) -> Result<Option<String>> {
        if !validate_name(name) {
            return Err(Error::invalid_name(name));
        }

        let node = namehash(name);
        let raw = self
            .chain
            .call_raw(resolver, CONTENTHASH_OF, (node,))
            .await?;

        if raw.iter().all(|byte| *byte == 0) {
            return Ok(None);
        }

        Ok(Some(word_to_hex(&raw)))
    }

    /// Read the text records named in `keys`.
    pub async fn get_text_records(
        &self,
        resolver: Address,
        name: &str,
        keys: &[String],
    ) -> Result<Vec<TextRecord>> {
        if !validate_name(name) {
            return Err(Error::invalid_name(name));
        }

        let node = namehash(name);
        let mut records = Vec::with_capacity(keys.len());

        for key in keys {
            let raw = self
                .chain
                .call_raw(resolver, TEXT_OF, (node, key.clone()))
                .await?;

            if let Some(value) = decode_string(&raw) {
                records.push(TextRecord {
                    key: key.clone(),
                    value,
                });
            }
        }

        Ok(records)
    }

    /// Read the address this name publishes for a coin type (SLIP-44).
    pub async fn get_coin_address(
        &self,
        resolver: Address,
        name: &str,
        coin_type: u32,
    ) -> Result<Option<String>> {
        if !validate_name(name) {
            return Err(Error::invalid_name(name));
        }

        let node = namehash(name);
        let raw = self
            .chain
            .call_raw(resolver, COIN_ADDR_OF, (node, ethers::core::types::U256::from(coin_type)))
            .await?;

        Ok(decode_string(&raw).or_else(|| word_to_address(&raw)))
    }

    /// Read several coin addresses at once.
    pub async fn get_coin_addresses(
        &self,
        resolver: Address,
        name: &str,
        coin_types: &[u32],
    ) -> Result<Vec<CoinAddress>> {
        let mut addresses = Vec::with_capacity(coin_types.len());

        for coin_type in coin_types {
            if let Some(address) = self.get_coin_address(resolver, name, *coin_type).await? {
                addresses.push(CoinAddress {
                    coin_type: *coin_type,
                    address,
                });
            }
        }

        Ok(addresses)
    }

    /// Persist a record.
    pub async fn set_record(&self, record: &ENSRecord) -> Result<()> {
        self.database.save_record(record).await
    }
}
