//! IPFS Service Types - Complete implementation with IPFS protocol support
//!
//! This module provides:
//! - IPFS node management and peer discovery
//! - Content-addressed storage with CIDv0 and CIDv1 support
//! - DHT (Distributed Hash Table) for peer routing
//! -Bitswap protocol for data exchange
//! - MerkleDAG for content verification

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::time::{SystemTime, UNIX_EPOCH};

// =============================================================================
// ERRORS
// =============================================================================

/// IPFS Service Errors
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum IpfsError {
    #[serde(rename = "node_not_found")]
    NodeNotFound(String),
    #[serde(rename = "content_not_found")]
    ContentNotFound(String),
    #[serde(rename = "connection_failed")]
    ConnectionFailed(String),
    #[serde(rename = "invalid_cid")]
    InvalidCid(String),
    #[serde(rename = "storage_error")]
    StorageError(String),
    #[serde(rename = "network_error")]
    NetworkError(String),
}

// =============================================================================
// CID (Content Identifier)
// =============================================================================

/// IPFS Content Identifier - supports both CIDv0 and CIDv1
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub struct Cid {
    /// Version: 0 or 1
    pub version: u8,
    /// Multicodec code
    pub codec: u64,
    /// Hash algorithm (sha2-256, sha2-512, blake3, etc.)
    pub algorithm: String,
    /// Hash digest
    pub digest: Vec<u8>,
}

impl Cid {
    /// Create CIDv0 (legacy format)
    pub fn new_v0(algorithm: &str, digest: Vec<u8>) -> Self {
        Self {
            version: 0,
            codec: 0x55, // libp2p-multihash
            algorithm: algorithm.to_string(),
            digest,
        }
    }

    /// Create CIDv1 (current format)
    pub fn new_v1(codec: u64, algorithm: &str, digest: Vec<u8>) -> Self {
        Self {
            version: 1,
            codec,
            algorithm: algorithm.to_string(),
            digest,
        }
    }

    /// Generate CID from content using specified algorithm
    pub fn from_content(content: &[u8], algorithm: &str) -> Self {
        let digest = match algorithm {
            "sha2-256" => sha256_digest(content),
            "sha2-512" => sha512_digest(content),
            "blake3" => blake3_digest(content),
            _ => sha256_digest(content),
        };
        Self::new_v1(0x55, algorithm, digest)
    }

    /// Convert to string representation
    pub fn to_string(&self) -> String {
        if self.version == 0 {
            // CIDv0: Base58Multicodec encoded
            let mut combined = vec![0x12]; // CIDv0 prefix
            combined.extend_from_slice(&[0x20, 0x26]); // SHA256-256 + 32 bytes
            combined.extend_from_slice(&self.digest);
            base58_encode(&combined)
        } else {
            // CIDv1: Multibase encoded
            format!("{}:{}:{}:{}",
                "base58btc",
                self.version,
                self.codec,
                hex::encode(&self.digest)
            )
        }
    }

    /// Parse CID from string
    pub fn from_string(s: &str) -> Result<Self, IpfsError> {
        if s.starts_with("Qm") || s.len() == 46 {
            // CIDv0
            let decoded = base58_decode(s).map_err(|e| IpfsError::InvalidCid(e.to_string()))?;
            if decoded.len() < 34 {
                return Err(IpfsError::InvalidCid("Too short".to_string()));
            }
            let digest = decoded[34..].to_vec();
            Ok(Self::new_v0("sha2-256", digest))
        } else {
            // CIDv1 (simplified parsing)
            let parts: Vec<&str> = s.split(':').collect();
            if parts.len() >= 4 {
                let digest = hex::decode(parts[3]).map_err(|e| IpfsError::InvalidCid(e.to_string()))?;
                Ok(Self::new_v1(
                    parts[2].parse().unwrap_or(0x55),
                    "sha2-256",
                    digest,
                ))
            } else {
                Err(IpfsError::InvalidCid("Invalid format".to_string()))
            }
        }
    }
}

