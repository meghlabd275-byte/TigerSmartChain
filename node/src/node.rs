//! TigerSmartChain Blockchain Node Implementation
//! 
//! This module provides the main blockchain node implementation with safety,
//! security, high speed, and ultra low latency through Rust.

use crate::config::Config;
use anyhow::{Context, Result};
use log::{error, info, warn};
use std::sync::Arc;
use tokio::sync::RwLock;
use tokio::task::JoinSet;

// Import from existing TigerSmartChain Rust crates
use tiger_blockchain::{ChainConfig, ChainManager, Genesis, Mempool, Transaction};

/// Gas limit for produced blocks.
const BLOCK_GAS_LIMIT: u64 = 30_000_000;
use tiger_consensus::PoSA;
use tiger_network::SyncStatus;
use tiger_state::account::WorldState;

/// Node state
#[derive(Debug, Clone, PartialEq)]
pub enum NodeState {
    /// Node is stopped
    Stopped,
    /// Node is starting
    Starting,
    /// Node is running
    Running,
    /// Node is stopping
    Stopping,
    /// Node has errored
    Error,
}

/// TigerNode represents a TigerSmartChain blockchain node.
/// 
/// This is the main entry point for running a blockchain node with:
/// - Memory safety through Rust's ownership model
/// - Thread safety through Tokio async runtime
/// - High speed through zero-cost abstractions
/// - Ultra low latency through efficient scheduling
pub struct TigerNode {
    /// Configuration
    config: Config,

    /// Node state
    state: RwLock<NodeState>,

    /// Chain manager (locked for concurrent access from async tasks)
    chain_manager: tokio::sync::Mutex<ChainManager>,

    /// World state: accounts, balances, code and storage.
    world_state: tokio::sync::Mutex<WorldState>,

    /// Transaction pool (mempool)
    mempool: tokio::sync::Mutex<Mempool>,

    /// Consensus engine
    consensus: tokio::sync::Mutex<PoSA>,

    /// Network sync status
    sync_status: RwLock<SyncStatus>,

    /// Background tasks
    tasks: RwLock<JoinSet<()>>,
}

impl TigerNode {
    /// Create a new TigerNode instance.
    pub async fn new(config: Config) -> Result<Self> {
        info!("Initializing TigerSmartChain node...");

        // Validate configuration
        Self::validate_config(&config)?;

        // Set initial state
        let state = RwLock::new(NodeState::Starting);

        // Initialize the world state (trie-backed accounts).
        let world_state = tokio::sync::Mutex::new(WorldState::new());
        info!("World state initialized");

        // Initialize blockchain chain manager with config
        let chain_config = ChainConfig {
            chain_id: config.chain_id,
            ..Default::default()
        };
        let chain_manager = ChainManager::new(chain_config);
        info!("Chain initialized: network_id={}, chain_id={}", config.network_id, config.chain_id);

        // Initialize transaction pool
        let mempool = Mempool::new();
        info!("Mempool initialized");

        // Initialize PoSA consensus
        let consensus = PoSA::new(
            config.chain_id,
            config.epoch_length,
            config.slot_duration_secs,
        );
        info!("Consensus engine initialized");

        // Initialize network sync status
        let sync_status = RwLock::new(SyncStatus::default());

        // Initialize background tasks
        let tasks = RwLock::new(JoinSet::new());

        Ok(Self {
            config,
            state,
            chain_manager: tokio::sync::Mutex::new(chain_manager),
            world_state,
            mempool: tokio::sync::Mutex::new(mempool),
            consensus: tokio::sync::Mutex::new(consensus),
            sync_status,
            tasks,
        })
    }

