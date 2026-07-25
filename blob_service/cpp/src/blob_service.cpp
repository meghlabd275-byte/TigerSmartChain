/**
 * TigerScan Blob Service Implementation (EIP-4844)
 */

#include "blob_service.hpp"
#include <iostream>
#include <thread>
#include <chrono>

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
    curl_easy_setopt(curl_, CURLOPT_TIMEOUT, 30L);
    
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

std::string RPCClient::eth_getTransactionByHash(const std::string& hash) {
    std::string body = "{\"jsonrpc\":\"2.0\",\"method\":\"eth_getTransactionByHash\","
                      "\"params\":[\"" + hash + "\"],\"id\":1}";
    return post(body);
}

std::string RPCClient::eth_call(const std::string& method, const std::string& params) {
    std::string body = "{\"jsonrpc\":\"2.0\",\"method\":\"" + method + "\","
                      "\"params\":" + params + ",\"id\":1}";
    return post(body);
}

std::string RPCClient::beacon_getSidecars(const std::string& block_root) {
    // Beacon chain API call
    return "";
}

// BlobService Implementation
BlobService::BlobService(const BlobConfig& config)
    : config_(config), 
      rpc_client_(std::make_unique<RPCClient>(config.ethereum_rpc)) {
    stats_ = {};
}

BlobService::~BlobService() {
    stop();
}

bool BlobService::initialize() {
    return true;
}

void BlobService::start() {
    is_running_ = true;
    
    // Start sync loop in background thread
    std::thread([this]() {
        sync_loop();
    }).detach();
}

void BlobService::stop() {
    is_running_ = false;
}

std::optional<BlobTransaction> BlobService::get_blob_transaction(const std::string& hash) {
    std::lock_guard lock(cache_mutex_);
    
    auto it = blob_cache_.find(hash);
    if (it != blob_cache_.end()) {
        return it->second;
    }
    
    return std::nullopt;
}

std::vector<BlobTransaction> BlobService::get_block_blobs(uint64_t block_number) {
    std::lock_guard lock(cache_mutex_);
    
    auto it = block_blobs_cache_.find(block_number);
    if (it != block_blobs_cache_.end()) {
        return it->second;
    }
    
    return {};
}

BlobStats BlobService::get_stats() const {
    return stats_;
}

std::vector<BlobBlock> BlobService::get_blob_history(uint64_t start_block, uint64_t end_block) {
    std::vector<BlobBlock> history;
    
    std::lock_guard lock(cache_mutex_);
    
    for (uint64_t block = start_block; block <= end_block; block++) {
        auto it = blocks_cache_.find(block);
        if (it != blocks_cache_.end()) {
            history.push_back(it->second);
        }
    }
    
    return history;
}

double BlobService::get_current_blob_gas_price() const {
    // Get current blob gas price from recent blocks
    std::lock_guard lock(cache_mutex_);
    
    if (blocks_cache_.empty()) {
        return 0.0;
    }
    
    // Return average from last 10 blocks
    double total = 0.0;
    int count = 0;
    
    for (auto it = blocks_cache_.rbegin(); it != blocks_cache_.rend() && count < 10; ++it) {
        total += it->second.avg_blob_gas_price;
        count++;
    }
    
    return count > 0 ? total / count : 0.0;
}

void BlobService::subscribe(BlobCallback callback) {
    std::lock_guard lock(subscriber_mutex_);
    subscribers_.push_back(callback);
}

void BlobService::sync_loop() {
    while (is_running_) {
        // Get latest block number
        std::string response = rpc_client_->eth_call("eth_blockNumber", "[]");
        
        // Parse and update
        std::this_thread::sleep_for(std::chrono::milliseconds(config_.sync_interval_ms));
    }
}

bool BlobService::fetch_block_blobs(uint64_t block_number) {
    // Fetch block with full transactions
    std::string hex_num = "0x" + std::to_string(block_number);
    std::string response = rpc_client_->eth_getBlockByNumber(hex_num, true);
    
    if (response.empty()) {
        return false;
    }
    
    // Parse transactions and extract blob transactions
    // In production, would parse JSON response
    
    BlobBlock block;
    block.number = block_number;
    block.timestamp = std::time(nullptr);
    
    // Cache block
    {
        std::lock_guard lock(cache_mutex_);
        blocks_cache_[block_number] = block;
    }
    
    return true;
}

void BlobService::update_stats(const BlobTransaction& blob) {
    stats_.total_blobs++;
    stats_.total_transactions++;
    stats_.total_gas_used += blob.blob_gas_used;
    stats_.last_updated_block = blob.block_number;
}

void BlobService::notify_subscribers(const BlobTransaction& blob) {
    std::lock_guard lock(subscriber_mutex_);
    for (const auto& callback : subscribers_) {
        callback(blob);
    }
}

} // namespace tigerscan