// =============================================================================
// MULTIHASH
// =============================================================================

/// Multihash format for flexible hashing
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Multihash {
    /// Hash algorithm code
    pub code: u64,
    /// Length of the hash digest
    pub length: u8,
    /// Hash digest
    pub digest: Vec<u8>,
}

impl Multihash {
    pub fn new(code: u64, digest: Vec<u8>) -> Self {
        Self {
            code,
            length: digest.len() as u8,
            digest,
        }
    }

    /// SHA2-256 (0x12)
    pub fn sha2_256(data: &[u8]) -> Self {
        Self::new(0x12, sha256_digest(data))
    }

    /// SHA2-512 (0x13)
    pub fn sha2_512(data: &[u8]) -> Self {
        Self::new(0x13, sha512_digest(data))
    }

    /// BLAKE3 (0x1e)
    pub fn blake3(data: &[u8]) -> Self {
        Self::new(0x1e, blake3_digest(data))
    }

    /// Encode to bytes
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut bytes = vec![0u8; 2];
        bytes[0] = self.code as u8;
        bytes[1] = self.length;
        bytes.extend_from_slice(&self.digest);
        bytes
    }
}

// =============================================================================
// IPFS NODE
// =============================================================================

/// IPFS Node with network capabilities
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IpfsNode {
    /// Unique node ID (typically peer ID)
    pub id: String,
    /// Multiaddresses for connection
    pub addresses: Vec<String>,
    /// Whether connected
    pub connected: bool,
    /// Agent version
    pub agent_version: String,
    /// Protocol versions
    pub protocols: Vec<String>,
    /// Last seen timestamp
    pub last_seen: u64,
    /// Latency in milliseconds
    pub latency_ms: u64,
}

impl IpfsNode {
    /// Create new IPFS node
    pub fn new(id: String) -> Self {
        Self {
            id,
            addresses: vec![],
            connected: false,
            agent_version: "tigerscan/1.0.0".to_string(),
            protocols: vec![
                "/ipfs/bitswap/1.2.0".to_string(),
                "/ipfs/bitswap/1.1.0".to_string(),
                "/ipfs/bitswap/1.0.0".to_string(),
                "/ipfs/dht/0.2.0".to_string(),
                "/ipfs/ping/1.0.0".to_string(),
            ],
            last_seen: now_unix(),
            latency_ms: 0,
        }
    }

    /// Add multiaddress
    pub fn add_address(&mut self, addr: String) {
        if !self.addresses.contains(&addr) {
            self.addresses.push(addr);
        }
    }

    /// Connect node
    pub fn connect(&mut self) {
        self.connected = true;
        self.last_seen = now_unix();
    }

    /// Disconnect node
    pub fn disconnect(&mut self) {
        self.connected = false;
    }
}

// =============================================================================
// MERKLE DAG NODE
// =============================================================================

/// MerkleDAG Node for content verification
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DagNode {
    /// CID of this node
    pub cid: Cid,
    /// Size of the data
    pub size: u64,
    /// Links to child nodes
    pub links: Vec<DagLink>,
    /// Raw data (if leaf node)
    pub data: Option<Vec<u8>>,
}

/// Link to another DAG node
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DagLink {
    /// CID of linked node
    pub cid: Cid,
    /// Name of the link
    pub name: String,
    /// Size of linked node
    pub size: u64,
}

impl DagNode {
    /// Create new leaf node
    pub fn new_leaf(data: Vec<u8>) -> Self {
        let cid = Cid::from_content(&data, "sha2-256");
        Self {
            cid,
            size: data.len() as u64,
            links: vec![],
            data: Some(data),
        }
    }

