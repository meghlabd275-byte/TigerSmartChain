//! Node Configuration
//! 
//! Defines configuration structures for the TigerSmartChain blockchain node.

use serde::{Deserialize, Serialize};

/// Reward paid to the proposer of each block, in wei: 2 TGR.
pub const DEFAULT_BLOCK_REWARD: u128 = 2_000_000_000_000_000_000;
/// Floor gas price reported on an empty chain, in wei: 1 Gwei.
pub const DEFAULT_MIN_GAS_PRICE: u64 = 1_000_000_000;
/// Gas ceiling for read-only calls: 50 million.
pub const DEFAULT_CALL_GAS_LIMIT: u64 = 50_000_000;
/// Requests per second one client address may make. High enough for an
/// explorer's polling and a wallet's refresh, low enough that a single client
/// cannot exhaust the node's CPU.
pub const DEFAULT_RPC_RATE_LIMIT: u32 = 100;

/// Node configuration holding all settings for the TigerSmartChain node.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    /// Network settings
    pub network_id: u64,
    pub chain_id: u64,
    pub network: String,

    /// P2P settings
    pub listen_addr: String,
    pub boot_nodes: Vec<String>,
    pub max_peers: usize,

    /// RPC settings
    pub rpc_enabled: bool,
    pub rpc_addr: String,
    /// Comma-separated list of origins allowed to call the RPC from a browser.
    ///
    /// Empty means no browser origin is allowed, which is the safe default: a
    /// public endpoint that answers `Access-Control-Allow-Origin: *` lets any
    /// page a user visits read the chain through their own node, and lets a
    /// malicious page issue state-changing calls with the user's node as the
    /// source. Operators who want a web explorer set this to its origin.
    pub rpc_cors_host: String,
    /// Maximum JSON-RPC requests per second from one client address. Zero
    /// disables the limit.
    pub rpc_rate_limit: u32,

    /// Serve the WebSocket JSON-RPC endpoint, which is where `eth_subscribe`
    /// and its push notifications live.
    pub ws_enabled: bool,
    pub ws_addr: String,

    /// State settings
    pub data_dir: String,
    pub cache_size: usize,

    /// Consensus settings
    pub validator_addr: Option<String>,
    pub validator_key: Option<String>,
    pub epoch_length: u64,
    pub slot_duration_secs: u64,
    /// Reward paid to the proposer of each block, in wei.
    pub block_reward: u128,
    /// Addresses that may propose blocks, each staked at the consensus
    /// minimum. An empty list means this node is not a validator and will
    /// follow the chain without producing blocks.
    pub validators: Vec<String>,
    /// Accept a block from a proposer that is not in the validator set and add
    /// it to the set. Only safe on a private development chain: on a public
    /// network it hands the chain to whoever proposes first.
    pub allow_unlisted_validators: bool,
    /// Floor reported by `eth_gasPrice` on an empty chain, in wei.
    pub min_gas_price: u64,
    /// Gas ceiling for `eth_call` and `eth_estimateGas`.
    pub call_gas_limit: u64,

    /// Metrics settings
    pub metrics_enabled: bool,
    pub metrics_addr: String,

    /// Genesis
    pub genesis_file: Option<String>,
}

