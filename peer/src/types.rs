//! Peer Types

use serde::{Deserialize, Serialize};

// =============================================================================
// PEER
// =============================================================================

/// Peer
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Peer {
    pub id: String,
    pub address: String,
    pub port: u16,
    pub enode: String,
    pub version: String,
    pub capabilities: Vec<String>,
}

/// Peer Set
pub struct PeerSet {
    peers: std::collections::HashMap<String, Peer>,
}

impl PeerSet {
    pub fn new() -> Self {
        Self {
            peers: std::collections::HashMap::new(),
        }
    }

    /// Add peer
    pub fn add(&mut self, peer: Peer) {
        self.peers.insert(peer.id.clone(), peer);
    }

    /// Remove peer
    pub fn remove(&mut self, id: &str) {
        self.peers.remove(id);
    }

    /// Get peer
    pub fn get(&self, id: &str) -> Option<&Peer> {
        self.peers.get(id)
    }

    /// All peers
    pub fn all(&self) -> &std::collections::HashMap<String, Peer> {
        &self.peers
    }
}

impl Default for PeerSet {
    fn default() -> Self {
        Self::new()
    }
}