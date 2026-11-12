//! ENS reverse resolution: address to primary name.
//!
//! EIP-181: the record for an address lives under `<lowercase-hex>.addr.reverse`.
//! The forward-confirmed rule is applied here: a reverse record is only reported
//! when the name it claims also resolves back to the same address. Without that
//! check any address could claim any name.

use std::sync::Arc;

use ethers::core::types::Address;

use crate::chain::Chain;
use crate::crypto::{namehash, reverse_name};
use crate::database::Database;
use crate::errors::{Error, Result};
use crate::registry::ENSRegistry;

pub struct ReverseLookup {
    chain: Chain,
    database: Arc<Database>,
    registry: Arc<ENSRegistry>,
}

impl ReverseLookup {
    /// Build a reverse reader. It needs the registry so it can forward-confirm
    /// the claimed name.
    pub fn new(rpc_url: &str, database: Arc<Database>, registry: Arc<ENSRegistry>) -> Result<Self> {
        Ok(Self {
            chain: Chain::new(rpc_url)?,
            database,
            registry,
        })
    }

    /// The primary name of `address`, when a reverse record exists and the name
    /// resolves back to the same address.
    pub async fn lookup(&self, address: Address) -> Result<Option<String>> {
        let reverse = reverse_name(&format!("{address:?}"));

        let Some(record) = self.database.get_record(&reverse).await? else {
            return Ok(None);
        };

        let Some(name) = record.address else {
            return Ok(None);
        };

        // Forward-confirm: the name must resolve to the address that claimed it.
        let forward = self.registry.resolve(&name).await?;
        let confirmed = forward
            .address
            .as_deref()
            .and_then(|resolved| resolved.parse::<Address>().ok())
            .map(|resolved| resolved == address)
            .unwrap_or(false);

        if confirmed {
            Ok(Some(name))
        } else {
            Ok(None)
        }
    }

    /// The reverse node for `address`, which is what a resolver call would use.
    pub fn reverse_node(&self, address: Address) -> [u8; 32] {
        namehash(&reverse_name(&format!("{address:?}")))
    }

    /// Every name that has claimed `address`.
    pub async fn get_all_records(&self, address: Address) -> Result<Vec<String>> {
        let names = self.database.names_claiming(address).await?;

        if names.is_empty() {
            return Err(Error::not_found(format!("no reverse record for {address:?}")));
        }

        Ok(names)
    }

    /// Confirm that `name` resolves back to `address`.
    pub async fn confirm(&self, name: &str, address: Address) -> Result<bool> {
        let record = self.registry.resolve(name).await?;

        Ok(record
            .address
            .as_deref()
            .and_then(|resolved| resolved.parse::<Address>().ok())
            .map(|resolved| resolved == address)
            .unwrap_or(false))
    }

    /// True when the RPC endpoint answers.
    pub async fn is_reachable(&self) -> bool {
        self.chain.is_reachable().await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_reverse_node_is_the_namehash_of_the_reverse_name() {
        let address: Address = "0x1111111111111111111111111111111111111111".parse().unwrap();
        let expected = namehash("1111111111111111111111111111111111111111.addr.reverse");

        // The node is derived from the name, not from the address bytes.
        assert_eq!(
            namehash(&reverse_name(&format!("{address:?}"))),
            expected
        );
    }
}
