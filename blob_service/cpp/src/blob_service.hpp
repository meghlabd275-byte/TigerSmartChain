/**
 * TigerScan Blob Service (EIP-4844)
 * 
 * High-performance C++ service for indexing and tracking blob transactions
 * (EIP-4844) with full data availability sampling.
 * 
 * Features:
 * - Blob transaction indexing
 * - Blob gas price tracking
 * - Data availability sampling
 * - Blob sidecar tracking
 */

#ifndef TIGERSCAN_BLOB_SERVICE_HPP
#define TIGERSCAN_BLOB_SERVICE_HPP

#include <string>
#include <vector>
#include <map>
#include <unordered_map>
#include <memory>
#include <mutex>
#include <atomic>
#include <chrono>
#include <functional>
#include <curl/curl.h>

namespace tigerscan {

// Blob types
struct BlobTransaction {
    std::string hash;
    std::string block_hash;
    uint64_t block_number;
    uint64_t transaction_index;
    std::string sender;
    std::string recipient;
    uint64_t blob_gas_used;
    uint64_t blob_gas_limit;
    uint64_t max_blob_gas_fee;
    uint64_t blob_gas_price;
    std::string versioned_hash;
    std::vector<std::string> blob_hashes;
    std::string data_value;
    uint64_t timestamp;
};

struct BlobSidecar {
    std::string block_hash;
    uint64_t block_number;
    std::string sidecar_hash;
    std::vector<std::string> blobs;
    std::vector<std::string> commitments;
    std::vector<std::string> proofs;
};

struct BlobBlock {
    uint64_t number;
    std::string hash;
    std::string parent_hash;
    uint64_t timestamp;
    uint64_t blob_gas_used;
    uint64_t blob_gas_limit;
    uint64_t excess_blob_gas;
    std::vector<BlobTransaction> transactions;
    std::vector<BlobSidecar> sidecars;
    double avg_blob_gas_price;
    uint64_t total_blobs;
}

struct BlobStats {
    uint64_t total_blobs;
    uint64_t total_transactions;
    uint64_t total_gas_used;
    double avg_blob_size;
    double avg_gas_price;
    uint64_t last_updated_block;
}

// Configuration
struct BlobConfig {
    std::string ethereum_rpc;
    std::string beacon_rpc;
    std::string redis_url;
    int sync_interval_ms;
    int max_concurrent_fetches;
    bool enable_sampling;
    uint64_t sampling_rate;
};

// Service
class BlobService {
public:
    BlobService(const BlobConfig& config);
    ~BlobService();
    
    // Initialize service
    bool initialize();
    
    // Start syncing
    void start();
    
    // Stop syncing
    void stop();
    
    // Get blob transaction
    std::optional<BlobTransaction> get_blob_transaction(const std::string& hash);
    
    // Get blob transactions for block
    std::vector<BlobTransaction> get_block_blobs(uint64_t block_number);
    
    // Get blob stats
    BlobStats get_stats() const;
    
    // Get historical blob data
    std::vector<BlobBlock> get_blob_history(uint64_t start_block, uint64_t end_block);
    
    // Get current blob gas price
    double get_current_blob_gas_price() const;
    
    // Subscribe to new blobs
    using BlobCallback = std::function<void(const BlobTransaction&)>;
    void subscribe(BlobCallback callback);

private:
    BlobConfig config_;
    std::unique_ptr<RPCClient> rpc_client_;
    
    // Cache
    mutable std::mutex cache_mutex_;
    std::unordered_map<std::string, BlobTransaction> blob_cache_;
    std::unordered_map<uint64_t, std::vector<BlobTransaction>> block_blobs_cache_;
    std::unordered_map<uint64_t, BlobBlock> blocks_cache_;
    
    BlobStats stats_;
    std::atomic<bool> is_running_{false};
    std::atomic<uint64_t> current_block_{0};
    
    std::vector<BlobCallback> subscribers_;
    mutable std::mutex subscriber_mutex_;
    
    // Internal methods
    void sync_loop();
    bool fetch_block_blobs(uint64_t block_number);
    void update_stats(const BlobTransaction& blob);
    void notify_subscribers(const BlobTransaction& blob);
};

// RPC Client
class RPCClient {
public:
    RPCClient(const std::string& rpc_url);
    ~RPCClient();
    
    std::string eth_getBlockByNumber(const std::string& block_num, bool full_tx);
    std::string eth_getTransactionByHash(const std::string& hash);
    std::string eth_call(const std::string& method, const std::string& params);
    std::string beacon_getSidecars(const std::string& block_root);
    
private:
    std::string rpc_url_;
    CURL* curl_;
    
    std::string post(const std::string& body);
};

} // namespace tigerscan

#endif // TIGERSCAN_BLOB_SERVICE_HPP
