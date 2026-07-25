/**
 * @file transfer_graph.hpp
 * @brief High-Performance Token Transfer Graph Engine
 * @author TigerScan Team
 * @version 1.0.0
 * 
 * C++ implementation for real-time token transfer graph visualization
 * with ultra-low latency for global blockchain analysis
 */

#ifndef TRANSFER_GRAPH_HPP
#define TRANSFER_GRAPH_HPP

#include <cstdint>
#include <vector>
#include <string>
#include <array>
#include <unordered_map>
#include <unordered_set>
#include <queue>
#include <stack>
#include <memory>
#include <optional>
#include <chrono>
#include <algorithm>
#include <numeric>
#include <limits>

namespace tigerchain {
namespace transfer {

// =============================================================================
// Constants
// =============================================================================

constexpr size_t MAX_NODES = 1000000;
constexpr size_t MAX_EDGES = 10000000;
constexpr size_t CACHE_SIZE = 10000;
constexpr uint64_t DEFAULT_DEPTH = 10;
constexpr uint64_t MAX_PATH_LENGTH = 100;

// Graph directions
enum class GraphDirection : uint8_t {
    INCOMING = 0,
    OUTGOING = 1,
    UNDIRECTED = 2
};

// Node types
enum class NodeType : uint8_t {
    ADDRESS = 0,
    TOKEN = 1,
    TRANSACTION = 2,
    BLOCK = 3,
    CONTRACT = 4
};

// Edge types
enum class EdgeType : uint8_t {
    TRANSFER = 0,
    APPROVAL = 1,
    MINT = 2,
    BURN = 3,
    SWAP = 4,
    BRIDGE = 5
};

// Transaction types
enum class TxType : uint8_t {
    ERC20_TRANSFER = 0,
    ERC20_TRANSFER_FROM = 1,
    ERC721_TRANSFER = 2,
    ERC721_SAFE_TRANSFER = 3,
    ERC1155_TRANSFER = 4,
    NATIVE_TRANSFER = 5
};

// =============================================================================
// Type Definitions
// =============================================================================

using Address = std::array<uint8_t, 20>;
using Hash = std::array<uint8_t, 32>;
using Timestamp = uint64_t;
using BlockNumber = uint64_t;
using Amount = std::string;
using TokenID = std::string;

// =============================================================================
// Data Structures
// =============================================================================

/**
 * @struct Node
 * @brief Graph node representing an entity (address, token, etc.)
 */
struct Node {
    uint64_t id;
    NodeType type;
    Address address;
    std::string label;
    std::string metadata;
    Timestamp first_seen;
    Timestamp last_updated;
    uint64_t degree_in;
    uint64_t degree_out;
    double centrality;
    bool is_whale;
    bool is_contract;
    
    Node();
    explicit Node(uint64_t id, NodeType type, const Address& addr);
    
    std::string address_to_string() const;
    static Address string_to_address(const std::string& str);
};

/**
 * @struct Edge
 * @brief Graph edge representing a transfer relationship
 */
struct Edge {
    uint64_t id;
    uint64_t source;
    uint64_t target;
    EdgeType type;
    Amount amount;
    TokenID token_id;
    Hash transaction_hash;
    BlockNumber block_number;
    Timestamp timestamp;
    uint64_t gas_used;
    bool is_flash_loan;
    bool is_suspicious;
    
    Edge();
    Edge(uint64_t id, uint64_t source, uint64_t target, EdgeType type);
    
    std::string to_string() const;
};

/**
 * @struct TransferPath
 * @brief Path between two addresses
 */
struct TransferPath {
    std::vector<uint64_t> nodes;
    std::vector<Edge> edges;
    Amount total_amount;
    uint64_t hops;
    Timestamp start_time;
    Timestamp end_time;
    
    TransferPath();
    bool is_valid() const;
    double get_efficiency() const;
};

/**
 * @struct GraphStats
 * @brief Statistics about the transfer graph
 */
struct GraphStats {
    uint64_t total_nodes;
    uint64_t total_edges;
    uint64_t active_addresses_24h;
    uint64_t total_volume_24h;
    double average_degree;
    uint64_t max_degree;
    double density;
    uint64_t connected_components;
    
    GraphStats();
    std::string to_json() const;
};

/**
 * @struct TokenFlow
 * @brief Token flow analysis
 */
struct TokenFlow {
    Address token_address;
    Amount total_in;
    Amount total_out;
    Amount net_flow;
    uint64_t transaction_count;
    std::vector<Address> top_senders;
    std::vector<Address> top_receivers;
    