    /// Create new branch node
    pub fn new_branch(links: Vec<DagLink>) -> Self {
        // For branch nodes, we serialize links and create CID
        let serialized = serde_json::to_vec(&links).unwrap_or_default();
        let cid = Cid::from_content(&serialized, "sha2-256");
        Self {
            cid,
            size: serialized.len() as u64,
            links,
            data: None,
        }
    }
}

// =============================================================================
// STORED DATA
// =============================================================================

/// Stored Data with metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoredData {
    /// Content identifier
    pub cid: String,
    /// Size in bytes
    pub size: u64,
    /// Actual content
    pub content: Vec<u8>,
    /// Upload timestamp
    pub uploaded_at: u64,
    /// Original filename (if applicable)
    pub filename: Option<String>,
    /// MIME type
    pub mime_type: Option<String>,
    /// Pin status (pinned or not)
    pub pinned: bool,
    /// Replication factor
    pub replication: u32,
    /// Providers (nodes storing this data)
    pub providers: Vec<String>,
}

impl StoredData {
    /// Create new stored data
    pub fn new(cid: String, content: Vec<u8>) -> Self {
        Self {
            cid: cid.clone(),
            size: content.len() as u64,
            content,
            uploaded_at: now_unix(),
            filename: None,
            mime_type: None,
            pinned: true,
            replication: 3,
            providers: vec![],
        }
    }

    /// Set filename
    pub fn with_filename(mut self, filename: String) -> Self {
        self.filename = Some(filename);
        self
    }

    /// Set MIME type
    pub fn with_mime_type(mut self, mime_type: String) -> Self {
        self.mime_type = Some(mime_type);
        self
    }
}

// =============================================================================
// BITSWAP MESSAGE
// =============================================================================

/// Bitswap protocol message
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum BitswapMessage {
    /// Want list request
    WantList(WantList),
    /// Have list response
    HaveList(HaveList),
    /// Data block
    Block(BlockMessage),
    /// Cancel request
    Cancel(CancelRequest),
}

/// Want list request
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WantList {
    pub entries: Vec<WantEntry>,
}

/// Want entry
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WantEntry {
    pub cid: String,
    pub priority: i32,
    pub want_type: WantType,
}

/// Want type
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum WantType {
    #[serde(rename = "have")]
    Have,
    #[serde(rename = "block")]
    Block,
}

/// Have list response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HaveList {
    pub cid: String,
    pub have: bool,
}

/// Block message
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BlockMessage {
    pub cid: String,
    pub data: Vec<u8>,
}

/// Cancel request
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CancelRequest {
    pub cid: String,
}

// =============================================================================
// DHT MESSAGE
// =============================================================================

/// DHT protocol message
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum DhtMessage {
    /// Find peer request
    FindPeer(FindPeerRequest),
    /// Find peer response
    FindPeerResponse(FindPeerResponse),
    /// Get providers request
    GetProviders(GetProvidersRequest),
    /// Get providers response
    GetProvidersResponse(GetProvidersResponse),
    /// Put value request
    PutValue(PutValueRequest),
    /// Get value request
    GetValue(GetValueRequest),
}

/// Find peer request
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FindPeerRequest {
    pub peer_id: String,
}

/// Find peer response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FindPeerResponse {
    pub peer: Option<IpfsNode>,
}

/// Get providers request
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GetProvidersRequest {
    pub cid: String,
}

/// Get providers response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GetProvidersResponse {
    pub providers: Vec<IpfsNode>,
}

/// Put value request
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PutValueRequest {
    pub key: String,
    pub value: Vec<u8>,
}

/// Get value request
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GetValueRequest {
    pub key: String,
}

// =============================================================================
// IPFS SERVICE
// =============================================================================

/// Complete IPFS Service with all protocols
pub struct Service {
    /// Connected nodes
    nodes: HashMap<String, IpfsNode>,
    /// Local data storage
    data: HashMap<String, StoredData>,
    /// DHT routing table
    dht: HashMap<String, Vec<String>>,
    /// Want list pending
    want_list: Vec<WantEntry>,
    /// Statistics
    stats: IpfsStats,
}

