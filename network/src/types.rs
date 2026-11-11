//! Network Types

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
    pub connected: bool,
    pub last_seen: i64,
    pub latency: u64,
    pub score: f64,
}

/// A peer identity: the peer's 32-byte Ed25519 public key.
///
/// The id is the key, so a peer cannot claim an id it does not hold the key
/// for. Serialised as a hex string.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct PeerID(pub [u8; 32]);

impl PeerID {
    /// Generate a random peer id.
    ///
    /// The whole 64-byte space is filled from the operating system's
    /// cryptographic RNG. An earlier version only seeded the first 8 bytes
    /// from the clock, which made ids collide and be predictable.
    pub fn random() -> Self {
        use rand::RngCore;
        let mut id = [0u8; 32];
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
        if hex.len() != 64 {
            return Err(serde::de::Error::custom(
                "peer id must be 64 hex characters",
            ));
        }
        let mut id = [0u8; 32];
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
// =============================================================================
// ERRORS
// =============================================================================

/// Why a network operation failed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NetworkError {
    /// The peer list is already full, so a new connection cannot be accepted.
    TooManyPeers,
    /// The handshake failed. The connection is not authenticated and must not
    /// carry data.
    Handshake(String),
    /// A frame could not be sealed.
    Encryption,
    /// A frame was longer than the protocol allows.
    FrameTooLarge(usize),
    /// The socket failed.
    Transport(String),
    /// A message could not be decoded, or was not valid for the protocol.
    Protocol(String),
    /// No peer could be reached.
    NoPeers,
}

impl std::fmt::Display for NetworkError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            NetworkError::TooManyPeers => write!(f, "the peer limit has been reached"),
            NetworkError::Handshake(reason) => write!(f, "handshake failed: {reason}"),
            NetworkError::Encryption => write!(f, "a frame could not be encrypted"),
            NetworkError::FrameTooLarge(size) => write!(f, "frame of {size} bytes is too large"),
            NetworkError::Transport(reason) => write!(f, "transport error: {reason}"),
            NetworkError::Protocol(reason) => write!(f, "protocol error: {reason}"),
            NetworkError::NoPeers => write!(f, "no peer is connected"),
        }
    }
}

impl std::error::Error for NetworkError {}

// =============================================================================
// MESSAGES
// =============================================================================

/// A message exchanged with a peer.
///
/// Every variant carries only data the receiver can verify for itself: a block
/// is checked against the chain, a transaction's signature is recovered before
/// it reaches the pool. Nothing a peer says is taken on trust.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum PeerMessage {
    /// Sent immediately after the handshake: the sender's chain height and the
    /// hash of its head block, so the receiver can tell whether it is behind.
    Status {
        chain_id: u64,
        height: u64,
        head_hash: String,
    },
    /// Announce a block that has been added.
    NewBlock(Box<crate::Block>),
    /// Announce a transaction hash. The body is not included, because most
    /// peers already have it; a peer that does not asks for it.
    NewTransaction(String),
    /// Ask for a transaction's body by hash.
    GetTransaction(String),
    /// A transaction in response to [`PeerMessage::GetTransaction`].
    Transaction(Box<crate::Transaction>),
    /// Ask for a block by number.
    GetBlock(u64),
    /// A block in response to [`PeerMessage::GetBlock`].
    Block(Box<crate::Block>),
    /// Ask for a range of block numbers, for bulk sync.
    GetBlocks { from: u64, count: u64 },
    /// A range of blocks, in ascending order.
    Blocks(Vec<crate::Block>),
    /// A peer's periodic proof of life.
    Ping(u64),
    /// The reply to [`PeerMessage::Ping`].
    Pong(u64),
    /// Ask the peer to stop sending. A peer that is misbehaving is told to go
    /// away rather than being disconnected silently.
    Disconnect(String),
}

/// Encode a message for the wire.
pub fn encode_message(message: &PeerMessage) -> Result<Vec<u8>, NetworkError> {
    serde_json::to_vec(message).map_err(|e| NetworkError::Protocol(e.to_string()))
}

/// Decode a message from the wire.
///
/// The frame has already been authenticated, so this only guards against a
/// well-formed peer sending something the node cannot understand.
pub fn decode_message(bytes: &[u8]) -> Result<PeerMessage, NetworkError> {
    serde_json::from_slice(bytes).map_err(|e| NetworkError::Protocol(e.to_string()))
}
