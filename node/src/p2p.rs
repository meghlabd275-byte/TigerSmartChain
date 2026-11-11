//! The node's peer-to-peer service.
//!
//! Owns the listener, the bootnode dialler and the per-connection loop. Each
//! connection is driven by exactly one task, which both reads frames and
//! drains that peer's outbound queue; the socket never leaves that task, so
//! the transport's sequence counters need no lock and a peer that stops
//! reading only fills its own queue.
//!
//! Nothing a peer says is trusted. A block goes through
//! [`TigerNode::import_block`], which re-executes it and checks the resulting
//! state root, and a transaction goes through the same signature recovery the
//! RPC path uses, with the sender recovered from the signature rather than
//! taken from the wire.

use std::sync::Arc;
use std::time::Duration;

use anyhow::Result;
use log::{debug, info, warn};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::Mutex;

use tiger_network::peer::{PeerCommand, PeerManager};
use tiger_network::transport::{Identity, Session};
use tiger_network::types::{
    decode_message, encode_message, NetworkConfig, PeerID, PeerMessage,
};

use crate::node::TigerNode;

/// How often a peer is sent a ping.
const PING_INTERVAL: Duration = Duration::from_secs(15);
/// How long to wait before retrying an unreachable bootnode.
const DIAL_RETRY: Duration = Duration::from_secs(30);
/// How many blocks to ask for in one bulk request.
const SYNC_BATCH: u64 = 64;
/// How long to wait for a frame before sending a ping, so a silent peer is
/// detected rather than held open forever.
const READ_TIMEOUT: Duration = Duration::from_secs(60);

/// The node's networking state.
pub struct P2pService {
    manager: Arc<Mutex<PeerManager>>,
    node: Arc<TigerNode>,
}

impl P2pService {
    /// Build the service and start listening and dialling.
    ///
    /// Returns `Ok(None)` when the listen address cannot be bound. A node that
    /// cannot accept peers can still follow the chain and serve RPC, so this
    /// is reported rather than fatal.
    pub async fn start(node: Arc<TigerNode>) -> Result<Option<Arc<Self>>> {
        let listen = node.config().listen_addr.clone();
        let boot_nodes = node.config().boot_nodes.clone();
        let max_peers = node.config().max_peers;

        let identity = load_or_create_identity(&node);
        info!("P2P identity: {}", identity.peer_id());

        let network_config = NetworkConfig {
            listen_addr: listen.clone(),
            max_peers,
            bootstrap_nodes: boot_nodes.clone(),
            discovery_enabled: !boot_nodes.is_empty(),
            ..NetworkConfig::default()
        };
        let manager = Arc::new(Mutex::new(PeerManager::new(
            network_config,
            identity.clone(),
            node.config().chain_id,
        )));

        let listener = match TcpListener::bind(normalize_listen_addr(&listen)).await {
            Ok(listener) => listener,
            Err(e) => {
                warn!("P2P listener cannot bind {listen}: {e}; the node will not accept peers");
                return Ok(None);
            }
        };
        info!("P2P listening on {listen}");

        let service = Arc::new(Self {
            manager,
            node: Arc::clone(&node),
        });

        // Accept loop.
        {
            let service = Arc::clone(&service);
            let identity = identity.clone();
            tokio::spawn(async move {
                loop {
                    if !service.node.is_running().await {
                        break;
                    }
                    match listener.accept().await {
                        Ok((stream, address)) => {
                            let service = Arc::clone(&service);
                            let identity = identity.clone();
                            tokio::spawn(async move {
                                match service
                                    .handle_inbound(stream, identity, address.to_string())
                                    .await
                                {
                                    Ok(()) => debug!("inbound peer {address} disconnected"),
                                    Err(e) => debug!("inbound peer {address} refused: {e}"),
                                }
                            });
                        }
                        Err(e) => {
                            warn!("P2P accept failed: {e}");
                            tokio::time::sleep(Duration::from_millis(200)).await;
                        }
                    }
                }
            });
        }

        // Bootnode dialler.
        if !boot_nodes.is_empty() {
            let service = Arc::clone(&service);
            let identity = identity.clone();
            tokio::spawn(async move {
                loop {
                    if !service.node.is_running().await {
                        break;
                    }
                    let full = {
                        let manager = service.manager.lock().await;
                        !manager.has_capacity()
                    };
                    if !full {
                        for boot in &boot_nodes {
                            if let Err(e) = service.dial(boot, &identity).await {
                                debug!("cannot reach bootnode {boot}: {e}");
                            }
                        }
                    }
                    tokio::time::sleep(DIAL_RETRY).await;
                }
            });
        }

        // Liveness sweep: drop peers that have gone quiet.
        {
            let service = Arc::clone(&service);
            tokio::spawn(async move {
                let mut interval = tokio::time::interval(Duration::from_secs(10));
                loop {
                    interval.tick().await;
                    if !service.node.is_running().await {
                        break;
                    }
                    let stale = {
                        let mut manager = service.manager.lock().await;
                        manager.prune_stale()
                    };
                    for id in stale {
                        debug!("dropping silent peer {id}");
                    }
                }
            });
        }

        Ok(Some(service))
    }

