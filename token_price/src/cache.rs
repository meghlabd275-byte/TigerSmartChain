//! Price Cache for Token Price Service

use crate::types::*;
use std::collections::HashMap;
use std::time::{Duration, Instant};

// =============================================================================
// CACHE
// =============================================================================

/// Price Cache with TTL support
pub struct PriceCache {
    cache: HashMap<String, CacheEntry>,
    ttl: Duration,
}

struct CacheEntry {
    price: TokenPrice,
    cached_at: Instant,
}

impl PriceCache {
    /// Create new cache
    pub fn new(ttl_secs: u64) -> Self {
        Self {
            cache: HashMap::new(),
            ttl: Duration::from_secs(ttl_secs),
        }
    }

    /// Get a price from cache
    pub fn get(&self, key: &str) -> Option<TokenPrice> {
        self.cache.get(key).and_then(|entry| {
            if entry.cached_at.elapsed() < self.ttl {
                Some(entry.price.clone())
            } else {
                None
            }
        })
    }

    /// Set a price in cache
    pub fn set(&mut self, key: String, price: TokenPrice) {
        self.cache.insert(key, CacheEntry {
            price,
            cached_at: Instant::now(),
        });
    }

    /// Clear the cache
    pub fn clear(&mut self) {
        self.cache.clear();
    }

    /// Get cache size
    pub fn size(&self) -> usize {
        self.cache.len()
    }

    /// Remove expired entries
    pub fn remove_expired(&mut self) {
        self.cache.retain(|_, entry| entry.cached_at.elapsed() < self.ttl);
    }
}

impl Default for PriceCache {
    fn default() -> Self {
        Self::new(60)
    }
}