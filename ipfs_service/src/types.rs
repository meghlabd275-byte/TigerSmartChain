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

    /// The multihash this CID wraps.
    pub fn to_multihash(&self) -> Multihash {
        Multihash::new(algorithm_code(&self.algorithm), self.digest.clone())
    }

    /// Convert to the canonical string form.
    ///
    /// CIDv0 is the bare base58btc encoding of the multihash. CIDv1 is the
    /// multibase-prefixed encoding of `version || codec || multihash`, with
    /// base32-lower as the default. The previous version emitted a colon-
    /// separated `base58btc:1:85:deadbeef` string, which is not a CID and
    /// which its own parser then mis-read.
    pub fn to_string(&self) -> String {
        let multihash = self.to_multihash();

        if self.version == 0 {
            return base58_encode(&multihash.to_bytes());
        }

        let mut bytes = encode_varint(1);
        bytes.extend(encode_varint(self.codec));
        bytes.extend(multihash.to_bytes());
        format!("b{}", base32_lower_nopad_encode(&bytes))
    }

    /// Parse a CID from its canonical string form.
    pub fn from_string(s: &str) -> Result<Self, IpfsError> {
        if s.is_empty() {
            return Err(IpfsError::InvalidCid("empty string".to_string()));
        }

        // CIDv0: exactly 46 characters, base58btc, always sha2-256.
        if s.len() == 46 && s.starts_with("Qm") {
            let decoded = base58_decode(s).map_err(IpfsError::InvalidCid)?;
            let multihash = Multihash::from_bytes(&decoded)?;
            if multihash.code != 0x12 {
                return Err(IpfsError::InvalidCid(
                    "CIDv0 must use sha2-256".to_string(),
                ));
            }
            return Ok(Self::new_v0("sha2-256", multihash.digest));
        }

        // CIDv1: a multibase prefix, then version, codec and multihash.
        let (prefix, rest) = s.split_at(1);
        let bytes = match prefix {
            "b" | "B" => base32_lower_nopad_decode(&rest.to_lowercase())
                .map_err(IpfsError::InvalidCid)?,
            "z" => base58_decode(rest).map_err(IpfsError::InvalidCid)?,
            "f" | "F" => hex::decode(rest).map_err(|e| IpfsError::InvalidCid(e.to_string()))?,
            _ => {
                return Err(IpfsError::InvalidCid(format!(
                    "unsupported multibase prefix '{prefix}'"
                )))
            }
        };

        let mut cursor = 0usize;
        let version = read_varint(&bytes, &mut cursor)?;
        if version != 1 {
            return Err(IpfsError::InvalidCid(format!(
                "unsupported CID version {version}"
            )));
        }
        let codec = read_varint(&bytes, &mut cursor)?;
        let multihash = Multihash::from_bytes(&bytes[cursor..])?;

        Ok(Self::new_v1(
            codec,
            algorithm_name(multihash.code),
            multihash.digest,
        ))
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

    /// Encode to bytes: `varint(code) || varint(length) || digest`.
    ///
    /// The previous version wrote the code and length as single raw bytes,
    /// which only happens to work for codes below 128 and cannot represent the
    /// 32-bit codes the multihash table uses.
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut bytes = encode_varint(self.code);
        bytes.extend(encode_varint(self.digest.len() as u64));
        bytes.extend_from_slice(&self.digest);
        bytes
    }

    /// Decode a multihash, checking that the declared length matches the data.
    pub fn from_bytes(data: &[u8]) -> Result<Self, IpfsError> {
        let mut cursor = 0usize;
        let code = read_varint(data, &mut cursor)?;
        let length = read_varint(data, &mut cursor)? as usize;

        if length > 255 {
            return Err(IpfsError::InvalidCid(format!(
                "multihash length {length} is too large"
            )));
        }
        if cursor + length > data.len() {
            return Err(IpfsError::InvalidCid(
                "multihash digest is truncated".to_string(),
            ));
        }

        Ok(Self {
            code,
            length: length as u8,
            digest: data[cursor..cursor + length].to_vec(),
        })
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

    /// Build a DAG: one leaf per file, plus a root node linking them all.
    ///
    /// The previous version discarded the filename and returned only leaves,
    /// so `name` was unused and the "root" the caller expects was never built.
    pub fn create_dag(&self, files: Vec<(String, Vec<u8>)>) -> (DagNode, Vec<DagNode>) {
        let leaves: Vec<DagNode> = files
            .into_iter()
            .map(|(_name, data)| DagNode::new_leaf(data))
            .collect();

        let links: Vec<DagLink> = leaves
            .iter()
            .map(|leaf| DagLink {
                cid: leaf.cid.clone(),
                name: String::new(),
                size: leaf.size,
            })
            .collect();

        (DagNode::new_branch(links), leaves)
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

/// SHA-256 digest.
///
/// This was previously a `DefaultHasher` of the input, which is not SHA-256,
/// is not stable across Rust releases, and produced a digest no IPFS node
/// would agree with.
fn sha256_digest(data: &[u8]) -> Vec<u8> {
    use sha2::{Digest, Sha256};
    Sha256::digest(data).to_vec()
}

/// SHA-512 digest.
fn sha512_digest(data: &[u8]) -> Vec<u8> {
    use sha2::{Digest, Sha512};
    Sha512::digest(data).to_vec()
}

/// BLAKE3 digest.
fn blake3_digest(data: &[u8]) -> Vec<u8> {
    blake3::hash(data).as_bytes().to_vec()
}

/// Multihash algorithm code for a name.
fn algorithm_code(name: &str) -> u64 {
    match name {
        "sha2-512" => 0x13,
        "blake3" => 0x1e,
        _ => 0x12,
    }
}

/// Multihash algorithm name for a code.
fn algorithm_name(code: u64) -> &'static str {
    match code {
        0x13 => "sha2-512",
        0x1e => "blake3",
        _ => "sha2-256",
    }
}

/// Encode an unsigned varint (LEB128).
fn encode_varint(value: u64) -> Vec<u8> {
    let mut out = Vec::new();
    let mut remaining = value;
    loop {
        let mut byte = (remaining & 0x7f) as u8;
        remaining >>= 7;
        if remaining != 0 {
            byte |= 0x80;
        }
        out.push(byte);
        if remaining == 0 {
            break;
        }
    }
    out
}

/// Decode an unsigned varint, advancing the cursor.
fn read_varint(data: &[u8], cursor: &mut usize) -> Result<u64, IpfsError> {
    let mut result = 0u64;
    let mut shift = 0u32;

    loop {
        if *cursor >= data.len() {
            return Err(IpfsError::InvalidCid("varint is truncated".to_string()));
        }
        let byte = data[*cursor];
        *cursor += 1;

        if shift >= 64 {
            return Err(IpfsError::InvalidCid("varint is too long".to_string()));
        }
        result |= ((byte & 0x7f) as u64) << shift;

        if byte & 0x80 == 0 {
            break;
        }
        shift += 7;
    }

    Ok(result)
}

/// The base58btc alphabet.
const BASE58_ALPHABET: &[u8] = b"123456789ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz";

/// The RFC 4648 lowercase alphabet, without padding.
const BASE32_ALPHABET: &[u8] = b"abcdefghijklmnopqrstuvwxyz234567";

/// Encode bytes as base58btc.
fn base58_encode(data: &[u8]) -> String {
    if data.is_empty() {
        return String::new();
    }

    // Leading zero bytes become leading '1' characters.
    let zeros = data.iter().take_while(|&&b| b == 0).count();

    // The rest is a big-endian base-256 to base-58 conversion. Digits are
    // accumulated least-significant first.
    let mut digits: Vec<u8> = Vec::with_capacity(data.len() * 138 / 100 + 1);
    for &byte in &data[zeros..] {
        let mut carry = byte as u32;
        for digit in digits.iter_mut() {
            carry += (*digit as u32) << 8;
            *digit = (carry % 58) as u8;
            carry /= 58;
        }
        while carry > 0 {
            digits.push((carry % 58) as u8);
            carry /= 58;
        }
    }

    let mut out = String::with_capacity(zeros + digits.len());
    for _ in 0..zeros {
        out.push('1');
    }
    for &digit in digits.iter().rev() {
        out.push(BASE58_ALPHABET[digit as usize] as char);
    }

    out
}

/// Decode base58btc.
///
/// The previous version left its scratch buffer uninitialised and never
/// shifted the divisor, so it returned garbage for every input.
fn base58_decode(s: &str) -> Result<Vec<u8>, String> {
    if s.is_empty() {
        return Ok(Vec::new());
    }

    let zeros = s.chars().take_while(|&c| c == '1').count();

    let mut bytes: Vec<u8> = Vec::with_capacity(s.len());
    for c in s.chars() {
        let value = BASE58_ALPHABET
            .iter()
            .position(|&x| x == c as u8)
            .ok_or_else(|| format!("invalid base58 character '{c}'"))? as u32;

        let mut carry = value;
        for byte in bytes.iter_mut() {
            carry += (*byte as u32) * 58;
            *byte = (carry & 0xff) as u8;
            carry >>= 8;
        }
        while carry > 0 {
            bytes.push((carry & 0xff) as u8);
            carry >>= 8;
        }
    }

    let mut out = vec![0u8; zeros];
    out.extend(bytes.iter().rev());
    Ok(out)
}

/// Encode bytes as lowercase base32 without padding, as multibase `b` expects.
fn base32_lower_nopad_encode(data: &[u8]) -> String {
    let mut out = String::with_capacity(data.len().div_ceil(5) * 8);
    let mut buffer: u64 = 0;
    let mut bits: u32 = 0;

    for &byte in data {
        buffer = (buffer << 8) | byte as u64;
        bits += 8;

        while bits >= 5 {
            bits -= 5;
            out.push(BASE32_ALPHABET[((buffer >> bits) & 0x1f) as usize] as char);
        }

        buffer &= if bits == 0 { 0 } else { (1u64 << bits) - 1 };
    }

    if bits > 0 {
        out.push(BASE32_ALPHABET[((buffer << (5 - bits)) & 0x1f) as usize] as char);
    }

    out
}

/// Decode lowercase base32 without padding.
fn base32_lower_nopad_decode(s: &str) -> Result<Vec<u8>, String> {
    let mut out = Vec::with_capacity(s.len() * 5 / 8);
    let mut buffer: u64 = 0;
    let mut bits: u32 = 0;

    for c in s.chars() {
        let value = BASE32_ALPHABET
            .iter()
            .position(|&x| x == c as u8)
            .ok_or_else(|| format!("invalid base32 character '{c}'"))? as u64;

        buffer = (buffer << 5) | value;
        bits += 5;

        while bits >= 8 {
            bits -= 8;
            out.push(((buffer >> bits) & 0xff) as u8);
        }

        buffer &= if bits == 0 { 0 } else { (1u64 << bits) - 1 };
    }

    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sha256_matches_the_known_vector() {
        // NIST vector for "abc".
        assert_eq!(
            hex::encode(sha256_digest(b"abc")),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }

    #[test]
    fn sha512_matches_the_known_vector() {
        assert_eq!(
            hex::encode(sha512_digest(b"abc")),
            "ddaf35a193617abacc417349ae20413112e6fa4e89a97ea20a9eeee64b55d39a2192992a274fc1a836ba3c23a3feebbd454d4423643ce80e2a9ac94fa54ca49f"
        );
    }

    #[test]
    fn blake3_matches_the_known_vector() {
        assert_eq!(
            hex::encode(blake3_digest(b"abc")),
            "6437b3ac38465133ffb63b75273a8db548c558465d79db03fd359c6cd5bd9d85"
        );
    }

    #[test]
    fn base58_round_trips() {
        for input in [
            vec![],
            vec![0u8],
            vec![0u8, 0u8, 1u8],
            b"hello world".to_vec(),
            vec![0xffu8; 40],
        ] {
            let encoded = base58_encode(&input);
            let decoded = base58_decode(&encoded).unwrap();
            assert_eq!(decoded, input, "round trip failed for {input:?}");
        }
    }

    #[test]
    fn base58_rejects_invalid_characters() {
        // '0', 'O', 'I' and 'l' are not in the base58 alphabet.
        assert!(base58_decode("0OIl").is_err());
    }

    #[test]
    fn base32_round_trips() {
        for input in [vec![], vec![0u8], vec![1u8, 2u8, 3u8], vec![0xffu8; 33]] {
            let encoded = base32_lower_nopad_encode(&input);
            assert_eq!(base32_lower_nopad_decode(&encoded).unwrap(), input);
        }
    }

    #[test]
    fn varints_round_trip_including_multi_byte_values() {
        for value in [0u64, 1, 127, 128, 0x12, 0x55, 300, 1 << 32, u64::MAX] {
            let encoded = encode_varint(value);
            let mut cursor = 0;
            assert_eq!(read_varint(&encoded, &mut cursor).unwrap(), value);
            assert_eq!(cursor, encoded.len());
        }
    }

    #[test]
    fn cidv0_round_trips_through_its_string_form() {
        let cid = Cid::from_content(b"hello", "sha2-256");
        let v0 = Cid::new_v0("sha2-256", cid.digest.clone());
        let s = v0.to_string();

        // A CIDv0 string is exactly 46 characters starting with "Qm".
        assert_eq!(s.len(), 46);
        assert!(s.starts_with("Qm"));

        let parsed = Cid::from_string(&s).unwrap();
        assert_eq!(parsed, v0);
    }

    #[test]
    fn cidv1_round_trips_through_its_string_form() {
        let cid = Cid::from_content(b"hello", "sha2-256");
        let s = cid.to_string();

        // A CIDv1 string is multibase base32, so it starts with 'b'.
        assert!(s.starts_with('b'));

        let parsed = Cid::from_string(&s).unwrap();
        assert_eq!(parsed, cid);
        assert_eq!(parsed.digest, cid.digest);
        assert_eq!(parsed.codec, 0x55);
    }

    #[test]
    fn different_content_gives_different_cids() {
        let a = Cid::from_content(b"one", "sha2-256");
        let b = Cid::from_content(b"two", "sha2-256");
        assert_ne!(a.digest, b.digest);
        assert_ne!(a.to_string(), b.to_string());
    }

    #[test]
    fn a_corrupted_cid_string_is_rejected() {
        assert!(Cid::from_string("").is_err());
        assert!(Cid::from_string("not-a-cid").is_err());
        // A CIDv1 claiming version 2 must be rejected.
        assert!(Cid::from_string("bafy").is_err());
    }

    #[test]
    fn a_truncated_multihash_is_rejected() {
        // Code 0x12, length 32, but no digest bytes.
        let bytes = [0x12u8, 0x20];
        assert!(Multihash::from_bytes(&bytes).is_err());
    }

    #[test]
    fn multihash_round_trips_and_keeps_its_algorithm() {
        for algorithm in ["sha2-256", "sha2-512", "blake3"] {
            let digest = match algorithm {
                "sha2-512" => sha512_digest(b"x"),
                "blake3" => blake3_digest(b"x"),
                _ => sha256_digest(b"x"),
            };
            let multihash = Multihash::new(algorithm_code(algorithm), digest);
            let decoded = Multihash::from_bytes(&multihash.to_bytes()).unwrap();

            assert_eq!(decoded.code, multihash.code);
            assert_eq!(decoded.digest, multihash.digest);
            assert_eq!(algorithm_name(decoded.code), algorithm);
        }
    }

    #[test]
    fn the_dag_root_links_every_file() {
        let service = Service::new();
        let files = vec![
            ("a.txt".to_string(), b"aaa".to_vec()),
            ("b.txt".to_string(), b"bbb".to_vec()),
        ];

        let (root, leaves) = service.create_dag(files);

        assert_eq!(leaves.len(), 2);
        assert_eq!(root.links.len(), 2);
        assert!(root.data.is_none());
        // The root's links must point at the leaves' real CIDs.
        assert_eq!(root.links[0].cid, leaves[0].cid);
        assert_eq!(root.links[1].cid, leaves[1].cid);
    }

    #[test]
    fn stored_content_is_retrievable_by_its_cid() {
        let mut service = Service::new();
        let cid = service.store(b"tiger".to_vec()).unwrap();

        assert!(service.has(&cid));
        assert_eq!(service.retrieve(&cid).unwrap().content, b"tiger");
        assert!(service.retrieve("bmissing").is_err());
    }

    #[test]
    fn storing_the_same_content_twice_yields_the_same_cid() {
        let mut service = Service::new();
        let first = service.store(b"same".to_vec()).unwrap();
        let second = service.store(b"same".to_vec()).unwrap();
        assert_eq!(first, second);
    }

    #[test]
    fn pin_and_unpin_track_state() {
        let mut service = Service::new();
        let cid = service.store(b"pinned".to_vec()).unwrap();

        assert!(service.retrieve(&cid).unwrap().pinned);
        service.unpin(&cid).unwrap();
        assert!(!service.retrieve(&cid).unwrap().pinned);
        service.pin(&cid).unwrap();
        assert!(service.retrieve(&cid).unwrap().pinned);

        assert!(service.pin("bmissing").is_err());
        assert!(service.unpin("bmissing").is_err());
    }
}
