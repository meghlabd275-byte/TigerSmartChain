//! ENS Cache Management
//! High-performance in-memory cache with TTL

use std::collections::HashMap;
use std::time::{Duration, Instant};

use parking_lot::RwLock;

use crate::types::ENSRecord;

pub struct ENSCache {
    records: RwLock<HashMap<String, CacheEntry>>,
    max_size: usize,
    ttl: Duration,
}

struct CacheEntry {
    record: ENSRecord,
    created_at: Instant,
}

impl ENSCache {
    /// Create new cache
    pub fn new(max_size: usize, ttl_seconds: u64) -> Self {
        Self {
            records: RwLock::new(HashMap::new()),
            max_size,
            ttl: Duration::from_secs(ttl_seconds),
        }
    }
    
    /// Get record
    pub fn get(&self, name: &str) -> Option<ENSRecord> {
        let records = self.records.read();
        if let Some(entry) = records.get(name) {
            if entry.created_at.elapsed() < self.ttl {
                return Some(entry.record.clone());
            }
        }
        None
    }
    
    /// Set record
    pub fn set(&self, record: ENSRecord) {
        let mut records = self.records.write();
        
        // Evict if full
        if records.len() >= self.max_size {
            self.evict_oldest(&mut records);
        }
        
        records.insert(
            record.name.clone(),
            CacheEntry {
                record,
                created_at: Instant::now(),
            },
        );
    }
    
    /// Remove record
    pub fn remove(&self, name: &str) {
        self.records.write().remove(name);
    }
    
    /// Clear cache
    pub fn clear(&self) {
        self.records.write().clear();
    }
    
    /// Get cache size
    pub fn size(&self) -> usize {
        self.records.read().len()
    }
    
    /// Evict oldest entry
    fn evict_oldest(&self, records: &mut HashMap<String, CacheEntry>) {
        if let Some(oldest) = records
            .iter()
            .min_by_key(|(_, e)| e.created_at)
            .map(|(k, _)| k.clone())
        {
            records.remove(&oldest);
        }
    }
}