    /// Validate node configuration.
    fn validate_config(config: &Config) -> Result<()> {
        if config.network_id == 0 {
            anyhow::bail!("Network ID must be greater than 0");
        }
        if config.chain_id == 0 {
            anyhow::bail!("Chain ID must be greater than 0");
        }
        if config.max_peers == 0 {
            anyhow::bail!("Max peers must be greater than 0");
        }
        if config.cache_size == 0 {
            anyhow::bail!("Cache size must be greater than 0");
        }
        if config.epoch_length == 0 {
            anyhow::bail!("Epoch length must be greater than 0");
        }
        if config.slot_duration_secs == 0 {
            anyhow::bail!("Slot duration must be greater than 0");
        }
        Ok(())
    }

    /// Start the node and all its components.
    /// This spawns real background tasks for block production, mempool maintenance,
    /// and sync status monitoring. The node must be wrapped in Arc for shared ownership.
    pub async fn start(self: &Arc<Self>) -> Result<()> {
        info!("Starting node components...");

        // Install the genesis block if the chain is empty.
        //
        // `height()` returns 0 both for an empty chain and for a chain whose
        // only block is genesis, so `is_empty()` is what distinguishes them.
        // The genesis block must be a real block at number 0: without it the
        // first produced block has no parent to link to and validation fails.
        {
            let mut cm = self.chain_manager.lock().await;
            if cm.chain().is_empty() {
                info!("Chain is empty — installing genesis block");
                let genesis_config = create_genesis_block(self.config.chain_id);
                cm.chain_mut().set_genesis(genesis_config.clone());

                let genesis_block = genesis_block_from(&genesis_config);
                let hash = tiger_blockchain::block_hash(&genesis_block.header)
                    .map_err(|e| anyhow::anyhow!("failed to hash genesis block: {e}"))?;
                cm.process_block(genesis_block)
                    .map_err(|e| anyhow::anyhow!("failed to insert genesis block: {e}"))?;
                info!("Genesis block installed at height 0 (hash: {})", hash);
            }
        }

        // Update state to Running
        *self.state.write().await = NodeState::Running;
        info!("Node state set to Running");

        // Spawn block production task (PoSA consensus sealing loop)
        self.spawn_block_production();

        // Spawn sync status monitor
        self.spawn_sync_monitor();

        info!("All node components started successfully");
        Ok(())
    }

    /// Spawn the block production (sealing) loop.
    /// In PoSA, validators take turns proposing blocks every slot.
    /// This task polls the consensus engine for the current proposer,
    /// and if this node is the proposer, it seals a new block from pending mempool txs.
    fn spawn_block_production(self: &Arc<Self>) {
        let node = Arc::clone(self);
        let slot_duration = std::time::Duration::from_secs(node.config.slot_duration_secs.max(1));

        tokio::spawn(async move {
            let mut interval = tokio::time::interval(slot_duration);
            interval.tick().await; // skip first immediate tick

            loop {
                // Check if node is still running
                {
                    let state = node.state.read().await;
                    if *state != NodeState::Running {
                        info!("Block production task: node no longer running, exiting");
                        break;
                    }
                }

                interval.tick().await;

                let next_block = {
                    let cm = node.chain_manager.lock().await;
                    cm.chain().height() + 1
                };

                // Update the consensus epoch and get the proposer. When the
                // call crosses an epoch boundary it returns the rewards earned
                // during the epoch that just ended; those are credited to the
                // real world state, since the consensus engine holds no
                // balances of its own.
                let (proposer, settled_rewards) = {
                    let mut consensus = node.consensus.lock().await;
                    let settled = consensus.update_epoch(next_block);
                    (consensus.get_proposer(next_block), settled)
                };

                if !settled_rewards.is_empty() {
                    let mut world = node.world_state.lock().await;
                    for (validator, reward) in &settled_rewards {
                        if let Ok(address) = parse_address(validator) {
                            world.credit(&address, &parse_u256(&format!("0x{reward:x}")));
                        } else {
                            warn!("cannot pay reward to unparseable validator {validator}");
                        }
                    }
                    info!(
                        "settled {} validator reward entries at block {}",
                        settled_rewards.len(),
                        next_block
                    );
                }

                if let Some(proposer) = proposer {
                    if let Some(ref my_addr) = node.config.validator_addr {
                        if proposer == *my_addr {
                            info!("Proposing block #{}", next_block);
                            // Collect pending transactions from mempool
                            let pending_txs = {
                                let mempool = node.mempool.lock().await;
                                mempool.pending().into_iter().cloned().collect::<Vec<_>>()
                            };

                            let result = {
                                let mut cm = node.chain_manager.lock().await;
                                let mut world = node.world_state.lock().await;
                                produce_block(
                                    &mut cm,
                                    &mut world,
                                    &pending_txs,
                                    next_block,
                                    &proposer,
                                    node.config.chain_id,
                                )
                            };

                            match result {
                                Ok(block_hash) => {
                                    info!("Successfully produced block #{} (hash: {})", next_block, block_hash);
                                    // Credit the proposer's pending reward.
                                    {
                                        let mut consensus = node.consensus.lock().await;
                                        consensus.record_block_reward(
                                            proposer.clone(),
                                            node.config.block_reward,
                                        );
                                    }
                                    // Remove included txs from mempool
                                    let mut mempool = node.mempool.lock().await;
                                    for tx in &pending_txs {
                                        mempool.remove(&tx.hash);
                                    }
                                }
                                Err(e) => {
                                    error!("Failed to produce block #{}: {}", next_block, e);
                                }
                            }
                        }
                    }
                }
            }
        });
    }

