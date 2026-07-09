#include "cache.hpp"
#include <iostream>
#include <algorithm>

namespace tigersmartchain {

Cache::Cache(uint64_t default_ttl_seconds) : default_ttl_seconds_(default_ttl_seconds) {}

Cache::~Cache() = default;

void Cache::put(const std::string& key, const std::string& value, uint64_t ttl_seconds) {
    std::lock_guard<std::mutex> lock(mutex_);
    
    // Evict if full
    if (cache_.size() >= MAX_CACHE_SIZE) {
        evict_lru();
    }
    
    CacheEntry entry;
    entry.value = value;
    entry.timestamp = std::chrono::steady_clock::now();
    entry.ttl_seconds = ttl_seconds > 0 ? ttl_seconds : default_ttl_seconds_;
    
    cache_[key] = entry;
    access_order_[key] = entry.timestamp;
}

std::optional<std::string> Cache::get(const std::string& key) {
    std::lock_guard<std::mutex> lock(mutex_);
    
    auto it = cache_.find(key);
    if (it == cache_.end()) {
        misses_++;
        return std::nullopt;
    }
    
    if (it->second.is_expired()) {
        cache_.erase(it);
        access_order_.erase(key);
        misses_++;
        return std::nullopt;
    }
    
    // Update access order for LRU
    access_order_[key] = std::chrono::steady_clock::now();
    it->second.timestamp = std::chrono::steady_clock::now();
    hits_++;
    
    return it->second.value;
}

void Cache::remove(const std::string& key) {
    std::lock_guard<std::mutex> lock(mutex_);
    cache_.erase(key);
    access_order_.erase(key);
}

void Cache::clear() {
    std::lock_guard<std::mutex> lock(mutex_);
    cache_.clear();
    access_order_.clear();
}

size_t Cache::size() const {
    std::lock_guard<std::mutex> lock(mutex_);
    return cache_.size();
}

void Cache::evict_expired() {
    auto now = std::chrono::steady_clock::now();
    for (auto it = cache_.begin(); it != cache_.end(); ) {
        if (it->second.is_expired()) {
            access_order_.erase(it->first);
            it = cache_.erase(it);
        } else {
            ++it;
        }
    }
}

void Cache::evict_lru() {
    if (access_order_.empty()) return;
    
    auto oldest = std::min_element(access_order_.begin(), access_order_.end(),
        [](const auto& a, const auto& b) { return a.second < b.second; });
    
    cache_.erase(oldest->first);
    access_order_.erase(oldest);
}

// RedisCache implementation stubs
class RedisCache::Impl {};

RedisCache::RedisCache(const std::string& host, uint16_t port, const std::string& password) : impl_(nullptr) {
    // Would implement Redis connection here
}

RedisCache::~RedisCache() = default;

void RedisCache::put(const std::string& key, const std::string& value, uint64_t ttl_seconds) {}
std::optional<std::string> RedisCache::get(const std::string& key) { return std::nullopt; }
void RedisCache::remove(const std::string& key) {}
bool RedisCache::ping() { return false; }

} // namespace tigersmartchain