impl Config {
    /// Create a new configuration with all parameters.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        network_id: u64,
        chain_id: u64,
        network: String,
        listen_addr: String,
        boot_nodes: Vec<String>,
        max_peers: usize,
        rpc_enabled: bool,
        rpc_addr: String,
        rpc_cors_host: String,
        rpc_rate_limit: u32,
        ws_enabled: bool,
        ws_addr: String,
        data_dir: String,
        cache_size: usize,
        validator_addr: Option<String>,
        validator_key: Option<String>,
        validators: Vec<String>,
        allow_unlisted_validators: bool,
        epoch_length: u64,
        slot_duration_secs: u64,
        metrics_enabled: bool,
        metrics_addr: String,
        genesis_file: Option<String>,
    ) -> Self {
        Self {
            network_id,
            chain_id,
            network,
            listen_addr,
            boot_nodes,
            max_peers,
            rpc_enabled,
            rpc_addr,
            rpc_cors_host,
            rpc_rate_limit,
            ws_enabled,
            ws_addr,
            data_dir,
            cache_size,
            validator_addr,
            validator_key,
            validators,
            allow_unlisted_validators,
            epoch_length,
            slot_duration_secs,
            block_reward: DEFAULT_BLOCK_REWARD,
            min_gas_price: DEFAULT_MIN_GAS_PRICE,
            call_gas_limit: DEFAULT_CALL_GAS_LIMIT,
            
            
            metrics_enabled,
            metrics_addr,
            genesis_file,
        }
    }

    /// Create default configuration for mainnet.
    pub fn default_mainnet() -> Self {
        Self {
            network_id: 6666,
            chain_id: 6666,
            network: "mainnet".to_string(),
            listen_addr: ":30303".to_string(),
            boot_nodes: Vec::new(),
            max_peers: 50,
            rpc_enabled: true,
            rpc_addr: ":8545".to_string(),
            rpc_cors_host: String::new(),
            rpc_rate_limit: DEFAULT_RPC_RATE_LIMIT,
            ws_enabled: true,
            ws_addr: ":8546".to_string(),
            data_dir: "./data".to_string(),
            cache_size: 1024,
            validator_addr: None,
            validator_key: None,
            epoch_length: 200,
            slot_duration_secs: 3,
            block_reward: DEFAULT_BLOCK_REWARD,
            validators: Vec::new(),
            allow_unlisted_validators: false,
            min_gas_price: DEFAULT_MIN_GAS_PRICE,
            call_gas_limit: DEFAULT_CALL_GAS_LIMIT,
            
            
            metrics_enabled: false,
            metrics_addr: ":9090".to_string(),
            genesis_file: None,
        }
    }

    /// Create default configuration for testnet.
    pub fn default_testnet() -> Self {
        Self {
            network_id: 97,
            chain_id: 97,
            network: "testnet".to_string(),
            listen_addr: ":30303".to_string(),
            boot_nodes: Vec::new(),
            max_peers: 30,
            rpc_enabled: true,
            rpc_addr: ":8545".to_string(),
            rpc_cors_host: String::new(),
            rpc_rate_limit: DEFAULT_RPC_RATE_LIMIT,
            ws_enabled: true,
            ws_addr: ":8546".to_string(),
            data_dir: "./data/testnet".to_string(),
            cache_size: 512,
            validator_addr: None,
            validator_key: None,
            epoch_length: 200,
            slot_duration_secs: 3,
            block_reward: DEFAULT_BLOCK_REWARD,
            validators: Vec::new(),
            allow_unlisted_validators: false,
            min_gas_price: DEFAULT_MIN_GAS_PRICE,
            call_gas_limit: DEFAULT_CALL_GAS_LIMIT,
            
            
            metrics_enabled: false,
            metrics_addr: ":9090".to_string(),
            genesis_file: None,
        }
    }
}

impl Default for Config {
    fn default() -> Self {
        Self::default_mainnet()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_mainnet() {
        let config = Config::default_mainnet();
        assert_eq!(config.network_id, 6666);
        assert_eq!(config.chain_id, 6666);
        assert_eq!(config.network, "mainnet");
    }

    #[test]
    fn test_default_testnet() {
        let config = Config::default_testnet();
        assert_eq!(config.network_id, 97);
        assert_eq!(config.chain_id, 97);
        assert_eq!(config.network, "testnet");
    }

    #[test]
    fn test_config_serialization() {
        let config = Config::default_mainnet();
        let serialized = serde_json::to_string(&config).unwrap();
        let deserialized: Config = serde_json::from_str(&serialized).unwrap();
        assert_eq!(config.network_id, deserialized.network_id);
    }
}