    /// The number of connected peers.
    pub async fn peer_count(&self) -> usize {
        self.manager.lock().await.peer_count()
    }

    /// A snapshot of the connected peers.
    pub async fn peers(&self) -> Vec<tiger_network::types::Peer> {
        self.manager.lock().await.snapshot()
    }

    /// Tell every peer about a block this node produced.
    pub async fn announce_block(&self, block: tiger_blockchain::types::Block) {
        let message = PeerMessage::NewBlock(Box::new(block));
        match encode_message(&message) {
            Ok(frame) => {
                let manager = self.manager.lock().await;
                let sent = manager.broadcast(frame);
                debug!("announced a block to {sent} peer(s)");
            }
            Err(e) => warn!("cannot encode a block announcement: {e}"),
        }
    }

    /// Tell every peer about a transaction this node accepted.
    pub async fn announce_transaction(&self, hash: &str) {
        let message = PeerMessage::NewTransaction(hash.to_string());
        match encode_message(&message) {
            Ok(frame) => {
                let manager = self.manager.lock().await;
                let sent = manager.broadcast(frame);
                debug!("announced transaction {hash} to {sent} peer(s)");
            }
            Err(e) => warn!("cannot encode a transaction announcement: {e}"),
        }
    }

    /// Dial a bootnode and serve the connection.
    async fn dial(&self, address: &str, identity: &Identity) -> Result<()> {
        let stream = TcpStream::connect(address).await?;
        let session = Session::connect(stream, identity, None).await?;
        self.run_session(session, address.to_string()).await
    }

    /// Accept an inbound connection.
    async fn handle_inbound(
        &self,
        stream: TcpStream,
        identity: Identity,
        address: String,
    ) -> Result<()> {
        {
            let manager = self.manager.lock().await;
            if !manager.has_capacity() {
                return Err(anyhow::anyhow!("the peer limit has been reached"));
            }
        }
        let session = Session::accept(stream, &identity).await?;
        self.run_session(session, address).await
    }

    /// Register a session, exchange status, then drive the connection.
    async fn run_session(&self, session: Session, address: String) -> Result<()> {
        let peer_id = session.peer_id.clone();

        let (connection, mut commands) = {
            let mut manager = self.manager.lock().await;
            manager.register(&session, address.clone())?
        };

        // Announce ourselves so the peer can tell whether it is behind.
        let (height, head_hash) = head_status(&self.node).await;
        let status = PeerMessage::Status {
            chain_id: self.node.config().chain_id,
            height,
            head_hash,
        };
        {
            let peer = connection.lock().await;
            peer.send_frame(encode_message(&status)?).await?;
        }

        info!("peer {peer_id} connected at {address}");
        let result = self.serve(session, &connection, &mut commands, &peer_id).await;

        {
            let mut manager = self.manager.lock().await;
            manager.remove(&peer_id);
        }
        let _ = connection;
        result
    }

    /// Read and act on messages from one peer until the connection ends.
    ///
    /// The socket and the outbound queue are both driven here, in one task, so
    /// the transport's send and receive counters are only ever touched by the
    /// task that owns them.
    async fn serve(
        &self,
        mut session: Session,
        connection: &Arc<Mutex<tiger_network::peer::PeerConnection>>,
        commands: &mut tokio::sync::mpsc::Receiver<PeerCommand>,
        peer_id: &PeerID,
    ) -> Result<()> {
        let mut ping_interval = tokio::time::interval(PING_INTERVAL);
        ping_interval.tick().await;

        loop {
            tokio::select! {
                // Frames from the peer.
                frame = tokio::time::timeout(READ_TIMEOUT, session.recv()) => {
                    let frame = match frame {
                        Ok(Ok(frame)) => frame,
                        Ok(Err(e)) => return Err(anyhow::anyhow!("{e}")),
                        Err(_) => {
                            // Nothing for a minute: ask for proof of life and
                            // let the liveness sweep decide.
                            let ping = encode_message(&PeerMessage::Ping(now_seconds()))?;
                            let peer = connection.lock().await;
                            if !peer.try_send_frame(ping) {
                                return Ok(());
                            }
                            continue;
                        }
                    };

                    self.manager.lock().await.mark_seen(peer_id);

                    let message = match decode_message(&frame) {
                        Ok(message) => message,
                        Err(e) => {
                            warn!("peer {peer_id} sent an unreadable message: {e}");
                            continue;
                        }
                    };
                    self.handle_message(connection, peer_id, message).await;
                }

                // Frames this node wants to send to the peer.
                command = commands.recv() => {
                    match command {
                        Some(PeerCommand::Send(frame)) => {
                            if let Err(e) = session.send(&frame).await {
                                debug!("cannot send to peer {peer_id}: {e}");
                                return Ok(());
                            }
                        }
                        Some(PeerCommand::Close) | None => return Ok(()),
                    }
                }

                _ = ping_interval.tick() => {
                    let ping = encode_message(&PeerMessage::Ping(now_seconds()))?;
                    let peer = connection.lock().await;
                    peer.try_send_frame(ping);
                }
            }
        }
    }

