//! Network Sync

use crate::types::*;
use std::sync::Arc;

// =============================================================================
// SYNC
// =============================================================================

/// Sync Manager
pub struct SyncManager {
    config: NetworkConfig,
    status: SyncStatus,
    peers: std::collections::HashMap<String, Peer>,
}

impl SyncManager {
    pub fn new(config: NetworkConfig) -> Self {
        Self {
            config,
            status: SyncStatus::default(),
            peers: std::collections::HashMap::new(),
        }
    }

    /// Start sync
    pub fn start_sync(&mut self, from_block: u64) {
        self.status.syncing = true;
        self.status.starting_block = from_block;
    }

    /// Stop sync
    pub fn stop_sync(&mut self) {
        self.status.syncing = false;
    }

    /// Get status
    pub fn status(&self) -> &SyncStatus {
        &self.status
    }

    /// Add peer
    pub fn add_peer(&mut self, peer: Peer) {
        self.peers.insert(peer.id.clone(), peer);
    }

    /// Remove peer
    pub fn remove_peer(&mut self, id: &str) {
        self.peers.remove(id);
    }

    /// Get best peer
    pub fn best_peer(&self) -> Option<&Peer> {
        self.peers.values().max_by(|a, b| a.score.partial_cmp(&b.score).unwrap())
    }
}

// =============================================================================
// FAST SYNC
// =============================================================================

/// Fast Sync
pub struct FastSync {
    snapshots: std::collections::HashMap<u64, Vec<u8>>,
}

impl FastSync {
    pub fn new() -> Self {
        Self {
            snapshots: std::collections::HashMap::new(),
        }
    }

    /// Download snapshot
    pub fn download_snapshot(&mut self, block: u64, data: Vec<u8>) {
        self.snapshots.insert(block, data);
    }

    /// Get snapshot
    pub fn get_snapshot(&self, block: u64) -> Option<&Vec<u8>> {
        self.snapshots.get(&block)
    }
}

impl Default for FastSync {
    fn default() -> Self {
        Self::new()
    }
}