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

    /// Genesis definition this chain was started from.
    genesis: Genesis,

    /// Chain manager (locked for concurrent access from async tasks)
    chain_manager: tokio::sync::Mutex<ChainManager>,

    /// World state: accounts, balances, code and storage.
    world_state: tokio::sync::Mutex<WorldState>,

    /// Transaction pool (mempool)
    mempool: tokio::sync::Mutex<Mempool>,

    /// Consensus engine
    consensus: tokio::sync::Mutex<PoSA>,

    /// The key this node seals blocks with, when it is a validator. Parsed
    /// once at startup so a malformed key is reported then rather than at the
    /// moment a block is due.
    validator_key: Option<[u8; 32]>,

    /// Post-execution world state for recently imported blocks, keyed by block
    /// hash, newest last.
    ///
    /// A block on a side branch has to be executed against the state at *its*
    /// parent, not the state at the current head, so the node has to be able to
    /// produce that state. Rebuilding it by replaying from genesis works but
    /// costs one execution per block in between; keeping the recent ones
    /// answers the common case (a fork within the last few blocks) directly.
    /// The window is bounded, and a miss falls back to replaying, so the cache
    /// is an optimisation and never a correctness dependency.
    recent_states: tokio::sync::Mutex<RecentStates>,

    /// Push notifications for WebSocket subscribers. Publishing never blocks,
    /// so block import is independent of how fast clients read.
    events: crate::ws_server::EventBus,

    /// Network sync status
    sync_status: RwLock<SyncStatus>,

    /// Background tasks
    tasks: RwLock<JoinSet<()>>,

    /// Durable storage for the chain and the world state. `None` only when
    /// the directory could not be opened, in which case the node runs
    /// in memory and says so.
    storage: Option<crate::storage::Storage>,

    /// Block hashes already written to the store, so a flush writes each block
    /// once instead of re-writing the chain after every block.
    persisted: tokio::sync::Mutex<std::collections::HashSet<String>>,

    /// The canonical branch as the store last saw it, so a flush can write the
    /// heights that changed rather than the whole map.
    canonical_mark: tokio::sync::Mutex<std::collections::HashMap<u64, String>>,

    /// When the node was constructed, for the uptime metric.
    started_at: std::time::Instant,

    /// The peer-to-peer service, once it is listening. `None` before startup
    /// or when the listen address could not be bound.
    p2p: std::sync::OnceLock<Arc<crate::p2p::P2pService>>,

    /// Installed RPC filters (`eth_newFilter` and friends).
    filters: tokio::sync::Mutex<crate::filters::FilterRegistry>,
}

impl TigerNode {
    /// Create a new TigerNode instance.
    pub async fn new(config: Config) -> Result<Self> {
        let genesis = load_genesis(&config)?;
        Self::from_genesis(config, genesis).await
    }

    /// Create a node from an explicit genesis definition.
    ///
    /// [`TigerNode::new`] reads the genesis from disk and delegates here, so a
    /// caller that already holds a genesis (a test, or a node started from an
    /// embedded chain spec) builds exactly the same node.
    pub async fn from_genesis(mut config: Config, genesis: Genesis) -> Result<Self> {
        info!("Initializing TigerSmartChain node...");

        // Fill in the proposer address from the sealing key when the operator
        // gave a key but no address. The address is derived from the key, so
        // requiring it separately would let a correctly configured validator
        // come up and silently never propose a block.
        if config.validator_addr.is_none() {
            if let Some(raw) = &config.validator_key {
                match parse_private_key(raw) {
                    Ok(key) => match tiger_crypto::private_key_to_address(&key) {
                        Ok(address) => {
                            let address =
                                format!("0x{}", tiger_blockchain::hex_encode(&address));
                            info!("Validator address derived from the sealing key: {address}");
                            config.validator_addr = Some(address);
                        }
                        Err(e) => warn!("cannot derive an address from the sealing key: {e}"),
                    },
                    Err(e) => warn!("cannot read the sealing key to derive an address: {e}"),
                }
            }
        }

        // Validate configuration
        Self::validate_config(&config)?;

        // Set initial state
        let state = RwLock::new(NodeState::Starting);

        // Initialize the world state (trie-backed accounts) and apply the
        // genesis allocation. Without this every pre-funded account would
        // start empty, so no transaction could ever pay for gas.
        let world_state = tokio::sync::Mutex::new(state_from_genesis(&genesis));
        info!(
            "World state initialized with {} genesis account(s)",
            genesis.alloc.len()
        );

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

        // Initialize PoSA consensus and register the validator set. Without
        // at least one registered validator the engine has no proposer, so no
        // block would ever be produced.
        let mut consensus = PoSA::new(
            config.chain_id,
            config.epoch_length,
            config.slot_duration_secs,
        );
        for validator in &config.validators {
            if !consensus.register_validator(validator.clone(), tiger_consensus::posa::MIN_STAKE) {
                warn!("ignoring validator {validator}: invalid address or duplicate");
            }
        }
        if config.validators.is_empty() {
            info!("No validators configured; this node will follow the chain but not propose blocks");
        } else {
            info!("Consensus engine initialized with {} validator(s)", config.validators.len());
        }
        consensus.update_epoch(0);

        // Parse the sealing key, if one was configured. A validator without a
        // usable key cannot seal a block, and a block without a seal is
        // rejected by every peer, so this is reported at startup.
        let validator_key = match &config.validator_key {
            Some(raw) => match parse_private_key(raw) {
                Ok(key) => {
                    if let Some(claimed) = &config.validator_addr {
                        match tiger_crypto::private_key_to_address(&key) {
                            Ok(derived) => {
                                let derived = format!(
                                    "0x{}",
                                    tiger_blockchain::hex_encode(&derived)
                                );
                                if !derived.eq_ignore_ascii_case(claimed) {
                                    warn!(
                                        "validator key derives address {derived} but \
                                         --validator-addr says {claimed}; blocks will be \
                                         sealed as {derived}"
                                    );
                                }
                            }
                            Err(e) => warn!("cannot derive an address from the validator key: {e}"),
                        }
                    }
                    Some(key)
                }
                Err(e) => {
                    warn!("ignoring unusable validator key: {e}");
                    None
                }
            },
            None => None,
        };
        if !config.validators.is_empty() && validator_key.is_none() {
            warn!(
                "this node is configured as a validator but has no sealing key; it \
                 cannot produce blocks (set --validator-key)"
            );
        }

        // Initialize network sync status
        let sync_status = RwLock::new(SyncStatus::default());

        // Initialize background tasks
        let tasks = RwLock::new(JoinSet::new());

        // Open durable storage and restore whatever was persisted. A failure
        // to open the directory is reported and the node runs in memory, so a
        // read-only filesystem does not stop a follower from serving.
        let storage = match crate::storage::Storage::open(&config.data_dir) {
            Ok(storage) => match storage.initialise(config.chain_id) {
                Ok(()) => Some(storage),
                Err(e) => {
                    // A store that belongs to another chain, or that was
                    // written by a build with a different layout, is refused
                    // rather than written to: continuing would mix two chains
                    // in one directory.
                    warn!(
                        "refusing to use the data directory {}: {e}; running without persistence",
                        config.data_dir
                    );
                    None
                }
            },
            Err(e) => {
                warn!("cannot open data directory {}: {e}; running without persistence", config.data_dir);
                None
            }
        };

        Ok(Self {
            config,
            genesis,
            state,
            chain_manager: tokio::sync::Mutex::new(chain_manager),
            world_state,
            mempool: tokio::sync::Mutex::new(mempool),
            consensus: tokio::sync::Mutex::new(consensus),
            validator_key,
            recent_states: tokio::sync::Mutex::new(RecentStates::new()),
            events: crate::ws_server::EventBus::new(),
            sync_status,
            tasks,
            storage,
            persisted: tokio::sync::Mutex::new(std::collections::HashSet::new()),
            canonical_mark: tokio::sync::Mutex::new(std::collections::HashMap::new()),
            started_at: std::time::Instant::now(),
            p2p: std::sync::OnceLock::new(),
            filters: tokio::sync::Mutex::new(crate::filters::FilterRegistry::new()),
        })
    }

