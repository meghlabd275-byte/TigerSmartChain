//! Ethereum L1 chain adapter for the bridge.

use crate::{Chain, ChainConfig};
use ethers_providers::{Http, Provider};

/// Ethereum-specific adapter that wraps an ethers provider for the L1.
pub struct EthereumAdapter {
    pub chain: Chain,
    pub config: ChainConfig,
    pub provider: Option<Provider<Http>>,
}

impl EthereumAdapter {
    pub fn new(config: ChainConfig) -> Self {
        Self {
            chain: Chain::Ethereum,
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