    TokenFlow();
    bool is_inflow() const;
    bool is_outflow() const;
};

/**
 * @struct ClusterInfo
 * @brief Address cluster information
 */
struct ClusterInfo {
    uint64_t cluster_id;
    std::vector<uint64_t> members;
    Address central_address;
    double total_volume;
    uint64_t transaction_count;
    std::string cluster_type;
    
    ClusterInfo();
};

/**
 * @struct QueryResult
 * @brief Generic query result
 */
template<typename T>
struct QueryResult {
    std::vector<T> data;
    uint64_t total_count;
    uint64_t page;
    uint64_t page_size;
    bool has_next;
    double query_time_ms;
};

// =============================================================================
// Graph Implementation
// =============================================================================

/**
 * @class TransferGraph
 * @brief High-performance transfer graph implementation
 */
class TransferGraph {
public:
    TransferGraph();
    ~TransferGraph();
    
    // Node operations
    uint64_t add_node(const Node& node);
    bool remove_node(uint64_t node_id);
    std::optional<Node> get_node(uint64_t node_id) const;
    std::optional<Node> get_node_by_address(const Address& addr) const;
    std::vector<Node> get_neighbors(uint64_t node_id, GraphDirection dir) const;
    bool node_exists(uint64_t node_id) const;
    bool address_exists(const Address& addr) const;
    
    // Edge operations
    uint64_t add_edge(const Edge& edge);
    bool remove_edge(uint64_t edge_id);
    std::optional<Edge> get_edge(uint64_t edge_id) const;
    std::vector<Edge> get_edges(uint64_t node_id, GraphDirection dir) const;
    std::vector<Edge> get_edges_in_block(BlockNumber block) const;
    
    // Transfer operations
    void add_transfer(
        const Address& from,
        const Address& to,
        const Amount& amount,
        const Address& token,
        const Hash& tx_hash,
        BlockNumber block,
        Timestamp timestamp,
        TxType type
    );
    
    // Path finding
    std::optional<TransferPath> find_path(
        const Address& from,
        const Address& to,
        uint64_t max_hops = DEFAULT_DEPTH
    );
    
    std::vector<TransferPath> find_all_paths(
        const Address& from,
        const Address& to,
        uint64_t max_hops = DEFAULT_DEPTH
    );
    
    // Graph analysis
    GraphStats compute_stats() const;
    std::vector<Node> find_whales(double threshold = 0.01) const;
    std::vector<ClusterInfo> detect_clusters(uint64_t min_size = 3) const;
    TokenFlow analyze_token_flow(const Address& token) const;
    
    // Queries
    QueryResult<Edge> query_transfers(
        const Address& address,
        Timestamp start_time,
        Timestamp end_time,
        uint64_t page = 0,
        uint64_t page_size = 100
    ) const;
    
    QueryResult<Node> query_addresses(
        const std::string& search_term,
        uint64_t page = 0,
        uint64_t page_size = 100
    ) const;
    
    // Graph algorithms
    std::vector<uint64_t> bfs(uint64_t start, uint64_t depth) const;
    std::vector<uint64_t> dfs(uint64_t start, uint64_t depth) const;
    std::vector<std::vector<uint64_t>> find_connected_components() const;
    std::vector<TransferPath> find_cycles(uint64_t start) const;
    
    // Centrality measures
    double calculate_degree_centrality(uint64_t node_id) const;
    double calculate_betweenness_centrality(uint64_t node_id) const;
    double calculate_pagerank(uint64_t node_id, double damping = 0.85) const;
    
    // Time-based queries
    QueryResult<Edge> get_recent_transfers(uint64_t limit) const;
    QueryResult<Edge> get_transfers_by_block(BlockNumber block) const;
    std::vector<Node> get_active_addresses(Timestamp since) const;
    
    // Aggregation
    Amount calculate_total_volume(const Address& address) const;
    Amount calculate_token_volume(const Address& token) const;
    std::unordered_map<Address, Amount> get_top_tokens(uint64_t count) const;
    
    // Export
    std::string to_graphson() const;
    std::string to_gexf() const;
    std::string to_json() const;
    void export_to_file(const std::string& filename) const;
    
    // Serialization
    std::vector<uint8_t> serialize() const;
    void deserialize(const std::vector<uint8_t>& data);
    
    // Memory management
    void clear_cache();
    void optimize();
    size_t memory_usage() const;
    
private:
    // Graph data structures
    std::unordered_map<uint64_t, Node> nodes_;
    std::unordered_map<Address, uint64_t> address_to_id_;
    std::unordered_map<uint64_t, std::vector<uint64_t>> adjacency_in_;
    std::unordered_map<uint64_t, std::vector<uint64_t>> adjacency_out_;
    std::unordered_map<uint64_t, Edge> edges_;
    
