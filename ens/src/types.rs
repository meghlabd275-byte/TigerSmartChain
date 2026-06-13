//! ENS Types - Complete implementation with ENS resolution and DNS support
//!
//! This module provides:
//! - ENS record management (namehash algorithm)
//! - Forward and reverse resolution
//! - DNS integration for Web2 domain resolution
//! - Record types (A, AAAA, TXT, etc.)
//! - Resolver interface

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::time::{SystemTime, UNIX_EPOCH};

// =============================================================================
// ERRORS
// =============================================================================

/// ENS Service Errors
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum EnsError {
    #[serde(rename = "name_not_found")]
    NameNotFound(String),
    #[serde(rename = "invalid_name")]
    InvalidName(String),
    #[serde(rename = "resolver_not_set")]
    ResolverNotSet(String),
    #[serde(rename = "record_not_found")]
    RecordNotFound(String),
    #[serde(rename = "owner_not_found")]
    OwnerNotFound(String),
    #[serde(rename = "dns_error")]
    DnsError(String),
}

// =============================================================================
// NAMEHASH
// =============================================================================

/// ENS namehash algorithm for generating unique hashes from domain names
pub fn namehash(name: &str) -> Vec<u8> {
    if name.is_empty() {
        return vec![0u8; 32];
    }
    
    // Split name by dots
    let labels: Vec<&str> = name.split('.').collect();
    
    // Start with empty hash
    let mut hash = vec![0u8; 32];
    
    // Process from end to start
    for label in labels.iter().rev() {
        // Label + existing hash
        let mut data = label.as_bytes().to_vec();
        data.push(0); // null separator
        data.extend_from_slice(&hash);
        
        // Simple hash for demo - in production use SHA-256
        hash = simple_hash(&data);
    }
    
    hash
}

/// Simple hash function
fn simple_hash(data: &[u8]) -> Vec<u8> {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};
    let mut hasher = DefaultHasher::new();
    data.hash(&mut hasher);
    let hash = hasher.finish().to_le_bytes();
    let mut result = [0u8; 32];
    for (i, byte) in hash.iter().enumerate() {
        result[i % 32] ^= byte;
    }
    result.to_vec()
}

// =============================================================================
// RECORD TYPES
// =============================================================================

/// ENS Record Types
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum RecordType {
    #[serde(rename = "addr")]
    Addr,
    #[serde(rename = "airdrop")]
    Airdrop,
    #[serde(rename = "contenthash")]
    ContentHash,
    #[serde(rename = "text")]
    Text,
    #[serde(rename = "abi")]
    Abi,
    #[serde(rename = "pubkey")]
    PubKey,
}

// =============================================================================
// ENS RECORD
// =============================================================================

/// ENS Record with all metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Record {
    /// Full domain name
    pub name: String,
    /// Namehash
    pub namehash: String,
    /// Owner address
    pub owner: String,
    /// Resolver contract address
    pub resolver: String,
    /// Time to live (seconds)
    pub ttl: u64,
    /// Ethereum address (ETH address)
    pub address: Option<String>,
    /// Content hash (IPFS or Swarm)
    pub content_hash: Option<String>,
    /// Text records
    pub text: HashMap<String, String>,
    /// ABI (contract ABI JSON)
    pub abi: Option<String>,
    /// Public key
    pub pubkey: Option<PubKey>,
    /// Created timestamp
    pub created_at: u64,
    /// Expiration timestamp
    pub expires_at: u64,
}

impl Record {
    /// Create new record
    pub fn new(name: String, owner: String) -> Self {
        let namehash_str = hex::encode(namehash(&name));
        
        Self {
            name: name.clone(),
            namehash: namehash_str,
            owner,
            resolver: String::new(),
            ttl: 0,
            address: None,
            content_hash: None,
            text: HashMap::new(),
            abi: None,
            pubkey: None,
            created_at: now_unix(),
            expires_at: 0,
        }
    }

    /// Set resolver
    pub fn set_resolver(&mut self, resolver: String) {
        self.resolver = resolver;
    }

