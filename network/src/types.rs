//! Network Types

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

// =============================================================================
// PEER
// =============================================================================

/// Peer
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Peer {
    pub id: String,
    pub address: String,
    pub port: u16,
    pub connected: bool,
    pub last_seen: i64,
    pub latency: u64,
    pub score: f64,
}

/// Peer ID
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PeerID(pub [u8; 64]);

impl PeerID {
    pub fn random() -> Self {
        use std::time::{SystemTime, UNIX_EPOCH};
        let seed = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos() as u64;
        let mut id = [0u8; 64];
        id[..8].copy_from_slice(&seed.to_be_bytes());
        Self(id)
    }
}

// =============================================================================
// NETWORK CONFIG
// =============================================================================

/// Network Config
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetworkConfig {
    pub listen_addr: String,
    pub port: u16,
    pub max_peers: usize,
    pub bootstrap_nodes: Vec<String>,
    pub discovery_enabled: bool,
}

impl Default for NetworkConfig {
    fn default() -> Self {
        Self {
            listen_addr: "0.0.0.0".to_string(),
            port: 30303,
            max_peers: 50,
            bootstrap_nodes: vec![],
            discovery_enabled: true,
        }
    }
}

// =============================================================================
// PROTOCOL
// =============================================================================

/// Network Protocol
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Protocol {
    ETH,
    LES,
    DISC,
    SNAP,
}

/// Message
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetworkMessage {
    pub code: u64,
    pub data: Vec<u8>,
}

// =============================================================================
// SYNC STATUS
// =============================================================================

/// Sync Status
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncStatus {
    pub syncing: bool,
    pub starting_block: u64,
    pub current_block: u64,
    pub highest_block: u64,
    pub pulled_states: u64,
    pub known_states: u64,
}

impl Default for SyncStatus {
    fn default() -> Self {
        Self {
            syncing: false,
            starting_block: 0,
            current_block: 0,
            highest_block: 0,
            pulled_states: 0,
            known_states: 0,
        }
    }
}