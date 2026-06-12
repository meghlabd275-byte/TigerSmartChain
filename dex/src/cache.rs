//! Cache Module for TigerScan DEX

use std::time::{Duration, Instant};

// =============================================================================
// CACHE
// =============================================================================

/// LRU Cache with TTL
pub struct LRUCache<K, V> {
    cache: std::collections::HashMap<K, CacheEntry<V>>,
    order: std::collections::VecDeque<K>,
    max_size: usize,
    ttl: Duration,
}

struct CacheEntry<V> {
    value: V,
    expires: Instant,
}

impl<K: std::hash::Hash + Eq, V> LRUCache<K, V> {
    /// Create new cache
    pub fn new(max_size: usize, ttl: Duration) -> Self {
        Self {
            cache: std::collections::HashMap::new(),
            order: std::collections::VecDeque::new(),
            max_size,
            ttl,
        }
    }

    /// Get value
    pub fn get(&self, key: &K) -> Option<V> {
        self.cache.get(key).and_then(|entry| {
            if Instant::now() > entry.expires {
                None
            } else {
                Some(entry.value.clone())
            }
        })
    }

    /// Set value
    pub fn set(&mut self, key: K, value: V) {
        let expires = Instant::now() + self.ttl;
        
        // Remove if exists
        if self.cache.contains_key(&key) {
            self.cache.remove(&key);
        }
        
        // Add new
        self.cache.insert(key.clone(), CacheEntry { value, expires });
        self.order.push_back(key);
        
        // Evict if needed
        while self.cache.len() > self.max_size {
            if let Some(key) = self.order.pop_front() {
                self.cache.remove(&key);
            }
        }
    }

    /// Remove value
    pub fn remove(&mut self, key: &K) {
        self.cache.remove(key);
    }

    /// Clear cache
    pub fn clear(&mut self) {
        self.cache.clear();
        self.order.clear();
    }

    /// Clean expired entries
    pub fn clean(&mut self) {
        let now = Instant::now();
        self.cache.retain(|_, entry| now < entry.expires);
    }
}

// =============================================================================
// TESTS
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use std::thread;
    use std::time::Duration;

    #[test]
    fn test_cache() {
        let mut cache = LRUCache::new(10, Duration::from_secs(1));
        
        cache.set("key1", "value1");
        assert_eq!(cache.get(&"key1"), Some("value1"));
    }

    #[test]
    fn test_cache_expiry() {
        let mut cache = LRUCache::new(10, Duration::from_millis(50));
        
        cache.set("key1", "value1");
        thread::sleep(Duration::from_millis(100));
        assert_eq!(cache.get(&"key1"), None);
    }

    #[test]
    fn test_cache_eviction() {
        let mut cache = LRUCache::new(2, Duration::from_secs(60));
        
        cache.set("key1", "value1");
        cache.set("key2", "value2");
        cache.set("key3", "value3");
        
        assert_eq!(cache.get(&"key1"), None);
        assert_eq!(cache.get(&"key2"), Some("value2"));
        assert_eq!(cache.get(&"key3"), Some("value3"));
    }
}