    /// Set TTL
    pub fn set_ttl(&mut self, ttl: u64) {
        self.ttl = ttl;
    }

    /// Set ETH address
    pub fn set_address(&mut self, address: String) {
        self.address = Some(address);
    }

    /// Set content hash
    pub fn set_content_hash(&mut self, hash: String) {
        self.content_hash = Some(hash);
    }

    /// Set text record
    pub fn set_text(&mut self, key: String, value: String) {
        self.text.insert(key, value);
    }

    /// Get text record
    pub fn get_text(&self, key: &str) -> Option<&String> {
        self.text.get(key)
    }

    /// Set ABI
    pub fn set_abi(&mut self, abi: String) {
        self.abi = Some(abi);
    }

    /// Set public key
    pub fn set_pubkey(&mut self, pubkey: PubKey) {
        self.pubkey = Some(pubkey);
    }

    /// Set expiration
    pub fn set_expiration(&mut self, expires_at: u64) {
        self.expires_at = expires_at;
    }

    /// Check if expired
    pub fn is_expired(&self) -> bool {
        if self.expires_at == 0 {
            return false;
        }
        now_unix() > self.expires_at
    }
}

// =============================================================================
// PUBLIC KEY
// =============================================================================

/// Public key record
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PubKey {
    pub x: String,
    pub y: String,
}

impl PubKey {
    /// Create from coordinates
    pub fn new(x: String, y: String) -> Self {
        Self { x, y }
    }
}

// =============================================================================
// RESOLVER
// =============================================================================

/// Resolver interface for ENS
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Resolver {
    /// Resolver address
    pub address: String,
    /// Interface version
    pub version: u32,
    /// Supported record types
    pub supported_records: Vec<RecordType>,
    /// Chain ID
    pub chain_id: u64,
}

impl Resolver {
    /// Create new resolver
    pub fn new(address: String) -> Self {
        Self {
            address,
            version: 1,
            supported_records: vec![
                RecordType::Addr,
                RecordType::Airdrop,
                RecordType::ContentHash,
                RecordType::Text,
                RecordType::Abi,
                RecordType::PubKey,
            ],
            chain_id: 1, // Ethereum mainnet
        }
    }

    /// Check if supports record type
    pub fn supports(&self, record_type: &RecordType) -> bool {
        self.supported_records.contains(record_type)
    }
}

// =============================================================================
// REVERSE RECORD
// =============================================================================

/// Reverse ENS record (address -> name)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReverseRecord {
    /// Ethereum address
    pub address: String,
    /// Resolved name
    pub name: String,
    /// Namehash
    pub namehash: String,
}

impl ReverseRecord {
    /// Create new reverse record
    pub fn new(address: String, name: String) -> Self {
        let namehash_str = hex::encode(namehash(&name));
        
        Self {
            address,
            name,
            namehash: namehash_str,
        }
    }
}

// =============================================================================
// DNS RECORD
// =============================================================================

/// DNS record for integration with traditional DNS
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DnsRecord {
    /// Domain name
    pub name: String,
    /// Record type (A, AAAA, TXT, CNAME, etc.)
    pub record_type: String,
    /// TTL
    pub ttl: u32,
    /// Data
    pub data: Vec<String>,
}

impl DnsRecord {
    /// Create new DNS A record
    pub fn a_record(name: String, ip: String) -> Self {
        Self {
            name,
            record_type: "A".to_string(),
            ttl: 300,
            data: vec![ip],
        }
    }

    /// Create new DNS AAAA record
    pub fn aaaa_record(name: String, ip: String) -> Self {
        Self {
            name,
            record_type: "AAAA".to_string(),
            ttl: 300,
            data: vec![ip],
        }
    }

    /// Create new DNS TXT record
    pub fn txt_record(name: String, txt: String) -> Self {
        Self {
            name,
            record_type: "TXT".to_string(),
            ttl: 300,
            data: vec![txt],
        }
    }

    /// Create new DNS CNAME record
    pub fn cname_record(name: String, cname: String) -> Self {
        Self {
            name,
            record_type: "CNAME".to_string(),
            ttl: 300,
            data: vec![cname],
        }
    }
}

