#pragma once

#include <string>
#include <optional>
#include <unordered_map>
#include <mutex>
#include <chrono>

namespace tigersmartchain {

/**
 * In-memory LRU cache with TTL support
 */
class Cache {
public:
    explicit Cache(uint64_t default_ttl_seconds = 60);
    ~Cache();
    
    // Cache operations
    void put(const std::string& key, const std::string& value, uint64_t ttl_seconds = 0);
    std::optional<std::string> get(const std::string& key);
    void remove(const std::string& key);
    void clear();
    
    // Statistics
    size_t size() const;
    size_t hits() const { return hits_; }
    size_t misses() const { return misses_; }
    
private:
    struct CacheEntry {
        std::string value;
        std::chrono::steady_clock::time_point timestamp;
        uint64_t ttl_seconds;
        
        bool is_expired() const {
            auto now = std::chrono::steady_clock::now();
            auto age = std::chrono::duration_cast<std::chrono::seconds>(now - timestamp).count();
            return age > ttl_seconds;
        }
    };
    
    std::unordered_map<std::string, CacheEntry> cache_;
    mutable std::mutex mutex_;
    uint64_t default_ttl_seconds_;
    
    // LRU tracking
    std::unordered_map<std::string, std::chrono::steady_clock::time_point> access_order_;
    static constexpr size_t MAX_CACHE_SIZE = 10000;
    
    // Statistics
    size_t hits_ = 0;
    size_t misses_ = 0;
    
    void evict_expired();
    void evict_lru();
};

/**
 * Redis-backed distributed cache
 * For multi-instance deployments
 */
class RedisCache {
public:
    RedisCache(const std::string& host, uint16_t port, const std::string& password = "");
    ~RedisCache();
    
    void put(const std::string& key, const std::string& value, uint64_t ttl_seconds = 0);
    std::optional<std::string> get(const std::string& key);
    void remove(const std::string& key);
    bool ping();
    
private:
    class Impl;
    std::unique_ptr<Impl> impl_;
};

} // namespace tigersmartchain
