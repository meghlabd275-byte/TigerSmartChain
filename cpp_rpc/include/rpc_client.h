#pragma once

#include <string>
#include <vector>
#include <map>
#include <memory>
#include <curl/curl.h>
#include <json/json.h>

namespace tigersmartchain {

// High-performance RPC client for BSC
class BSCRPCClient {
public:
    BSCRPCClient(const std::string& url, int timeout_ms = 5000);
    ~BSCRPCClient();

    // Block operations
    Json::Value getBlockByNumber(uint64_t block_number, bool full_transactions = false);
    Json::Value getBlockByHash(const std::string& hash, bool full_transactions = false);
    Json::Value getLatestBlock();
    uint64_t getBlockNumber();

    // Transaction operations  
    Json::Value getTransactionByHash(const std::string& hash);
    Json::Value getTransactionReceipt(const std::string& hash);

    // Account operations
    std::string getBalance(const std::string& address, const std::string& block = "latest");
    std::string getCode(const std::string& address);
    std::string getStorageAt(const std::string& address, const std::string& slot);
    uint64_t getNonce(const std::string& address);

    // Contract operations
    Json::Value call(const std::string& from, const std::string& to, const std::string& data);
    Json::Value getLogs(const Json::Value& filter);

    // Gas operations
    std::string getGasPrice();
    std::string getMaxPriorityFeePerGas();
    std::string getBaseFee(uint64_t block_number);

    // Chain info
    std::string getChainId();
    Json::Value getBlockReceipts(uint64_t block_number);

private:
    Json::Value sendRequest(const std::string& method, const std::vector<Json::Value>& params);
    static size_t writeCallback(void* contents, size_t size, size_t nmemb, void* userp);

    CURL* curl_;
    std::string url_;
    int timeout_ms_;
    std::string response_buffer_;
};

// Connection pool for high throughput
class RPCConnectionPool {
public:
    RPCConnectionPool(const std::vector<std::string>& urls, int pool_size = 10);
    ~RPCConnectionPool();

    std::shared_ptr<BSCRPCClient> getConnection();
    void returnConnection(std::shared_ptr<BSCRPCClient> client);
    
    // Batch operations
    std::vector<Json::Value> batchGetBlocks(const std::vector<uint64_t>& block_numbers);
    std::vector<Json::Value> batchGetTransactions(const std::vector<std::string>& hashes);

private:
    std::vector<std::string> urls_;
    std::vector<std::shared_ptr<BSCRPCClient>> pool_;
    std::mutex mutex_;
    size_t current_index_;
};

// Cached RPC client with Redis-like caching
class CachedRPCClient {
public:
    CachedRPCClient(BSCRPCClient* client, int cache_ttl_seconds = 30);
    
    Json::Value getBlockByNumberCached(uint64_t block_number);
    Json::Value getTransactionByHashCached(const std::string& hash);
    std::string getBalanceCached(const std::string& address);
    
    void invalidateCache(const std::string& key);
    void clearCache();

private:
    std::string getCacheKey(const std::string& prefix, const std::string& value);
    
    BSCRPCClient* client_;
    int cache_ttl_;
    std::map<std::string, std::pair<Json::Value, time_t>> cache_;
    std::mutex cache_mutex_;
};

} // namespace tigersmartchain