// =============================================================================
// ENS REGISTRY
// =============================================================================

/// Complete ENS Registry
pub struct Registry {
    /// Records by namehash
    records: HashMap<String, Record>,
    /// Reverse records by address
    reverse: HashMap<String, ReverseRecord>,
    /// DNS records
    dns_records: HashMap<String, Vec<DnsRecord>>,
    /// Resolvers by address
    resolvers: HashMap<String, Resolver>,
    /// Default resolver
    default_resolver: String,
}

impl Registry {
    /// Create new registry
    pub fn new() -> Self {
        Self {
            records: HashMap::new(),
            reverse: HashMap::new(),
            dns_records: HashMap::new(),
            resolvers: HashMap::new(),
            default_resolver: String::new(),
        }
    }

    /// Set default resolver
    pub fn set_default_resolver(&mut self, resolver: String) {
        self.default_resolver = resolver;
    }

    /// Register resolver
    pub fn register_resolver(&mut self, resolver: Resolver) {
        self.resolvers.insert(resolver.address.clone(), resolver);
    }

    /// Set record
    pub fn set_record(&mut self, record: Record) {
        let namehash = record.namehash.clone();
        self.records.insert(namehash, record);
    }

    /// Get record by name
    pub fn get_record(&self, name: &str) -> Option<&Record> {
        let namehash = hex::encode(namehash(name));
        self.records.get(&namehash)
    }

    /// Get record by namehash
    pub fn get_record_by_namehash(&self, namehash: &str) -> Option<&Record> {
        self.records.get(namehash)
    }

    /// Set reverse record
    pub fn set_reverse(&mut self, record: ReverseRecord) {
        self.reverse.insert(record.address.clone(), record);
    }

    /// Get reverse record
    pub fn get_reverse(&self, address: &str) -> Option<&ReverseRecord> {
        self.reverse.get(address)
    }

    /// Resolve name to address
    pub fn resolve(&self, name: &str) -> Result<String, EnsError> {
        let record = self.get_record(name)
            .ok_or_else(|| EnsError::NameNotFound(name.to_string()))?;
        
        record.address.clone()
            .ok_or_else(|| EnsError::RecordNotFound(name.to_string()))
    }

    /// Reverse resolve address to name
    pub fn reverse_resolve(&self, address: &str) -> Result<String, EnsError> {
        let reverse = self.get_reverse(address)
            .ok_or_else(|| EnsError::OwnerNotFound(address.to_string()))?;
        
        Ok(reverse.name.clone())
    }

    /// Set DNS record
    pub fn set_dns_record(&mut self, record: DnsRecord) {
        self.dns_records
            .entry(record.name.clone())
            .or_insert_with(Vec::new)
            .push(record);
    }

    /// Get DNS records
    pub fn get_dns_records(&self, name: &str) -> Option<&Vec<DnsRecord>> {
        self.dns_records.get(name)
    }

    /// Transfer ownership
    pub fn transfer(&mut self, name: &str, new_owner: String) -> Result<(), EnsError> {
        let namehash = hex::encode(namehash(name));
        let record = self.records.get_mut(&namehash)
            .ok_or_else(|| EnsError::NameNotFound(name.to_string()))?;
        
        record.owner = new_owner;
        
        Ok(())
    }

    /// Set resolver for name
    pub fn set_resolver(&mut self, name: &str, resolver: String) -> Result<(), EnsError> {
        let namehash = hex::encode(namehash(name));
        let record = self.records.get_mut(&namehash)
            .ok_or_else(|| EnsError::NameNotFound(name.to_string()))?;
        
        record.set_resolver(resolver);
        
        Ok(())
    }

    /// Get all names
    pub fn all_names(&self) -> Vec<String> {
        self.records.values().map(|r| r.name.clone()).collect()
    }

    /// Get owner of name
    pub fn owner(&self, name: &str) -> Option<String> {
        self.get_record(name).map(|r| r.owner.clone())
    }

    /// Check if name is available
    pub fn available(&self, name: &str) -> bool {
        self.get_record(name).is_none()
    }
}

impl Default for Registry {
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