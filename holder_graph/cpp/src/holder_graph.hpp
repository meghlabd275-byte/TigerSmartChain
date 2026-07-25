/**
 * TigerScan Holder Graph Service
 * 
 * High-performance C++ implementation for analyzing token holder networks
 * and generating holder relationship graphs.
 * 
 * Features:
 * - Token holder network analysis
 * - Holder clustering and grouping
 * - Whale detection
 * - Relationship mapping
 * - Transfer pattern analysis
 */

#ifndef TIGERSCAN_HOLDER_GRAPH_HPP
#define TIGERSCAN_HOLDER_GRAPH_HPP

#include <string>
#include <vector>
#include <map>
#include <set>
#include <unordered_map>
#include <unordered_set>
#include <memory>
#include <mutex>
#include <shared_mutex>
#include <atomic>
#include <future>
#include <optional>
#include <chrono>
#include <algorithm>
#include <numeric>
#include <functional>
#include <queue>
#include <stack>
#include <curl/curl.h>
#include <jansson.h>

namespace tigerscan {

// Holder types
struct Holder {
    std::string address;
    double balance;
    double balance_usd;
    int token_count;
    double percent_supply;
    std::string first_seen;
    std::string last_active;
    int transaction_count;
    std::vector<std::string> tags;
    bool is_contract;
    bool is_whale;
    double risk_score;
    
    Holder() : balance(0), balance_usd(0), token_count(0), 
               percent_supply(0), transaction_count(0), 
               is_contract(false), is_whale(false), risk_score(0) {}
};

struct HolderRelationship {
    std::string from_address;
    std::string to_address;
    double amount;
    int transaction_count;
    std::string first_interaction;
    std::string last_interaction;
    std::string relationship_type; // transfer, swap, mint, burn
};

struct HolderCluster {
    int cluster_id;
    std::vector<std::string> members;
    double total_balance;
    double total_balance_usd;
    std::string cluster_type; // whale, degen, normal, airdrop
    std::vector<std::string> common_tokens;
};

struct HolderGraph {
    std::string token_address;
    std::string token_symbol;
    int total_holders;
    double total_supply;
    std::vector<Holder> holders;
    std::vector<HolderRelationship> relationships;
    std::vector<HolderCluster> clusters;
    std::chrono::system_clock::time_point generated_at;
    
    HolderGraph() : total_holders(0), total_supply(0) {}
};

struct GraphMetrics {
    int total_nodes;
    int total_edges;
    double density;
    double clustering_coefficient;
    int connected_components;
    double average_degree;
    int max_degree;
    std::string largest_whale;
    double largest_whale_balance;
};

struct TransferPattern {
    std::string address;
    std::string pattern_type; // accumulation, distribution, trading, dormant
    double avg_inflow;
    double avg_outflow;
    int inflow_count;
    int outflow_count;
    double hold_time_avg;
    double hold_time_std;
};

// Graph algorithms
class GraphAlgorithms {
public:
    // Calculate degree centrality
    static std::map<std::string, double> degree_centrality(
        const std::vector<HolderRelationship>& relationships
    );
    
    // Find connected components
    static std::vector<std::vector<std::string>> find_connected_components(
        const std::vector<HolderRelationship>& relationships
    );
    
    // Detect communities using simple label propagation
    static std::map<std::string, int> community_detection(
        const std::vector<HolderRelationship>& relationships,
        int max_iterations
    );
    
    // Calculate PageRank
    static std::map<std::string, double> pagerank(
        const std::vector<HolderRelationship>& relationships,
        double damping_factor,
        int max_iterations
    );
    
    // Find bridges (edges whose removal disconnects graph)
    static std::vector<std::pair<std::string, std::string>> find_bridges(
        const std::vector<HolderRelationship>& relationships
    );
    
    // Calculate clustering coefficient
    static double clustering_coefficient(
        const std::vector<HolderRelationship>& relationships
    );
};

// HTTP Client
class HolderGraphClient {
public:
    HolderGraphClient();
    ~HolderGraphClient();
    
    std::string get(const std::string& url,
                   const std::map<std::string, std::string>& headers = {},
                   int timeout_ms = 5000);
    
    void set_api_key(const std::string& key);
    
private:
    CURL* curl_;
    std::string api_key_;
    
    static size_t write_callback(void* contents, size_t size, size_t nmemb, void* userp);
};

// Main holder graph service
class HolderGraphService {
public:
    HolderGraphService();
    ~HolderGraphService();
    
    // Initialize service
    void initialize(const std::string& ethereum_rpc,
                   const std::string& redis_url,
                   const std::string& postgres_url);
    
    // Generate holder graph for a token
    HolderGraph generate_graph(
        const std::string& token_address,
        int max_holders = 1000,
        bool include_relationships = true
    );
    
    // Get holder information
    Holder get_holder(const std::string& address, const std::string& token);
    
    // Get whale holders (top 10%)
    std::vector<Holder> get_whale_holders(const std::string& token, int limit = 100);
    
    // Get holder relationships
    std::vector<HolderRelationship> get_relationships(
        const std::string& address,
        int limit = 100
    );
    
    // Get clusters
    std::vector<HolderCluster> get_clusters(const std::string& token);
    
    // Get graph metrics
    GraphMetrics get_metrics(const std::string& token);
    
    // Detect transfer patterns
    std::vector<TransferPattern> analyze_patterns(const std::string& token);
    
    // Search holders
    std::vector<Holder> search_holders(
        const std::string& token,
        const std::string& query,
        int limit = 50
    );
    
    // Subscribe to holder updates
    using HolderUpdateCallback = std::function<void(const Holder&)>;
    void subscribe(const std::string& token, HolderUpdateCallback callback);
    void unsubscribe(const std::string& token);

private:
    std::string ethereum_rpc_;
    std::unique_ptr<HolderGraphClient> client_;
    
    // Cache
    mutable std::shared_mutex cache_mutex_;
    std::unordered_map<std::string, HolderGraph> graph_cache_;
    std::unordered_map<std::string, Holder> holder_cache_;
    
    // Subscriptions
    std::map<std::string, std::vector<HolderUpdateCallback>> subscribers_;
    mutable std::mutex subscriber_mutex_;
    
    // Internal methods
    std::vector<Holder> fetch_holders_from_chain(const std::string& token, int limit);
    std::vector<HolderRelationship> fetch_relationships(
        const std::string& token,
        const std::string& address
    );
    
    void calculate_holder_metrics(Holder& holder, const std::string& token);
    void detect_whales(std::vector<Holder>& holders);
    std::vector<HolderCluster> detect_clusters(
        const std::vector<Holder>& holders,
        const std::vector<HolderRelationship>& relationships
    );
    
    GraphMetrics calculate_metrics(
        const std::vector<Holder>& holders,
        const std::vector<HolderRelationship>& relationships
    );
    
    TransferPattern analyze_address_pattern(
        const std::string& address,
        const std::string& token
    );
};

} // namespace tigerscan

#endif // TIGERSCAN_HOLDER_GRAPH_HPP
