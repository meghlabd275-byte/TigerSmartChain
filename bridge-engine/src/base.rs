//! Base L2 chain adapter for the bridge.

use crate::{Chain, ChainConfig};
use ethers_providers::{Http, Provider};

/// Base adapter wrapping an ethers provider.
pub struct BaseAdapter {
    pub chain: Chain,
    pub config: ChainConfig,
    pub provider: Option<Provider<Http>>,
}

impl BaseAdapter {
    pub fn new(config: ChainConfig) -> Self {
        Self {
            chain: Chain::Base,
            config,
            provider: None,
        }
    }

    pub async fn connect(&mut self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let provider = Provider::<Http>::try_from(self.config.rpc_url.as_str())?;
        self.provider = Some(provider);
        Ok(())
    }

    pub fn provider(&self) -> Option<&Provider<Http>> {
        self.provider.as_ref()
    }
}
