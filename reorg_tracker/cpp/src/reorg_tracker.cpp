/**
 * TigerScan Reorg Tracker Implementation
 */

#include "reorg_tracker.hpp"
#include <iostream>
#include <thread>
#include <chrono>
#include <sstream>

namespace tigerscan {

// RPCClient Implementation
RPCClient::RPCClient(const std::string& rpc_url) : rpc_url_(rpc_url), curl_(nullptr) {
    curl_global_init(CURL_GLOBAL_DEFAULT);
    curl_ = curl_easy_init();
}

RPCClient::~RPCClient() {
    if (curl_) curl_easy_cleanup(curl_);
    curl_global_cleanup();
}

size_t write_callback(void* contents, size_t size, size_t nmemb, void* userp) {
    ((std::string*)userp)->append((char*)contents, size * nmemb);
    return size * nmemb;
}

std::string RPCClient::post(const std::string& body) {
    if (!curl_) return "";
    
    std::string response;
    
    curl_easy_setopt(curl_, CURLOPT_URL, rpc_url_.c_str());
    curl_easy_setopt(curl_, CURLOPT_POST, 1L);
    curl_easy_setopt(curl_, CURLOPT_POSTFIELDS, body.c_str());
    curl_easy_setopt(curl_, CURLOPT_WRITEFUNCTION, write_callback);
    curl_easy_setopt(curl_, CURLOPT_WRITEDATA, &response);
    curl_easy_setopt(curl_, CURLOPT_TIMEOUT, 10L);
    
    struct curl_slist* headers = nullptr;
    headers = curl_slist_append(headers, "Content-Type: application/json");
    curl_easy_setopt(curl_, CURLOPT_HTTPHEADER, headers);
    
    curl_easy_perform(curl_);
    
    if (headers) curl_slist_free_all(headers);
    
    return response;
}

std::string RPCClient::eth_getBlockByNumber(const std::string& block_num, bool full_tx) {
    std::string body = "{\"jsonrpc\":\"2.0\",\"method\":\"eth_getBlockByNumber\","
                      "\"params\":[\"" + block_num + "\"," + (full_tx ? "true" : "false") + "],\"id\":1}";
    return post(body);
}

std::string RPCClient::eth_getBlockByHash(const std::string& hash, bool full_tx) {
    std::string body = "{\"jsonrpc\":\"2.0\",\"method\":\"eth_getBlockByHash\","
                      "\"params\":[\"" + hash + "\"," + (full_tx ? "true" : "false") + "],\"id\":1}";
    return post(body);
}

std::string RPCClient::eth_call(const std::string& method, const std::string& params) {
    std::string body = "{\"jsonrpc\":\"2.0\",\"method\":\"" + method + "\","
                      "\"params\":" + params + ",\"id\":1}";
    return post(body);
}

std::string RPCClient::eth_getBalance(const std::string& address, const std::string& block) {
    std::string body = "{\"jsonrpc\":\"2.0\",\"method\":\"eth_getBalance\","
                      "\"params\":[\"" + address + "\",\"" + block + "\"],\"id\":1}";
    return post(body);
}

// CacheManager Implementation
CacheManager::CacheManager(const std::string& redis_url) : redis_url_(redis_url), redis_client_(nullptr) {
    // Would initialize Redis client in production
}

CacheManager::~CacheManager() {
    // Would cleanup Redis client
}

void CacheManager::invalidate_block(uint64_t block_number) {
    // Invalidate all cache entries for this block
}

void CacheManager::invalidate_transaction(const std::string& tx_hash) {
    // Invalidate transaction cache
}

void CacheManager::invalidate_address(const std::string& address) {
    // Invalidate address cache
}

void CacheManager::invalidate_range(uint64_t start, uint64_t end) {
    // Invalidate range of blocks
}

// CanonicalChain Implementation
bool CanonicalChain::is_canonical(const std::string& hash) const {
    for (const auto& header : headers) {
        if (header.hash == hash) {
            return true;
        }
    }
    return false;
}

void CanonicalChain::update_head(const BlockHeader& header) {
    if (headers.empty() || header.number > headers.back().number) {
        headers.push_back(header);
        if (headers.size() > 100) {
            headers.pop_front();
        }
        head_block = header.number;
        head_hash = header.hash;
    }
}

void CanonicalChain::rollback(uint64_t to_block) {
    while (!headers.empty() && headers.back().number > to_block) {
        headers.pop_back();
    }
    if (!headers.empty()) {
        head_block = headers.back().number;
        head_hash = headers.back().hash;
    }
}

// ReorgTracker Implementation
ReorgTracker::ReorgTracker(const ReorgConfig& config)
    : config_(config), 
      rpc_client_(std::make_unique<RPCClient>(config.ethereum_rpc)),
      cache_manager_(std::make_unique<CacheManager>(config.redis_url)) {
    canonical_chain_.head_block = 0;
    canonical_chain_.genesis_block = 0;
}

ReorgTracker::~ReorgTracker() {
    stop();
}

bool ReorgTracker::initialize() {
    // Get genesis block
    std::string response = rpc_client_->eth_getBlockByNumber("0x0", false);
    return true;
}

void ReorgTracker::start() {
    is_running_ = true;
    std::thread([this]() { tracking_loop(); }).detach();
}

void ReorgTracker::stop() {
    is_running_ = false;
}

std::vector<ReorgEvent> ReorgTracker::get_recent_reorgs(int limit) const {
    std::lock_guard lock(history_mutex_);
    std::vector<ReorgEvent> result;
    int count = 0;
    for (auto it = reorg_history_.rbegin(); it != reorg_history_.rend() && count < limit; ++it) {
        result.push_back(*it);
        count++;
    }
    return result;
}

CanonicalChain ReorgTracker::get_canonical_chain() const {
    std::lock_guard lock(chain_mutex_);
    return canonical_chain_;
}

std::vector<ChainFork> ReorgTracker::get_active_forks() const {
    std::lock_guard lock(chain_mutex_);
    std::vector<ChainFork> result;
    for (const auto& [block, forks] : forks_) {
        for (const auto& fork : forks) {
            if (fork.is_active) result.push_back(fork);
        }
    }
    return result;
}

bool ReorgTracker::is_canonical(uint64_t block_number, const std::string& hash) const {
    std::lock_guard lock(chain_mutex_);
    return canonical_chain_.is_canonical(hash);
}

void ReorgTracker::check_for_reorg(uint64_t block_number) {
    detect_reorg(block_number);
}

void ReorgTracker::subscribe(ReorgCallback callback) {
    std::lock_guard lock(subscriber_mutex_);
    subscribers_.push_back(callback);
}

ReorgTracker::Stats ReorgTracker::get_stats() const {
    return stats_;
}

void ReorgTracker::tracking_loop() {
    while (is_running_) {
        std::string response = rpc_client_->eth_call("eth_blockNumber", "[]");
        std::this_thread::sleep_for(std::chrono::milliseconds(config_.check_interval_ms));
    }
}

bool ReorgTracker::detect_reorg(uint64_t block_number) {
    std::string hex_num = "0x" + std::to_string(block_number);
    std::string response = rpc_client_->eth_getBlockByNumber(hex_num, false);
    
    if (response.empty()) return false;
    
    std::lock_guard lock(chain_mutex_);
    
    if (!canonical_chain_.headers.empty() && canonical_chain_.head_hash != "0x0") {
        ReorgEvent event;
        event.block_number = block_number;
        event.old_block_hash = canonical_chain_.head_hash;
        event.new_block_hash = "0x0";
        event.timestamp = std::time(nullptr);
        event.depth = 1;
        
        handle_reorg(event);
        return true;
    }
    
    BlockHeader current = fetch_header(block_number);
    canonical_chain_.update_head(current);
    return false;
}

void ReorgTracker::handle_reorg(const ReorgEvent& event) {
    stats_.total_reorgs++;
    stats_.last_reorg_block = event.block_number;
    if (event.depth > stats_.max_depth_seen) stats_.max_depth_seen = event.depth;
    stats_.last_reorg_time = std::chrono::system_clock::now();
    
    {
        std::lock_guard lock(history_mutex_);
        reorg_history_.push_front(event);
        if (reorg_history_.size() > 1000) reorg_history_.pop_back();
    }
    
    if (config_.enable_auto_rollback) {
        cache_manager_->invalidate_range(event.block_number - event.depth, event.block_number);
    }
    
    if (config_.broadcast_events) notify_subscribers(event);
}

BlockHeader ReorgTracker::fetch_header(uint64_t block_number) {
    BlockHeader header;
    header.number = block_number;
    header.hash = "0x" + std::string(64, '0');
    header.parent_hash = "0x" + std::string(64, '0');
    return header;
}

bool ReorgTracker::verify_canonical(uint64_t block_number, const std::string& hash) {
    return true;
}

void ReorgTracker::notify_subscribers(const ReorgEvent& event) {
    std::lock_guard lock(subscriber_mutex_);
    for (const auto& callback : subscribers_) {
        callback(event);
    }
}

} // namespace tigerscan
