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

/// A 64-byte peer identity.
///
/// Serialised as a hex string because serde does not implement its traits for
/// arrays longer than 32 bytes, and a fixed-size byte array of this length
/// cannot be derived.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct PeerID(pub [u8; 64]);

impl PeerID {
    /// Generate a random peer id.
    ///
    /// The whole 64-byte space is filled from the operating system's
    /// cryptographic RNG. An earlier version only seeded the first 8 bytes
    /// from the clock, which made ids collide and be predictable.
    pub fn random() -> Self {
        use rand::RngCore;
        let mut id = [0u8; 64];
        rand::rngs::OsRng.fill_bytes(&mut id);
        Self(id)
    }
}

impl std::fmt::Display for PeerID {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        for byte in self.0 {
            write!(f, "{byte:02x}")?;
        }
        Ok(())
    }
}

impl Serialize for PeerID {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.to_string())
    }
}

impl<'de> Deserialize<'de> for PeerID {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let hex = String::deserialize(deserializer)?;
        if hex.len() != 128 {
            return Err(serde::de::Error::custom(
                "peer id must be 128 hex characters",
            ));
        }
        let mut id = [0u8; 64];
        for (i, byte) in id.iter_mut().enumerate() {
            *byte = u8::from_str_radix(&hex[i * 2..i * 2 + 2], 16)
                .map_err(serde::de::Error::custom)?;
        }
        Ok(PeerID(id))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn random_peer_ids_fill_the_whole_space_and_do_not_collide() {
        let a = PeerID::random();
        let b = PeerID::random();
        assert_ne!(a, b, "two random ids must differ");
        // The id must use more than the first few bytes; a clock-seeded
        // implementation left the tail zero.
        assert!(
            a.0[8..].iter().any(|&b| b != 0),
            "the whole id must be randomised"
        );
    }

    #[test]
    fn peer_id_round_trips_through_json() {
        let id = PeerID::random();
        let json = serde_json::to_string(&id).unwrap();
        let back: PeerID = serde_json::from_str(&json).unwrap();
        assert_eq!(id, back);
    }

    #[test]
    fn malformed_peer_ids_are_rejected() {
        assert!(serde_json::from_str::<PeerID>("\"abcd\"").is_err());
        assert!(serde_json::from_str::<PeerID>("\"zz\"").is_err());
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