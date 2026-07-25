/**
 * TigerScan Reorg Tracker Service
 * 
 * High-performance C++ service for real-time chain reorganization detection
 * and handling with automatic cache invalidation.
 * 
 * Features:
 * - Real-time chain reorg detection
 * - Block header monitoring
 * - Fork detection
 * - Automatic cache invalidation
 * - Reorg event broadcasting
 */

#ifndef TIGERSCAN_REORG_TRACKER_HPP
#define TIGERSCAN_REORG_TRACKER_HPP

#include <string>
#include <vector>
#include <map>
#include <unordered_map>
#include <memory>
#include <mutex>
#include <atomic>
#include <chrono>
#include <functional>
#include <deque>
#include <curl/curl.h>

namespace tigerscan {

// Reorg types
struct ReorgEvent {
    uint64_t block_number;
    std::string old_block_hash;
    std::string new_block_hash;
    std::string parent_hash;
    uint64_t depth;
    uint64_t timestamp;
    std::vector<std::string> affected_transactions;
    std::vector<std::string> affected_addresses;
};

struct ChainFork {
    std::string fork_id;
    uint64_t fork_block;
    std::vector<std::string> block_hashes;
    double total_difficulty;
    bool is_active;
};

struct BlockHeader {
    std::string hash;
    std::string parent_hash;
    uint64_t number;
    std::string miner;
    std::string gas_limit;
    std::string gas_used;
    std::string timestamp;
    std::string nonce;
    std::string difficulty;
    std::string total_difficulty;
    std::vector<std::string> transactions;
    std::string receipts_root;
    std::string state_root;
    std::string prev_randao;
};

struct CanonicalChain {
    std::deque<BlockHeader> headers;
    uint64_t head_block;
    uint64_t genesis_block;
    std::string head_hash;
    
    bool is_canonical(const std::string& hash) const;
    void update_head(const BlockHeader& header);
    void rollback(uint64_t to_block);
};

// Configuration
struct ReorgConfig {
    std::string ethereum_rpc;
    std::string beacon_rpc;
    std::string redis_url;
    int check_interval_ms;
    int confirmations_required;
    uint64_t max_reorg_depth;
    bool enable_auto_rollback;
    bool broadcast_events;
};

// Cache manager for invalidation
class CacheManager {
public:
    CacheManager(const std::string& redis_url);
    ~CacheManager();
    
    void invalidate_block(uint64_t block_number);
    void invalidate_transaction(const std::string& tx_hash);
    void invalidate_address(const std::string& address);
    void invalidate_range(uint64_t start, uint64_t end);
    
private:
    std::string redis_url_;
    void* redis_client_;
};

// Main service
class ReorgTracker {
public:
    ReorgTracker(const ReorgConfig& config);
    ~ReorgTracker();
    
    // Initialize
    bool initialize();
    
    // Start tracking
    void start();
    
    // Stop tracking
    void stop();
    
    // Get recent reorgs
    std::vector<ReorgEvent> get_recent_reorgs(int limit = 10) const;
    
    // Get current chain state
    CanonicalChain get_canonical_chain() const;
    
    // Get active forks
    std::vector<ChainFork> get_active_forks() const;
    
    // Check if block is canonical
    bool is_canonical(uint64_t block_number, const std::string& hash) const;
    
    // Force reorg check
    void check_for_reorg(uint64_t block_number);
    
    // Subscribe to reorg events
    using ReorgCallback = std::function<void(const ReorgEvent&)>;
    void subscribe(ReorgCallback callback);
    
    // Get statistics
    struct Stats {
        std::atomic<uint64_t> total_reorgs{0};
        std::atomic<uint64_t> last_reorg_block{0};
        std::atomic<uint64_t> max_depth_seen{0};
        std::chrono::system_clock::time_point last_reorg_time;
    };
    Stats get_stats() const;

private:
    ReorgConfig config_;
    std::unique_ptr<RPCClient> rpc_client_;
    std::unique_ptr<CacheManager> cache_manager_;
    
    // Chain state
    mutable std::mutex chain_mutex_;
    CanonicalChain canonical_chain_;
    std::map<uint64_t, std::vector<ChainFork>> forks_;
    
    // Reorg history
    mutable std::mutex history_mutex_;
    std::deque<ReorgEvent> reorg_history_;
    
    // Subscribers
    std::vector<ReorgCallback> subscribers_;
    mutable std::mutex subscriber_mutex_;
    
    // State
    std::atomic<bool> is_running_{false};
    std::atomic<uint64_t> last_checked_block_{0};
    Stats stats_;
    
    // Internal methods
    void tracking_loop();
    bool detect_reorg(uint64_t block_number);
    void handle_reorg(const ReorgEvent& event);
    BlockHeader fetch_header(uint64_t block_number);
    bool verify_canonical(uint64_t block_number, const std::string& hash);
    void notify_subscribers(const ReorgEvent& event);
};

// RPC Client
class RPCClient {
public:
    RPCClient(const std::string& rpc_url);
    ~RPCClient();
    
    std::string eth_getBlockByNumber(const std::string& block_num, bool full_tx);
    std::string eth_getBlockByHash(const std::string& hash, bool full_tx);
    std::string eth_call(const std::string& method, const std::string& params);
    std::string eth_getBalance(const std::string& address, const std::string& block);
    
private:
    std::string rpc_url_;
    CURL* curl_;
    
    std::string post(const std::string& body);
};

} // namespace tigerscan

#endif // TIGERSCAN_REORG_TRACKER_HPP