    // Indices for fast queries
    std::unordered_map<BlockNumber, std::vector<uint64_t>> block_index_;
    std::unordered_map<Timestamp, std::vector<uint64_t>> time_index_;
    std::unordered_map<Address, std::vector<uint64_t>> token_index_;
    
    // Caches
    mutable std::unordered_map<uint64_t, GraphStats> stats_cache_;
    mutable std::unordered_map<uint64_t, double> centrality_cache_;
    
    // Counters
    uint64_t next_node_id_;
    uint64_t next_edge_id_;
    
    // Thread safety
    mutable std::mutex graph_mutex_;
    
    // Helper methods
    void update_indices(const Node& node);
    void update_indices(const Edge& edge);
    void remove_from_indices(uint64_t node_id);
    void remove_from_indices(uint64_t edge_id);
    
    // Path finding helpers
    std::vector<TransferPath> dfs_paths(
        uint64_t current,
        uint64_t target,
        std::vector<uint64_t>& visited,
        uint64_t max_hops
    ) const;
    
    // Clustering helpers
    std::vector<std::vector<uint64_t>> find_strongly_connected_components() const;
    std::vector<ClusterInfo> label_propagation() const;
};

// =============================================================================
// Stream Parser
// =============================================================================

/**
 * @class TransferStreamParser
 * @brief Parses blockchain events into transfer graph
 */
class TransferStreamParser {
public:
    TransferStreamParser(TransferGraph& graph);
    ~TransferStreamParser();
    
    void parse_transaction(const std::vector<uint8_t>& tx_data);
    void parse_log(const std::vector<uint8_t>& log_data);
    void parse_trace(const std::vector<uint8_t>& trace_data);
    
    void process_block(BlockNumber block_number, const std::vector<uint8_t>& block_data);
    
private:
    TransferGraph& graph_;
    
    void process_erc20_transfer(const Address& from, const Address& to, const Amount& amount, const Hash& tx_hash);
    void process_erc721_transfer(const Address& from, const Address& to, const TokenID& token_id, const Hash& tx_hash);
    void process_erc1155_transfer(const Address& from, const Address& to, const TokenID& token_id, const Amount& amount, const Hash& tx_hash);
    void process_native_transfer(const Address& from, const Address& to, const Amount& amount, const Hash& tx_hash);
};

// =============================================================================
// Analytics
// =============================================================================

/**
 * @class TransferAnalytics
 * @brief Advanced analytics for transfer graph
 */
class TransferAnalytics {
public:
    explicit TransferAnalytics(const TransferGraph& graph);
    ~TransferAnalytics();
    
    // Volume analysis
    Amount get_24h_volume() const;
    Amount get_weekly_volume() const;
    Amount get_monthly_volume() const;
    
    // Pattern detection
    std::vector<Address> detect_wash_trading(double threshold) const;
    std::vector<std::pair<Address, Address>> detect_cycles(double min_volume) const;
    std::vector<Address> detect_ponzi_schemes() const;
    
    // Network analysis
    double calculate_network_density() const;
    double calculate_average_clustering() const;
    uint64_t calculate_diameter() const;
    
    // Anomaly detection
    std::vector<Address> detect_anomalies(double z_score_threshold) const;
    std::vector<Edge> detect_suspicious_transfers() const;
    
    // Time series
    std::vector<std::pair<Timestamp, Amount>> get_volume_time_series(Timestamp start, Timestamp end) const;
    std::vector<std::pair<Timestamp, uint64_t>> get_transaction_time_series(Timestamp start, Timestamp end) const;
    
private:
    const TransferGraph& graph_;
    
    std::vector<Address> find_common_receivers(const Address& address) const;
    std::vector<Address> find_common_senders(const Address& address) const;
    bool is_wash_trading_pattern(const Address& a, const Address& b) const;
};

// =============================================================================
// Utility Functions
// =============================================================================

std::string node_type_to_string(NodeType type);
NodeType string_to_node_type(const std::string& str);

std::string edge_type_to_string(EdgeType type);
EdgeType string_to_edge_type(const std::string& str);

std::string tx_type_to_string(TxType type);
TxType string_to_tx_type(const std::string& str);

Address address_from_string(const std::string& str);
std::string address_to_string(const Address& addr);

Hash hash_from_string(const std::string& str);
std::string hash_to_string(const Hash& hash);

} // namespace transfer
} // namespace tigerchain

#endif // TRANSFER_GRAPH_HPP