    /// Act on one message from a peer.
    async fn handle_message(
        &self,
        connection: &Arc<Mutex<tiger_network::peer::PeerConnection>>,
        peer_id: &PeerID,
        message: PeerMessage,
    ) {
        match message {
            PeerMessage::Status {
                chain_id,
                height,
                head_hash,
            } => {
                if chain_id != self.node.config().chain_id {
                    warn!(
                        "peer {peer_id} is on chain {chain_id}, not {}; disconnecting",
                        self.node.config().chain_id
                    );
                    let _ = connection.lock().await.close();
                    return;
                }
                {
                    let mut manager = self.manager.lock().await;
                    manager.record_status(peer_id, height, head_hash);
                }
                // A peer claiming a taller chain is asked for the blocks this
                // node is missing.
                self.request_missing_blocks(connection, height).await;
            }

            PeerMessage::NewBlock(block) => {
                let number = block.header.number;
                match self.node.import_block(*block).await {
                    Ok(true) => info!("imported block #{number} announced by {peer_id}"),
                    Ok(false) => debug!("block #{number} from {peer_id} was already known"),
                    Err(e) => {
                        warn!("rejected block #{number} from {peer_id}: {e}");
                        // A peer that sends a block this node cannot accept is
                        // not necessarily malicious, so the connection is kept
                        // and the block is simply dropped.
                    }
                }
            }

            PeerMessage::NewTransaction(hash) => {
                // An announcement carries only the hash. If the body is not
                // already in the pool, ask for it; a peer that announces a
                // transaction it cannot produce is simply ignored.
                let known = {
                    let mempool = self.node.get_mempool().await;
                    mempool.get(&hash).is_some()
                };
                if !known {
                    if let Ok(frame) = encode_message(&PeerMessage::GetTransaction(hash)) {
                        let peer = connection.lock().await;
                        peer.try_send_frame(frame);
                    }
                }
            }

            PeerMessage::GetTransaction(hash) => {
                let tx = {
                    let mempool = self.node.get_mempool().await;
                    mempool.get(&hash).cloned()
                };
                if let Some(tx) = tx {
                    if let Ok(frame) =
                        encode_message(&PeerMessage::Transaction(Box::new(tx)))
                    {
                        let peer = connection.lock().await;
                        peer.try_send_frame(frame);
                    }
                }
            }

            PeerMessage::Transaction(tx) => {
                // A transaction from a peer is held to the same standard as one
                // submitted over RPC: the sender is recovered from the
                // signature and the derived fields are recomputed.
                match crate::node::validate_transaction(&tx, self.node.config().chain_id) {
                    Ok(tx) => {
                        let hash = tx.hash.clone();
                        let mut mempool = self.node.get_mempool().await;
                        match mempool.add(tx) {
                            Ok(()) => {
                                debug!("accepted transaction {hash} from {peer_id}");
                                // Pass it on, so it reaches the rest of the
                                // network even if this node never proposes a
                                // block.
                                drop(mempool);
                                let announce = PeerMessage::NewTransaction(hash);
                                if let Ok(frame) = encode_message(&announce) {
                                    let manager = self.manager.lock().await;
                                    manager.broadcast(frame);
                                }
                            }
                            Err(e) => debug!("refused transaction from {peer_id}: {e}"),
                        }
                    }
                    Err(e) => warn!("peer {peer_id} sent an invalid transaction: {e}"),
                }
            }

            PeerMessage::GetBlock(number) => {
                let block = {
                    let cm = self.node.get_chain_manager().await;
                    cm.chain().get_block(number).cloned()
                };
                if let Some(block) = block {
                    if let Ok(frame) = encode_message(&PeerMessage::Block(Box::new(block))) {
                        // `try_send_frame`, not `send_frame`: this runs inside
                        // the same task that drains the queue, so waiting for
                        // room here would deadlock.
                        let peer = connection.lock().await;
                        peer.try_send_frame(frame);
                    }
                }
            }

            PeerMessage::Block(block) => {
                let number = block.header.number;
                match self.node.import_block(*block).await {
                    Ok(true) => info!("imported block #{number} from {peer_id}"),
                    Ok(false) => debug!("block #{number} from {peer_id} was already known"),
                    Err(e) => warn!("rejected block #{number} from {peer_id}: {e}"),
                }
            }

            PeerMessage::GetBlocks { from, count } => {
                let blocks = {
                    let cm = self.node.get_chain_manager().await;
                    let chain = cm.chain();
                    let head = chain.height();
                    let end = from.saturating_add(count.min(SYNC_BATCH)).min(head + 1);
                    (from..end)
                        .filter_map(|n| chain.get_block(n).cloned())
                        .collect::<Vec<_>>()
                };
                if !blocks.is_empty() {
                    if let Ok(frame) = encode_message(&PeerMessage::Blocks(blocks)) {
                        let peer = connection.lock().await;
                        peer.try_send_frame(frame);
                    }
                }
            }

            PeerMessage::Blocks(blocks) => {
                // A batch is applied in order, and the first block that does
                // not fit stops the batch: a gap means the rest cannot be
                // applied either, and asking for them again is cheaper than
                // buffering them.
                for block in blocks {
                    let number = block.header.number;
                    match self.node.import_block(block).await {
                        Ok(true) => debug!("imported block #{number} from a batch"),
                        Ok(false) => {}
                        Err(e) => {
                            debug!("stopping a block batch at #{number}: {e}");
                            break;
                        }
                    }
                }
            }

            PeerMessage::Ping(nonce) => {
                if let Ok(frame) = encode_message(&PeerMessage::Pong(nonce)) {
                    let peer = connection.lock().await;
                    peer.try_send_frame(frame);
                }
            }

            PeerMessage::Pong(_) => {
                // Any frame refreshes liveness, which `mark_seen` already did.
            }

            PeerMessage::Disconnect(reason) => {
                info!("peer {peer_id} asked to disconnect: {reason}");
                let _ = connection.lock().await.close();
            }
        }
    }