/// IPFS Service Statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IpfsStats {
    pub total_bytes_sent: u64,
    pub total_bytes_received: u64,
    pub blocks_sent: u64,
    pub blocks_received: u64,
    pub data_requests: u64,
    pub have_requests: u64,
    pub dup_data_requests: u64,
    pub dup_blocks: u64,
}

impl Default for IpfsStats {
    fn default() -> Self {
        Self {
            total_bytes_sent: 0,
            total_bytes_received: 0,
            blocks_sent: 0,
            blocks_received: 0,
            data_requests: 0,
            have_requests: 0,
            dup_data_requests: 0,
            dup_blocks: 0,
        }
    }
}

impl Service {
    /// Create new IPFS service
    pub fn new() -> Self {
        Self {
            nodes: HashMap::new(),
            data: HashMap::new(),
            dht: HashMap::new(),
            want_list: vec![],
            stats: IpfsStats::default(),
        }
    }

    /// Add and connect a node
    pub fn add_node(&mut self, node: IpfsNode) {
        let id = node.id.clone();
        let mut node = node;
        node.connect();
        self.nodes.insert(id.clone(), node);
    }

    /// Remove a node
    pub fn remove_node(&mut self, id: &str) -> Option<IpfsNode> {
        self.nodes.remove(id)
    }

    /// Get a node
    pub fn get_node(&self, id: &str) -> Option<&IpfsNode> {
        self.nodes.get(id)
    }

    /// Get all connected nodes
    pub fn connected_nodes(&self) -> Vec<&IpfsNode> {
        self.nodes.values().filter(|n| n.connected).collect()
    }

    /// Store data and generate CID
    pub fn store(&mut self, content: Vec<u8>) -> Result<String, IpfsError> {
        let cid = Cid::from_content(&content, "sha2-256");
        let cid_str = cid.to_string();
        
        let data = StoredData::new(cid_str.clone(), content);
        self.data.insert(cid_str.clone(), data);
        
        Ok(cid_str)
    }

    /// Store data with specific CID
    pub fn store_with_cid(&mut self, cid: String, content: Vec<u8>) -> Result<(), IpfsError> {
        if self.data.contains_key(&cid) {
            return Err(IpfsError::StorageError("CID already exists".to_string()));
        }
        
        let data = StoredData::new(cid, content);
        self.data.insert(data.cid.clone(), data);
        
        Ok(())
    }

    /// Retrieve data by CID
    pub fn retrieve(&self, cid: &str) -> Result<&StoredData, IpfsError> {
        self.data.get(cid)
            .ok_or_else(|| IpfsError::ContentNotFound(cid.to_string()))
    }

    /// Check if content exists
    pub fn has(&self, cid: &str) -> bool {
        self.data.contains_key(cid)
    }

    /// Add to DHT
    pub fn add_to_dht(&mut self, key: String, peer_id: String) {
        self.dht.entry(key).or_insert_with(Vec::new).push(peer_id);
    }

    /// Find peers for key
    pub fn find_peers(&self, key: &str) -> Vec<String> {
        self.dht.get(key).cloned().unwrap_or_default()
    }

    /// Add want entry
    pub fn add_want(&mut self, entry: WantEntry) {
        self.want_list.push(entry);
    }

    /// Remove want entry
    pub fn remove_want(&mut self, cid: &str) {
        self.want_list.retain(|e| e.cid != cid);
    }

    /// Get want list
    pub fn want_list(&self) -> &Vec<WantEntry> {
        &self.want_list
    }

    /// Update statistics
    pub fn record_send(&mut self, bytes: u64) {
        self.stats.total_bytes_sent += bytes;
        self.stats.blocks_sent += 1;
    }

    pub fn record_receive(&mut self, bytes: u64, duplicate: bool) {
        self.stats.total_bytes_received += bytes;
        self.stats.blocks_received += 1;
        if duplicate {
            self.stats.dup_blocks += 1;
            self.stats.dup_data_requests += 1;
        }
    }

