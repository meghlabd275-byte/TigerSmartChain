//! The peer set.
//!
//! Tracks which peers are connected, what each one says its chain looks like,
//! and which to ask for blocks. The manager owns no sockets: a connection is
//! driven by a task that reads frames and calls into the manager, so there is
//! one place that decides whether a peer is useful or should be dropped.

use std::collections::HashMap;
use std::sync::Arc;

use tokio::sync::{mpsc, Mutex};

use crate::transport::{now_millis, Identity, Session};
use crate::types::{NetworkConfig, NetworkError, Peer, PeerID};

/// How long a peer may go without a message before it is considered dead.
pub const PEER_TIMEOUT_MILLIS: i64 = 60_000;

/// A command for the task that owns a peer's connection.
///
/// A connection is driven by exactly one task, which both reads frames and
/// drains this queue. Keeping the socket in one task is what makes the
/// sequence-number counters safe without a lock, and it means a peer that
/// stops reading cannot stall the node: its queue fills and it is dropped.
pub enum PeerCommand {
    /// Send a frame to the peer.
    Send(Vec<u8>),
    /// Close the connection and stop the task.
    Close,
}

/// Everything the node knows about one connected peer.
pub struct PeerConnection {
    /// The peer's identity, proven by the handshake.
    pub id: PeerID,
    /// Address the peer is reachable at, for reconnecting.
    pub address: String,
    /// Height the peer last reported.
    pub height: u64,
    /// Hash of the peer's head block, as it last reported.
    pub head_hash: String,
    /// When the peer last sent anything, in milliseconds since the epoch.
    pub last_seen: i64,
    /// Whether this node dialled the peer.
    pub outbound: bool,
    /// Channel used to hand frames to the task that owns the connection.
    sender: mpsc::Sender<PeerCommand>,
}

impl PeerConnection {
    /// Queue a frame for delivery.
    pub async fn send_frame(&self, frame: Vec<u8>) -> Result<(), NetworkError> {
        self.sender
            .send(PeerCommand::Send(frame))
            .await
            .map_err(|_| NetworkError::Transport("the peer's connection has closed".to_string()))
    }

    /// Queue a frame without waiting for room in the queue.
    ///
    /// Used for broadcast, where one peer with a full queue must not delay
    /// delivery to the others.
    pub fn try_send_frame(&self, frame: Vec<u8>) -> bool {
        self.sender.try_send(PeerCommand::Send(frame)).is_ok()
    }

    /// Ask the peer's task to close the connection.
    pub fn close(&self) {
        let _ = self.sender.try_send(PeerCommand::Close);
    }

    /// Whether the peer has gone quiet for too long.
    pub fn is_stale(&self) -> bool {
        now_millis() - self.last_seen > PEER_TIMEOUT_MILLIS
    }
}

/// The set of connected peers.
pub struct PeerManager {
    config: NetworkConfig,
    identity: Identity,
    chain_id: u64,
    peers: HashMap<PeerID, Arc<Mutex<PeerConnection>>>,
}

impl PeerManager {
    pub fn new(config: NetworkConfig, identity: Identity, chain_id: u64) -> Self {
        Self {
            config,
            identity,
            chain_id,
            peers: HashMap::new(),
        }
    }

    pub fn identity(&self) -> &Identity {
        &self.identity
    }

    pub fn chain_id(&self) -> u64 {
        self.chain_id
    }

    pub fn config(&self) -> &NetworkConfig {
        &self.config
    }

    /// The number of connected peers.
    pub fn peer_count(&self) -> usize {
        self.peers.len()
    }

    /// Whether another peer may be accepted.
    pub fn has_capacity(&self) -> bool {
        self.peers.len() < self.config.max_peers
    }

    /// Whether a peer is already connected.
    pub fn contains(&self, id: &PeerID) -> bool {
        self.peers.contains_key(id)
    }