    /// Ask a peer for the blocks this node is missing.
    async fn request_missing_blocks(
        &self,
        connection: &Arc<Mutex<tiger_network::peer::PeerConnection>>,
        peer_height: u64,
    ) {
        let our_height = {
            let cm = self.node.get_chain_manager().await;
            cm.chain().height()
        };
        if peer_height <= our_height {
            return;
        }
        let request = PeerMessage::GetBlocks {
            from: our_height + 1,
            count: (peer_height - our_height).min(SYNC_BATCH),
        };
        if let Ok(frame) = encode_message(&request) {
            let peer = connection.lock().await;
            peer.try_send_frame(frame);
        }
    }
}

/// The node's head height and block hash, as reported in a status message.
async fn head_status(node: &Arc<TigerNode>) -> (u64, String) {
    let cm = node.get_chain_manager().await;
    let chain = cm.chain();
    let height = chain.height();
    let hash = chain
        .get_block(height)
        .and_then(|block| tiger_blockchain::block_hash(&block.header).ok())
        .unwrap_or_default();
    (height, hash)
}

/// Load the node's identity from its data directory, or create one.
///
/// A stable identity is what lets an operator recognise the node, so it is
/// persisted; if the file cannot be written the node still runs, with an
/// identity that changes on restart.
fn load_or_create_identity(node: &Arc<TigerNode>) -> Identity {
    let path = std::path::Path::new(node.config().data_dir.as_str()).join("p2p-identity.key");
    if let Ok(bytes) = std::fs::read(&path) {
        if let Some(identity) = Identity::from_secret_bytes(&bytes) {
            return identity;
        }
        warn!("{} is not a valid identity; generating a new one", path.display());
    }

    let identity = Identity::generate();
    if let Some(parent) = path.parent() {
        if let Err(e) = std::fs::create_dir_all(parent) {
            warn!("cannot create {}: {e}", parent.display());
            return identity;
        }
    }
    if let Err(e) = std::fs::write(&path, identity.secret_bytes()) {
        warn!("cannot persist the P2P identity to {}: {e}", path.display());
    }
    identity
}

/// Turn a listen string into something `TcpListener::bind` accepts.
///
/// The configuration uses `:30303` for "every interface", which the standard
/// library does not accept; it needs a host part.
fn normalize_listen_addr(addr: &str) -> String {
    if let Some(port) = addr.strip_prefix(':') {
        return format!("0.0.0.0:{port}");
    }
    addr.to_string()
}

/// Seconds since the epoch, for ping nonces.
fn now_seconds() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}
