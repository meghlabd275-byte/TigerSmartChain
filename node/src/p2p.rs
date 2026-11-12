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
/// How often the catch-up driver re-checks whether this node is behind.
const SYNC_INTERVAL: Duration = Duration::from_secs(3);
/// How long to wait for a frame before sending a ping, so a silent peer is
/// detected rather than held open forever.
const READ_TIMEOUT: Duration = Duration::from_secs(60);
/// The most connections one address may hold at once.
///
/// `max_peers` alone bounds the node but not the damage: one address opening
/// every slot crowds out the honest peers, which is the cheap form of an
/// eclipse. A handful of connections from one address is what a real peer
/// needs (a home router, a second node behind the same NAT), so the ceiling
/// sits just above that.
const MAX_CONNECTIONS_PER_IP: usize = 8;
/// The most inbound connections accepted from one address per second.
///
/// The per-address connection cap bounds *held* connections; without a rate
/// limit a peer can still churn them, forcing a handshake (two signatures) for
/// every connect-and-drop. The bucket is small because a legitimate peer
/// reconnects rarely, and a reconnecting client that trips it is delayed, not
/// banned.
const ACCEPT_BURST_PER_IP: f64 = 4.0;
/// How many handshakes may be in progress at once.
///
/// A handshake costs two signatures and a key exchange. Bounding how many run
/// concurrently keeps a burst of half-open connections from spending the
/// node's CPU before any of them has proven it can complete.
const MAX_CONCURRENT_HANDSHAKES: usize = 16;
/// How long a single inbound handshake may hold a slot before it is dropped.
/// `Session::accept` has its own read timeout; this is the outer bound.
const HANDSHAKE_SLOT_TIMEOUT: Duration = Duration::from_secs(15);

/// Admission control for inbound connections.
///
/// The peer count is bounded, but that bound alone is not enough: one address
/// can hold every slot, and a peer that connects and immediately drops forces
/// a full handshake each time. This tracks how many connections each address
/// holds and how fast it opens them, so a flood is refused before a handshake
/// is attempted rather than after.
struct Admission {
    /// Connections currently held, per address.
    held: std::sync::Mutex<std::collections::HashMap<std::net::IpAddr, usize>>,
    /// Accept-rate token buckets, per address.
    buckets: std::sync::Mutex<std::collections::HashMap<std::net::IpAddr, Bucket>>,
    /// Handshakes in progress, to bound the work a burst can demand.
    handshakes: tokio::sync::Semaphore,
}

/// A refilling token bucket for inbound accepts.
struct Bucket {
    tokens: f64,
    last: std::time::Instant,
}

impl Admission {
    fn new() -> Self {
        Self {
            held: std::sync::Mutex::new(std::collections::HashMap::new()),
            buckets: std::sync::Mutex::new(std::collections::HashMap::new()),
            handshakes: tokio::sync::Semaphore::new(MAX_CONCURRENT_HANDSHAKES),
        }
    }

    /// Whether a connection from `ip` may proceed, recording it if so.
    ///
    /// Both checks run under one decision so a refused connection never leaves
    /// a count behind: the rate limit is consulted first, then the held count,
    /// and only a connection that passes both is recorded.
    fn admit(&self, ip: std::net::IpAddr) -> bool {
        let now = std::time::Instant::now();
        let mut buckets = self.buckets.lock().unwrap_or_else(|e| e.into_inner());
        if buckets.len() > 100_000 {
            buckets.retain(|_, b| now.duration_since(b.last).as_secs() < 60);
        }
        let bucket = buckets.entry(ip).or_insert(Bucket {
            tokens: ACCEPT_BURST_PER_IP,
            last: now,
        });
        let elapsed = now.duration_since(bucket.last).as_secs_f64();
        bucket.tokens = (bucket.tokens + elapsed).min(ACCEPT_BURST_PER_IP);
        bucket.last = now;
        if bucket.tokens < 1.0 {
            return false;
        }

        let mut held = self.held.lock().unwrap_or_else(|e| e.into_inner());
        let count = held.entry(ip).or_insert(0);
        if *count >= MAX_CONNECTIONS_PER_IP {
            return false;
        }
        *count += 1;
        // Only spend the token once the connection is certain to be counted.
        bucket.tokens -= 1.0;
        true
    }

    /// Release a connection slot for `ip`.
    fn release(&self, ip: std::net::IpAddr) {
        let mut held = self.held.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(count) = held.get_mut(&ip) {
            *count = count.saturating_sub(1);
            if *count == 0 {
                held.remove(&ip);
            }
        }
    }
}