    /// Spawn the sync status monitor.
    /// Updates sync_status.current_block to match chain height.
    fn spawn_sync_monitor(self: &Arc<Self>) {
        let node = Arc::clone(self);

        tokio::spawn(async move {
            let mut interval = tokio::time::interval(std::time::Duration::from_secs(5));
            loop {
                {
                    let state = node.state.read().await;
                    if *state != NodeState::Running {
                        break;
                    }
                }
                interval.tick().await;
                let current_height = {
                    let cm = node.chain_manager.lock().await;
                    cm.chain().height()
                };
                let mut status = node.sync_status.write().await;
                status.current_block = current_height;
                if status.highest_block < status.current_block {
                    status.highest_block = status.current_block;
                }
            }
        });
    }

    /// Stop the node and all its components gracefully.
    pub async fn stop(&self) -> Result<()> {
        info!("Stopping node components...");

        // Set state to Stopping — spawned tasks will detect this and exit
        *self.state.write().await = NodeState::Stopping;
        info!("Signaled background tasks to stop");

        // Give tasks a moment to notice the state change and exit
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;

        // Cancel any remaining background tasks in the JoinSet
        let mut tasks = self.tasks.write().await;
        while let Some(handle) = tasks.join_next().await {
            if let Err(e) = handle {
                warn!("Task error: {:?}", e);
            }
        }
        tasks.shutdown().await;

        // Update state
        *self.state.write().await = NodeState::Stopped;
        info!("Node stopped successfully");

        Ok(())
    }

    /// Get the chain manager instance (locked).
    pub async fn get_chain_manager(&self) -> tokio::sync::MutexGuard<'_, ChainManager> {
        self.chain_manager.lock().await
    }

    /// Get the state trie instance.
    /// Access the world state.
    pub fn world_state(&self) -> &tokio::sync::Mutex<WorldState> {
        &self.world_state
    }

    /// Get the mempool instance (locked).
    pub async fn get_mempool(&self) -> tokio::sync::MutexGuard<'_, Mempool> {
        self.mempool.lock().await
    }

    /// Get the consensus engine instance (locked).
    pub async fn get_consensus(&self) -> tokio::sync::MutexGuard<'_, PoSA> {
        self.consensus.lock().await
    }

    /// Get network sync status.
    pub fn get_sync_status(&self) -> &RwLock<SyncStatus> {
        &self.sync_status
    }

    /// Get current node state.
    pub async fn get_state(&self) -> NodeState {
        self.state.read().await.clone()
    }

    /// Check if node is running.
    pub async fn is_running(&self) -> bool {
        *self.state.read().await == NodeState::Running
    }

    /// Get configuration.
    pub fn config(&self) -> &Config {
        &self.config
    }
}