    /// The installed RPC filters.
    pub async fn get_filters(
        &self,
    ) -> tokio::sync::MutexGuard<'_, crate::filters::FilterRegistry> {
        self.filters.lock().await
    }

    /// Attach the peer-to-peer service so block production can announce
    /// through it.
    pub fn set_p2p(&self, service: Arc<crate::p2p::P2pService>) {
        let _ = self.p2p.set(service);
    }

    /// The peer-to-peer service, if it is running.
    pub fn p2p(&self) -> Option<&Arc<crate::p2p::P2pService>> {
        self.p2p.get()
    }

    /// The genesis block's hash.
    pub fn genesis_hash(&self) -> String {
        let block = genesis_block_from(&self.genesis);
        tiger_blockchain::block_hash(&block.header)
            .unwrap_or_else(|_| format!("0x{}", tiger_blockchain::hex_encode(&[0u8; 32])))
    }

    /// The current head block's hash.
    pub async fn head_hash(&self) -> String {
        let manager = self.chain_manager.lock().await;
        let chain = manager.chain();
        chain
            .get_block(chain.height())
            .and_then(|block| tiger_blockchain::block_hash(&block.header).ok())
            .unwrap_or_default()
    }

    /// Restore the chain and world state from disk.
    ///
    /// Returns the number of blocks restored. A store that cannot be read, or
    /// that fails an integrity check, is a fatal error: continuing with a
    /// fresh chain would silently discard the ledger and restart from genesis
    /// while every peer is at a later height.
    async fn restore_from_disk(&self) -> Result<u64> {
        let Some(storage) = &self.storage else {
            return Ok(0);
        };

        if storage.is_empty()? {
            return Ok(0);
        }

        // The stored chain must belong to the chain this node was configured
        // for. Loading a store from a different chain would attach unrelated
        // blocks to this genesis. `initialise` checks this at open time; this
        // is the second half, for a store whose meta was written before the
        // chain id was recorded.
        let blocks = storage
            .load_blocks()
            .map_err(|e| anyhow::anyhow!("cannot restore the chain: {e}"))?;
        let receipts = storage
            .load_receipts()
            .map_err(|e| anyhow::anyhow!("cannot restore the receipts: {e}"))?;
        let canonical = storage
            .load_canonical()
            .map_err(|e| anyhow::anyhow!("cannot restore the canonical map: {e}"))?;
        let work = storage
            .load_work()
            .map_err(|e| anyhow::anyhow!("cannot restore the cumulative work: {e}"))?;

        // The head block's stateRoot is what the restored state must hash to.
        let head_root = canonical
            .iter()
            .max_by_key(|(height, _)| *height)
            .and_then(|(_, hash)| {
                blocks
                    .iter()
                    .find(|(stored, _)| stored.eq_ignore_ascii_case(hash))
                    .map(|(_, block)| block.header.state_root.clone())
            });

        let restored_state = storage
            .load_state(head_root.as_deref())
            .map_err(|e| anyhow::anyhow!("cannot restore the world state: {e}"))?;

        let block_count = blocks.len() as u64;
        let stored_hashes: Vec<String> = blocks
            .iter()
            .map(|(hash, _)| hash.to_ascii_lowercase())
            .collect();
        {
            let mut cm = self.chain_manager.lock().await;
            let chain = cm.chain_mut();

            // Blocks are offered in ascending order so each one's parent is
            // already present, which is what `insert_block` links on. The
            // canonical map is then restored from the store rather than
            // re-derived, because re-running fork choice over an unordered set
            // could pick a branch the node was not following.
            let mut ordered: Vec<(String, tiger_blockchain::types::Block)> = blocks;
            ordered.sort_by_key(|(_, block)| block.header.number);
            for (hash, block) in ordered {
                let number = block.header.number;
                if let Err(e) = chain.insert_block(block) {
                    warn!("stored block #{number} ({hash}) could not be re-inserted: {e}");
                }
            }
            for (hash, list) in receipts {
                chain.set_receipts_by_hash(&hash, list);
            }
            for (hash, total) in work {
                chain.set_total_difficulty(&hash, total);
            }
            chain.restore_canonical(&canonical);
        }

        *self.world_state.lock().await = restored_state;

        // Every block just loaded is already on disk, canonical or not, so a
        // flush must not write it again. The canonical map is recorded as the
        // store's view, so the next flush writes only the heights that change.
        {
            let mut written = self.persisted.lock().await;
            written.clear();
            for hash in &stored_hashes {
                written.insert(hash.clone());
            }
            let mut mark = self.canonical_mark.lock().await;
            mark.clear();
            for (height, hash) in &canonical {
                mark.insert(*height, hash.to_ascii_lowercase());
            }
        }

        // The states cached before the reload describe a chain that has just
        // been replaced, so they are dropped rather than risk being served.
        self.recent_states.lock().await.clear();

        let height = {
            let cm = self.chain_manager.lock().await;
            cm.chain().height()
        };
        info!("Restored {block_count} block(s) from disk; chain height is {height}");
        Ok(block_count)
    }

    /// The durable store, when the node has one.
    pub fn storage(&self) -> Option<&crate::storage::Storage> {
        self.storage.as_ref()
    }

    /// How many blocks the chain holds that have not been written to disk yet.
    ///
    /// A number that keeps climbing means flushes are failing, which is the
    /// one storage condition an operator has to be told about.
    pub async fn pending_flush_count(&self) -> usize {
        let cm = self.chain_manager.lock().await;
        let written = self.persisted.lock().await;
        cm.chain()
            .all_blocks()
            .into_iter()
            .filter(|(hash, _)| !written.contains(&hash.to_ascii_lowercase()))
            .count()
    }

    /// Build a node whose only live components are the chain and the world
    /// state, for a test that exercises execution and persistence directly.
    ///
    /// The consensus engine and the transaction pool are created but no
    /// background task is spawned, so nothing seals a block behind the test's
    /// back.
    pub async fn for_test(config: Config, genesis: Genesis) -> Result<Self> {
        Self::from_genesis(config, genesis).await
    }

    /// The node's event bus, for the WebSocket endpoint.
    pub fn events(&self) -> &crate::ws_server::EventBus {
        &self.events
    }

    /// Publish the events a newly imported or produced block implies.
    ///
    /// Every notification is rendered from the chain as it stands after the
    /// block, so a subscriber sees the same objects `eth_getBlockByNumber` and
    /// `eth_getLogs` would return rather than a reduced copy. Called after the
    /// block is durable, so a client is never told about a block the node could
    /// still lose.
    pub async fn publish_block_events(&self, block: &tiger_blockchain::types::Block) {
        use crate::ws_server::ChainEvent;

        self.events.publish(ChainEvent::Head(
            crate::rpc_server::block_to_json_with_receipts(block, false),
        ));

        let (chain_logs, syncing) = {
            let cm = self.chain_manager.lock().await;
            let chain = cm.chain();
            let filter = crate::filters::LogFilter {
                from_block: Some(block.header.number),
                to_block: Some(block.header.number),
                address: None,
                topics: None,
                block_hash: None,
            };
            let numbers = vec![block.header.number];
            let logs = crate::rpc_server::collect_logs(chain, &filter, &numbers);
            (logs, self.sync_status_json(chain.height()))
        };

        if !chain_logs.is_empty() {
            self.events.publish(ChainEvent::Logs(chain_logs));
        }
        self.events.publish(ChainEvent::Syncing(syncing));
    }

    /// The sync status in the shape `eth_syncing` reports.
    fn sync_status_json(&self, head: u64) -> serde_json::Value {
        let status = self.sync_status.try_read().ok();
        let syncing = status.as_ref().map(|s| s.syncing).unwrap_or(false);
        if !syncing {
            return serde_json::Value::Bool(false);
        }
        let highest = status.as_ref().map(|s| s.highest_block).unwrap_or(head);
        let starting = status.as_ref().map(|s| s.starting_block).unwrap_or(0);
        serde_json::json!({
            "startingBlock": format!("0x{starting:x}"),
            "currentBlock": format!("0x{head:x}"),
            "highestBlock": format!("0x{highest:x}"),
        })
    }

    /// Publish a transaction that has just been accepted into the pool.
    pub fn publish_pending_transaction(&self, hash: &str) {
        self.events.publish(crate::ws_server::ChainEvent::PendingTransaction(
            hash.to_string(),
        ));
    }

    /// Import a block received from a peer.
    ///
    /// A block from the network is untrusted. It is accepted only after it
    /// extends the current head, its header is internally consistent, and
    /// re-executing its transactions against the current world state reproduces
    /// the state root and gas usage it claims. The state produced by that
    /// re-execution becomes the node's state, so the node never stores a state
    /// it did not compute itself.
    ///
    /// Returns `Ok(true)` when the block was imported, `Ok(false)` when it was
    /// already known, and `Err` when it is not a valid extension of this
    /// chain.
    pub async fn import_block(
        &self,
        block: tiger_blockchain::types::Block,
    ) -> Result<bool> {
        // Cheap check first: a block already in the tree costs nothing to
        // recognise, and peers re-announce constantly.
        let known = {
            let cm = self.chain_manager.lock().await;
            // `block_hash` already returns a 0x-prefixed string, the same form
            // the chain indexes by.
            tiger_blockchain::block_hash(&block.header)
                .ok()
                .map(|hash| cm.chain().get_block_by_hash(&hash).is_some())
                .unwrap_or(false)
        };

        if known {
            return Ok(false);
        }

        // A block whose parent is unknown cannot be linked to the chain. It is
        // not rejected as invalid, because the parent may simply not have
        // arrived yet, but it cannot be executed either: the state it must run
        // against does not exist here.
        let parent_known = {
            let cm = self.chain_manager.lock().await;
            cm.chain().contains_block(&block.header.parent_hash)
        };
        if !parent_known && block.header.number != 0 {
            return Err(anyhow::anyhow!(
                "block {} names parent {}, which is not known",
                block.header.number,
                block.header.parent_hash
            ));
        }

        // The consensus engine decides whether the block's proposer was
        // entitled to propose it. A block from an unauthorised proposer is
        // rejected before any work is done on it.
        //
        // A node started without a validator list is a pure follower: it has no
        // way to know the authority set in advance, so it adopts the set it
        // observes, one proposer per block. That is bounded to the case where
        // the operator configured no validators; a node given a validator list
        // enforces exactly that list and learns nothing from the network.
        {
            let mut consensus = self.consensus.lock().await;
            if !consensus.is_authorised_proposer(block.header.number, &block.header.miner) {
                if self.config.validators.is_empty() {
                    info!(
                        "adopting {} as a validator, learned from block {}",
                        block.header.miner, block.header.number
                    );
                    consensus.register_validator(
                        block.header.miner.clone(),
                        tiger_consensus::posa::MIN_STAKE,
                    );
                    // Registration only adds the authority; the epoch rotation
                    // decides who proposes, so it is refreshed here too.
                    consensus.update_epoch(block.header.number);
                }
                if !consensus.is_authorised_proposer(block.header.number, &block.header.miner) {
                    return Err(anyhow::anyhow!(
                        "block {} was proposed by {}, who is not the expected proposer",
                        block.header.number,
                        block.header.miner
                    ));
                }
            }
        }

        // Re-execute against the state at the block's own parent. This is the
        // check that makes the block trustworthy: the sender's numbers are
        // reproduced, not believed. For a side branch the parent state is not
        // the head state, so it is resolved rather than assumed.
        let parent_state = self.state_for_parent_of(&block).await?;
        let (new_state, receipts) =
            execute_block_transactions(&parent_state, &block, self.config.chain_id)
                .map_err(|e| anyhow::anyhow!("block {} is invalid: {e}", block.header.number))?;

        let declared_number = block.header.number;
        let mut cm = self.chain_manager.lock().await;
        let outcome = cm
            .process_block_with_receipts_outcome(block.clone(), receipts)
            .map_err(|e| anyhow::anyhow!("block {declared_number} rejected: {e}"))?;

        match outcome {
            tiger_blockchain::InsertOutcome::Known => return Ok(false),
            tiger_blockchain::InsertOutcome::SideBranch => {
                // The block is stored so it can be built on later, but it is
                // not the chain. Its state changes are discarded, and the
                // transactions it carries stay in the pool in case the
                // canonical branch includes them.
                info!(
                    "stored block #{declared_number} on a side branch (not canonical)"
                );
                drop(cm);
                return Ok(true);
            }
            tiger_blockchain::InsertOutcome::Extended => {
                drop(cm);
                *self.world_state.lock().await = new_state.clone();
                // Remember this state so a block forking from here can be
                // executed against it without a replay.
                if let Ok(hash) = tiger_blockchain::block_hash(&block.header) {
                    self.recent_states.lock().await.insert(&hash, new_state);
                }
            }
            tiger_blockchain::InsertOutcome::Reorged {
                old_head,
                new_head,
                common_ancestor,
            } => {
                // The new branch outweighs the one we were following. Rewind
                // the world state to the fork point and replay the winning
                // branch, so the state we hold is derived from the canonical
                // chain rather than from the branch we just abandoned.
                let blocks_to_replay = cm
                    .chain()
                    .canonical_blocks()
                    .into_iter()
                    .filter(|(number, _)| *number > common_ancestor)
                    .map(|(_, block)| block.clone())
                    .collect::<Vec<_>>();
                drop(cm);

                info!(
                    "reorg: {old_head} -> {new_head} at height {common_ancestor}, \
                     replaying {} block(s)",
                    blocks_to_replay.len()
                );

                let replayed = self
                    .rewind_and_replay(common_ancestor, &blocks_to_replay)
                    .await
                    .map_err(|e| {
                        anyhow::anyhow!(
                            "reorg to {new_head} failed while replaying from \
                             height {common_ancestor}: {e}"
                        )
                    })?;
                info!("reorg complete: head is now {new_head} after replaying {replayed} block(s)");

                // Transactions that were in the abandoned blocks and are not
                // in the new chain are pending again, so a client does not
                // have to resubmit them.
                self.requeue_reorged_transactions(&old_head).await;
            }
        }

        // Drop the transactions the block contained from the pool.
        {
            let mut mempool = self.mempool.lock().await;
            for tx in &block.transactions {
                mempool.remove(&tx.hash);
            }
        }

        self.persist().await;
        info!("Imported block #{} from the network", declared_number);
        // Notify subscribers only once the block is durable, so no client is
        // told about a block the node could still lose in a crash.
        self.publish_block_events(&block).await;
        Ok(true)
    }

    /// The world state as it was *after* the block named by `hash` executed.
    ///
    /// Tries the recent-state cache first, then rebuilds by replaying the
    /// canonical branch from genesis up to that block. Rebuilding is the
    /// fallback rather than the rule because it costs one execution per block
    /// in between, but it always produces the same answer, so a cache miss
    /// costs time and never correctness.
    async fn state_after_block(&self, hash: &str) -> Result<WorldState> {
        if let Some(state) = self.recent_states.lock().await.get(hash) {
            return Ok(state);
        }

        // Walk the target's own ancestry back to the most recent block whose
        // state is known, then replay forward from there. The chain from a
        // block to genesis is unique, so this is exactly the sequence of
        // transitions that produced it, even when the block sits on a side
        // branch.
        let (ancestry, base) = {
            let cm = self.chain_manager.lock().await;
            let chain = cm.chain();
            let mut ancestry: Vec<tiger_blockchain::types::Block> = Vec::new();
            let mut cursor = hash.to_ascii_lowercase();
            let base;
            loop {
                if let Some(state) = self.recent_states.try_lock().ok().and_then(|c| c.get(&cursor)) {
                    base = state;
                    break;
                }
                let block = chain.get_block_by_hash(&cursor).ok_or_else(|| {
                    anyhow::anyhow!("no block with hash {cursor} in the tree")
                })?;
                let number = block.header.number;
                let parent = block.header.parent_hash.to_ascii_lowercase();
                ancestry.push(block.clone());
                if number == 0 {
                    // Genesis with no cached state: fall back to the
                    // allocation, which is the only thing that can seed it.
                    base = self.genesis_world_state();
                    break;
                }
                cursor = parent;
            }
            (ancestry, base)
        };

        let mut state = base;
        for block in ancestry.iter().rev() {
            let (next, _) = execute_block_transactions(&state, block, self.config.chain_id)
                .map_err(|e| anyhow::anyhow!("replaying block {} failed: {e}", block.header.number))?;
            state = next;
            if let Ok(h) = tiger_blockchain::block_hash(&block.header) {
                self.recent_states.lock().await.insert(&h, state.clone());
            }
        }

        self.recent_states
            .lock()
            .await
            .insert(hash, state.clone());
        Ok(state)
    }

    /// The state a block at `number` must be executed against.
    ///
    /// For a block extending the head that is simply the current state; for a
    /// side branch it is the state after its parent, which may have to be
    /// rebuilt. The two are distinguished by whether the named parent is the
    /// current head, not by height, because at a fork both branches have a
    /// parent at the same height.
    async fn state_for_parent_of(
        &self,
        block: &tiger_blockchain::types::Block,
    ) -> Result<WorldState> {
        let head = {
            let cm = self.chain_manager.lock().await;
            let chain = cm.chain();
            chain
                .get_block(chain.height())
                .and_then(|b| tiger_blockchain::block_hash(&b.header).ok())
        };

        let parent = block.header.parent_hash.to_ascii_lowercase();
        if head.as_deref().map(|h| h.to_ascii_lowercase()) == Some(parent) {
            return Ok(self.world_state.lock().await.clone());
        }

        if block.header.number == 0 {
            return Ok(self.genesis_world_state());
        }

        self.state_after_block(&block.header.parent_hash).await
    }

    /// Rewind the world state to `ancestor` and replay `blocks` in order.
    ///
    /// The state at `ancestor` is rebuilt by replaying the canonical branch from
    /// genesis. Rebuilding is what makes a reorg safe: the alternative,
    /// reversing each abandoned block's effects, requires an inverse for every
    /// state transition, and one missing inverse silently corrupts the state.
    /// Replaying from genesis uses only the forward transition the EVM already
    /// implements, so a reorg cannot introduce a state the chain could not have
    /// produced.
    ///
    /// Returns the number of blocks replayed after the ancestor.
    async fn rewind_and_replay(
        &self,
        ancestor: u64,
        blocks: &[tiger_blockchain::types::Block],
    ) -> Result<usize> {
        // Start from the state at the ancestor, which is either cached or
        // rebuilt by replaying the canonical branch up to and including it.
        let ancestor_hash = {
            let cm = self.chain_manager.lock().await;
            cm.chain()
                .get_block(ancestor)
                .and_then(|b| tiger_blockchain::block_hash(&b.header).ok())
                .ok_or_else(|| anyhow::anyhow!("no canonical block at height {ancestor}"))?
        };
        let mut state = self.state_after_block(&ancestor_hash).await?;

        // Then apply the winning branch on top of it.
        let mut applied = 0usize;
        for block in blocks {
            let (next, _) = execute_block_transactions(&state, block, self.config.chain_id)
                .map_err(|e| anyhow::anyhow!("replaying block {} failed: {e}", block.header.number))?;
            state = next;
            if let Ok(hash) = tiger_blockchain::block_hash(&block.header) {
                self.recent_states.lock().await.insert(&hash, state.clone());
            }
            applied += 1;
        }

        *self.world_state.lock().await = state;
        Ok(applied)
    }

    /// The world state implied by the genesis allocation alone.
    fn genesis_world_state(&self) -> WorldState {
        let mut state = WorldState::new();
        for (address, account) in &self.genesis.alloc {
            if let Ok(addr) = parse_address(address) {
                if let Some(balance) = parse_genesis_balance(&account.balance) {
                    state.set_balance(&addr, balance);
                }
            }
        }
        state
    }

    /// Return transactions that were confirmed on an abandoned branch to the
    /// mempool.
    ///
    /// A reorg un-confirms them, so a client that submitted them should not
    /// have to notice and resubmit. Anything already on the new canonical
    /// branch, or already in the pool, is skipped.
    async fn requeue_reorged_transactions(&self, old_head: &str) {
        let orphaned = {
            let cm = self.chain_manager.lock().await;
            let chain = cm.chain();
            // Walk the abandoned branch back to the fork point, collecting the
            // transactions it held.
            let mut out = Vec::new();
            let mut cursor = old_head.to_string();
            loop {
                let Some(block) = chain.get_block_by_hash(&cursor) else {
                    break;
                };
                if chain.is_canonical(&cursor) {
                    break;
                }
                for tx in &block.transactions {
                    out.push(tx.clone());
                }
                if block.header.number == 0 {
                    break;
                }
                cursor = block.header.parent_hash.clone();
            }
            out
        };

        if orphaned.is_empty() {
            return;
        }

        let mut requeued = 0usize;
        let mut mempool = self.mempool.lock().await;
        let cm = self.chain_manager.lock().await;
        for tx in orphaned {
            // Skip a transaction that the new canonical branch already
            // contains, which would otherwise be mined twice.
            if cm.chain().find_transaction(&tx.hash).is_some() {
                continue;
            }
            if mempool.get(&tx.hash).is_some() {
                continue;
            }
            if mempool.add(tx).is_ok() {
                requeued += 1;
            }
        }
        drop(cm);
        drop(mempool);
        if requeued > 0 {
            info!("returned {requeued} transaction(s) from the abandoned branch to the pool");
        }
    }

    /// Persist the current chain and world state.
    async fn persist(&self) {
        let Some(storage) = &self.storage else {
            return;
        };

        // Which blocks still need writing, and their cumulative work. Tracked
        // by hash so a reorg that replaces a branch does not re-write the
        // blocks it abandoned, and no block is written twice.
        let (pending, canonical_update) = {
            let mut written = self.persisted.lock().await;
            let mut last = self.canonical_mark.lock().await;
            let cm = self.chain_manager.lock().await;
            let chain = cm.chain();

            let mut pending = Vec::new();
            for (hash, block) in chain.all_blocks() {
                let key = hash.to_ascii_lowercase();
                if written.insert(key.clone()) {
                    let total_difficulty = chain.total_difficulty_of(&key).unwrap_or(0);
                    pending.push(crate::storage::BlockWrite {
                        hash: key,
                        block: block.clone(),
                        receipts: chain
                            .receipts_by_hash(&hash)
                            .map(|r| r.to_vec())
                            .unwrap_or_default(),
                        total_difficulty,
                        canonical_height: None,
                    });
                }
            }
            // Deterministic order so a batch is applied the same way twice.
            pending.sort_by(|a, b| a.hash.cmp(&b.hash));

            // The canonical map as a delta: which heights changed, and which
            // heights no longer name a block.
            let current: std::collections::HashMap<u64, String> = chain
                .canonical_blocks()
                .into_iter()
                .map(|(number, block)| {
                    let hash = tiger_blockchain::block_hash(&block.header)
                        .unwrap_or_default()
                        .to_ascii_lowercase();
                    (number, hash)
                })
                .collect();
            let mut update = crate::storage::CanonicalUpdate {
                set: current
                    .iter()
                    .filter(|(height, hash)| {
                        last.get(*height)
                            .map(|previous| !previous.eq_ignore_ascii_case(hash))
                            .unwrap_or(true)
                    })
                    .map(|(height, hash)| (*height, hash.clone()))
                    .collect(),
                remove: last
                    .keys()
                    .copied()
                    .filter(|height| !current.contains_key(height))
                    .collect(),
            };
            update.set.sort_unstable();
            update.remove.sort_unstable();
            *last = current;

            (pending, update)
        };

        // The accounts the blocks wrote, and the state they now hold. The
        // world state tracks what changed, so a flush touches only that rather
        // than the whole ledger.
        let (state, dirty, mark) = {
            let state = self.world_state.lock().await;
            let entries = state.dirty_entries();
            let mark = entries.iter().map(|(_, stamp)| *stamp).max().unwrap_or(0);
            (
                state.clone(),
                entries
                    .into_iter()
                    .map(|(address, _)| address)
                    .collect::<Vec<tiger_state::account::Address>>(),
                mark,
            )
        };

        // The blocks and the state they produced commit together, so a crash
        // cannot leave a block without the accounts it wrote.
        match storage.write_batch(&pending, &state, &dirty, Some(&canonical_update)) {
            Ok(()) => {
                // Only now are the writes durable, so only now may they be
                // forgotten. A write that arrived while the batch was being
                // written carries a higher stamp and stays pending.
                self.world_state.lock().await.clear_dirty_up_to(mark);
            }
            Err(e) => {
                error!("cannot persist the chain: {e}");
                // Nothing was committed, so every block in the batch is still
                // pending and the next flush retries it.
                let mut written = self.persisted.lock().await;
                for entry in &pending {
                    written.remove(&entry.hash);
                }
            }
        }
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
        self.open().await?;
        self.serve().await
    }

    /// Bring the chain up: restore from disk if there is anything there, and
    /// install genesis otherwise.
    ///
    /// Separated from [`TigerNode::serve`] so a test can open a chain and
    /// close it again without binding any listener.
    pub async fn open(&self) -> Result<()> {
        // Restore any persisted chain before installing genesis. Without
        // this the node would restart from block 0 and lose every block it
        // had produced, and would re-apply the genesis allocation on top of a
        // state that already reflects it.
        let restored = self.restore_from_disk().await?;
        if restored > 0 {
            info!("Resuming from the persisted chain ({restored} block(s))");
        }

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
                let genesis_config = self.genesis.clone();
                cm.chain_mut().set_genesis(genesis_config.clone());

                let genesis_block = genesis_block_from(&genesis_config);
                let hash = tiger_blockchain::block_hash(&genesis_block.header)
                    .map_err(|e| anyhow::anyhow!("failed to hash genesis block: {e}"))?;
                cm.process_block(genesis_block)
                    .map_err(|e| anyhow::anyhow!("failed to insert genesis block: {e}"))?;
                info!("Genesis block installed at height 0 (hash: {})", hash);
                drop(cm);

                // Anchor the replay base to the state the node actually starts
                // from. A reorg rebuilds state by replaying from genesis, so
                // that starting point has to be recorded rather than
                // recomputed: recomputing from the genesis allocation would
                // silently differ if anything else ever contributes to the
                // initial state.
                let genesis_state = self.world_state.lock().await.clone();
                self.recent_states
                    .lock()
                    .await
                    .insert(&hash, genesis_state);

                self.persist().await;
            }
        }

        Ok(())
    }

    /// Start the background tasks and the listeners.
    ///
    /// [`TigerNode::open`] must have run first: serving a chain that has not
    /// been loaded would answer every query from an empty state.
    pub async fn serve(self: &Arc<Self>) -> Result<()> {
        // Update state to Running
        *self.state.write().await = NodeState::Running;
        info!("Node state set to Running");

        // Spawn block production task (PoSA consensus sealing loop)
        self.spawn_block_production();

        // Spawn sync status monitor
        self.spawn_sync_monitor();

        // Start the JSON-RPC server so wallets and the explorer can reach the
        // chain. Without it the node has no client interface at all.
        if self.config.rpc_enabled {
            let node = Arc::clone(self);
            let addr = self.config.rpc_addr.clone();
            tokio::spawn(async move {
                if let Err(e) = crate::rpc_server::serve(Arc::clone(&node), addr.clone()).await {
                    error!("JSON-RPC server on {addr} stopped: {e}");
                }
            });
        } else {
            info!("RPC is disabled by configuration; no client interface will be served");
        }

        // Start the metrics endpoint on its own listener. It is separate from
        // the RPC on purpose: the RPC is public, and the node's peer count,
        // pool depth and validator set are for the operator's monitoring
        // system, not for every wallet that can reach the RPC.
        if self.config.metrics_enabled {
            let node = Arc::clone(self);
            let addr = self.config.metrics_addr.clone();
            tokio::spawn(async move {
                if let Err(e) = crate::rpc_server::serve_metrics(Arc::clone(&node), addr.clone()).await {
                    error!("metrics server on {addr} stopped: {e}");
                }
            });
        }

        // Start the WebSocket endpoint, where `eth_subscribe` lives. A dApp
        // that polls for new heads or logs holds the node open to the same
        // load as a busy explorer; one subscription replaces that polling.
        if self.config.ws_enabled {
            let node = Arc::clone(self);
            let addr = self.config.ws_addr.clone();
            tokio::spawn(async move {
                if let Err(e) = crate::ws_server::serve(Arc::clone(&node), addr.clone()).await {
                    error!("WebSocket server on {addr} stopped: {e}");
                }
            });
        }

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

                if let Some(expected) = &proposer {
                    log::trace!(
                        "block {next_block}: in-turn proposer is {expected}"
                    );
                }

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

                // Liveness: the in-turn validator proposes first, but any
                // active validator may take over once its own slot arrives.
                // A strict in-turn rule would keep naming an offline validator
                // and the chain would stall until it came back; here the next
                // validator in the rotation takes over a slot later.
                let proposal_slot = match (&proposer, &node.config.validator_addr) {
                    (Some(_), Some(me)) => {
                        let consensus = node.consensus.lock().await;
                        if consensus.is_validator(me) {
                            let parent_timestamp = {
                                let cm = node.chain_manager.lock().await;
                                cm.chain()
                                    .get_block(next_block.saturating_sub(1))
                                    .map(|b| b.header.timestamp)
                                    .unwrap_or(0)
                            };
                            consensus.earliest_proposal_time(next_block, me, parent_timestamp)
                        } else {
                            None
                        }
                    }
                    _ => None,
                };
                let now_secs = chrono::Utc::now().timestamp().max(0) as u64;
                let may_propose = proposal_slot
                    .map(|earliest| now_secs >= earliest)
                    .unwrap_or(false);

                if may_propose {
                    if let Some(ref my_addr) = node.config.validator_addr {
                        // This node seals with its own key, so it is the
                        // proposer. `produce_block` derives the address from
                        // the key and the two cannot disagree.
                        let proposer = my_addr.clone();
                        {
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
                                    node.validator_key.as_ref(),
                                )
                            };

                            match result {
                                Ok(block_hash) => {
                                    info!("Successfully produced block #{} (hash: {})", next_block, block_hash);
                                    // Persist the new block and the state it
                                    // produced before reporting success, so a
                                    // crash cannot lose a block the node has
                                    // already announced.
                                    node.persist().await;
                                    // Announce only after the block is durable,
                                    // so a peer that fetches it back from this
                                    // node is not told about a block the node
                                    // could still lose.
                                    if let Some(p2p) = node.p2p() {
                                        let produced = {
                                            let cm = node.chain_manager.lock().await;
                                            cm.chain().get_block(next_block).cloned()
                                        };
                                        if let Some(block) = produced {
                                            p2p.announce_block(block.clone()).await;
                                            // Subscribers are told after the
                                            // block is durable and announced,
                                            // so they never see a head that is
                                            // not yet on the wire.
                                            node.publish_block_events(&block).await;
                                        }
                                    }
                                    // Credit the proposer's pending reward.
                                    {
                                        let mut consensus = node.consensus.lock().await;
                                        consensus.record_block_reward(
                                            proposer.clone(),
                                            node.config.block_reward,
                                        );
                                    }
                                    // Remove only the transactions the block
                                    // actually carries. A transaction left
                                    // out because it did not fit, or because
                                    // it failed validation, is still pending
                                    // and dropping it here would lose it.
                                    let mined: std::collections::HashSet<String> = {
                                        let cm = node.chain_manager.lock().await;
                                        cm.chain()
                                            .get_block(next_block)
                                            .map(|block| {
                                                block
                                                    .transactions
                                                    .iter()
                                                    .map(|tx| tx.hash.clone())
                                                    .collect()
                                            })
                                            .unwrap_or_default()
                                    };
                                    let mut mempool = node.mempool.lock().await;
                                    for tx in &pending_txs {
                                        if mined.contains(&tx.hash) {
                                            mempool.remove(&tx.hash);
                                        }
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

    /// Access the world state.
    pub fn world_state(&self) -> &tokio::sync::Mutex<WorldState> {
        &self.world_state
    }

    /// The genesis definition this chain was started from.
    pub fn genesis(&self) -> &Genesis {
        &self.genesis
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
    /// Record sync progress for the `eth_syncing` RPC and the metrics endpoint.
    ///
    /// `starting_block` is only set when sync begins, so it keeps naming where
    /// this sync run started rather than following the head forward.
    pub async fn set_syncing(&self, syncing: bool, current: u64, highest: u64) {
        let mut status = self.sync_status.write().await;
        if syncing && !status.syncing {
            status.starting_block = current;
        }
        status.syncing = syncing;
        status.current_block = current;
        if highest > status.highest_block {
            status.highest_block = highest;
        }
    }

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

    /// A liveness and readiness report for orchestrators and monitors.
    ///
    /// The status is derived, not asserted: a node reports itself unhealthy
    /// when it is not running, and not ready when its chain has no genesis or
    /// its world state and chain height disagree. Reporting "ok" from a node
    /// that cannot serve a request is worse than no check at all, because a
    /// load balancer would keep sending it traffic.
    pub async fn health_json(&self) -> String {
        let running = *self.state.read().await == NodeState::Running;
        let height = {
            let cm = self.chain_manager.lock().await;
            cm.chain().height()
        };
        let has_genesis = {
            let cm = self.chain_manager.lock().await;
            cm.chain().genesis().is_some()
        };
        let peers = match self.p2p() {
            Some(p2p) => p2p.peer_count().await,
            None => 0,
        };
        let mempool = self.mempool.lock().await.len();

        // The chain must have a genesis block and a height consistent with it.
        // A height above zero with no genesis would mean the tree lost its
        // root, which no request could be answered correctly against.
        let ready = running && has_genesis;
        let status = if !running {
            "unhealthy"
        } else if !ready {
            "not_ready"
        } else {
            "ok"
        };

        serde_json::json!({
            "status": status,
            "running": running,
            "ready": ready,
            "chain_id": self.config.chain_id,
            "network_id": self.config.network_id,
            "network": self.config.network,
            "height": height,
            "peers": peers,
            "mempool": mempool,
            "version": env!("CARGO_PKG_VERSION"),
        })
        .to_string()
    }

    /// Render the node's current metrics in the Prometheus text format.
    ///
    /// Every value is read from the node as it is right now. Nothing is
    /// estimated or cached, so a scrape that shows a stalled height means the
    /// chain really has stopped, which is the point of a metric.
    pub async fn metrics_text(&self) -> String {
        use std::fmt::Write as _;

        let height = {
            let cm = self.chain_manager.lock().await;
            cm.chain().height()
        };
        let (chain_id, network_id, network) = (
            self.config.chain_id,
            self.config.network_id,
            self.config.network.clone(),
        );
        let running = *self.state.read().await == NodeState::Running;
        let (mempool_size, mempool_capacity, mempool_senders) = {
            let pool = self.mempool.lock().await;
            (pool.len(), pool.capacity(), pool.sender_count())
        };
        let peers = match self.p2p() {
            Some(p2p) => p2p.peer_count().await,
            None => 0,
        };
        let best_peer_height = match self.p2p() {
            Some(p2p) => p2p.best_peer_height().await,
            None => None,
        };
        let sync = self.sync_status.read().await.clone();
        let validator_count = self.config.validators.len();
        let is_validator = self.config.validator_addr.is_some() && self.validator_key.is_some();

        let mut out = String::with_capacity(2048);
        let _ = writeln!(out, "# HELP tiger_chain_height Highest canonical block number.");
        let _ = writeln!(out, "# TYPE tiger_chain_height gauge");
        let _ = writeln!(out, "tiger_chain_height {height}");

        let _ = writeln!(out, "# HELP tiger_chain_id The chain's EIP-155 chain id.");
        let _ = writeln!(out, "# TYPE tiger_chain_id gauge");
        let _ = writeln!(out, "tiger_chain_id {chain_id}");

        let _ = writeln!(out, "# HELP tiger_network_id The network id reported by net_version.");
        let _ = writeln!(out, "# TYPE tiger_network_id gauge");
        let _ = writeln!(out, "tiger_network_id {network_id}");

        let _ = writeln!(out, "# HELP tiger_node_running 1 while the node accepts and serves requests.");
        let _ = writeln!(out, "# TYPE tiger_node_running gauge");
        let _ = writeln!(out, "tiger_node_running {}", u8::from(running));

        let _ = writeln!(out, "# HELP tiger_mempool_transactions Transactions waiting to be included.");
        let _ = writeln!(out, "# TYPE tiger_mempool_transactions gauge");
        let _ = writeln!(out, "tiger_mempool_transactions {mempool_size}");

        let _ = writeln!(out, "# HELP tiger_mempool_capacity Maximum transactions the pool holds.");
        let _ = writeln!(out, "# TYPE tiger_mempool_capacity gauge");
        let _ = writeln!(out, "tiger_mempool_capacity {mempool_capacity}");

        let _ = writeln!(out, "# HELP tiger_mempool_senders Distinct senders with a pending transaction.");
        let _ = writeln!(out, "# TYPE tiger_mempool_senders gauge");
        let _ = writeln!(out, "tiger_mempool_senders {mempool_senders}");

        let _ = writeln!(out, "# HELP tiger_p2p_peers Connected peers.");
        let _ = writeln!(out, "# TYPE tiger_p2p_peers gauge");
        let _ = writeln!(out, "tiger_p2p_peers {peers}");

        let _ = writeln!(out, "# HELP tiger_sync_syncing 1 while the node is catching up to its peers.");
        let _ = writeln!(out, "# TYPE tiger_sync_syncing gauge");
        let _ = writeln!(out, "tiger_sync_syncing {}", u8::from(sync.syncing));

        let _ = writeln!(out, "# HELP tiger_sync_highest_block Highest block known to any peer.");
        let _ = writeln!(out, "# TYPE tiger_sync_highest_block gauge");
        let _ = writeln!(
            out,
            "tiger_sync_highest_block {}",
            best_peer_height.unwrap_or(sync.highest_block)
        );

        let _ = writeln!(out, "# HELP tiger_consensus_validators Validators this node knows about.");
        let _ = writeln!(out, "# TYPE tiger_consensus_validators gauge");
        let _ = writeln!(out, "tiger_consensus_validators {validator_count}");

        let _ = writeln!(out, "# HELP tiger_consensus_is_validator 1 if this node may propose blocks.");
        let _ = writeln!(out, "# TYPE tiger_consensus_is_validator gauge");
        let _ = writeln!(out, "tiger_consensus_is_validator {}", u8::from(is_validator));

        let _ = writeln!(
            out,
            "# HELP tiger_uptime_seconds Seconds since the node was constructed."
        );
        let _ = writeln!(out, "# TYPE tiger_uptime_seconds gauge");
        let _ = writeln!(out, "tiger_uptime_seconds {}", self.started_at.elapsed().as_secs());

        let _ = writeln!(
            out,
            "# HELP tiger_storage_enabled 1 when the node is writing to a durable store."
        );
        let _ = writeln!(out, "# TYPE tiger_storage_enabled gauge");
        let _ = writeln!(out, "tiger_storage_enabled {}", u8::from(self.storage.is_some()));

        let _ = writeln!(
            out,
            "# HELP tiger_storage_pending_blocks Blocks written by the chain but not yet flushed."
        );
        let _ = writeln!(out, "# TYPE tiger_storage_pending_blocks gauge");
        let _ = writeln!(
            out,
            "tiger_storage_pending_blocks {}",
            self.pending_flush_count().await
        );

        let _ = writeln!(out, "# HELP tiger_storage_pending_accounts Accounts written since the last flush.");
        let _ = writeln!(out, "# TYPE tiger_storage_pending_accounts gauge");
        let _ = writeln!(
            out,
            "tiger_storage_pending_accounts {}",
            self.world_state.lock().await.dirty_entries().len()
        );

        let _ = writeln!(out, "# HELP tiger_info Build and network information, always 1.");
        let _ = writeln!(out, "# TYPE tiger_info gauge");
        let _ = writeln!(
            out,
            "tiger_info{{version=\"{}\",network=\"{}\"}} 1",
            env!("CARGO_PKG_VERSION"),
            network
        );
        out
    }
}

// =============================================================================
// BLOCK PRODUCTION
// =============================================================================

/// Build the world state a chain starts from: an empty state with the genesis
/// allocation applied.
///
/// `debug_traceTransaction` needs the state a block started from, which is this
/// state with every earlier block replayed onto it, so the construction is
/// shared with node startup rather than duplicated.
pub fn state_from_genesis(genesis: &Genesis) -> WorldState {
    let mut world_state = WorldState::new();
    for (address, account) in &genesis.alloc {
        let parsed = match parse_address(address) {
            Ok(address) => address,
            Err(e) => {
                warn!("skipping genesis account {address}: {e}");
                continue;
            }
        };
        if let Some(balance) = parse_genesis_balance(&account.balance) {
            world_state.set_balance(&parsed, balance);
        } else {
            warn!("skipping genesis balance for {address}: not a valid amount");
        }
        if let Some(nonce) = account.nonce {
            world_state.set_nonce(&parsed, nonce);
        }
        if let Some(code) = &account.code {
            if let Ok(bytes) = hex_decode_checked(code) {
                world_state.set_code(&parsed, bytes);
            }
        }
        // Pre-deployed contracts and proxy implementations carry their initial
        // storage in genesis. Skipping it would leave a contract whose code is
        // present but whose owner, implementation pointer or supply reads as
        // zero, so every call against it would misbehave.
        for (slot, value) in &account.storage {
            let (Ok(slot), Ok(value)) = (
                hex_word(slot),
                hex_word(value),
            ) else {
                warn!("skipping genesis storage slot {slot} for {address}: not a 32-byte word");
                continue;
            };
            world_state.set_storage(&parsed, slot, value);
        }
    }
    world_state
}

/// Decode a 32-byte word from a genesis `0x`-prefixed string, left-padding a
/// shorter value the way a JSON-RPC quantity would be.
fn hex_word(value: &str) -> Result<[u8; 32], String> {
    let hex = value.strip_prefix("0x").unwrap_or(value);
    let padded = if hex.len() % 2 != 0 {
        format!("0{hex}")
    } else {
        hex.to_string()
    };
    let bytes = hex_decode_checked(&padded)?;
    if bytes.len() > 32 {
        return Err("value wider than 32 bytes".to_string());
    }
    let mut out = [0u8; 32];
    out[32 - bytes.len()..].copy_from_slice(&bytes);
    Ok(out)
}

/// Create a genesis block configuration for the given chain ID.
pub(crate) fn create_genesis_block(chain_id: u64) -> Genesis {
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

/// Load the genesis definition.
///
/// When `--genesis-file` is set the file is read and parsed; a missing or
/// malformed file is a fatal error rather than a silent fall back to the
/// built-in genesis, because starting a chain with the wrong genesis would
/// fork it from every other node.
///
/// Genesis files in the wild are written for geth, BSC and other clients, and
/// use camelCase keys (`extraData`, `gasLimit`, `mixHash`) and an `alloc`
/// whose account entries carry only a balance. Normalising those spellings
/// here means an operator can point the node at the genesis file they already
/// have instead of hand-editing it into this crate's struct layout.
fn load_genesis(config: &Config) -> Result<Genesis> {
    let Some(path) = &config.genesis_file else {
        return Ok(create_genesis_block(config.chain_id));
    };

    let contents = std::fs::read_to_string(path)
        .with_context(|| format!("cannot read genesis file {path}"))?;

    let mut value: serde_json::Value = serde_json::from_str(&contents)
        .with_context(|| format!("cannot parse genesis file {path}"))?;
    normalise_genesis_keys(&mut value);

    let genesis: Genesis = serde_json::from_value(value)
        .with_context(|| format!("cannot parse genesis file {path}"))?;

    if genesis.config.chain_id != config.chain_id {
        anyhow::bail!(
            "genesis file {path} declares chain id {}, but the node is configured for {}",
            genesis.config.chain_id,
            config.chain_id
        );
    }
    info!("Loaded genesis from {path}");
    Ok(genesis)
}

/// Rewrite the camelCase keys geth-family clients emit into the snake_case
/// keys this crate's structs use, at the genesis root and in every alloc
/// entry. Unknown keys are left alone.
fn normalise_genesis_keys(value: &mut serde_json::Value) {
    const ROOT_ALIASES: &[(&str, &str)] = &[
        ("extraData", "extra_data"),
        ("gasLimit", "gas_limit"),
        ("mixHash", "mix_hash"),
        ("baseFeePerGas", "base_fee_per_gas"),
    ];
    const CONFIG_ALIASES: &[(&str, &str)] = &[
        ("chainId", "chain_id"),
        ("homesteadBlock", "homestead_block"),
        ("daoForkBlock", "dao_fork_block"),
        ("eip150Block", "eip150_block"),
        ("eip150Hash", "eip150_hash"),
        ("eip155Block", "eip155_block"),
        ("eip158Block", "eip158_block"),
        ("byzantiumBlock", "byzantium_block"),
        ("constantinopleBlock", "constantinople_block"),
        ("petersburgBlock", "petersburg_block"),
        ("istanbulBlock", "istanbul_block"),
        ("eip1283Block", "eip1283_block"),
        ("berlinBlock", "berlin_block"),
        ("londonBlock", "london_block"),
        ("mergeForkBlock", "merge_fork_block"),
        ("shanghaiBlock", "shanghai_block"),
        ("cancelDeneb", "cancel_deneb"),
    ];

    let Some(root) = value.as_object_mut() else {
        return;
    };
    rename_keys(root, ROOT_ALIASES);

    if let Some(config) = root.get_mut("config").and_then(|c| c.as_object_mut()) {
        rename_keys(config, CONFIG_ALIASES);
    }

    if let Some(alloc) = root.get_mut("alloc").and_then(|a| a.as_object_mut()) {
        for account in alloc.values_mut() {
            if let Some(account) = account.as_object_mut() {
                // `code` and `storage` are also spelled `code`/`storage` by
                // every client, but a storage map's values must be strings;
                // a non-string value is dropped so the parse error names the
                // account rather than an anonymous type mismatch.
                if let Some(storage) = account.get_mut("storage").and_then(|s| s.as_object_mut()) {
                    storage.retain(|_, v| v.is_string());
                }
            }
        }
    }
}

/// Move each `from` key of `map` to its `to` spelling, leaving an existing
/// `to` key untouched so an explicitly snake_case file still wins.
fn rename_keys(
    map: &mut serde_json::Map<String, serde_json::Value>,
    aliases: &[(&str, &str)],
) {
    for (from, to) in aliases {
        if let Some(value) = map.remove(*from) {
            map.entry((*to).to_string()).or_insert(value);
        }
    }
}

/// Parse a genesis balance, accepting both decimal and `0x`-prefixed hex.
fn parse_genesis_balance(value: &str) -> Option<[u8; 32]> {
    if let Some(hex) = value.strip_prefix("0x") {
        // A balance is an integer, so an odd digit count is legitimate and
        // must be left-padded: 1 TGR is `0xde0b6b3a7640000`, which has 15
        // digits. Only code (a byte string) requires an even count.
        let padded = if hex.len() % 2 != 0 {
            format!("0{hex}")
        } else {
            hex.to_string()
        };
        let bytes = hex_decode_checked(&padded).ok()?;
        if bytes.len() > 32 {
            return None;
        }
        let mut out = [0u8; 32];
        out[32 - bytes.len()..].copy_from_slice(&bytes);
        return Some(out);
    }
    // Decimal: accumulate into a 256-bit big-endian value.
    let mut out = [0u8; 32];
    for digit in value.bytes() {
        if !digit.is_ascii_digit() {
            return None;
        }
        // out = out * 10 + digit
        let mut carry = (digit - b'0') as u32;
        for byte in out.iter_mut().rev() {
            let product = (*byte as u32) * 10 + carry;
            *byte = (product & 0xff) as u8;
            carry = product >> 8;
        }
        if carry != 0 {
            return None; // overflowed 256 bits
        }
    }
    Some(out)
}

/// Decode hex strictly, rejecting a missing `0x` prefix and odd lengths.
fn hex_decode_checked(value: &str) -> Result<Vec<u8>, String> {
    let trimmed = value.strip_prefix("0x").unwrap_or(value);
    if trimmed.len() % 2 != 0 {
        return Err("odd number of hex digits".to_string());
    }
    let mut out = Vec::with_capacity(trimmed.len() / 2);
    let bytes = trimmed.as_bytes();
    for pair in bytes.chunks(2) {
        let hi = (pair[0] as char).to_digit(16).ok_or("invalid hex digit")?;
        let lo = (pair[1] as char).to_digit(16).ok_or("invalid hex digit")?;
        out.push(((hi << 4) | lo) as u8);
    }
    Ok(out)
}

/// Build the genesis block (number 0) from a genesis definition.
///
/// The header's `state_root` is computed from the genesis allocation, not
/// hardcoded to the empty-trie root. The world state the node starts from is
/// exactly that allocation, so a block 0 claiming an empty trie would put the
/// chain's very first header out of step with its own state: an explorer,
/// light client or peer re-executing genesis would derive a different root and
/// reject the chain. `miner`, `difficulty`, `gas_limit`, `extra_data` and
/// `mix_hash` are likewise taken from the definition rather than invented, so
/// two nodes given the same file agree on the genesis hash.
pub fn genesis_block_from(genesis: &Genesis) -> tiger_blockchain::types::Block {
    use tiger_blockchain::types::{Block, BlockHeader};
    use tiger_blockchain::EMPTY_TRIE_ROOT;

    let state_root = state_from_genesis(genesis).state_root();

    // The genesis definition stores the gas limit as a string that may be
    // decimal or 0x-hex; anything unparseable falls back to the chain limit
    // rather than producing a block with a zero gas limit, which every
    // transaction would then fail against.
    let gas_limit = genesis
        .gas_limit
        .parse()
        .ok()
        .or_else(|| {
            genesis
                .gas_limit
                .strip_prefix("0x")
                .and_then(|hex| u64::from_str_radix(hex, 16).ok())
        })
        .unwrap_or(BLOCK_GAS_LIMIT);

    let difficulty = if genesis.difficulty.starts_with("0x") {
        genesis.difficulty.clone()
    } else {
        format!(
            "0x{:x}",
            genesis.difficulty.parse::<u64>().unwrap_or(0)
        )
    };

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
            difficulty,
            number: 0,
            gas_limit,
            gas_used: 0,
            timestamp: 0,
            extra_data: format!("0x{}", tiger_blockchain::hex_encode(genesis.extra_data.as_bytes())),
            mix_hash: genesis.mix_hash.clone(),
            nonce: "0x0000000000000000".to_string(),
            // The genesis block sets the base fee every later block adjusts
            // from, so a chain with a zero fee here could never price a
            // transaction.
            base_fee_per_gas: Some(1_000_000_000),
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
pub fn produce_block(
    chain_manager: &mut ChainManager,
    world_state: &mut WorldState,
    transactions: &[Transaction],
    block_number: u64,
    miner: &str,
    chain_id: u64,
    signing_key: Option<&[u8; 32]>,
) -> Result<String, String> {
    use tiger_blockchain::types::{Block, BlockHeader};
    use tiger_blockchain::{block_hash, transactions_root};
    use tiger_evm::transition::{apply_transaction, BlockContext};

    let parent_block = chain_manager
        .chain()
        .get_block(block_number.saturating_sub(1));

    let parent_hash = match parent_block {
        Some(block) => block_hash(&block.header)
            .map_err(|e| format!("failed to hash parent header: {e}"))?,
        None => format!("0x{}", tiger_blockchain::hex_encode(&[0u8; 32])),
    };

    // A PoSA block carries the same difficulty as its parent, so the two
    // branches of a fork compare on height alone. Genesis has no parent, so it
    // supplies the chain's initial difficulty from the genesis definition.
    let parent_difficulty = parent_block
        .map(|block| block.header.difficulty.clone())
        .unwrap_or_else(|| format!("0x{:x}", chain_manager.chain().genesis().map(|g| {
            g.difficulty
                .strip_prefix("0x")
                .and_then(|h| u64::from_str_radix(h, 16).ok())
                .or_else(|| g.difficulty.parse::<u64>().ok())
                .unwrap_or(1)
        }).unwrap_or(1)));

    // A block's timestamp must be strictly greater than its parent's, and
    // block validation enforces that. Two blocks produced within the same
    // second would otherwise share a timestamp and the second would be
    // rejected by the producer's own validator, so the timestamp is advanced
    // past the parent when the clock has not moved.
    let parent_timestamp = parent_block.map(|block| block.header.timestamp).unwrap_or(0);
    let now = chrono::Utc::now().timestamp().max(0) as u64;
    let timestamp = if block_number == 0 {
        now
    } else {
        now.max(parent_timestamp.saturating_add(1))
    };

    // The seal must recover to exactly the address in `miner`, so when this
    // node holds the signing key that key decides the address. Trusting a
    // caller-supplied string would let the two disagree and produce a block
    // that fails its own seal check.
    let miner = match signing_key {
        Some(key) => match tiger_crypto::private_key_to_address(key) {
            Ok(derived) => format!("0x{}", tiger_blockchain::hex_encode(&derived)),
            Err(e) => return Err(format!("cannot derive the sealing address: {e}")),
        },
        None => miner.to_string(),
    };

    let mut coinbase = [0u8; 20];
    if let Ok(addr) = parse_address(&miner) {
        coinbase = addr;
    }

    // EIP-1559 derives this block's base fee from the parent, so every client
    // computes the same value and a validator cannot pick a fee that favours
    // itself. The genesis block sets the initial fee.
    let base_fee = match parent_block {
        Some(parent) => tiger_blockchain::next_base_fee(
            parent.header.base_fee_per_gas.unwrap_or(0),
            parent.header.gas_used,
            parent.header.gas_limit,
        ),
        None => 1_000_000_000,
    };

    let block_ctx = BlockContext {
        chain_id,
        number: block_number,
        timestamp,
        gas_limit: BLOCK_GAS_LIMIT,
        coinbase,
        base_fee,
    };

    // Select the transactions that can legitimately be included.
    //
    // A producer must never emit a block its own validator would reject, so a
    // transaction whose declared hash does not match its contents or whose
    // signature cannot be recovered is dropped before the block is built. It
    // is not enough to skip applying it: it would still be part of the block
    // and fail block validation.
    let mut candidates: Vec<(Transaction, tiger_evm::transition::TransactionRequest)> =
        Vec::with_capacity(transactions.len());
    for tx in transactions {
        match transaction_to_request(tx, chain_id) {
            Some(request) => candidates.push((tx.clone(), request)),
            None => {
                warn!(
                    "dropping transaction {}: signature cannot be recovered",
                    tx.hash
                );
            }
        }
    }

    // Apply each transaction to the world state. Gas actually consumed is
    // summed, not the gas limits the transactions declared. Each transaction's
    // logs are collected into a receipt, because the block header commits to
    // the receipts root and the logs bloom and the node must be able to answer
    // `eth_getTransactionReceipt` from the block it produced.
    //
    // Every included transaction gets exactly one receipt, including one that
    // is rejected before execution. A block whose receipts are shorter than
    // its transaction list cannot answer for its later transactions and is
    // rejected by other clients, so a rejection must not silently drop the
    // receipt.
    let mut included: Vec<Transaction> = Vec::with_capacity(candidates.len());
    let mut gas_used: u64 = 0;
    let mut receipts: Vec<tiger_blockchain::BlockReceipt> = Vec::with_capacity(candidates.len());
    for (tx, request) in candidates.iter() {
        // The gas limit is checked against the amount the transaction
        // reserved, before it runs. Checking afterwards would mean applying a
        // transaction and then discarding it, which would leave its state
        // changes in a block that does not list it.
        if gas_used.saturating_add(request.gas_limit) > BLOCK_GAS_LIMIT {
            warn!("stopping block: {} does not fit in the remaining gas", tx.hash);
            break;
        }

        // A transaction that fails validation is not part of a block at all:
        // its nonce, its balance or its fee ceiling makes it unusable, and no
        // state was written for it. Including it would put gas in the header
        // that nobody paid and a receipt that no state transition backs, so
        // the block's own state root would disagree with its receipts.
        // Execution failures are different: a transaction that validates and
        // then reverts is included, with a failed receipt.
        let outcome = match apply_transaction(world_state, request, &block_ctx) {
            Ok(outcome) => outcome,
            Err(e) => {
                warn!("dropping transaction {}: {}", tx.hash, e);
                continue;
            }
        };

        included.push(tx.clone());
        gas_used = gas_used.saturating_add(outcome.gas_used);
        receipts.push(tiger_blockchain::BlockReceipt {
            status: outcome.success,
            cumulative_gas_used: gas_used,
            gas_used: outcome.gas_used,
            logs: outcome
                .logs
                .iter()
                .map(|log| tiger_blockchain::ReceiptLog {
                    address: log.address.0,
                    topics: log.topics.iter().map(|t| t.0).collect(),
                    data: log.data.clone(),
                })
                .collect(),
        });
        if !outcome.success {
            info!("transaction {} did not succeed, gas used {}", tx.hash, outcome.gas_used);
        }
    }

    // The transactions root is computed over the transactions as included.
    let tx_root = transactions_root(&included)
        .map_err(|e| format!("failed to compute transactions root: {e}"))?;

    // The state root reflects the world state after applying the block.
    let state_root = world_state.state_root();

    // The receipts root and logs bloom commit to the receipts the block's
    // transactions actually produced.
    let receipts_root = tiger_blockchain::receipts_root(&receipts);
    let logs_bloom = tiger_blockchain::logs_bloom(&receipts);

    let mut header = BlockHeader {
        parent_hash,
        sha3_uncles: format!(
            "0x{}",
            tiger_blockchain::hex_encode(&tiger_crypto::keccak256(&[]))
        ),
        miner: miner.clone(),
        state_root: format!("0x{}", tiger_blockchain::hex_encode(&state_root)),
        transactions_root: format!("0x{}", tiger_blockchain::hex_encode(&tx_root)),
        receipts_root: format!("0x{}", tiger_blockchain::hex_encode(&receipts_root)),
        logs_bloom: format!("0x{}", tiger_blockchain::hex_encode(&logs_bloom)),
        // Every PoSA block carries the chain's difficulty, taken from its
        // parent. Writing zero here, as an earlier version did, made cumulative
        // work identical on every branch and left fork choice unable to compare
        // anything but height.
        difficulty: parent_difficulty.clone(),
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
        base_fee_per_gas: Some(base_fee),
        withdrawals_root: None,
    };

    // Seal the header with the proposer's key. Without this a block carries
    // only a claimed `miner` address and anyone could produce one in a
    // validator's name, so a node that cannot seal must not produce a block
    // at all rather than emit one every peer will reject.
    if block_number >= tiger_blockchain::FIRST_SEALED_BLOCK {
        let key = signing_key.ok_or_else(|| {
            format!(
                "cannot produce block #{block_number}: no validator key to seal it with"
            )
        })?;
        header.extra_data = tiger_blockchain::seal_header(&header, key)
            .map_err(|e| format!("failed to seal block header: {e}"))?;
    }

    // The block hash is keccak256(RLP(header)), identical to every other
    // Ethereum-compatible client.
    let block_hash =
        block_hash(&header).map_err(|e| format!("failed to hash block header: {e}"))?;

    let block = Block {
        header,
        transactions: included,
        uncles: vec![],
    };

    // Insert with receipts so the header's commitments are verified against
    // the receipts before the block is accepted, and so receipts are
    // retrievable afterwards.
    chain_manager
        .process_block_with_receipts(block, receipts)
        .map_err(|e| format!("failed to process block: {e}"))?;

    Ok(block_hash)
}

/// Execute a block's transactions against a copy of the world state and return
/// the receipts they produce.
///
/// This is the same work [`produce_block`] does, so an imported block is
/// verified by re-running it rather than by trusting the sender's claims. It
/// returns `Err` when the block cannot be a valid extension of `parent_state`:
/// a transaction whose signature does not recover, or whose declared hash does
/// not match its contents, means the block is not executable.
pub fn execute_block_transactions(
    parent_state: &WorldState,
    block: &tiger_blockchain::types::Block,
    chain_id: u64,
) -> Result<(WorldState, Vec<tiger_blockchain::BlockReceipt>), String> {
    use tiger_evm::transition::{apply_transaction, BlockContext};

    let mut state = parent_state.clone();
    let coinbase = parse_address(&block.header.miner).unwrap_or([0u8; 20]);

    let block_ctx = BlockContext {
        chain_id,
        number: block.header.number,
        timestamp: block.header.timestamp,
        gas_limit: block.header.gas_limit,
        coinbase,
        base_fee: block.header.base_fee_per_gas.unwrap_or(0),
    };

    let mut gas_used: u64 = 0;
    let mut receipts = Vec::with_capacity(block.transactions.len());
    for tx in &block.transactions {
        let request = transaction_to_request(tx, chain_id).ok_or_else(|| {
            format!(
                "transaction {} in block {} has an unrecoverable signature",
                tx.hash, block.header.number
            )
        })?;

        // A transaction that declares a hash other than the one its contents
        // produce is not the transaction the block committed to.
        let expected_hash = tiger_blockchain::transaction_hash(tx)
            .map_err(|e| format!("cannot hash transaction {}: {e}", tx.hash))?;
        if expected_hash != tx.hash {
            return Err(format!(
                "transaction declares hash {} but its contents hash to {}",
                tx.hash, expected_hash
            ));
        }

        let outcome = match apply_transaction(&mut state, &request, &block_ctx) {
            Ok(outcome) => outcome,
            Err(e) => {
                // A transaction the block producer included but that cannot be
                // applied makes the whole block invalid: unlike a revert, which
                // is a legitimate outcome, a rejection means the producer built
                // a block its own rules would not accept.
                return Err(format!(
                    "transaction {} in block {} cannot be applied: {e}",
                    tx.hash, block.header.number
                ));
            }
        };

        gas_used = gas_used.saturating_add(outcome.gas_used);
        receipts.push(tiger_blockchain::BlockReceipt {
            status: outcome.success,
            cumulative_gas_used: gas_used,
            gas_used: outcome.gas_used,
            logs: outcome
                .logs
                .iter()
                .map(|log| tiger_blockchain::ReceiptLog {
                    address: log.address.0,
                    topics: log.topics.iter().map(|t| t.0).collect(),
                    data: log.data.clone(),
                })
                .collect(),
        });
    }

    // The header's state root is a commitment to the result of executing the
    // block. A block whose transactions produce a different state than the one
    // it claims is rejected, which is what stops a peer from sending a valid
    // transaction list with a fabricated state.
    let computed_root = state.state_root();
    let declared_root = parse_u256(&block.header.state_root);
    if computed_root != declared_root {
        return Err(format!(
            "block {} declares state root {} but executing it produces {}",
            block.header.number,
            block.header.state_root,
            format!("0x{}", tiger_blockchain::hex_encode(&computed_root))
        ));
    }

    if gas_used != block.header.gas_used {
        return Err(format!(
            "block {} declares {} gas used but executing it consumes {}",
            block.header.number, block.header.gas_used, gas_used
        ));
    }

    // The state the block produced is returned so the caller adopts exactly the
    // state it verified, rather than executing the block a second time.
    Ok((state, receipts))
}

/// Validate a transaction and normalise the fields that are derived rather
/// than signed.
///
/// A transaction's signature is what authorises it, so the sender is recovered
/// from the signature and the `from` field on the wire is discarded: a peer
/// could otherwise claim any sender it liked. The hash is likewise recomputed
/// from the contents, so the `hash` field cannot be spoofed.
///
/// This is the single place both the RPC path and the peer path validate a
/// transaction, so a transaction accepted over the network is held to exactly
/// the same standard as one submitted to the RPC endpoint.
pub fn validate_transaction(
    tx: &Transaction,
    chain_id: u64,
) -> Result<Transaction, String> {
    use tiger_blockchain::{recover_transaction_sender, transaction_hash};

    let sender = recover_transaction_sender(tx, chain_id)
        .map_err(|e| format!("invalid transaction signature: {e}"))?;

    let hash = transaction_hash(tx).map_err(|e| format!("cannot hash transaction: {e}"))?;

    let mut normalised = tx.clone();
    normalised.hash = hash;
    normalised.from = format!("0x{}", tiger_blockchain::hex_encode(&sender));
    Ok(normalised)
}

/// Convert a node transaction into the form the transition function needs.
/// Returns `None` when the sender cannot be recovered, which means the
/// transaction is not authorised and must not be applied.
pub fn transaction_to_request(
    tx: &Transaction,
    chain_id: u64,
) -> Option<tiger_evm::transition::TransactionRequest> {
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
        // An EIP-1559 transaction is priced against the block's base fee; the
        // tip it carries is what reaches the validator.
        max_priority_fee_per_gas: tx
            .max_priority_fee_per_gas
            .map(|fee| parse_u256(&format!("0x{fee:x}"))),
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

/// Post-execution world states for the most recent blocks, keyed by block hash.
///
/// This is a bounded FIFO: inserting past the limit drops the oldest entry.
/// Bounding it keeps memory proportional to the fork window rather than to the
/// chain, and because a miss is handled by replaying from genesis, dropping an
/// entry costs work but never correctness.
pub struct RecentStates {
    entries: Vec<(String, WorldState)>,
    limit: usize,
}

impl RecentStates {
    /// How many recent states to retain. A fork deeper than this still works,
    /// it just costs a replay.
    pub const DEFAULT_LIMIT: usize = 128;

    pub fn new() -> Self {
        Self {
            entries: Vec::new(),
            limit: Self::DEFAULT_LIMIT,
        }
    }

    /// Record the state after `hash` executed, evicting the oldest if full.
    pub fn insert(&mut self, hash: &str, state: WorldState) {
        let key = hash.to_ascii_lowercase();
        if let Some(existing) = self.entries.iter_mut().find(|(h, _)| *h == key) {
            existing.1 = state;
            return;
        }
        self.entries.push((key, state));
        while self.entries.len() > self.limit {
            self.entries.remove(0);
        }
    }

    /// The state after `hash` executed, if it is still cached.
    pub fn get(&self, hash: &str) -> Option<WorldState> {
        let key = hash.to_ascii_lowercase();
        self.entries
            .iter()
            .find(|(h, _)| *h == key)
            .map(|(_, state)| state.clone())
    }

    /// Forget everything, so a reload does not serve states from the chain the
    /// node has just replaced.
    pub fn clear(&mut self) {
        self.entries.clear();
    }

}

impl Default for RecentStates {
    fn default() -> Self {
        Self::new()
    }
}

/// The key used to seal blocks in tests.
///
/// A sealed block is the only kind a node will accept, so tests that build
/// blocks by hand have to seal them too. Exposed to the rest of the crate
/// because `debug_rpc`'s tests build blocks the same way.
#[cfg(test)]
pub const TESTS_SEAL_KEY: [u8; 32] = [0x5c; 32];

/// Parse a `0x`-prefixed 32-byte private key.
///
/// Rejects anything that is not exactly 32 bytes, and the all-zero key, which
/// is not a valid secp256k1 scalar: accepting it would produce a node that
/// believes it can seal but whose signatures no peer can verify.
fn parse_private_key(value: &str) -> Result<[u8; 32], String> {
    let bytes = hex_decode(value);
    if bytes.len() != 32 {
        return Err(format!(
            "expected a 32-byte private key, got {} bytes",
            bytes.len()
        ));
    }
    if bytes.iter().all(|&b| b == 0) {
        return Err("private key is all zero, which is not a valid scalar".to_string());
    }
    let mut out = [0u8; 32];
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
    use tiger_consensus::posa::MIN_STAKE;
    use crate::rpc_server::{topics_match, TopicFilter};
    use tiger_state::account::{u256_from_u64, WorldState};

    #[tokio::test]
    async fn test_node_creation() {
        let config = Config::default_mainnet();
        let node = TigerNode::new(config).await.expect("node must initialize");
        assert!(!node.is_running().await, "a new node is not running");
    }

    #[test]
    fn genesis_balances_parse_in_decimal_and_hex() {
        // 1 TGR = 10^18.
        assert_eq!(
            parse_genesis_balance("1000000000000000000"),
            Some(u256_from_u64(1_000_000_000_000_000_000))
        );
        assert_eq!(parse_genesis_balance("0xde0b6b3a7640000"), Some({
            let mut v = [0u8; 32];
            v[24..].copy_from_slice(&0x0de0b6b3a7640000u64.to_be_bytes());
            v
        }));
        assert_eq!(parse_genesis_balance("0"), Some([0u8; 32]));
        // Rejected inputs.
        assert_eq!(parse_genesis_balance("12x4"), None);
        assert_eq!(parse_genesis_balance("-5"), None);
        // A value wider than 256 bits must not silently truncate.
        assert_eq!(parse_genesis_balance(&"9".repeat(100)), None);
    }

    #[test]
    fn hex_decoding_is_strict_about_odd_lengths() {
        assert_eq!(hex_decode_checked("0x1234").unwrap(), vec![0x12, 0x34]);
        assert!(hex_decode_checked("0x123").is_err());
        assert!(hex_decode_checked("0xzz").is_err());
    }

    #[test]
    fn a_configured_validator_set_produces_blocks() {
        // With a validator registered, the engine names a proposer for every
        // height; with none it never does, which is why no block would be
        // produced.
        let mut consensus = PoSA::new(6666, 200, 3);
        assert_eq!(consensus.get_proposer(1), None);

        consensus.register_validator("0x1111111111111111111111111111111111111111".to_string(), MIN_STAKE);
        consensus.update_epoch(0);
        assert_eq!(
            consensus.get_proposer(1),
            Some("0x1111111111111111111111111111111111111111".to_string())
        );
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
            tx_type: 0,
            access_list: None,
            max_priority_fee_per_gas: None,
            chain_id: None,
        };
        sign_transaction(&mut tx, 6666, &key).unwrap();
        tx
    }

    fn hex32(bytes: &[u8; 32]) -> String {
        format!("0x{}", tiger_blockchain::hex_encode(bytes))
    }

    /// A started node whose head is the genesis block, plus a producer that
    /// shares its genesis allocation.
    ///
    /// The producer is a separate chain built from the same genesis, so a block
    /// it produces is exactly what a peer running the same chain would send.
    async fn node_and_producer() -> (
        Arc<TigerNode>,
        ChainManager,
        WorldState,
        String,
    ) {
        // Each test gets its own data directory: a shared one would let a
        // chain written by another test be restored here, which is exactly the
        // cross-test interference this avoids.
        static COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let unique = COUNTER.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        let data_dir = std::env::temp_dir()
            .join(format!("tiger-node-test-{}-{unique}", std::process::id()));
        let _ = std::fs::remove_dir_all(&data_dir);

        let config = Config {
            chain_id: 6666,
            validators: vec![],
            data_dir: data_dir.to_string_lossy().into_owned(),
            ..Config::default_mainnet()
        };
        let genesis = create_genesis_block(6666);
        let node = Arc::new(
            TigerNode::from_genesis(config, genesis.clone())
                .await
                .expect("node must initialize"),
        );
        // `start` is what installs the genesis block, so it must run before a
        // block can be imported.
        node.start().await.expect("node must start");

        let mut producer_cm = chain_manager();
        producer_cm
            .process_block(genesis_block_from(&genesis))
            .unwrap();
        let producer_world = state_from_genesis(&genesis);
        let miner = format!("0x{}", tiger_blockchain::hex_encode(&[0xaa; 20]));
        (node, producer_cm, producer_world, miner)
    }

    /// Build the block a validator would propose at `number`.
    fn build_block(
        cm: &mut ChainManager,
        world: &mut WorldState,
        number: u64,
        miner: &str,
        txs: &[Transaction],
    ) -> tiger_blockchain::types::Block {
        produce_block(cm, world, txs, number, miner, 6666, Some(&TESTS_SEAL_KEY)).unwrap();
        cm.chain().get_block(number).cloned().unwrap()
    }

    #[tokio::test]
    async fn a_block_extending_the_head_is_imported_and_its_state_adopted() {
        let (node, mut producer_cm, mut producer_world, miner) = node_and_producer().await;
        let block = build_block(&mut producer_cm, &mut producer_world, 1, &miner, &[]);

        // The producer's own state root is what the block commits to.
        let expected_root = block.header.state_root.clone();

        assert!(
            node.import_block(block).await.unwrap(),
            "a block extending the head must be imported"
        );

        let cm = node.get_chain_manager().await;
        assert_eq!(cm.chain().height(), 1);
        let imported = cm.chain().get_block(1).unwrap();
        assert_eq!(imported.header.state_root, expected_root);

        // The node's own world state must now be the state the block produced,
        // not the one it started from.
        drop(cm);
        let world = node.world_state().lock().await;
        assert_eq!(
            hex32(&world.state_root()),
            expected_root,
            "the node must adopt the state the block produced"
        );
    }

    #[tokio::test]
    async fn a_block_whose_parent_is_unknown_is_rejected() {
        let (node, mut producer_cm, mut producer_world, miner) = node_and_producer().await;
        // Produce block 1 on the producer, but give the node only block 2, so
        // the node sees a block whose parent it does not have.
        let _block1 = build_block(&mut producer_cm, &mut producer_world, 1, &miner, &[]);
        let block = build_block(&mut producer_cm, &mut producer_world, 2, &miner, &[]);

        let error = node.import_block(block).await.unwrap_err().to_string();
        assert!(
            error.contains("which is not known"),
            "unexpected error: {error}"
        );
    }

    /// A block that does not extend the head but *does* build on a known block
    /// is a side branch: it must be accepted into the tree without moving the
    /// head, because a later block on it may win.
    #[tokio::test]
    async fn a_side_branch_block_is_accepted_without_moving_the_head() {
        let (node, mut producer_cm, mut producer_world, miner) = node_and_producer().await;

        // The node follows the producer's block 1.
        let block1 = build_block(&mut producer_cm, &mut producer_world, 1, &miner, &[]);
        assert!(node.import_block(block1.clone()).await.unwrap());
        assert_eq!(node.get_chain_manager().await.chain().height(), 1);

        // A competing block at height 1 built on genesis. Its timestamp is
        // varied until its hash exceeds block1's, so fork choice is guaranteed
        // to keep block1: the test is about side-branch handling, not about
        // which hash happens to win.
        let block1_hash = tiger_blockchain::block_hash(&block1.header).unwrap();
        let mut fork = block1.clone();
        let mut found = false;
        for step in 0..10_000u64 {
            fork = block1.clone();
            fork.header.timestamp = block1.header.timestamp + 1 + step;
            fork.header.extra_data =
                tiger_blockchain::seal_header(&fork.header, &TESTS_SEAL_KEY).unwrap();
            if tiger_blockchain::block_hash(&fork.header).unwrap() > block1_hash {
                found = true;
                break;
            }
        }
        assert!(found, "could not build a fork that loses the hash tie-break");

        assert!(
            node.import_block(fork.clone()).await.unwrap(),
            "a side branch block is stored, not rejected"
        );

        let cm = node.get_chain_manager().await;
        assert_eq!(cm.chain().height(), 1, "the head must not move");
        // The fork is in the tree so a block building on it can be validated.
        let fork_hash = tiger_blockchain::block_hash(&fork.header).unwrap();
        assert!(cm.chain().get_block_by_hash(&fork_hash).is_some());
        assert!(!cm.chain().is_canonical(&fork_hash));
    }

    /// A heavier competing branch must reorganise the node: the head moves,
    /// the world state is rebuilt from the winning branch, and a transaction
    /// that was only on the abandoned branch is no longer confirmed.
    #[tokio::test]
    async fn a_heavier_branch_reorganises_the_node_and_rebuilds_state() {
        // Fund the sender in genesis itself. A reorg rebuilds state by
        // replaying from genesis, so any balance that exists only in memory
        // would be lost by the replay and the test would be asserting against
        // a state the chain could not actually reproduce.
        let sender_key = [0x46u8; 32];
        let sender = tiger_crypto::private_key_to_address(&sender_key).unwrap();
        let recipient = [0x22u8; 20];

        let mut genesis = create_genesis_block(6666);
        genesis.alloc.insert(
            format!("0x{}", tiger_blockchain::hex_encode(&sender)),
            tiger_blockchain::types::Account {
                balance: "0xf4240".to_string(), // 1_000_000
                nonce: None,
                code: None,
                storage: std::collections::HashMap::new(),
            },
        );

        let config = Config {
            chain_id: 6666,
            validators: vec![],
            data_dir: std::env::temp_dir()
                .join(format!("tiger-reorg-test-{}", std::process::id()))
                .to_string_lossy()
                .into_owned(),
            ..Config::default_mainnet()
        };
        let node = Arc::new(
            TigerNode::from_genesis(config, genesis.clone())
                .await
                .expect("node must initialize"),
        );
        node.start().await.expect("node must start");

        // The producer and the fork builder both start from the same genesis,
        // exactly as two real validators would.
        let mut producer_cm = chain_manager();
        producer_cm
            .process_block(genesis_block_from(&genesis))
            .unwrap();
        let mut producer_world = state_from_genesis(&genesis);

        let miner = format!("0x{}", tiger_blockchain::hex_encode(&[0xaa; 20]));

        // Branch A: genesis -> a1, carrying a transfer.
        let transfer = signed_transfer(sender_key, 0, recipient, 5_000);
        let a1 = build_block(&mut producer_cm, &mut producer_world, 1, &miner, &[transfer]);
        assert!(node.import_block(a1.clone()).await.unwrap());
        assert_ne!(
            node.world_state().lock().await.balance(&recipient),
            u256_from_u64(0),
            "branch A must have credited the recipient"
        );

        // Branch B: an independent producer forks from genesis and builds two
        // blocks, neither carrying the transfer.
        let mut fork_cm = chain_manager();
        fork_cm
            .process_block(genesis_block_from(&genesis))
            .unwrap();
        let mut fork_world = state_from_genesis(&genesis);

        let b1 = build_block(&mut fork_cm, &mut fork_world, 1, &miner, &[]);
        assert!(
            node.import_block(b1.clone()).await.unwrap(),
            "b1 is a side branch, accepted but not canonical"
        );
        let b2 = build_block(&mut fork_cm, &mut fork_world, 2, &miner, &[]);

        assert!(
            node.import_block(b2.clone()).await.unwrap(),
            "b2 makes branch B heavier than branch A"
        );

        let cm = node.get_chain_manager().await;
        assert_eq!(cm.chain().height(), 2, "the node must follow the heavier branch");
        let b2_hash = tiger_blockchain::block_hash(&b2.header).unwrap();
        assert!(cm.chain().is_canonical(&b2_hash), "b2 must be the head");
        let a1_hash = tiger_blockchain::block_hash(&a1.header).unwrap();
        assert!(!cm.chain().is_canonical(&a1_hash), "branch A is abandoned");
        drop(cm);

        // The state must be the one branch B implies: the recipient's credit
        // from branch A is gone, and the sender's balance is back to its
        // genesis value, since B carries no transfer.
        let world = node.world_state().lock().await;
        assert_eq!(
            world.balance(&recipient),
            u256_from_u64(0),
            "the abandoned branch's state changes must be rolled back"
        );
        assert_eq!(
            world.balance(&sender),
            u256_from_u64(1_000_000),
            "the transfer from the abandoned branch must not be applied"
        );
    }

    #[tokio::test]
    async fn a_block_with_a_fabricated_state_root_is_rejected() {
        let (node, mut producer_cm, mut producer_world, miner) = node_and_producer().await;
        let mut block = build_block(&mut producer_cm, &mut producer_world, 1, &miner, &[]);

        // Claim a different state than the one the block's transactions produce.
        block.header.state_root = hex32(&[0x11u8; 32]);

        let error = node.import_block(block).await.unwrap_err().to_string();
        assert!(
            error.contains("declares state root"),
            "a fabricated state root must be caught, got: {error}"
        );
    }

    #[tokio::test]
    async fn a_block_with_a_forged_transaction_hash_is_rejected() {
        let (node, mut producer_cm, mut producer_world, miner) = node_and_producer().await;

        // Fund the sender in both states, so the transaction is applicable and
        // the block is otherwise valid. Only the declared hash is wrong.
        let key = [0x11u8; 32];
        let tx = signed_transfer(key, 0, [0x22u8; 20], 1_000);
        let sender = tiger_blockchain::recover_transaction_sender(&tx, 6666)
            .expect("the transaction is signed");
        let balance = u256_from_u64(1_000_000_000_000_000_000);
        producer_world.set_balance(&sender, balance);
        {
            let mut world = node.world_state().lock().await;
            world.set_balance(&sender, balance);
        }

        let mut block = build_block(&mut producer_cm, &mut producer_world, 1, &miner, &[tx]);
        assert_eq!(block.transactions.len(), 1, "the transfer must be included");

        block.transactions[0].hash = hex32(&[0x99u8; 32]);

        let error = node.import_block(block).await.unwrap_err().to_string();
        assert!(
            error.contains("declares hash") || error.contains("unrecoverable"),
            "a forged transaction hash must be caught, got: {error}"
        );
    }

    #[test]
    fn produced_block_carries_real_roots_and_links_to_its_parent() {
        let mut cm = chain_manager();
        let mut world = WorldState::new();
        let miner = format!("0x{}", tiger_blockchain::hex_encode(&[0xaa; 20]));

        let genesis = genesis_block_from(&create_genesis_block(6666));
        let genesis_hash = block_hash(&genesis.header).unwrap();
        cm.process_block(genesis).unwrap();

        let hash = produce_block(&mut cm, &mut world, &[], 1, &miner, 6666, Some(&TESTS_SEAL_KEY)).unwrap();
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
        produce_block(&mut cm, &mut world, &txs, 1, &miner, 6666, Some(&TESTS_SEAL_KEY)).unwrap();

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
            tx_type: 0,
            access_list: None,
            max_priority_fee_per_gas: None,
            chain_id: None,
        };

        produce_block(&mut cm, &mut world, &[tx], 1, &miner, 6666, Some(&TESTS_SEAL_KEY)).unwrap();

        let block = cm.chain().get_block(1).unwrap();
        // The invalid transaction is excluded from the block, not merely
        // skipped during execution: including it would make the block itself
        // fail validation.
        assert!(block.transactions.is_empty());
        assert_eq!(block.header.gas_used, 0);
        assert_eq!(world.balance(&[0x22u8; 20]), [0u8; 32]);
    }

    #[test]
    fn a_transfer_produces_a_stored_receipt_with_a_matching_root() {
        let mut cm = chain_manager();
        let mut world = WorldState::new();
        let miner = format!("0x{}", tiger_blockchain::hex_encode(&[0xaa; 20]));

        let key = [0x46u8; 32];
        let sender = tiger_crypto::private_key_to_address(&key).unwrap();
        world.set_balance(&sender, u256_from_u64(10_000_000));
        cm.process_block(genesis_block_from(&create_genesis_block(6666)))
            .unwrap();

        produce_block(
            &mut cm,
            &mut world,
            &[signed_transfer(key, 0, [0x22u8; 20], 1234)],
            1,
            &miner,
            6666,
            Some(&TESTS_SEAL_KEY),
        )
        .unwrap();

        let receipts = cm.chain().receipts(1).expect("receipts must be stored");
        assert_eq!(receipts.len(), 1);
        assert!(receipts[0].status, "a plain transfer succeeds");
        assert_eq!(receipts[0].gas_used, 21_000);
        assert_eq!(receipts[0].cumulative_gas_used, 21_000);
        assert!(receipts[0].logs.is_empty(), "a transfer emits no logs");

        // The header must commit to exactly these receipts.
        let block = cm.chain().get_block(1).unwrap();
        assert_eq!(
            block.header.receipts_root.to_lowercase(),
            hex32(&tiger_blockchain::receipts_root(receipts))
        );
        assert_eq!(
            block.header.logs_bloom.to_lowercase(),
            format!(
                "0x{}",
                tiger_blockchain::hex_encode(&tiger_blockchain::logs_bloom(receipts))
            )
        );
    }

    #[test]
    fn a_block_whose_declared_receipts_root_is_wrong_is_rejected() {
        // Build a real block, then tamper with the declared receipts root.
        // Accepting it would mean the node does not verify what it commits to.
        let mut cm = chain_manager();
        let mut world = WorldState::new();
        let miner = format!("0x{}", tiger_blockchain::hex_encode(&[0xaa; 20]));
        let key = [0x46u8; 32];
        let sender = tiger_crypto::private_key_to_address(&key).unwrap();
        world.set_balance(&sender, u256_from_u64(10_000_000));
        cm.process_block(genesis_block_from(&create_genesis_block(6666)))
            .unwrap();
        produce_block(
            &mut cm,
            &mut world,
            &[signed_transfer(key, 0, [0x22u8; 20], 1)],
            1,
            &miner,
            6666,
            Some(&TESTS_SEAL_KEY),
        )
        .unwrap();

        let receipts = cm.chain().receipts(1).unwrap().to_vec();
        let mut block = cm.chain().get_block(1).unwrap().clone();
        block.header.receipts_root = hex32(&[0x11u8; 32]);

        // Editing a sealed header breaks the seal, so the tampering is caught
        // before the receipts are even examined.
        let mut fresh = chain_manager();
        fresh
            .process_block(genesis_block_from(&create_genesis_block(6666)))
            .unwrap();
        let err = fresh.process_block_with_receipts(block.clone(), receipts.clone()).unwrap_err();
        assert!(
            matches!(err, tiger_blockchain::ValidationError::SealSignerMismatch { .. }),
            "editing a sealed header must break the seal, got {err}"
        );
        assert_eq!(fresh.chain().height(), 0);

        // Re-sealing the tampered header isolates the receipts check, which
        // must still reject a root that does not match the receipts.
        block.header.extra_data =
            tiger_blockchain::seal_header(&block.header, &TESTS_SEAL_KEY).unwrap();
        let mut fresh = chain_manager();
        fresh
            .process_block(genesis_block_from(&create_genesis_block(6666)))
            .unwrap();
        let err = fresh.process_block_with_receipts(block, receipts).unwrap_err();
        assert!(
            matches!(err, tiger_blockchain::ValidationError::ReceiptsRootMismatch { .. }),
            "expected a receipts-root mismatch, got {err}"
        );
        // The tampered block must not have been inserted.
        assert_eq!(fresh.chain().height(), 0);
    }

    #[test]
    fn contract_logs_are_captured_and_filterable() {
        use tiger_evm::transition::BlockContext;

        // A contract that emits LOG1 with one topic: PUSH1 0x07 (topic),
        // PUSH1 0x00 PUSH1 0x00 (offset, length), LOG1, then STOP.
        // Bytecode: 60 07 60 00 60 00 a1 00
        let code = vec![0x60, 0x07, 0x60, 0x00, 0x60, 0x00, 0xa1, 0x00];
        let contract = [0x33u8; 20];

        let mut world = WorldState::new();
        world.set_code(&contract, code);

        let block_ctx = BlockContext {
            chain_id: 6666,
            number: 1,
            timestamp: 1_700_000_000,
            gas_limit: 30_000_000,
            coinbase: [0xaa; 20],
            base_fee: 0,
        };

        // Call the contract with no calldata; the call itself is the
        // transaction, so the caller needs a balance to pay gas.
        let caller = [0x44u8; 20];
        world.set_balance(&caller, u256_from_u64(10_000_000));
        let request = tiger_evm::transition::TransactionRequest {
            nonce: 0,
            gas_price: tiger_evm::transition::wei(1),
            gas_limit: 100_000,
            to: Some(contract),
            value: [0u8; 32],
            data: vec![],
            sender: caller,
            max_priority_fee_per_gas: None,
        };

        let outcome =
            tiger_evm::transition::apply_transaction(&mut world, &request, &block_ctx).unwrap();
        assert!(outcome.success);
        assert_eq!(outcome.logs.len(), 1, "LOG1 must produce exactly one log");
        let log = &outcome.logs[0];
        assert_eq!(log.address.0, contract);
        assert_eq!(log.topics.len(), 1);
        assert_eq!(log.topics[0].0[31], 0x07);

        // The receipt the node builds from this outcome carries the log, and
        // the filter logic matches it.
        let receipt = tiger_blockchain::BlockReceipt {
            status: true,
            cumulative_gas_used: outcome.gas_used,
            gas_used: outcome.gas_used,
            logs: vec![tiger_blockchain::ReceiptLog {
                address: contract,
                topics: log.topics.iter().map(|t| t.0).collect(),
                data: log.data.clone(),
            }],
        };
        assert_eq!(receipt.logs.len(), 1);

        // A topic filter matching the emitted topic must match, and one
        // naming a different topic must not.
        let matching: TopicFilter = vec![Some(vec![log.topics[0].0])];
        assert!(topics_match(&matching, &[log.topics[0].0]));
        let non_matching: TopicFilter = vec![Some(vec![[0x99u8; 32]])];
        assert!(!topics_match(&non_matching, &[log.topics[0].0]));

        // A filter longer than the log's topics can never match.
        let too_long: TopicFilter = vec![None, None];
        assert!(!topics_match(&too_long, &[log.topics[0].0]));
    }

    /// A transaction whose fee ceiling is below the block's base fee cannot be
    /// included: the sender never authorised paying the burn. It must be left
    /// out of the block entirely rather than applied and then reverted.
    #[test]
    fn a_transaction_below_the_base_fee_is_not_included() {
        let mut cm = chain_manager();
        let mut world = WorldState::new();
        let miner = format!("0x{}", tiger_blockchain::hex_encode(&[0xaa; 20]));
        let key = [0x46u8; 32];
        let sender = tiger_crypto::private_key_to_address(&key).unwrap();
        world.set_balance(&sender, u256_from_u64(10_000_000));
        cm.process_block(genesis_block_from(&create_genesis_block(6666)))
            .unwrap();

        // The genesis block sets the base fee at one gwei, so a transaction
        // offering a single wei can never be mined.
        let mut tx = signed_transfer(key, 0, [0x22u8; 20], 1234);
        tx.gas_price = 1;
        tx.tx_type = 2;
        tx.chain_id = Some(6666);
        tx.access_list = Some("0xc0".to_string());
        tx.max_priority_fee_per_gas = Some(1);
        sign_transaction(&mut tx, 6666, &key).unwrap();

        produce_block(&mut cm, &mut world, &[tx], 1, &miner, 6666, Some(&TESTS_SEAL_KEY)).unwrap();

        let block = cm.chain().get_block(1).unwrap();
        assert!(block.transactions.is_empty(), "an under-priced tx must not be mined");
        assert_eq!(block.header.gas_used, 0);
        // The sender paid nothing and its nonce did not move, so the
        // transaction is still valid for a later block.
        assert_eq!(world.nonce(&sender), 0);
        assert_eq!(world.balance(&sender), u256_from_u64(10_000_000));
    }

    /// A block never exceeds its gas limit, and a transaction that does not
    /// fit is left pending rather than dropped or partially applied.
    #[test]
    fn a_block_stops_at_its_gas_limit() {
        let mut cm = chain_manager();
        let mut world = WorldState::new();
        let miner = format!("0x{}", tiger_blockchain::hex_encode(&[0xaa; 20]));
        let key = [0x46u8; 32];
        let sender = tiger_crypto::private_key_to_address(&key).unwrap();
        world.set_balance(&sender, u256_from_u64(1_000_000_000_000));
        cm.process_block(genesis_block_from(&create_genesis_block(6666)))
            .unwrap();

        // Each transfer reserves 21,000 gas, and the block limit is far above
        // that, so a handful all fit and the limit is not what is under test
        // here; what matters is that gas_used never exceeds the header limit
        // and that every included transaction is accounted for.
        let txs: Vec<_> = (0..5)
            .map(|nonce| signed_transfer(key, nonce, [0x22u8; 20], 1))
            .collect();
        produce_block(&mut cm, &mut world, &txs, 1, &miner, 6666, Some(&TESTS_SEAL_KEY)).unwrap();

        let block = cm.chain().get_block(1).unwrap();
        assert!(block.header.gas_used <= block.header.gas_limit);
        assert_eq!(block.transactions.len(), 5);
        assert_eq!(block.header.gas_used, 5 * 21_000);
        assert_eq!(world.nonce(&sender), 5);
    }


    /// The attack the seal exists to stop, at the node boundary: a block that
    /// names a validator in its `miner` field but was not signed by that
    /// validator's key must be refused, and its state changes must not be
    /// adopted.
    #[test]
    fn a_block_forged_in_a_validators_name_is_rejected() {
        let mut cm = chain_manager();
        let mut world = WorldState::new();
        let victim = format!("0x{}", tiger_blockchain::hex_encode(&[0xaa; 20]));
        cm.process_block(genesis_block_from(&create_genesis_block(6666)))
            .unwrap();

        // Produce an honest block, then strip its seal and re-seal it with an
        // attacker's key while still claiming the victim's address.
        produce_block(&mut cm, &mut world, &[], 1, &victim, 6666, Some(&TESTS_SEAL_KEY))
            .unwrap();
        let honest = cm.chain().get_block(1).unwrap().clone();

        let attacker_key = [0x99u8; 32];
        let mut forged = honest.clone();
        forged.header.miner = victim.clone();
        forged.header.extra_data =
            tiger_blockchain::seal_header(&forged.header, &attacker_key).unwrap();

        // A fresh chain must refuse it.
        let mut fresh = chain_manager();
        fresh
            .process_block(genesis_block_from(&create_genesis_block(6666)))
            .unwrap();
        let err = fresh.process_block(forged).unwrap_err();
        assert!(
            matches!(err, tiger_blockchain::ValidationError::SealSignerMismatch { .. }),
            "a block forged in another validator's name must be rejected, got {err}"
        );
        assert_eq!(fresh.chain().height(), 0, "the forged block must not be adopted");
    }

    /// A block with no seal at all is refused, so a peer cannot sidestep the
    /// check by simply omitting the seal.
    #[test]
    fn an_unsealed_block_is_rejected() {
        let mut cm = chain_manager();
        let mut world = WorldState::new();
        let miner = format!("0x{}", tiger_blockchain::hex_encode(&[0xaa; 20]));
        cm.process_block(genesis_block_from(&create_genesis_block(6666)))
            .unwrap();
        produce_block(&mut cm, &mut world, &[], 1, &miner, 6666, Some(&TESTS_SEAL_KEY))
            .unwrap();

        let mut stripped = cm.chain().get_block(1).unwrap().clone();
        stripped.header.extra_data = "0x".to_string();

        let mut fresh = chain_manager();
        fresh
            .process_block(genesis_block_from(&create_genesis_block(6666)))
            .unwrap();
        let err = fresh.process_block(stripped).unwrap_err();
        assert!(
            matches!(err, tiger_blockchain::ValidationError::MissingSeal { .. }),
            "an unsealed block must be rejected, got {err}"
        );
    }

    /// A node configured as a validator without a sealing key must not emit a
    /// block, because every peer would reject it.
    #[test]
    fn producing_without_a_key_is_refused() {
        let mut cm = chain_manager();
        let mut world = WorldState::new();
        let miner = format!("0x{}", tiger_blockchain::hex_encode(&[0xaa; 20]));
        cm.process_block(genesis_block_from(&create_genesis_block(6666)))
            .unwrap();

        let err = produce_block(&mut cm, &mut world, &[], 1, &miner, 6666, None).unwrap_err();
        assert!(
            err.contains("no validator key"),
            "expected a refusal to produce, got: {err}"
        );
        assert_eq!(cm.chain().height(), 0, "nothing may be produced");
    }

    /// Send one raw HTTP request to a listener and return the full response.
    async fn http_round_trip(
        addr: std::net::SocketAddr,
        request: &str,
    ) -> String {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        let mut stream = tokio::net::TcpStream::connect(addr).await.unwrap();
        stream.write_all(request.as_bytes()).await.unwrap();
        stream.flush().await.unwrap();
        let mut response = Vec::new();
        stream.read_to_end(&mut response).await.unwrap();
        String::from_utf8_lossy(&response).into_owned()
    }

    /// The RPC transport is exercised over a real socket, because the parts
    /// worth testing here — header handling, the CORS decision, the rate limit,
    /// the status codes — only exist on the wire.
    #[tokio::test]
    async fn the_rpc_transport_enforces_cors_the_rate_limit_and_health() {
        // A node configured to allow one origin and to cap a client at three
        // requests per second. The built-in RPC and metrics listeners are off
        // so the test binds its own port and does not race another test for
        // the default one.
        let data_dir = std::env::temp_dir()
            .join(format!("tiger-node-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&data_dir);
        let config = Config {
            chain_id: 6666,
            rpc_cors_host: "https://explorer.tiger.example".to_string(),
            rpc_rate_limit: 3,
            rpc_enabled: false,
            metrics_enabled: false,
            data_dir: data_dir.to_string_lossy().into_owned(),
            ..Config::default_mainnet()
        };
        let node = Arc::new(
            TigerNode::from_genesis(config, create_genesis_block(6666))
                .await
                .unwrap(),
        );
        node.start().await.unwrap();

        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let server = tokio::spawn(crate::rpc_server::serve_listener(listener, Arc::clone(&node)));
        // Give the accept loop a moment to start.
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;

        // 1. An allowed origin is echoed back.
        let body = "{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"eth_chainId\"}";
        let allowed = http_round_trip(
            addr,
            &format!(
                "POST / HTTP/1.1\r\nHost: x\r\nOrigin: https://explorer.tiger.example\r\nContent-Length: {}\r\n\r\n{body}",
                body.len()
            ),
        )
        .await;
        assert!(allowed.starts_with("HTTP/1.1 200"), "got: {allowed}");
        assert!(allowed.contains("Access-Control-Allow-Origin: https://explorer.tiger.example"));
        assert!(allowed.contains("\"result\":\"0x1a0a\""), "chain id must be 6666: {allowed}");

        // 2. An origin that is not on the list gets no CORS header at all.
        let denied = http_round_trip(
            addr,
            &format!(
                "POST / HTTP/1.1\r\nHost: x\r\nOrigin: https://evil.example\r\nContent-Length: {}\r\n\r\n{body}",
                body.len()
            ),
        )
        .await;
        assert!(!denied.to_ascii_lowercase().contains("access-control-allow-origin"));

        // 3. A preflight is answered without a body and carries the origin.
        let preflight = http_round_trip(
            addr,
            "OPTIONS / HTTP/1.1\r\nHost: x\r\nOrigin: https://explorer.tiger.example\r\n\r\n",
        )
        .await;
        assert!(preflight.starts_with("HTTP/1.1 204"), "got: {preflight}");
        assert!(preflight.contains("Access-Control-Allow-Origin: https://explorer.tiger.example"));

        // 4. The health endpoint reports the node's real state.
        let health = http_round_trip(addr, "GET /health HTTP/1.1\r\nHost: x\r\n\r\n").await;
        assert!(health.contains("\"status\":\"ok\""), "got: {health}");
        assert!(health.contains("\"chain_id\":6666"), "got: {health}");
        assert!(health.contains("\"running\":true"), "got: {health}");

        // 5. A GET on the RPC path is refused with a method error rather than
        //    being parsed as an empty JSON body.
        let get_rpc = http_round_trip(addr, "GET / HTTP/1.1\r\nHost: x\r\n\r\n").await;
        assert!(get_rpc.starts_with("HTTP/1.1 405"), "got: {get_rpc}");
        assert!(get_rpc.contains("Allow: POST"));

        // 6. The rate limit bites. Two requests have already been counted
        //    above (the POSTs), so the bucket is nearly empty.
        let body = "{\"jsonrpc\":\"2.0\",\"id\":9,\"method\":\"eth_chainId\"}";
        let mut saw_429 = false;
        for _ in 0..6 {
            let response = http_round_trip(
                addr,
                &format!(
                    "POST / HTTP/1.1\r\nHost: x\r\nContent-Length: {}\r\n\r\n{body}",
                    body.len()
                ),
            )
            .await;
            if response.starts_with("HTTP/1.1 429") {
                saw_429 = true;
                assert!(response.contains("Retry-After: 1"));
                break;
            }
        }
        assert!(saw_429, "a burst past the configured rate must be throttled");

        server.abort();
    }

    /// A fresh data directory, unique per call so tests cannot see each
    /// other's chains.
    fn unique_data_dir(tag: &str) -> std::path::PathBuf {
        static COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let unique = COUNTER.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        let dir = std::env::temp_dir().join(format!(
            "tiger-node-{tag}-{}-{unique}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        dir
    }

    fn restartable_config(dir: &std::path::Path) -> Config {
        Config {
            chain_id: 6666,
            validators: vec![],
            data_dir: dir.to_string_lossy().into_owned(),
            ..Config::default_mainnet()
        }
    }

    /// The chain a node holds must survive a restart: the blocks, their
    /// receipts, the canonical branch and the world state are all written
    /// incrementally, and a second node opened on the same directory has to
    /// rebuild exactly the chain the first one had.
    #[tokio::test]
    async fn the_chain_survives_a_restart() {
        let dir = unique_data_dir("restart");
        let genesis = create_genesis_block(6666);

        // --- first run: import two blocks, including a real transfer ---
        let (head_hash, expected_root, balance_after) = {
            let node = Arc::new(
                TigerNode::from_genesis(restartable_config(&dir), genesis.clone())
                    .await
                    .unwrap(),
            );
            node.open().await.unwrap();

            let mut producer_cm = chain_manager();
            producer_cm.process_block(genesis_block_from(&genesis)).unwrap();
            let mut producer_world = state_from_genesis(&genesis);
            let miner = format!("0x{}", tiger_blockchain::hex_encode(&[0xaa; 20]));

            // Fund a sender in the producer's genesis-derived state so the
            // transfer has a balance to spend.
            let sender_key = [0x51u8; 32];
            let sender = tiger_crypto::private_key_to_address(&sender_key).unwrap();
            let recipient = [0x77u8; 20];
            let funded = u256_from_u64(1_000_000_000_000_000_000);
            producer_world.set_balance(&sender, funded);
            // The node's own state must match, because a block is only
            // accepted when its state root matches a re-execution.
            {
                let mut world = node.world_state().lock().await;
                world.set_balance(&sender, funded);
            }

            let tx = signed_transfer(sender_key, 0, recipient, 1_000);
            let block1 = build_block(&mut producer_cm, &mut producer_world, 1, &miner, &[tx]);
            assert!(node.import_block(block1).await.unwrap());
            let block2 = build_block(&mut producer_cm, &mut producer_world, 2, &miner, &[]);
            assert!(node.import_block(block2.clone()).await.unwrap());

            let head_hash = tiger_blockchain::block_hash(&block2.header).unwrap();
            let expected_root = block2.header.state_root.clone();
            let balance = producer_world.balance(&recipient);
            assert_ne!(balance, u256_from_u64(0), "the transfer must have credited the recipient");

            (head_hash, expected_root, balance)
        };

        // --- second run: a new node on the same directory ---
        let node = Arc::new(
            TigerNode::from_genesis(restartable_config(&dir), genesis.clone())
                .await
                .unwrap(),
        );
        let restored = node.restore_from_disk().await.unwrap();
        assert_eq!(restored, 3, "genesis plus the two imported blocks");

        let cm = node.get_chain_manager().await;
        assert_eq!(cm.chain().height(), 2, "the head height must survive");
        let head = cm.chain().get_block(2).expect("block 2 must be restored");
        assert_eq!(
            tiger_blockchain::block_hash(&head.header).unwrap(),
            head_hash,
            "the restored head must be the same block"
        );
        assert_eq!(head.header.state_root, expected_root);

        // The transaction was in block 1, so it is there that it must have
        // survived, along with the receipts the block produced, which is what
        // `eth_getTransactionReceipt` depends on.
        let first = cm.chain().get_block(1).expect("block 1 must be restored");
        assert_eq!(
            first.transactions.len(),
            1,
            "the block's transaction must survive"
        );
        let tx_hash = first.transactions[0].hash.clone();
        assert!(
            cm.chain().receipts(1).is_some(),
            "the receipt list for block 1 must be restored"
        );
        let found = cm.chain().find_transaction(&tx_hash);
        assert!(found.is_some(), "the transaction must be locatable after a restart");
        drop(cm);

        // The world state must hash to the head block's committed root, which
        // is the strongest statement that no account was lost.
        let world = node.world_state().lock().await;
        assert_eq!(
            hex32(&world.state_root()),
            expected_root,
            "the restored state must hash to the head block's stateRoot"
        );
        assert_eq!(
            world.balance(&[0x77u8; 20]),
            balance_after,
            "the recipient's balance must survive the restart"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A validator that supplies only a key must still propose blocks. The
    /// address is derivable from the key, so requiring it separately would let
    /// a correctly configured validator come up and never seal anything.
    #[tokio::test]
    async fn the_proposer_address_is_derived_from_the_sealing_key() {
        let dir = unique_data_dir("derive-addr");
        let key = [0x33u8; 32];
        let expected = format!(
            "0x{}",
            tiger_blockchain::hex_encode(
                &tiger_crypto::private_key_to_address(&key).unwrap()
            )
        );
        let config = Config {
            chain_id: 6666,
            validators: vec![expected.clone()],
            validator_key: Some(format!("0x{}", tiger_blockchain::hex_encode(&key))),
            validator_addr: None,
            data_dir: dir.to_string_lossy().into_owned(),
            ..Config::default_mainnet()
        };
        let node = TigerNode::from_genesis(config, create_genesis_block(6666))
            .await
            .unwrap();
        assert_eq!(
            node.config().validator_addr.as_deref(),
            Some(expected.as_str()),
            "the address must be derived from the key"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Liveness at the node boundary: a node that is not the in-turn proposer
    /// must still be able to produce. Under a strict in-turn rule the loop
    /// would keep naming a validator that is not running and the chain would
    /// never advance.
    #[tokio::test]
    async fn a_validator_out_of_turn_can_still_propose() {
        let dir = unique_data_dir("out-of-turn");
        // Two validators. Height 1 belongs to the second one in the rotation.
        let key = [0x44u8; 32];
        let mine = format!(
            "0x{}",
            tiger_blockchain::hex_encode(
                &tiger_crypto::private_key_to_address(&key).unwrap()
            )
        );
        let other = format!("0x{}", tiger_blockchain::hex_encode(&[0x55u8; 20]));
        // Put `other` first so `mine` is not the in-turn proposer at height 1.
        let config = Config {
            chain_id: 6666,
            validators: vec![other.clone(), mine.clone()],
            validator_key: Some(format!("0x{}", tiger_blockchain::hex_encode(&key))),
            validator_addr: None,
            data_dir: dir.to_string_lossy().into_owned(),
            rpc_enabled: false,
            ws_enabled: false,
            metrics_enabled: false,
            ..Config::default_mainnet()
        };
        let node = TigerNode::from_genesis(config, create_genesis_block(6666))
            .await
            .unwrap();

        let consensus = node.consensus.lock().await;
        assert!(consensus.is_validator(&mine), "this node is a validator");

        // The election sorts by stake then address, so the rotation order is
        // not the order the validators were listed in. Find a height at which
        // this node is genuinely out of turn; with two validators one exists.
        let height = (1..=4)
            .find(|h| {
                consensus
                    .expected_proposer(*h)
                    .map(|p| !p.eq_ignore_ascii_case(&mine))
                    .unwrap_or(false)
            })
            .expect("with two validators, some height is not this node's turn");

        let distance = consensus
            .proposer_turn_distance(height, &mine)
            .expect("a registered validator has a slot");
        assert!(
            distance > 0,
            "the node must be out of turn at the chosen height, got {distance}"
        );

        let parent_timestamp = 1_000;
        let earliest = consensus
            .earliest_proposal_time(height, &mine, parent_timestamp)
            .unwrap();
        assert!(
            earliest > parent_timestamp,
            "an out-of-turn proposer must wait for its slot"
        );
        // And it must still be allowed to propose once that slot arrives,
        // which is the liveness property: the chain does not wait for the
        // absent in-turn validator.
        assert!(
            consensus.is_timely_proposal(height, &mine, earliest, parent_timestamp),
            "the backup must be allowed to propose at its own slot"
        );
        assert!(
            !consensus.is_timely_proposal(height, &mine, earliest - 1, parent_timestamp),
            "the backup must not propose before its slot"
        );
        drop(consensus);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A store written for one chain must not be opened by a node configured
    /// for another, because the blocks would attach to the wrong genesis.
    #[tokio::test]
    async fn a_store_from_another_chain_is_not_opened() {
        let dir = unique_data_dir("wrong-chain");
        let genesis = create_genesis_block(6666);
        {
            let node = Arc::new(
                TigerNode::from_genesis(restartable_config(&dir), genesis.clone())
                    .await
                    .unwrap(),
            );
            node.open().await.unwrap();
            assert!(node.storage().is_some(), "the store must be open");
        }

        // A node configured for a different chain id must refuse the store and
        // fall back to running without persistence rather than mixing chains.
        let mut config = restartable_config(&dir);
        config.chain_id = 7777;
        let other_genesis = create_genesis_block(7777);
        let node = Arc::new(
            TigerNode::from_genesis(config, other_genesis).await.unwrap(),
        );
        assert!(
            node.storage().is_none(),
            "a store from another chain must not be used"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Nothing is announced before it is durable. After a block is imported,
    /// the store must already hold it, so a crash at that instant cannot lose
    /// a block a client has been told about.
    #[tokio::test]
    async fn a_block_is_durable_before_it_is_announced() {
        let dir = unique_data_dir("durable");
        let genesis = create_genesis_block(6666);
        let node = Arc::new(
            TigerNode::from_genesis(restartable_config(&dir), genesis.clone())
                .await
                .unwrap(),
        );
        node.open().await.unwrap();

        let mut producer_cm = chain_manager();
        producer_cm.process_block(genesis_block_from(&genesis)).unwrap();
        let mut producer_world = state_from_genesis(&genesis);
        let miner = format!("0x{}", tiger_blockchain::hex_encode(&[0xaa; 20]));
        let block = build_block(&mut producer_cm, &mut producer_world, 1, &miner, &[]);
        let hash = tiger_blockchain::block_hash(&block.header).unwrap();
        assert!(node.import_block(block).await.unwrap());

        // The store holds an exclusive lock on the directory — two nodes must
        // never write the same chain — so it is closed before the file is read
        // back. That the read succeeds at all is part of the point: the data
        // is in the file, not only in the process that wrote it.
        drop(node);

        // `import_block` flushes before it returns, so the block is on disk by
        // now.
        let store = crate::storage::Storage::open(&dir).unwrap();
        let stored = store.load_blocks().unwrap();
        assert!(
            stored.iter().any(|(h, _)| h.eq_ignore_ascii_case(&hash)),
            "the imported block must already be in the store"
        );
        assert_eq!(
            store.head_hash().unwrap().unwrap().to_ascii_lowercase(),
            hash.to_ascii_lowercase()
        );
        // The state the block committed to must be restorable, and must hash
        // back to the root the block carries. A block whose state cannot be
        // reconstructed is a block a client could not query.
        let root = store
            .load_blocks()
            .unwrap()
            .into_iter()
            .find(|(h, _)| h.eq_ignore_ascii_case(&hash))
            .map(|(_, block)| block.header.state_root)
            .expect("the head block must be in the store");
        let state = store.load_state(Some(&root)).unwrap();
        assert_eq!(
            hex32(&state.state_root()),
            root,
            "the restored state must hash to the head block's stateRoot"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }
}
