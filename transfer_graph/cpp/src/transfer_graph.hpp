/**
 * TigerScan Transfer Graph Service
 * 
 * High-performance C++ implementation for token transfer tracking
 * and transfer flow visualization.
 * 
 * Features:
 * - Real-time transfer tracking
 * - Transfer flow analysis
 * - Large transfer detection
 * - Transfer timeline visualization
 * - Wallet clustering by transfer patterns
 */

#ifndef TIGERSCAN_TRANSFER_GRAPH_HPP
#define TIGERSCAN_TRANSFER_GRAPH_HPP

#include <string>
#include <vector>
#include <map>
#include <set>
#include <unordered_map>
#include <memory>
#include <mutex>
#include <shared_mutex>
#include <atomic>
#include <chrono>
#include <algorithm>
#include <functional>
#include <curl/curl.h>

namespace tigerscan {

// Transfer types
struct Transfer {
    std::string hash;
    std::string from_address;
    std::string to_address;
    double amount;
    std::string token_address;
    std::string token_symbol;
    int token_decimals;
    uint64_t block_number;
    uint64_t timestamp;
    std::string transaction_hash;
    bool is_large_transfer;
    bool is_suspicious;
    double usd_value;
    
    Transfer() : amount(0), token_decimals(18), 
                 block_number(0), timestamp(0),
                 is_large_transfer(false), is_suspicious(false), usd_value(0) {}
};

struct TransferGraph {
    std::string token_address;
    std::vector<Transfer> transfers;
    std::map<std::string, std::vector<Transfer>> address_transfers;
    std::chrono::system_clock::time_point generated_at;
    uint64_t start_block;
    uint64_t end_block;
};

struct TransferStats {
    std::string token_address;
    int total_transfers;
    int unique_senders;
    int unique_receivers;
    double total_volume;
    double total_volume_usd;
    double avg_transfer_size;
    double median_transfer_size;
    int large_transfers;  // > $10k
    int suspicious_transfers;
    std::string peak_transfer_hour;
    double peak_hour_volume;
};

struct TransferFlow {
    std::string from_address;
    std::string to_address;
    double amount;
    double amount_usd;
    int transfer_count;
    std::string first_transfer;
    std::string last_transfer;
    std::string flow_type;  // deposit, withdrawal, swap, transfer
};

struct TimelineEntry {
    uint64_t block_number;
    uint64_t timestamp;
    std::string hash;
    std::string from;
    std::string to;
    double amount;
    double amount_usd;
    std::string type;  // transfer, swap, mint, burn
};

// Transfer analyzer
class TransferAnalyzer {
public:
    TransferAnalyzer();
    ~TransferAnalyzer();
    
    // Analyze transfer patterns
    std::vector<TransferFlow> analyze_flows(
        const std::string& address,
        const std::string& token,
        int limit = 1000
    );
    
    // Detect large transfers
    std::vector<Transfer> detect_large_transfers(
        const std::string& token,
        double threshold_usd = 10000,
        int limit = 100
    );
    
    // Analyze transfer timing patterns
    std::map<std::string, double> analyze_timing_patterns(
        const std::string& address
    );
    
    // Calculate velocity (transfers per day)
    double calculate_velocity(
        const std::string& address,
        const std::string& token,
        int days = 30
    );
    
    // Detect wash trading patterns
    bool detect_wash_trading(const std::string& address);
    
    // Get top senders/receivers
    std::vector<std::pair<std::string, double>> get_top_senders(
        const std::string& token,
        int limit = 10
    );
    
    std::vector<std::pair<std::string, double>> get_top_receivers(
        const std::string& token,
        int limit = 10
    );

private:
    std::vector<Transfer> fetch_transfers(
        const std::string& token,
        const std::string& address = "",
        int limit = 1000
    );
};

// Transfer graph builder
class TransferGraphBuilder {
public:
    TransferGraphBuilder();
    ~TransferGraphBuilder();
    
    // Build transfer graph for a token
    TransferGraph build_graph(
        const std::string& token_address,
        uint64_t start_block,
        uint64_t end_block
    );
    
    // Build address-centric graph
    std::map<std::string, std::vector<Transfer>> build_address_graph(
        const std::string& token_address,
        uint64_t start_block,
        uint64_t end_block
    );
    
    // Generate timeline
    std::vector<TimelineEntry> generate_timeline(
        const std::string& token_address,
        const std::string& address = "",
        int limit = 100
    );

private:
    std::vector<Transfer> fetch_transfers_in_range(
        const std::string& token,
        uint64_t start_block,
        uint64_t end_block
    );
};

// Main transfer service
class TransferGraphService {
public:
    TransferGraphService();
    ~TransferGraphService();
    
    // Initialize
    void initialize(const std::string& ethereum_rpc,
                   const std::string& redis_url);
    
    // Get transfer stats
    TransferStats get_stats(const std::string& token);
    
    // Get transfers for address
    std::vector<Transfer> get_transfers(
        const std::string& address,
        const std::string& token = "",
        int limit = 100
    );
    
    // Get transfer flow analysis
    std::vector<TransferFlow> get_flows(
        const std::string& address,
        const std::string& token = "",
        int limit = 100
    );
    
    // Get transfer timeline
    std::vector<TimelineEntry> get_timeline(
        const std::string& address,
        const std::string& token = "",
        int limit = 100
    );
    
    // Get large transfers
    std::vector<Transfer> get_large_transfers(
        const std::string& token,
        double min_usd = 10000,
        int limit = 100
    );
    
    // Search transfers
    std::vector<Transfer> search(
        const std::string& query,
        const std::string& token = "",
        int limit = 50
    );
    
    // Subscribe to new transfers
    using TransferCallback = std::function<void(const Transfer&)>;
    void subscribe(const std::string& token, TransferCallback callback);
    void unsubscribe(const std::string& token);

private:
    std::string ethereum_rpc_;
    mutable std::mutex cache_mutex_;
    std::unordered_map<std::string, TransferStats> stats_cache_;
    
    std::map<std::string, std::vector<TransferCallback>> subscribers_;
    mutable std::mutex subscriber_mutex_;
    
    std::unique_ptr<TransferAnalyzer> analyzer_;
    std::unique_ptr<TransferGraphBuilder> builder_;
    
    TransferStats calculate_stats(const std::string& token);
    std::vector<Transfer> fetch_from_rpc(
        const std::string& address,
        const std::string& token,
        int limit
    );
};

} // namespace tigerscan

#endif // TIGERSCAN_TRANSFER_GRAPH_HPP