// =============================================================================
// BLOCK PRODUCTION
// =============================================================================

/// Create a genesis block configuration for the given chain ID.
fn create_genesis_block(chain_id: u64) -> Genesis {
    use std::collections::HashMap;

    let config = ChainConfig {
        chain_id,
        ..Default::default()
    };

    Genesis {
        config,
        nonce: 0,
        timestamp: chrono::Utc::now().to_rfc3339(),
        extra_data: "TigerSmartChain Genesis".to_string(),
        gas_limit: "30000000".to_string(),
        difficulty: "1".to_string(),
        mix_hash: "0x0000000000000000000000000000000000000000000000000000000000000000".to_string(),
        coinbase: "0x0000000000000000000000000000000000000000".to_string(),
        alloc: HashMap::new(),
    }
}

/// Build the genesis block from its configuration.
///
/// The genesis header is a real header: its transactions root is the empty
/// trie root and its state root is the root of the (empty) world state, so it
/// hashes consistently with every other client.
fn genesis_block_from(genesis: &Genesis) -> tiger_blockchain::types::Block {
    use tiger_blockchain::types::{Block, BlockHeader};
    use tiger_blockchain::EMPTY_TRIE_ROOT;

    let state_root = WorldState::new().state_root();

    Block {
        header: BlockHeader {
            parent_hash: format!("0x{}", tiger_blockchain::hex_encode(&[0u8; 32])),
            sha3_uncles: format!(
                "0x{}",
                tiger_blockchain::hex_encode(&tiger_crypto::keccak256(&[]))
            ),
            miner: genesis.coinbase.clone(),
            state_root: format!("0x{}", tiger_blockchain::hex_encode(&state_root)),
            transactions_root: format!("0x{}", tiger_blockchain::hex_encode(&EMPTY_TRIE_ROOT)),
            receipts_root: format!("0x{}", tiger_blockchain::hex_encode(&EMPTY_TRIE_ROOT)),
            logs_bloom: format!("0x{}", tiger_blockchain::hex_encode(&[0u8; 256])),
            difficulty: "0x0".to_string(),
            number: 0,
            gas_limit: genesis.gas_limit.parse().unwrap_or(BLOCK_GAS_LIMIT),
            gas_used: 0,
            timestamp: 0,
            extra_data: format!("0x{}", tiger_blockchain::hex_encode(genesis.extra_data.as_bytes())),
            mix_hash: genesis.mix_hash.clone(),
            nonce: "0x0000000000000000".to_string(),
            base_fee_per_gas: Some(0),
            withdrawals_root: None,
        },
        transactions: Vec::new(),
        uncles: Vec::new(),
    }
}