    /// Register a peer.
    ///
    /// Returns the connection handle and the receiver for the commands that
    /// handle carries. The caller owns the connection's task and must drive
    /// both the socket and the receiver, because only that task can read and
    /// write the session.
    pub fn register(
        &mut self,
        session: &Session,
        address: String,
    ) -> Result<
        (
            Arc<Mutex<PeerConnection>>,
            mpsc::Receiver<PeerCommand>,
        ),
        NetworkError,
    > {
        if !self.has_capacity() {
            return Err(NetworkError::TooManyPeers);
        }
        let id = session.peer_id.clone();
        if self.peers.contains_key(&id) {
            return Err(NetworkError::Protocol(format!(
                "peer {id} is already connected"
            )));
        }

        let (sender, receiver) = mpsc::channel::<PeerCommand>(256);
        let connection = PeerConnection {
            id: id.clone(),
            address,
            height: 0,
            head_hash: String::new(),
            last_seen: now_millis(),
            outbound: session.outbound,
            sender,
        };
        let handle = Arc::new(Mutex::new(connection));
        self.peers.insert(id, Arc::clone(&handle));
        Ok((handle, receiver))
    }

    /// Note that a peer sent something, so it is not considered stale.
    pub fn mark_seen(&mut self, id: &PeerID) {
        if let Some(peer) = self.peers.get(id) {
            // `try_lock` cannot fail here in practice: the peer's own task is
            // the only other holder and it does not hold the lock across an
            // await. If it somehow does, the timestamp is refreshed on the next
            // message rather than blocking the whole peer loop.
            if let Ok(mut peer) = peer.try_lock() {
                peer.last_seen = now_millis();
            }
        }
    }

    /// Record the chain state a peer reported.
    pub fn record_status(&mut self, id: &PeerID, height: u64, head_hash: String) {
        if let Some(peer) = self.peers.get(id) {
            if let Ok(mut peer) = peer.try_lock() {
                peer.height = height;
                peer.head_hash = head_hash;
                peer.last_seen = now_millis();
            }
        }
    }

    /// Remove a peer.
    pub fn remove(&mut self, id: &PeerID) -> Option<Arc<Mutex<PeerConnection>>> {
        self.peers.remove(id)
    }

    /// Drop every peer that has gone quiet.
    pub fn prune_stale(&mut self) -> Vec<PeerID> {
        let stale: Vec<PeerID> = self
            .peers
            .values()
            .filter_map(|peer| {
                let peer = peer.try_lock().ok()?;
                peer.is_stale().then(|| peer.id.clone())
            })
            .collect();
        for id in &stale {
            self.peers.remove(id);
        }
        stale
    }

    /// A snapshot of the connected peers, for the `admin_peers`-style views.
    pub fn snapshot(&self) -> Vec<Peer> {
        self.peers
            .values()
            .filter_map(|peer| {
                let peer = peer.try_lock().ok()?;
                Some(Peer {
                    id: peer.id.to_string(),
                    address: peer.address.clone(),
                    port: 0,
                    connected: true,
                    last_seen: peer.last_seen,
                    latency: 0,
                    // A peer that has reported a greater height than this node
                    // is the one worth asking for blocks, so score is the
                    // height it claims. It is only a hint: every block it
                    // sends is still validated.
                    score: peer.height as f64,
                    height: peer.height,
                    head_hash: peer.head_hash.clone(),
                    outbound: peer.outbound,
                })
            })
            .collect()
    }

    /// The peers that claim the greatest height, best first.
    pub fn best_peers(&self, limit: usize) -> Vec<Arc<Mutex<PeerConnection>>> {
        let mut peers: Vec<Arc<Mutex<PeerConnection>>> = self.peers.values().cloned().collect();
        peers.sort_by_key(|peer| {
            // Sort descending by reported height without holding the lock
            // during the sort.
            let height = peer
                .try_lock()
                .map(|p| p.height)
                .unwrap_or(0);
            std::cmp::Reverse(height)
        });
        peers.truncate(limit);
        peers
    }

