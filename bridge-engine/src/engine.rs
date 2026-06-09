//! Bridge Engine - Main Entry Point

use crate::{Chain, ChainConfig, BridgeConfig, Transfer, TransferStatus, TokenType};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

/// Bridge engine
pub struct BridgeEngine {
    config: BridgeConfig,
    transfers: Arc<RwLock<HashMap<String, Transfer>>>,
    chains: HashMap<Chain, ChainState>,
}

/// Chain state
struct ChainState {
    config: ChainConfig,
    provider: Option<ethers_providers::Provider<ethers_providers::Http>>,
}

/// Event emitted by bridge
#[derive(Debug, Clone)]
pub enum BridgeEvent {
    TransferInitiated(Transfer),
    TransferCompleted(Transfer),
    TransferFailed(Transfer),
    ValidatorSignature(String, String),
    RelayerMessage(String),
}

impl BridgeEngine {
    /// Create new bridge engine
    pub fn new(config: BridgeConfig) -> Self {
        let mut chains = HashMap::new();
        
        for chain_config in &config.chains {
            chains.insert(
                chain_config.chain,
                ChainState {
                    config: chain_config.clone(),
                    provider: None,
                },
            );
        }

        Self {
            config,
            transfers: Arc::new(RwLock::new(HashMap::new())),
            chains,
        }
    }

    /// Initialize chain providers
    pub async fn init(&mut self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        for (chain, state) in &mut self.chains {
            let provider = ethers_providers::Provider::try_connect(
                state.config.rpc_url.clone()
            ).await?;
            state.provider = Some(provider);
        }
        Ok(())
    }

    /// Get supported chains
    pub fn supported_chains(&self) -> Vec<Chain> {
        self.chains.keys().copied().collect()
    }

    /// Get chain configuration
    pub fn get_chain(&self, chain: Chain) -> Option<&ChainConfig> {
        self.chains.get(&chain).map(|s| &s.config)
    }

    /// Initiate cross-chain transfer
    pub async fn initiate_transfer(
        &self,
        source_chain: Chain,
        dest_chain: Chain,
        sender: String,
        recipient: String,
        token: String,
        token_type: TokenType,
        amount: String,
        token_id: Option<String>,
    ) -> Result<Transfer, Box<dyn std::error::Error + Send + Sync>> {
        // Validate chains
        if !self.supported_chains().contains(&source_chain) {
            return Err("Source chain not supported".into());
        }
        if !self.supported_chains().contains(&dest_chain) {
            return Err("Destination chain not supported".into());
        }
        if source_chain == dest_chain {
            return Err("Cannot transfer to same chain".into());
        }

        let transfer = Transfer {
            id: format!("0x{}", hex::encode(rand::random::<[u8; 32]())),
            source_chain,
            destination_chain: dest_chain,
            sender,
            recipient,
            token,
            token_type,
            amount,
            token_id,
            status: TransferStatus::Pending,
            source_tx: String::new(),
            destination_tx: None,
            timestamp: chrono::Utc::now().timestamp(),
            confirmations: 0,
        };

        // Store transfer
        let mut transfers = self.transfers.write().await;
        transfers.insert(transfer.id.clone(), transfer.clone());

        Ok(transfer)
    }

    /// Complete transfer
    pub async fn complete_transfer(
        &self,
        transfer_id: &str,
        destination_tx: String,
    ) -> Result<Transfer, Box<dyn std::error::Error + Send + Sync>> {
        let mut transfers = self.transfers.write().await;
        
        if let Some(transfer) = transfers.get_mut(transfer_id) {
            transfer.status = TransferStatus::Completed;
            transfer.destination_tx = Some(destination_tx);
            Ok(transfer.clone())
        } else {
            Err("Transfer not found".into())
        }
    }

    /// Get transfer status
    pub async fn get_transfer(&self, transfer_id: &str) -> Option<Transfer> {
        let transfers = self.transfers.read().await;
        transfers.get(transfer_id).cloned()
    }

    /// Calculate transfer fee
    pub fn calculate_fee(&self, amount: &str) -> Result<String, Box<dyn std::error::Error>> {
        let amount = amount.parse::<u64>().map_err(|e| e.to_string())?;
        let flat = self.config.fee.flat_fee.parse::<u64>().map_err(|e| e.to_string())?;
        let percentage = (amount as f64 * self.config.fee.percentage_fee) as u64;
        let min = self.config.fee.min_fee.parse::<u64>().map_err(|e| e.to_string())?;
        let max = self.config.fee.max_fee.parse::<u64>().map_err(|e| e.to_string())?;

        let fee = flat + percentage;
        let fee = fee.max(min).min(max);

        Ok(fee.to_string())
    }

    /// Get pending transfers
    pub async fn pending_transfers(&self) -> Vec<Transfer> {
        let transfers = self.transfers.read().await;
        transfers
            .values()
            .filter(|t| t.status == TransferStatus::Pending || t.status == TransferStatus::Minting)
            .cloned()
            .collect()
    }
}

use rand::Rng;
use hex;