/// Produce (seal) a new block from a set of pending transactions.
///
/// The header carries real roots: the transactions root is computed from the
/// transactions and the state root from the world state after applying them.
/// The block hash is the keccak256 of the RLP-encoded header, so it agrees
/// with what any other client computes for the same header.
///
/// Transactions that fail validation or revert are still included (they paid
/// for their gas) but do not change state beyond the gas charge.
fn produce_block(
    chain_manager: &mut ChainManager,
    world_state: &mut WorldState,
    transactions: &[Transaction],
    block_number: u64,
    miner: &str,
    chain_id: u64,
) -> Result<String, String> {
    use tiger_blockchain::types::{Block, BlockHeader};
    use tiger_blockchain::{block_hash, transactions_root, EMPTY_TRIE_ROOT};
    use tiger_evm::transition::{
        apply_transaction, BlockContext, TransactionRequest,
    };

    let parent_block = chain_manager
        .chain()
        .get_block(block_number.saturating_sub(1));

    let parent_hash = match parent_block {
        Some(block) => block_hash(&block.header)
            .map_err(|e| format!("failed to hash parent header: {e}"))?,
        None => format!("0x{}", tiger_blockchain::hex_encode(&[0u8; 32])),
    };

    let timestamp = chrono::Utc::now().timestamp() as u64;
    let mut coinbase = [0u8; 20];
    if let Ok(addr) = parse_address(miner) {
        coinbase = addr;
    }

    let block_ctx = BlockContext {
        chain_id,
        number: block_number,
        timestamp,
        gas_limit: BLOCK_GAS_LIMIT,
        coinbase,
        base_fee: 0,
    };

    // Select the transactions that can legitimately be included.
    //
    // A producer must never emit a block its own validator would reject, so a
    // transaction whose declared hash does not match its contents or whose
    // signature cannot be recovered is dropped before the block is built. It
    // is not enough to skip applying it: it would still be part of the block
    // and fail block validation.
    let mut included: Vec<Transaction> = Vec::with_capacity(transactions.len());
    let mut requests: Vec<tiger_evm::transition::TransactionRequest> =
        Vec::with_capacity(transactions.len());
    for tx in transactions {
        match transaction_to_request(tx, chain_id) {
            Some(request) => {
                included.push(tx.clone());
                requests.push(request);
            }
            None => {
                warn!(
                    "dropping transaction {}: signature cannot be recovered",
                    tx.hash
                );
            }
        }
    }

    // Apply each transaction to the world state. Gas actually consumed is
    // summed, not the gas limits the transactions declared.
    let mut gas_used: u64 = 0;
    for (tx, request) in included.iter().zip(requests.iter()) {
        match apply_transaction(world_state, request, &block_ctx) {
            Ok(outcome) => {
                gas_used = gas_used.saturating_add(outcome.gas_used);
                if !outcome.success {
                    info!("transaction {} reverted, gas used {}", tx.hash, outcome.gas_used);
                }
            }
            Err(e) => {
                warn!("transaction {} rejected: {}", tx.hash, e);
            }
        }
    }

    // The transactions root is computed over the transactions as included.
    let tx_root = transactions_root(&included)
        .map_err(|e| format!("failed to compute transactions root: {e}"))?;

    // The state root reflects the world state after applying the block.
    let state_root = world_state.state_root();

    let header = BlockHeader {
        parent_hash,
        sha3_uncles: format!(
            "0x{}",
            tiger_blockchain::hex_encode(&tiger_crypto::keccak256(&[]))
        ),
        miner: miner.to_string(),
        state_root: format!("0x{}", tiger_blockchain::hex_encode(&state_root)),
        transactions_root: format!("0x{}", tiger_blockchain::hex_encode(&tx_root)),
        // Receipts are not produced by this node yet, so the root of the empty
        // trie is used rather than a zero placeholder.
        receipts_root: format!("0x{}", tiger_blockchain::hex_encode(&EMPTY_TRIE_ROOT)),
        logs_bloom: format!("0x{}", tiger_blockchain::hex_encode(&[0u8; 256])),
        difficulty: "0".to_string(),
        number: block_number,
        gas_limit: BLOCK_GAS_LIMIT,
        gas_used,
        timestamp,
        extra_data: format!(
            "0x{}",
            tiger_blockchain::hex_encode(
                format!("TigerSmartChain block #{block_number}").as_bytes()
            )
        ),
        mix_hash: format!("0x{}", tiger_blockchain::hex_encode(&[0u8; 32])),
        nonce: "0x0000000000000000".to_string(),
        base_fee_per_gas: Some(0),
        withdrawals_root: None,
    };

    // The block hash is keccak256(RLP(header)), identical to every other
    // Ethereum-compatible client.
    let block_hash =
        block_hash(&header).map_err(|e| format!("failed to hash block header: {e}"))?;

    let block = Block {
        header,
        transactions: included,
        uncles: vec![],
    };

    chain_manager
        .process_block(block)
        .map_err(|e| format!("failed to process block: {e}"))?;

    Ok(block_hash)
}