    /// Broadcast a frame to every peer.
    ///
    /// A peer whose queue is full is skipped rather than blocking: one slow
    /// peer must not stall block propagation to the rest.
    pub fn broadcast(&self, frame: Vec<u8>) -> usize {
        let mut delivered = 0;
        for peer in self.peers.values() {
            if let Ok(peer) = peer.try_lock() {
                if peer.try_send_frame(frame.clone()) {
                    delivered += 1;
                }
            }
        }
        delivered
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::PeerMessage;

    fn manager(max_peers: usize) -> PeerManager {
        let config = NetworkConfig {
            max_peers,
            ..NetworkConfig::default()
        };
        PeerManager::new(config, Identity::generate(), 6666)
    }

    #[test]
    fn a_fresh_manager_has_no_peers() {
        let manager = manager(4);
        assert_eq!(manager.peer_count(), 0);
        assert!(manager.has_capacity());
        assert_eq!(manager.chain_id(), 6666);
    }

    #[test]
    fn capacity_is_enforced() {
        let manager = manager(0);
        assert!(!manager.has_capacity(), "a zero-peer limit accepts nobody");
    }

    #[test]
    fn stale_peers_are_pruned() {
        let mut manager = manager(4);
        // Insert a peer directly so the test does not need a live socket.
        let id = PeerID([5u8; 32]);
        let (sender, _receiver) = mpsc::channel::<PeerCommand>(1);
        let connection = PeerConnection {
            id: id.clone(),
            address: "127.0.0.1:30303".to_string(),
            height: 0,
            head_hash: String::new(),
            last_seen: now_millis() - PEER_TIMEOUT_MILLIS - 1,
            outbound: true,
            sender,
        };
        manager
            .peers
            .insert(id.clone(), Arc::new(Mutex::new(connection)));

        assert_eq!(manager.peer_count(), 1);
        let pruned = manager.prune_stale();
        assert_eq!(pruned, vec![id]);
        assert_eq!(manager.peer_count(), 0);
    }

    #[test]
    fn the_tallest_chain_is_preferred() {
        let mut manager = manager(4);
        for (byte, height) in [(1u8, 10u64), (2, 99), (3, 50)] {
            let id = PeerID([byte; 32]);
            let (sender, _receiver) = mpsc::channel::<PeerCommand>(1);
            manager.peers.insert(
                id.clone(),
                Arc::new(Mutex::new(PeerConnection {
                    id,
                    address: String::new(),
                    height,
                    head_hash: String::new(),
                    last_seen: now_millis(),
                    outbound: true,
                    sender,
                })),
            );
        }

        let best = manager.best_peers(2);
        let heights: Vec<u64> = best
            .iter()
            .map(|p| p.try_lock().unwrap().height)
            .collect();
        assert_eq!(heights, vec![99, 50]);
    }

    #[test]
    fn a_peer_status_updates_the_recorded_height() {
        let mut manager = manager(4);
        let id = PeerID([7u8; 32]);
        let (sender, _receiver) = mpsc::channel::<PeerCommand>(1);
        manager.peers.insert(
            id.clone(),
            Arc::new(Mutex::new(PeerConnection {
                id: id.clone(),
                address: String::new(),
                height: 0,
                head_hash: String::new(),
                last_seen: now_millis(),
                outbound: false,
                sender,
            })),
        );

        manager.record_status(&id, 1234, "0xabc".to_string());
        let peer = manager.peers.get(&id).unwrap().try_lock().unwrap();
        assert_eq!(peer.height, 1234);
        assert_eq!(peer.head_hash, "0xabc");
    }

    #[test]
    fn messages_round_trip_through_encoding() {
        let block = crate::Block {
            header: tiger_blockchain::types::BlockHeader {
                parent_hash: "0x0".to_string(),
                sha3_uncles: "0x0".to_string(),
                miner: "0x0".to_string(),
                state_root: "0x0".to_string(),
                transactions_root: "0x0".to_string(),
                receipts_root: "0x0".to_string(),
                logs_bloom: "0x0".to_string(),
                difficulty: "0".to_string(),
                number: 7,
                gas_limit: 30_000_000,
                gas_used: 21_000,
                timestamp: 1,
                extra_data: "0x".to_string(),
                mix_hash: "0x0".to_string(),
                nonce: "0x0".to_string(),
                base_fee_per_gas: Some(0),
                withdrawals_root: None,
            },
            transactions: vec![],
            uncles: vec![],
        };
        let message = PeerMessage::NewBlock(Box::new(block));
        let encoded = crate::types::encode_message(&message).unwrap();
        // Re-encoding the decoded message must reproduce the same bytes, which
        // is what makes the round trip lossless.
        let decoded = crate::types::decode_message(&encoded).unwrap();
        assert_eq!(crate::types::encode_message(&decoded).unwrap(), encoded);
    }

    #[test]
    fn a_malformed_message_is_rejected() {
        assert!(crate::types::decode_message(b"not json").is_err());
    }
}
