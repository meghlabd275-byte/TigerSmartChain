//! Cross-chain Service - Bridge Tracking & Multi-chain Portfolio

use std::collections::HashMap;
use std::sync::Arc;
use parking_lot::RwLock;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChainInfo {
    pub id: u32,
    pub name: String,
    pub symbol: String,
    pub rpc_url: String,
    pub explorer_url: String,
    pub bridge_addresses: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BridgeTransaction {
    pub id: String,
    pub bridge: String,
    pub from_chain: u32,
    pub to_chain: u32,
    pub from_address: String,
    pub to_address: String,
    pub token: String,
    pub amount: String,
    pub status: BridgeStatus,
    pub deposit_tx: String,
    pub receive_tx: Option<String>,
    pub timestamp: i64,
    pub confirmations: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BridgeStatus {
    Pending,
    Deposited,
    Confirmed,
    Executed,
    Failed,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Portfolio {
    pub address: String,
    pub chains: HashMap<u32, ChainBalance>,
    pub total_usd: f64,
    pub last_updated: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChainBalance {
    pub chain_id: u32,
    pub native_balance: String,
    pub tokens: Vec<TokenBalance>,
    pub nfts: Vec<NftBalance>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenBalance {
    pub address: String,
    pub symbol: String,
    pub balance: String,
    pub usd_value: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NftBalance {
    pub collection: String,
    pub token_ids: Vec<String>,
    pub count: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CrossChainTransfer {
    pub id: String,
    pub from_chain: u32,
    pub to_chain: u32,
    pub from: String,
    pub to: String,
    pub token: String,
    pub amount: String,
    pub status: TransferStatus,
    pub timestamp: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TransferStatus {
    Pending,
    SourceConfirmed,
    DestinationPending,
    Completed,
    Failed,
}

pub struct CrossChainService {
    chains: HashMap<u32, ChainInfo>,
    bridges: HashMap<String, BridgeConfig>,
    state: Arc<RwLock<CrossChainState>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BridgeConfig {
    pub name: String,
    pub from_chain: u32,
    pub to_chain: u32,
    pub token: String,
    pub fee: f64,
    pub min_amount: String,
    pub estimated_time: u32,
}

#[derive(Debug)]
pub struct CrossChainState {
    pub portfolios: HashMap<String, Portfolio>,
    pub bridge_txs: HashMap<String, Vec<BridgeTransaction>>,
    pub transfers: HashMap<String, Vec<CrossChainTransfer>>,
}

impl CrossChainService {
    pub fn new() -> Self {
        let chains = Self::default_chains();
        let bridges = Self::default_bridges();
        
        Self {
            chains,
            bridges,
            state: Arc::new(RwLock::new(CrossChainState {
                portfolios: HashMap::new(),
                bridge_txs: HashMap::new(),
                transfers: HashMap::new(),
            })),
        }
    }

    fn default_chains() -> HashMap<u32, ChainInfo> {
        vec![
            ChainInfo { id: 1, name: "Ethereum".to_string(), symbol: "ETH".to_string(), rpc_url: "https://eth-rpc.com".to_string(), explorer_url: "https://etherscan.io".to_string(), bridge_addresses: vec![] },
            ChainInfo { id: 56, name: "BNB Chain".to_string(), symbol: "BNB".to_string(), rpc_url: "https://bsc-rpc.com".to_string(), explorer_url: "https://bscscan.com".to_string(), bridge_addresses: vec![] },
            ChainInfo { id: 137, name: "Polygon".to_string(), symbol: "MATIC".to_string(), rpc_url: "https://polygon-rpc.com".to_string(), explorer_url: "https://polygonscan.com".to_string(), bridge_addresses: vec![] },
            ChainInfo { id: 42161, name: "Arbitrum".to_string(), symbol: "ETH".to_string(), rpc_url: "https://arb1-rpc.com".to_string(), explorer_url: "https://arbiscan.io".to_string(), bridge_addresses: vec![] },
            ChainInfo { id: 10, name: "Optimism".to_string(), symbol: "ETH".to_string(), rpc_url: "https://mainnet.optimism.io".to_string(), explorer_url: "https://optimistic.etherscan.io".to_string(), bridge_addresses: vec![] },
            ChainInfo { id: 250, name: "Fantom".to_string(), symbol: "FTM".to_string(), rpc_url: "https://rpc.fantom.network".to_string(), explorer_url: "https://ftmscan.com".to_string(), bridge_addresses: vec![] },
            ChainInfo { id: 43114, name: "Avalanche".to_string(), symbol: "AVAX".to_string(), rpc_url: "https://api.avax.network/ext/bc/C/rpc".to_string(), explorer_url: "https://snowtrace.io".to_string(), bridge_addresses: vec![] },
        ].into_iter().map(|c| (c.id, c)).collect()
    }

    fn default_bridges() -> HashMap<String, BridgeConfig> {
        vec![
            BridgeConfig { name: "Stargate".to_string(), from_chain: 1, to_chain: 56, token: "0x0000000000000000000000000000000000000000".to_string(), fee: 0.003, min_amount: "10".to_string(), estimated_time: 600 },
            BridgeConfig { name: "Stargate".to_string(), from_chain: 56, to_chain: 1, token: "0x0000000000000000000000000000000000000000".to_string(), fee: 0.003, min_amount: "10".to_string(), estimated_time: 600 },
            BridgeConfig { name: "Celer".to_string(), from_chain: 1, to_chain: 137, token: "0x0000000000000000000000000000000000000000".to_string(), fee: 0.005, min_amount: "50".to_string(), estimated_time: 1800 },
            BridgeConfig { name: "Across".to_string(), from_chain: 1, to_chain: 42161, token: "0x0000000000000000000000000000000000000000".to_string(), fee: 0.002, min_amount: "1".to_string(), estimated_time: 60 },
        ].into_iter().map(|b| (b.name.to_string(), b)).collect()
    }

    /// Get all supported chains
    pub fn get_chains(&self) -> Vec<&ChainInfo> {
        self.chains.values().collect()
    }

    /// Get chain by ID
    pub fn get_chain(&self, chain_id: u32) -> Option<&ChainInfo> {
        self.chains.get(&chain_id)
    }

    /// Get bridge info
    pub fn get_bridges(&self, from_chain: u32, to_chain: u32) -> Vec<&BridgeConfig> {
        self.bridges.values()
            .filter(|b| b.from_chain == from_chain && b.to_chain == to_chain)
            .collect()
    }

    /// Track bridge transaction
    pub fn track_bridge_tx(&self, tx: BridgeTransaction) {
        let mut state = self.state.write();
        state.bridge_txs.entry(tx.from_address.clone())
            .or_insert_with(Vec::new)
            .push(tx);
    }

    /// Get bridge transactions
    pub fn get_bridge_txs(&self, address: &str) -> Vec<BridgeTransaction> {
        let state = self.state.read();
        state.bridge_txs.get(address).cloned().unwrap_or_default()
    }

    /// Get multi-chain portfolio
    pub fn get_portfolio(&self, address: &str) -> Portfolio {
        let state = self.state.read();
        
        if let Some(portfolio) = state.portfolios.get(address) {
            return portfolio.clone();
        }
        
        Portfolio {
            address: address.to_string(),
            chains: HashMap::new(),
            total_usd: 0.0,
            last_updated: chrono::Utc::now().timestamp(),
        }
    }

    /// Update portfolio
    pub fn update_portfolio(&self, address: &str, chains: HashMap<u32, ChainBalance>) {
        let total_usd: f64 = chains.values()
            .map(|c| {
                let token_usd: f64 = c.tokens.iter().map(|t| t.usd_value).sum();
                let native_usd: f64 = 0.0; // Would calculate from price
                token_usd + native_usd
            })
            .sum();
        
        let mut state = self.state.write();
        state.portfolios.insert(address.to_string(), Portfolio {
            address: address.to_string(),
            chains,
            total_usd,
            last_updated: chrono::Utc::now().timestamp(),
        });
    }

    /// Get cross-chain transfers
    pub fn get_transfers(&self, address: &str) -> Vec<CrossChainTransfer> {
        let state = self.state.read();
        state.transfers.get(address).cloned().unwrap_or_default()
    }

    /// Calculate optimal bridge route
    pub fn find_best_route(&self, from_chain: u32, to_chain: u32, amount: &str) -> Option<BridgeRoute> {
        let bridges = self.get_bridges(from_chain, to_chain);
        
        if bridges.is_empty() {
            return None;
        }
        
        let bridge = bridges.first()?;
        
        let amount_f64: f64 = amount.parse().unwrap_or(0.0);
        if amount_f64 < bridge.min_amount.parse().unwrap_or(0.0) {
            return None;
        }
        
        let fee = amount_f64 * bridge.fee;
        let total = amount_f64 - fee;
        
        Some(BridgeRoute {
            bridge: bridge.name.clone(),
            from_chain,
            to_chain,
            amount: amount.to_string(),
            received_amount: total.to_string(),
            fee: fee.to_string(),
            estimated_time: bridge.estimated_time,
        })
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BridgeRoute {
    pub bridge: String,
    pub from_chain: u32,
    pub to_chain: u32,
    pub amount: String,
    pub received_amount: String,
    pub fee: String,
    pub estimated_time: u32,
}

// Chain IDs constant
pub const ETH_MAINNET: u32 = 1;
pub const BSC_MAINNET: u32 = 56;
pub const POLYGON: u32 = 137;
pub const ARBITRUM: u32 = 42161;
pub const OPTIMISM: u32 = 10;
pub const FANTOM: u32 = 250;
pub const AVALANCHE: u32 = 43114;