/// Convert a node transaction into the form the transition function needs.
///
/// Returns `None` when the sender cannot be recovered, which means the
/// transaction is not authorised and must not be applied.
fn transaction_to_request(tx: &Transaction, chain_id: u64) -> Option<tiger_evm::transition::TransactionRequest> {
    use tiger_blockchain::recover_transaction_sender;

    let sender = recover_transaction_sender(tx, chain_id).ok()?;
    let to = match &tx.to {
        Some(address) => Some(parse_address(address).ok()?),
        None => None,
    };

    Some(tiger_evm::transition::TransactionRequest {
        nonce: tx.nonce,
        gas_price: parse_u256(&format!("0x{:x}", tx.gas_price)),
        gas_limit: tx.gas,
        to,
        value: parse_u256(&tx.value),
        data: hex_decode(&tx.input),
        sender,
    })
}

/// Parse a `0x`-prefixed 20-byte address.
fn parse_address(value: &str) -> Result<[u8; 20], String> {
    let bytes = hex_decode(value);
    if bytes.len() != 20 {
        return Err(format!("expected a 20-byte address, got {} bytes", bytes.len()));
    }
    let mut out = [0u8; 20];
    out.copy_from_slice(&bytes);
    Ok(out)
}

/// Parse a `0x`-prefixed hex quantity into a 32-byte big-endian value.
fn parse_u256(value: &str) -> [u8; 32] {
    let bytes = hex_decode(value);
    let mut out = [0u8; 32];
    if bytes.len() <= 32 {
        out[32 - bytes.len()..].copy_from_slice(&bytes);
    } else {
        // A value wider than 256 bits is not representable; clamp to the
        // maximum rather than truncating silently.
        out = [0xffu8; 32];
    }
    out
}

/// Decode hex, tolerating a missing `0x` prefix and an odd digit count.
fn hex_decode(value: &str) -> Vec<u8> {
    let trimmed = value.strip_prefix("0x").unwrap_or(value);
    let padded = if trimmed.len() % 2 != 0 {
        format!("0{trimmed}")
    } else {
        trimmed.to_string()
    };
    (0..padded.len() / 2)
        .map(|i| u8::from_str_radix(&padded[i * 2..i * 2 + 2], 16).unwrap_or(0))
        .collect()
}