/// The node's networking state.
pub struct P2pService {
    manager: Arc<Mutex<PeerManager>>,
    node: Arc<TigerNode>,
    identity: Identity,
    admission: Arc<Admission>,
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
            identity: identity.clone(),
            admission: Arc::new(Admission::new()),
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
                            // Decide before spawning: a refused connection is
                            // closed without a task or a handshake, so a flood
                            // costs the node almost nothing.
                            if !service.admission.admit(address.ip()) {
                                debug!("refusing connection from {address}: admission limit");
                                drop(stream);
                                continue;
                            }
                            let service = Arc::clone(&service);
                            let identity = identity.clone();
                            let admission = Arc::clone(&service.admission);
                            let ip = address.ip();
                            tokio::spawn(async move {
                                // Bound concurrent handshakes so a burst of
                                // half-open connections cannot spend the CPU
                                // before any has proven it can complete.
                                let permit = match tokio::time::timeout(
                                    HANDSHAKE_SLOT_TIMEOUT,
                                    admission.handshakes.acquire(),
                                )
                                .await
                                {
                                    Ok(Ok(permit)) => permit,
                                    _ => {
                                        debug!("refusing connection from {address}: handshake queue full");
                                        admission.release(ip);
                                        return;
                                    }
                                };
                                let result = service
                                    .handle_inbound(stream, identity, address.to_string())
                                    .await;
                                drop(permit);
                                admission.release(ip);
                                match result {
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

        // Catch-up driver. The status handler asks for blocks once, when a
        // peer reports its height; if that reply is lost, or the node is
        // restarted behind the head, nothing would ask again. This loop keeps
        // the node moving toward the tallest peer until it reaches it.
        {
            let service = Arc::clone(&service);
            tokio::spawn(async move {
                let mut interval = tokio::time::interval(SYNC_INTERVAL);
                interval.tick().await;
                loop {
                    interval.tick().await;
                    if !service.node.is_running().await {
                        break;
                    }
                    let our_height = {
                        let cm = service.node.get_chain_manager().await;
                        cm.chain().height()
                    };
                    let peers = {
                        let manager = service.manager.lock().await;
                        manager.peers_ahead(our_height, 3)
                    };
                    if peers.is_empty() {
                        // Nothing is ahead, so this node is at the head.
                        service
                            .node
                            .set_syncing(false, our_height, our_height)
                            .await;
                        continue;
                    }
                    let mut highest = our_height;
                    for peer in peers {
                        let peer_height = {
                            let peer = peer.lock().await;
                            peer.height
                        };
                        if peer_height <= our_height {
                            continue;
                        }
                        highest = highest.max(peer_height);
                        service
                            .request_blocks(&peer, our_height + 1, peer_height - our_height)
                            .await;
                    }
                    // Report the gap so eth_syncing and the metrics endpoint
                    // describe what is actually happening.
                    service
                        .node
                        .set_syncing(true, our_height, highest)
                        .await;
                }
            });
        }

        Ok(Some(service))
    }

    /// This node's own peer id, as a hex string.
    pub fn peer_id(&self) -> String {
        self.identity.peer_id().to_string()
    }

    /// The number of connected peers.
    pub async fn peer_count(&self) -> usize {
        self.manager.lock().await.peer_count()
    }

    /// The greatest height any connected peer claims to have reached.
    ///
    /// This is the network's view of the chain head, which is what makes
    /// `eth_syncing` meaningful: this node is behind when that number is above
    /// its own height.
    pub async fn best_peer_height(&self) -> Option<u64> {
        let manager = self.manager.lock().await;
        manager.snapshot().iter().map(|peer| peer.height).max()
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
                                // Notify subscribers, then pass it on so it
                                // reaches the rest of the network even if this
                                // node never proposes a block.
                                self.node.publish_pending_transaction(&hash);
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
                let before = {
                    let cm = self.node.get_chain_manager().await;
                    cm.chain().height()
                };
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
                // A full batch means the peer has more. Asking again straight
                // away is what turns one batch per status into a catch-up that
                // runs to the head. The request is only repeated when the
                // batch actually advanced this node: if nothing imported, the
                // next request would be for the same range and would spin.
                let after = {
                    let cm = self.node.get_chain_manager().await;
                    cm.chain().height()
                };
                if after > before {
                    let peer_height = {
                        let manager = self.manager.lock().await;
                        manager.peer_height(&peer_id)
                    };
                    if let Some(peer_height) = peer_height {
                        if peer_height > after {
                            self.request_blocks(connection, after + 1, peer_height - after)
                                .await;
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

    /// Ask a peer for a batch of blocks starting at `from`.
    ///
    /// The count is capped at one batch: a peer that is asked for its whole
    /// chain would have to build a reply too large to send, and a batch that
    /// is applied triggers the next request, so catch-up still runs to
    /// completion.
    async fn request_blocks(
        &self,
        connection: &Arc<Mutex<tiger_network::peer::PeerConnection>>,
        from: u64,
        count: u64,
    ) {
        let request = PeerMessage::GetBlocks {
            from,
            count: count.min(SYNC_BATCH).max(1),
        };
        if let Ok(frame) = encode_message(&request) {
            let peer = connection.lock().await;
            peer.try_send_frame(frame);
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
        self.request_blocks(connection, our_height + 1, peer_height - our_height)
            .await;
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
    // The identity is a private key. Writing it with the process umask would
    // usually leave it readable by every account on the host, which lets
    // anyone impersonate this node to its peers.
    if let Err(e) = write_private_file(&path, &identity.secret_bytes()) {
        warn!("cannot persist the P2P identity to {}: {e}", path.display());
    }
    identity
}

/// Write a secret to `path` with owner-only permissions.
///
/// The file is created with mode 0600 from the start rather than written and
/// then narrowed, so there is no window in which the secret is readable by
/// others.
fn write_private_file(path: &std::path::Path, contents: &[u8]) -> std::io::Result<()> {
    use std::io::Write as _;

    let mut options = std::fs::OpenOptions::new();
    options.write(true).create(true).truncate(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt as _;
        options.mode(0o600);
    }
    let mut file = options.open(path)?;
    file.write_all(contents)?;
    file.sync_all()?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt as _;
        // `mode` on `open` only applies when the file is created; a file that
        // already existed keeps its old permissions, so they are set again.
        file.set_permissions(std::fs::Permissions::from_mode(0o600))?;
    }
    Ok(())
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

#[cfg(test)]
mod tests {
    use super::*;

    fn ip(s: &str) -> std::net::IpAddr {
        s.parse().unwrap()
    }

    /// One address must not be able to hold every peer slot. Without this the
    /// cheapest way to keep honest peers out is to open connections from a
    /// single address until the node is full.
    #[test]
    fn one_address_cannot_hold_every_connection_slot() {
        let admission = Admission::new();
        let attacker = ip("198.51.100.1");

        // The burst allowance is smaller than the connection cap, so filling
        // the cap takes several rounds spread over time; the cap is what is
        // asserted here, so the rate limit is drained first.
        let mut admitted = 0;
        for _ in 0..(MAX_CONNECTIONS_PER_IP + 4) {
            // Spend any available tokens so the rate limit does not mask the
            // cap, then check the cap directly.
            while admission.admit(attacker) {
                admitted += 1;
                if admitted > MAX_CONNECTIONS_PER_IP + 2 {
                    break;
                }
            }
            std::thread::sleep(std::time::Duration::from_millis(300));
        }
        assert!(
            admitted <= MAX_CONNECTIONS_PER_IP,
            "one address held {admitted} connections, above the cap of {MAX_CONNECTIONS_PER_IP}"
        );
    }

    /// The connection cap counts what is held, so releasing frees a slot.
    #[test]
    fn a_released_slot_becomes_available_again() {
        let admission = Admission::new();
        let addr = ip("203.0.113.50");

        // Admit up to the cap, working around the accept rate limit.
        let mut held = 0;
        while held < MAX_CONNECTIONS_PER_IP {
            if admission.admit(addr) {
                held += 1;
            } else {
                std::thread::sleep(std::time::Duration::from_millis(300));
            }
        }
        assert_eq!(held, MAX_CONNECTIONS_PER_IP);

        admission.release(addr);
        // The accept rate limit still applies, so wait for a token before
        // asserting the freed slot can be used.
        let deadline = std::time::Instant::now() + Duration::from_secs(5);
        let mut readmitted = false;
        while std::time::Instant::now() < deadline {
            if admission.admit(addr) {
                readmitted = true;
                break;
            }
            std::thread::sleep(Duration::from_millis(100));
        }
        assert!(readmitted, "a released slot must be reusable");
    }

    /// A flood of connects from one address is throttled rather than admitted
    /// one-for-one.
    #[test]
    fn a_connect_flood_from_one_address_is_throttled() {
        let admission = Admission::new();
        let attacker = ip("192.0.2.9");
        let mut admitted = 0;
        for _ in 0..1000 {
            if admission.admit(attacker) {
                admitted += 1;
            }
        }
        assert!(
            (admitted as f64) <= ACCEPT_BURST_PER_IP + 1.0,
            "an instantaneous flood admitted {admitted}, above the burst allowance"
        );
    }

    /// Each address has its own budget, so one flood does not lock out a
    /// different peer.
    #[test]
    fn addresses_do_not_share_a_budget() {
        let admission = Admission::new();
        let attacker = ip("192.0.2.10");
        let honest = ip("192.0.2.11");

        for _ in 0..100 {
            admission.admit(attacker);
        }
        assert!(
            admission.admit(honest),
            "a flood from one address must not deny a different address"
        );
    }

    /// The node's P2P identity is a private key. It must not be readable by
    /// other accounts on the host, or anyone with a shell could impersonate
    /// the node to its peers.
    #[cfg(unix)]
    #[test]
    fn the_p2p_identity_is_written_owner_only() {
        use std::os::unix::fs::PermissionsExt as _;

        let dir = std::env::temp_dir().join(format!("tiger-p2p-perm-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("p2p-identity.key");

        write_private_file(&path, &[0x11u8; 32]).unwrap();
        let mode = std::fs::metadata(&path).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o600, "the identity file must be owner-only");

        // A file that already existed with looser permissions must be
        // tightened rather than left as it was found.
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o644)).unwrap();
        write_private_file(&path, &[0x22u8; 32]).unwrap();
        let mode = std::fs::metadata(&path).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o600, "an existing file must be tightened");
        assert_eq!(
            std::fs::read(&path).unwrap(),
            vec![0x22u8; 32],
            "the file must hold the bytes that were written"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }
}