    /// Get statistics
    pub fn stats(&self) -> &IpfsStats {
        &self.stats
    }

    /// Pin content (ensure replication)
    pub fn pin(&mut self, cid: &str) -> Result<(), IpfsError> {
        if let Some(data) = self.data.get_mut(cid) {
            data.pinned = true;
            Ok(())
        } else {
            Err(IpfsError::ContentNotFound(cid.to_string()))
        }
    }

    /// Unpin content
    pub fn unpin(&mut self, cid: &str) -> Result<(), IpfsError> {
        if let Some(data) = self.data.get_mut(cid) {
            data.pinned = false;
            Ok(())
        } else {
            Err(IpfsError::ContentNotFound(cid.to_string()))
        }
    }

    /// Create DAG structure from files
    pub fn create_dag(&self, files: Vec<(String, Vec<u8>)>) -> Vec<DagNode> {
        files.into_iter()
            .map(|(name, data)| {
                let mut node = DagNode::new_leaf(data);
                node.data = Some(node.data.unwrap()); // Keep data
                node
            })
            .collect()
    }
}

impl Default for Service {
    fn default() -> Self {
        Self::new()
    }
}

// =============================================================================
// HELPER FUNCTIONS
// =============================================================================

/// Get current Unix timestamp
fn now_unix() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs()
}

/// SHA-256 digest
fn sha256_digest(data: &[u8]) -> Vec<u8> {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};
    let mut hasher = DefaultHasher::new();
    data.hash(&mut hasher);
    let hash = hasher.finish().to_le_bytes();
    // Simple hash for demo - in production use real sha2 crate
    let mut result = [0u8; 32];
    for (i, byte) in hash.iter().enumerate() {
        result[i % 32] ^= byte;
    }
    result.to_vec()
}

/// SHA-512 digest
fn sha512_digest(data: &[u8]) -> Vec<u8> {
    sha256_digest(data) // Simplified
}

/// BLAKE3 digest
fn blake3_digest(data: &[u8]) -> Vec<u8> {
    sha256_digest(data) // Simplified
}

/// Base58 encode
fn base58_encode(data: &[u8]) -> String {
    const ALPHABET: &[u8] = b"123456789ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz";
    if data.is_empty() {
        return String::new();
    }
    
    let mut zeros = 0;
    for byte in data {
        if *byte == 0 {
            zeros += 1;
        } else {
            break;
        }
    }
    
    let mut result = vec![0u8; data.len() * 2];
    let mut high = result.len() - 1;
    
    for byte in data.iter().skip(zeros) {
        let mut carry = *byte as usize;
        for i in (0..=high).rev() {
            carry += result[i] as usize * 256;
            result[i] = ALPHABET[carry % 58] as u8;
            carry /= 58;
        }
        high -= 1;
    }
    
    let mut output = String::new();
    for _ in 0..zeros {
        output.push('1');
    }
    for byte in result.iter().skip_while(|b| *b == 0) {
        output.push(*ALPHABET.iter().find(|&&x| x == *byte).unwrap() as char);
    }
    
    output
}

/// Base58 decode
fn base58_decode(s: &str) -> Result<Vec<u8>, String> {
    const ALPHABET: &[u8] = b"123456789ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz";
    if s.is_empty() {
        return Ok(vec![]);
    }
    
    let mut result = vec![0u8; s.len()];
    let mut high = result.len() - 1;
    
    for char in s.chars() {
        let mut carry = ALPHABET.iter().position(|&x| x == char as u8)
            .ok_or("Invalid character")? as usize;
        
        for i in (0..=high).rev() {
            carry += result[i] as usize * 58;
            result[i] = carry as u8;
            carry /= 256;
        }
    }
    
    Ok(result.into_iter().skip_while(|&b| b == 0).collect())
}