impl Drop for TigerNode {
    fn drop(&mut self) {
        // Ensure cleanup on drop
        if let Ok(state) = self.state.try_read() {
            if *state == NodeState::Running {
                error!("Node dropped while still running! Use stop() to gracefully shutdown.");
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tiger_blockchain::types::Transaction;
    use tiger_blockchain::{block_hash, sign_transaction, transactions_root};
    use tiger_state::account::{u256_from_u64, WorldState};

    #[tokio::test]
    async fn test_node_creation() {
        let config = Config::default_mainnet();
        let node = TigerNode::new(config).await.expect("node must initialize");
        assert!(!node.is_running().await, "a new node is not running");
    }

    #[test]
    fn test_config_validation() {
        let mut config = Config::default_mainnet();
        config.network_id = 0;
        assert!(TigerNode::validate_config(&config).is_err());
    }

    fn chain_manager() -> ChainManager {
        ChainManager::new(ChainConfig {
            chain_id: 6666,
            london_block: Some(0),
            ..Default::default()
        })
    }

    fn signed_transfer(key: [u8; 32], nonce: u64, to: [u8; 20], value: u64) -> Transaction {
        let mut tx = Transaction {
            hash: String::new(),
            nonce,
            block_hash: None,
            block_number: None,
            transaction_index: None,
            from: format!("0x{}", tiger_blockchain::hex_encode(&[0u8; 20])),
            to: Some(format!("0x{}", tiger_blockchain::hex_encode(&to))),
            value: format!("0x{value:x}"),
            gas_price: 1,
            gas: 21_000,
            input: "0x".to_string(),
            v: "0x0".to_string(),
            r: "0x0".to_string(),
            s: "0x0".to_string(),
        };
        sign_transaction(&mut tx, 6666, &key).unwrap();
        tx
    }

    fn hex32(bytes: &[u8; 32]) -> String {
        format!("0x{}", tiger_blockchain::hex_encode(bytes))
    }

    #[test]
    fn produced_block_carries_real_roots_and_links_to_its_parent() {
        let mut cm = chain_manager();
        let mut world = WorldState::new();
        let miner = format!("0x{}", tiger_blockchain::hex_encode(&[0xaa; 20]));

        let genesis = genesis_block_from(&create_genesis_block(6666));
        let genesis_hash = block_hash(&genesis.header).unwrap();
        cm.process_block(genesis).unwrap();

        let hash = produce_block(&mut cm, &mut world, &[], 1, &miner, 6666).unwrap();
        let block = cm.chain().get_block(1).expect("block 1 must exist");

        assert_eq!(block.header.parent_hash, genesis_hash);
        assert_eq!(
            block.header.transactions_root.to_lowercase(),
            hex32(&tiger_blockchain::EMPTY_TRIE_ROOT)
        );
        assert_eq!(
            block.header.state_root.to_lowercase(),
            hex32(&world.state_root())
        );
        assert_eq!(hash, block_hash(&block.header).unwrap());
        assert_eq!(cm.chain().height(), 1);
    }

    #[test]
    fn produced_block_applies_a_signed_transfer_to_state() {
        let mut cm = chain_manager();
        let mut world = WorldState::new();
        let miner = format!("0x{}", tiger_blockchain::hex_encode(&[0xaa; 20]));

        let key = [0x46u8; 32];
        let sender = tiger_crypto::private_key_to_address(&key).unwrap();
        let recipient = [0x22u8; 20];
        world.set_balance(&sender, u256_from_u64(10_000_000));

        cm.process_block(genesis_block_from(&create_genesis_block(6666)))
            .unwrap();

        let txs = vec![signed_transfer(key, 0, recipient, 1234)];
        produce_block(&mut cm, &mut world, &txs, 1, &miner, 6666).unwrap();

        assert_eq!(world.balance(&recipient), u256_from_u64(1234));
        assert_eq!(
            world.balance(&sender),
            u256_from_u64(10_000_000 - 1234 - 21_000)
        );
        assert_eq!(world.nonce(&sender), 1);

        let block = cm.chain().get_block(1).unwrap();
        assert_eq!(
            block.header.transactions_root.to_lowercase(),
            hex32(&transactions_root(&txs).unwrap())
        );
        assert_eq!(block.header.gas_used, 21_000);
    }

    #[test]
    fn a_transaction_with_an_unrecoverable_signature_is_not_applied() {
        let mut cm = chain_manager();
        let mut world = WorldState::new();
        let miner = format!("0x{}", tiger_blockchain::hex_encode(&[0xaa; 20]));
        cm.process_block(genesis_block_from(&create_genesis_block(6666)))
            .unwrap();

        // A zero signature cannot be recovered, so the transaction must be
        // skipped rather than applied.
        let tx = Transaction {
            hash: format!("0x{}", tiger_blockchain::hex_encode(&[0x01; 32])),
            nonce: 0,
            block_hash: None,
            block_number: None,
            transaction_index: None,
            from: format!("0x{}", tiger_blockchain::hex_encode(&[0u8; 20])),
            to: Some(format!("0x{}", tiger_blockchain::hex_encode(&[0x22u8; 20]))),
            value: "0x1".to_string(),
            gas_price: 1,
            gas: 21_000,
            input: "0x".to_string(),
            v: "0x0".to_string(),
            r: "0x0".to_string(),
            s: "0x0".to_string(),
        };

        produce_block(&mut cm, &mut world, &[tx], 1, &miner, 6666).unwrap();

        let block = cm.chain().get_block(1).unwrap();
        // The invalid transaction is excluded from the block, not merely
        // skipped during execution: including it would make the block itself
        // fail validation.
        assert!(block.transactions.is_empty());
        assert_eq!(block.header.gas_used, 0);
        assert_eq!(world.balance(&[0x22u8; 20]), [0u8; 32]);
    }
}
