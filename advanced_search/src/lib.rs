/**
 * Advanced Search - Bytecode Search, Event Log Search, Fuzzy Search
 * High-performance search using Rust for ultra-low latency
 * 
 * Features:
 * - Bytecode pattern search with exact and fuzzy matching
 * - Event log search with filtering
 * - Fuzzy search for addresses, transaction hashes, contract names
 * - Full-text search across all blockchain data
 */

use std::collections::HashMap;
use serde::{Deserialize, Serialize};
use fuzzy_matcher::FuzzyMatcher;
use fuzzy_matcher::skim::SkimMatcherV2;
use regex::Regex;
use std::sync::Mutex;

// ============================================
// Search Types
// ============================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchResult {
    pub result_type: ResultType,
    pub id: String,
    pub score: f64,
    pub highlights: Vec<String>,
    pub data: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ResultType {
    Transaction,
    Block,
    Address,
    Contract,
    Token,
    Event,
    Log,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BytecodeMatch {
    pub address: String,
    pub contract_name: String,
    pub bytecode: String,
    pub pattern: String,
    pub offset: usize,
    pub length: usize,
    pub match_type: BytecodeMatchType,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum BytecodeMatchType {
    Exact,
    Substring,
    Pattern,
    DeployedBytecode,
    ConstructorBytecode,
    RuntimeBytecode,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EventLog {
    pub address: String,
    pub topics: Vec<String>,
    pub data: String,
    pub block_number: u64,
    pub transaction_hash: String,
    pub log_index: u64,
    pub event_type: Option<String>,
    pub decoded: Option<DecodedEvent>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DecodedEvent {
    pub name: String,
    pub parameters: Vec<EventParameter>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EventParameter {
    pub name: String,
    pub value: String,
    pub type_: String,
    pub indexed: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchQuery {
    pub query: String,
    pub search_type: SearchType,
    pub filters: SearchFilters,
    pub limit: usize,
    pub offset: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SearchType {
    FullText,
    Bytecode,
    EventLog,
    Fuzzy,
    Exact,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchFilters {
    pub address: Option<String>,
    pub from_block: Option<u64>,
    pub to_block: Option<u64>,
    pub from_date: Option<u64>,
    pub to_date: Option<u64>,
    pub contract_type: Option<String>,
    pub token_type: Option<String>,
    pub min_value: Option<u64>,
    pub max_value: Option<u64>,
    pub has_error: Option<bool>,
}

// ============================================
// Search Engine
// ============================================

pub struct SearchEngine {
    // Transaction index
    transactions: HashMap<String, TransactionIndex>,
    // Address index
    addresses: HashMap<String, AddressIndex>,
    // Contract bytecode index
    contracts: HashMap<String, ContractIndex>,
    // Event log index
    event_logs: HashMap<String, Vec<EventLog>>,
    // Fuzzy matcher
    fuzzy_matcher: SkimMatcherV2,
    // Search cache
    cache: Mutex<lru::LruCache<String, Vec<SearchResult>>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TransactionIndex {
    pub hash: String,
    pub from: String,
    pub to: String,
    pub value: u64,
    pub block_number: u64,
    pub timestamp: u64,
    pub input: String,
    pub status: bool,
    pub method_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AddressIndex {
    pub address: String,
    pub balance: u64,
    pub nonce: u64,
    pub code_hash: String,
    pub contract_name: Option<String>,
    pub is_contract: bool,
    pub tags: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContractIndex {
    pub address: String,
    pub name: String,
    pub bytecode: String,
    pub abi: String,
    pub source_code: Option<String>,
    pub compiler_version: Option<String>,
    pub optimization: Option<bool>,
}

impl SearchEngine {
    pub fn new() -> Self {
        Self {
            transactions: HashMap::new(),
            addresses: HashMap::new(),
            contracts: HashMap::new(),
            event_logs: HashMap::new(),
            fuzzy_matcher: SkimMatcherV2::default(),
            cache: Mutex::new(lru::LruCache::new(1000)),
        }
    }

    // ============================================
    // Indexing Methods
    // ============================================

    pub fn index_transaction(&mut self, tx: TransactionIndex) {
        self.transactions.insert(tx.hash.clone(), tx);
    }

    pub fn index_address(&mut self, addr: AddressIndex) {
        self.addresses.insert(addr.address.clone(), addr);
    }

    pub fn index_contract(&mut self, contract: ContractIndex) {
        self.contracts.insert(contract.address.clone(), contract);
    }

    pub fn index_event_logs(&mut self, address: &str, logs: Vec<EventLog>) {
        self.event_logs.insert(address.to_string(), logs);
    }

    // ============================================
    // Full Text Search
    // ============================================

    pub fn search(&self, query: &SearchQuery) -> Vec<SearchResult> {
        let cache_key = format!("{:?}:{}", query.search_type, query.query);
        
        // Check cache
        if let Ok(cache) = self.cache.lock() {
            if let Some(results) = cache.get(&cache_key) {
                return results.clone();
            }
        }
        
        let results = match query.search_type {
            SearchType::FullText => self.full_text_search(query),
            SearchType::Bytecode => self.bytecode_search(&query.query, query.limit),
            SearchType::EventLog => self.event_log_search(&query.query, &query.filters),
            SearchType::Fuzzy => self.fuzzy_search(&query.query, query.limit),
            SearchType::Exact => self.exact_search(&query.query),
        };
        
        // Cache results
        if let Ok(mut cache) = self.cache.lock() {
            cache.put(cache_key, results.clone());
        }
        
        results
    }

    fn full_text_search(&self, query: &SearchQuery) -> Vec<SearchResult> {
        let mut results = Vec::new();
        let query_lower = query.query.to_lowercase();
        
        // Search transactions
        for (_, tx) in &self.transactions {
            if tx.hash.to_lowercase().contains(&query_lower)
                || tx.from.to_lowercase().contains(&query_lower)
                || tx.to.to_lowercase().contains(&query_lower)
                || tx.input.to_lowercase().contains(&query_lower)
            {
                let score = self.calculate_score(&query_lower, &[
                    &tx.hash, &tx.from, &tx.to, &tx.input
                ]);
                
                if score > 0.0 {
                    results.push(SearchResult {
                        result_type: ResultType::Transaction,
                        id: tx.hash.clone(),
                        score,
                        highlights: vec![tx.hash.clone()],
                        data: serde_json::json!({
                            "hash": tx.hash,
                            "from": tx.from,
                            "to": tx.to,
                            "value": tx.value,
                            "block": tx.block_number,
                        }),
                    });
                }
            }
        }
        
        // Search addresses
        for (_, addr) in &self.addresses {
            if addr.address.to_lowercase().contains(&query_lower)
                || addr.contract_name.as_ref().map(|n| n.to_lowercase().contains(&query_lower)).unwrap_or(false)
            {
                let score = self.calculate_score(&query_lower, &[
                    &addr.address,
                    addr.contract_name.as_deref().unwrap_or(""),
                ]);
                
                if score > 0.0 {
                    results.push(SearchResult {
                        result_type: ResultType::Address,
                        id: addr.address.clone(),
                        score,
                        highlights: vec![addr.address.clone()],
                        data: serde_json::json!({
                            "address": addr.address,
                            "balance": addr.balance,
                            "is_contract": addr.is_contract,
                            "contract_name": addr.contract_name,
                        }),
                    });
                }
            }
        }
        
        // Search contracts
        for (_, contract) in &self.contracts {
            if contract.name.to_lowercase().contains(&query_lower)
                || contract.address.to_lowercase().contains(&query_lower)
                || contract.bytecode.to_lowercase().contains(&query_lower)
            {
                let score = self.calculate_score(&query_lower, &[
                    &contract.name, &contract.address, &contract.bytecode
                ]);
                
                if score > 0.0 {
                    results.push(SearchResult {
                        result_type: ResultType::Contract,
                        id: contract.address.clone(),
                        score,
                        highlights: vec![contract.address.clone()],
                        data: serde_json::json!({
                            "address": contract.address,
                            "name": contract.name,
                            "bytecode": contract.bytecode,
                        }),
                    });
                }
            }
        }
        
        results.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap());
        results.truncate(query.limit);
        results
    }

    // ============================================
    // Bytecode Search
    // ============================================

    pub fn bytecode_search(&self, pattern: &str, limit: usize) -> Vec<SearchResult> {
        let mut results = Vec::new();
        let pattern_clean = pattern.trim_start_matches("0x").to_lowercase();
        
        // Normalize pattern
        let normalized_pattern = if pattern_clean.len() % 2 == 0 {
            pattern_clean.clone()
        } else {
            format!("0{}", pattern_clean)
        };
        
        for (_, contract) in &self.contracts {
            // Search deployed bytecode
            if let Some(pos) = contract.bytecode.to_lowercase().find(&normalized_pattern) {
                results.push(SearchResult {
                    result_type: ResultType::Contract,
                    id: contract.address.clone(),
                    score: 1.0,
                    highlights: vec![format!("Position: {}", pos / 2)],
                    data: serde_json::json!({
                        "address": contract.address,
                        "name": contract.name,
                        "bytecode": contract.bytecode,
                        "match_type": "deployed",
                        "offset": pos / 2,
                    }),
                });
            }
            
            // Search constructor bytecode
            if let Some(pos) = contract.bytecode[0..20].to_lowercase().find(&normalized_pattern) {
                results.push(SearchResult {
                    result_type: ResultType::Contract,
                    id: contract.address.clone(),
                    score: 0.9,
                    highlights: vec![format!("Constructor offset: {}", pos / 2)],
                    data: serde_json::json!({
                        "address": contract.address,
                        "name": contract.name,
                        "bytecode": contract.bytecode,
                        "match_type": "constructor",
                        "offset": pos / 2,
                    }),
                });
            }
        }
        
        results.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap());
        results.truncate(limit);
        results
    }

    /// Find contracts with similar bytecode
    pub fn find_similar_bytecode(&self, address: &str, threshold: f64) -> Vec<BytecodeMatch> {
        let mut matches = Vec::new();
        
        if let Some(target) = self.contracts.get(address) {
            let target_bytes = hex_to_bytes(&target.bytecode);
            
            for (_, contract) in &self.contracts {
                if contract.address == address {
                    continue;
                }
                
                let candidate_bytes = hex_to_bytes(&contract.bytecode);
                let similarity = calculate_similarity(&target_bytes, &candidate_bytes);
                
                if similarity >= threshold {
                    matches.push(BytecodeMatch {
                        address: contract.address.clone(),
                        contract_name: contract.name.clone(),
                        bytecode: contract.bytecode.clone(),
                        pattern: target.bytecode.clone(),
                        offset: 0,
                        length: candidate_bytes.len(),
                        match_type: BytecodeMatchType::Substring,
                    });
                }
            }
        }
        
        matches.sort_by(|a, b| b.match_type.cmp(&a.match_type));
        matches
    }

    // ============================================
    // Event Log Search
    // ============================================

    pub fn event_log_search(&self, query: &str, filters: &SearchFilters) -> Vec<SearchResult> {
        let mut results = Vec::new();
        let query_lower = query.to_lowercase();
        
        // Apply address filter if specified
        let addresses: Vec<&String> = if let Some(addr) = &filters.address {
            vec![addr]
        } else {
            self.event_logs.keys().collect()
        };
        
        for addr in addresses {
            if let Some(logs) = self.event_logs.get(addr) {
                for log in logs {
                    // Check if matches query
                    let matches = log.topics.iter().any(|t| t.to_lowercase().contains(&query_lower))
                        || log.data.to_lowercase().contains(&query_lower)
                        || log.event_type.as_ref().map(|e| e.to_lowercase().contains(&query_lower)).unwrap_or(false);
                    
                    if !matches {
                        continue;
                    }
                    
                    // Apply block filters
                    if let Some(from) = filters.from_block {
                        if log.block_number < from {
                            continue;
                        }
                    }
                    
                    if let Some(to) = filters.to_block {
                        if log.block_number > to {
                            continue;
                        }
                    }
                    
                    results.push(SearchResult {
                        result_type: ResultType::Log,
                        id: format!("{}-{}", log.transaction_hash, log.log_index),
                        score: 1.0,
                        highlights: log.topics.clone(),
                        data: serde_json::json!({
                            "address": log.address,
                            "topics": log.topics,
                            "data": log.data,
                            "block_number": log.block_number,
                            "transaction_hash": log.transaction_hash,
                            "event_type": log.event_type,
                        }),
                    });
                }
            }
        }
        
        results.truncate(filters.limit.unwrap_or(100));
        results
    }

    /// Decode event logs
    pub fn decode_event_log(&self, log: &EventLog, abi: &str) -> Option<DecodedEvent> {
        if log.topics.is_empty() {
            return None;
        }
        
        // Parse ABI
        let abi_json: serde_json::Value = serde_json::from_str(abi).ok()?;
        
        // Find matching event
        if let Some(events) = abi_json.get("events").and_then(|e| e.as_array()) {
            for event in events {
                let topic0 = event.get("topic").and_then(|t| t.as_str())?;
                if log.topics.get(0).map(|t| t == topic0).unwrap_or(false) {
                    let name = event.get("name").and_then(|n| n.as_str())?.to_string();
                    let inputs = event.get("inputs").and_then(|i| i.as_array())?;
                    
                    let mut params = Vec::new();
                    for (i, input) in inputs.iter().enumerate() {
                        let param_name = input.get("name").and_then(|n| n.as_str()).unwrap_or("").to_string();
                        let param_type = input.get("type").and_then(|t| t.as_str()).unwrap_or("").to_string();
                        let indexed = input.get("indexed").and_then(|i| i.as_bool()).unwrap_or(false);
                        
                        let value = if indexed && i < log.topics.len() {
                            log.topics[i].clone()
                        } else {
                            log.data.clone()
                        };
                        
                        params.push(EventParameter {
                            name: param_name,
                            value,
                            type_: param_type,
                            indexed,
                        });
                    }
                    
                    return Some(DecodedEvent {
                        name,
                        parameters: params,
                    });
                }
            }
        }
        
        None
    }

    // ============================================
    // Fuzzy Search
    // ============================================

    pub fn fuzzy_search(&self, query: &str, limit: usize) -> Vec<SearchResult> {
        let mut results = Vec::new();
        
        // Search transactions
        for (_, tx) in &self.transactions {
            if let Some(score) = self.fuzzy_matcher.fuzzy_match(&tx.hash, query) {
                results.push(SearchResult {
                    result_type: ResultType::Transaction,
                    id: tx.hash.clone(),
                    score: score as f64 / 100.0,
                    highlights: vec![tx.hash.clone()],
                    data: serde_json::json!({"hash": tx.hash}),
                });
            }
        }
        
        // Search addresses
        for (_, addr) in &self.addresses {
            if let Some(score) = self.fuzzy_matcher.fuzzy_match(&addr.address, query) {
                results.push(SearchResult {
                    result_type: ResultType::Address,
                    id: addr.address.clone(),
                    score: score as f64 / 100.0,
                    highlights: vec![addr.address.clone()],
                    data: serde_json::json!({"address": addr.address}),
                });
            }
            
            if let Some(name) = &addr.contract_name {
                if let Some(score) = self.fuzzy_matcher.fuzzy_match(name, query) {
                    results.push(SearchResult {
                        result_type: ResultType::Address,
                        id: addr.address.clone(),
                        score: score as f64 / 100.0,
                        highlights: vec![name.clone()],
                        data: serde_json::json!({"address": addr.address, "name": name}),
                    });
                }
            }
        }
        
        // Search contracts
        for (_, contract) in &self.contracts {
            if let Some(score) = self.fuzzy_matcher.fuzzy_match(&contract.name, query) {
                results.push(SearchResult {
                    result_type: ResultType::Contract,
                    id: contract.address.clone(),
                    score: score as f64 / 100.0,
                    highlights: vec![contract.name.clone()],
                    data: serde_json::json!({"address": contract.address, "name": contract.name}),
                });
            }
        }
        
        results.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap());
        results.truncate(limit);
        results
    }

    // ============================================
    // Exact Search
    // ============================================

    pub fn exact_search(&self, query: &str) -> Vec<SearchResult> {
        let mut results = Vec::new();
        let query_lower = query.to_lowercase();
        
        // Check transaction hash
        if let Some(tx) = self.transactions.get(&query_lower) {
            results.push(SearchResult {
                result_type: ResultType::Transaction,
                id: tx.hash.clone(),
                score: 1.0,
                highlights: vec![tx.hash.clone()],
                data: serde_json::json!({"hash": tx.hash, "from": tx.from, "to": tx.to}),
            });
        }
        
        // Check address
        if let Some(addr) = self.addresses.get(&query_lower) {
            results.push(SearchResult {
                result_type: ResultType::Address,
                id: addr.address.clone(),
                score: 1.0,
                highlights: vec![addr.address.clone()],
                data: serde_json::json!({"address": addr.address, "balance": addr.balance}),
            });
        }
        
        // Check contract address
        if let Some(contract) = self.contracts.get(&query_lower) {
            results.push(SearchResult {
                result_type: ResultType::Contract,
                id: contract.address.clone(),
                score: 1.0,
                highlights: vec![contract.address.clone()],
                data: serde_json::json!({"address": contract.address, "name": contract.name}),
            });
        }
        
        results
    }

    // ============================================
    // Helper Methods
    // ============================================

    fn calculate_score(&self, query: &str, fields: &[&str]) -> f64 {
        let mut score = 0.0;
        
        for (i, field) in fields.iter().enumerate() {
            if field.is_empty() {
                continue;
            }
            
            let field_lower = field.to_lowercase();
            
            // Exact match
            if field_lower == query {
                score += 1.0;
            }
            // Starts with
            else if field_lower.starts_with(query) {
                score += 0.8;
            }
            // Contains
            else if field_lower.contains(query) {
                score += 0.5;
            }
            // Fuzzy match
            else if let Some(fs) = self.fuzzy_matcher.fuzzy_match(field, query) {
                score += fs as f64 / 100.0 * 0.3;
            }
        }
        
        score
    }

    /// Get search statistics
    pub fn get_stats(&self) -> SearchStats {
        SearchStats {
            total_transactions: self.transactions.len(),
            total_addresses: self.addresses.len(),
            total_contracts: self.contracts.len(),
            total_event_logs: self.event_logs.values().map(|v| v.len()).sum(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchStats {
    pub total_transactions: usize,
    pub total_addresses: usize,
    pub total_contracts: usize,
    pub total_event_logs: usize,
}

// ============================================
// Helper Functions
// ============================================

fn hex_to_bytes(hex: &str) -> Vec<u8> {
    let hex = hex.trim_start_matches("0x");
    (0..hex.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&hex[i..i + 2], 16).unwrap_or(0))
        .collect()
}

fn calculate_similarity(a: &[u8], b: &[u8]) -> f64 {
    if a.is_empty() || b.is_empty() {
        return 0.0;
    }
    
    let mut matches = 0;
    let min_len = a.len().min(b.len());
    
    for i in 0..min_len {
        if a[i] == b[i] {
            matches += 1;
        }
    }
    
    matches as f64 / min_len as f64
}

// ============================================
// Simple LRU Cache
// ============================================

mod lru {
    use std::collections::VecDeque;
    use std::hash::Hash;

    pub struct LruCache<K: Hash + Eq, V> {
        capacity: usize,
        cache: VecDeque<(K, V)>,
    }

    impl<K: Hash + Eq + Clone, V> LruCache<K, V> {
        pub fn new(capacity: usize) -> Self {
            Self {
                capacity,
                cache: VecDeque::new(),
            }
        }

        pub fn get(&self, key: &K) -> Option<&V> {
            self.cache
                .iter()
                .find(|(k, _)| k == key)
                .map(|(_, v)| v)
        }

        pub fn put(&mut self, key: K, value: V) {
            // Remove if exists
            self.cache.retain(|(k, _)| k != &key);
            
            // Add new entry
            self.cache.push_front((key, value));
            
            // Remove oldest if over capacity
            while self.cache.len() > self.capacity {
                self.cache.pop_back();
            }
        }
    }
}

// ============================================
// Tests
// ============================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_bytecode_search() {
        let engine = SearchEngine::new();
        let results = engine.bytecode_search("608060405234", 10);
        assert!(results.is_empty()); // No contracts indexed
    }

    #[test]
    fn test_fuzzy_search() {
        let engine = SearchEngine::new();
        let results = engine.fuzzy_search("0x123", 10);
        assert!(results.is_empty()); // No data indexed
    }

    #[test]
    fn test_hex_to_bytes() {
        let bytes = hex_to_bytes("0x606060405234");
        assert_eq!(bytes.len(), 6);
    }
}