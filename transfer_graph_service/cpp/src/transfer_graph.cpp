/**
 * @file transfer_graph.cpp
 * @brief Transfer Graph Implementation
 * @author TigerScan Team
 */

#include "transfer_graph.hpp"
#include <cmath>
#include <random>
#include <fstream>

namespace tigerchain {
namespace transfer {

// =============================================================================
// Node Implementation
// =============================================================================

Node::Node() 
    : id(0), type(NodeType::ADDRESS), first_seen(0), last_updated(0)
    , degree_in(0), degree_out(0), centrality(0.0), is_whale(false), is_contract(false) {
    address.fill(0);
}

Node::Node(uint64_t id, NodeType type, const Address& addr) 
    : id(id), type(type), address(addr), first_seen(0), last_updated(0)
    , degree_in(0), degree_out(0), centrality(0.0), is_whale(false), is_contract(false) {}

std::string Node::address_to_string() const {
    std::ostringstream oss;
    oss << "0x";
    for (const auto& byte : address) {
        oss << std::hex << std::setw(2) << std::setfill('0') << (int)byte;
    }
    return oss.str();
}

Address Node::string_to_address(const std::string& str) {
    Address addr{};
    std::string hex_str = str;
    if (hex_str.substr(0, 2) == "0x") {
        hex_str = hex_str.substr(2);
    }
    for (size_t i = 0; i < hex_str.length() && i < 40; i += 2) {
        addr[i / 2] = static_cast<uint8_t>(std::stoi(hex_str.substr(i, 2), nullptr, 16));
    }
    return addr;
}

// =============================================================================
// Edge Implementation
// =============================================================================

Edge::Edge() 
    : id(0), source(0), target(0), type(EdgeType::TRANSFER)
    , block_number(0), timestamp(0), gas_used(0), is_flash_loan(false), is_suspicious(false) {}

Edge::Edge(uint64_t id, uint64_t source, uint64_t target, EdgeType type) 
    : id(id), source(source), target(target), type(type)
    , block_number(0), timestamp(0), gas_used(0), is_flash_loan(false), is_suspicious(false) {}

std::string Edge::to_string() const {
    std::ostringstream oss;
    oss << "Edge(" << id << "): " << source << " -> " << target 
        << " [" << edge_type_to_string(type) << "] " << amount;
    return oss.str();
}

// =============================================================================
// TransferPath Implementation
// =============================================================================

TransferPath::TransferPath() : hops(0), start_time(0), end_time(0) {}

bool TransferPath::is_valid() const {
    return !nodes.empty() && nodes.size() == edges.size() + 1;
}

double TransferPath::get_efficiency() const {
    if (hops == 0) return 0.0;
    // Efficiency = amount / hops
    // This is a simplified version
    return 1.0 / static_cast<double>(hops);
}

// =============================================================================
// GraphStats Implementation
// =============================================================================

GraphStats::GraphStats() 
    : total_nodes(0), total_edges(0), active_addresses_24h(0)
    , total_volume_24h(0), average_degree(0.0), max_degree(0)
    , density(0.0), connected_components(0) {}

std::string GraphStats::to_json() const {
    std::ostringstream oss;
    oss << "{";
    oss << "\"total_nodes\":" << total_nodes << ",";
    oss << "\"total_edges\":" << total_edges << ",";
    oss << "\"active_addresses_24h\":" << active_addresses_24h << ",";
    oss << "\"total_volume_24h\":\"" << total_volume_24h << "\",";
    oss << "\"average_degree\":" << average_degree << ",";
    oss << "\"max_degree\":" << max_degree << ",";
    oss << "\"density\":" << density << ",";
    oss << "\"connected_components\":" << connected_components;
    oss << "}";
    return oss.str();
}

// =============================================================================
// TokenFlow Implementation
// =============================================================================

TokenFlow::TokenFlow() : total_in("0"), total_out("0"), net_flow("0"), transaction_count(0) {}

bool TokenFlow::is_inflow() const {
    return net_flow > total_out;
}

bool TokenFlow::is_outflow() const {
    return total_out > total_in;
}

// =============================================================================
// ClusterInfo Implementation
// =============================================================================

ClusterInfo::ClusterInfo() 
    : cluster_id(0), total_volume(0), transaction_count(0) {}

// =============================================================================
// TransferGraph Implementation
// =============================================================================

TransferGraph::TransferGraph() : next_node_id_(1), next_edge_id_(1) {}

TransferGraph::~TransferGraph() {}

uint64_t TransferGraph::add_node(const Node& node) {
    std::lock_guard<std::mutex> lock(graph_mutex_);
    
    uint64_t id = node.id;
    if (id == 0) {
        id = next_node_id_++;
    }
    
    Node new_node = node;
    new_node.id = id;
    new_node.last_updated = std::chrono::duration_cast<std::chrono::seconds>(
        std::chrono::system_clock::now().time_since_epoch()
    ).count();
    
    if (new_node.first_seen == 0) {
        new_node.first_seen = new_node.last_updated;
    }
    
    nodes_[id] = new_node;
    address_to_id_[new_node.address] = id;
    
    update_indices(new_node);
    
    return id;
}

bool TransferGraph::remove_node(uint64_t node_id) {
    std::lock_guard<std::mutex> lock(graph_mutex_);
    
    auto it = nodes_.find(node_id);
    if (it == nodes_.end()) {
        return false;
    }
    
    // Remove all edges connected to this node
    if (adjacency_in_.count(node_id)) {
        for (const auto& edge_id : adjacency_in_[node_id]) {
            edges_.erase(edge_id);
        }
    }
    if (adjacency_out_.count(node_id)) {
        for (const auto& edge_id : adjacency_out_[node_id]) {
            edges_.erase(edge_id);
        }
    }
    
    address_to_id_.erase(it->second.address);
    nodes_.erase(it);
    
    remove_from_indices(node_id);
    
    return true;
}

std::optional<Node> TransferGraph::get_node(uint64_t node_id) const {
    std::lock_guard<std::mutex> lock(graph_mutex_);
    
    auto it = nodes_.find(node_id);
    if (it != nodes_.end()) {
        return it->second;
    }
    return std::nullopt;
}

std::optional<Node> TransferGraph::get_node_by_address(const Address& addr) const {
    std::lock_guard<std::mutex> lock(graph_mutex_);
    
    auto it = address_to_id_.find(addr);
    if (it != address_to_id_.end()) {
        return get_node(it->second);
    }
    return std::nullopt;
}

std::vector<Node> TransferGraph::get_neighbors(uint64_t node_id, GraphDirection dir) const {
    std::lock_guard<std::mutex> lock(graph_mutex_);
    
    std::vector<Node> neighbors;
    std::vector<uint64_t> neighbor_ids;
    
    if (dir == GraphDirection::INCOMING || dir == GraphDirection::UNDIRECTED) {
        if (adjacency_in_.count(node_id)) {
            neighbor_ids.insert(neighbor_ids.end(), 
                adjacency_in_[node_id].begin(), 
                adjacency_in_[node_id].end()
            );
        }
    }
    
    if (dir == GraphDirection::OUTGOING || dir == GraphDirection::UNDIRECTED) {
        if (adjacency_out_.count(node_id)) {
            neighbor_ids.insert(neighbor_ids.end(), 
                adjacency_out_[node_id].begin(), 
                adjacency_out_[node_id].end()
            );
        }
    }
    
    for (const auto& id : neighbor_ids) {
        auto node = get_node(id);
        if (node) {
            neighbors.push_back(*node);
        }
    }
    
    return neighbors;
}

bool TransferGraph::node_exists(uint64_t node_id) const {
    return nodes_.find(node_id) != nodes_.end();
}

bool TransferGraph::address_exists(const Address& addr) const {
    return address_to_id_.find(addr) != address_to_id_.end();
}

uint64_t TransferGraph::add_edge(const Edge& edge) {
    std::lock_guard<std::mutex> lock(graph_mutex_);
    
    uint64_t id = next_edge_id_++;
    Edge new_edge = edge;
    new_edge.id = id;
    
    // Ensure source and target nodes exist
    if (!node_exists(new_edge.source)) {
        Node source_node;
        source_node.id = new_edge.source;
        source_node.type = NodeType::ADDRESS;
        nodes_[new_edge.source] = source_node;
    }
    
    if (!node_exists(new_edge.target)) {
        Node target_node;
        target_node.id = new_edge.target;
        target_node.type = NodeType::ADDRESS;
        nodes_[new_edge.target] = target_node;
    }
    
    // Add to adjacency lists
    adjacency_out_[new_edge.source].push_back(id);
    adjacency_in_[new_edge.target].push_back(id);
    
    // Update node degrees
    nodes_[new_edge.source].degree_out++;
    nodes_[new_edge.target].degree_in++;
    
    // Store edge
    edges_[id] = new_edge;
    
    // Update indices
    block_index_[new_edge.block_number].push_back(id);
    time_index_[new_edge.timestamp].push_back(id);
    
    return id;
}

bool TransferGraph::remove_edge(uint64_t edge_id) {
    std::lock_guard<std::mutex> lock(graph_mutex_);
    
    auto it = edges_.find(edge_id);
    if (it == edges_.end()) {
        return false;
    }
    
    const Edge& edge = it->second;
    
    // Remove from adjacency lists
    auto& out_edges = adjacency_out_[edge.source];
    out_edges.erase(std::remove(out_edges.begin(), out_edges.end(), edge_id), out_edges.end());
    
    auto& in_edges = adjacency_in_[edge.target];
    in_edges.erase(std::remove(in_edges.begin(), in_edges.end(), edge_id), in_edges.end());
    
    // Update node degrees
    if (node_exists(edge.source)) {
        nodes_[edge.source].degree_out--;
    }
    if (node_exists(edge.target)) {
        nodes_[edge.target].degree_in--;
    }
    
    // Remove from indices
    remove_from_indices(edge_id);
    
    edges_.erase(it);
    return true;
}

std::optional<Edge> TransferGraph::get_edge(uint64_t edge_id) const {
    std::lock_guard<std::mutex> lock(graph_mutex_);
    
    auto it = edges_.find(edge_id);
    if (it != edges_.end()) {
        return it->second;
    }
    return std::nullopt;
}

std::vector<Edge> TransferGraph::get_edges(uint64_t node_id, GraphDirection dir) const {
    std::lock_guard<std::mutex> lock(graph_mutex_);
    
    std::vector<Edge> result;
    std::vector<uint64_t> edge_ids;
    
    if (dir == GraphDirection::INCOMING || dir == GraphDirection::UNDIRECTED) {
        if (adjacency_in_.count(node_id)) {
            edge_ids.insert(edge_ids.end(), 
                adjacency_in_[node_id].begin(), 
                adjacency_in_[node_id].end()
            );
        }
    }
    
    if (dir == GraphDirection::OUTGOING || dir == GraphDirection::UNDIRECTED) {
        if (adjacency_out_.count(node_id)) {
            edge_ids.insert(edge_ids.end(), 
                adjacency_out_[node_id].begin(), 
                adjacency_out_[node_id].end()
            );
        }
    }
    
    for (const auto& id : edge_ids) {
        auto edge = get_edge(id);
        if (edge) {
            result.push_back(*edge);
        }
    }
    
    return result;
}

std::vector<Edge> TransferGraph::get_edges_in_block(BlockNumber block) const {
    std::lock_guard<std::mutex> lock(graph_mutex_);
    
    std::vector<Edge> result;
    
    if (block_index_.count(block)) {
        for (const auto& edge_id : block_index_.at(block)) {
            auto edge = get_edge(edge_id);
            if (edge) {
                result.push_back(*edge);
            }
        }
    }
    
    return result;
}

void TransferGraph::add_transfer(
    const Address& from,
    const Address& to,
    const Amount& amount,
    const Address& token,
    const Hash& tx_hash,
    BlockNumber block,
    Timestamp timestamp,
    TxType type
) {
    std::lock_guard<std::mutex> lock(graph_mutex_);
    
    // Get or create node IDs
    uint64_t from_id, to_id;
    
    if (address_to_id_.count(from)) {
        from_id = address_to_id_[from];
    } else {
        from_id = next_node_id_++;
        Node from_node;
        from_node.id = from_id;
        from_node.address = from;
        from_node.type = NodeType::ADDRESS;
        nodes_[from_id] = from_node;
        address_to_id_[from] = from_id;
    }
    
    if (address_to_id_.count(to)) {
        to_id = address_to_id_[to];
    } else {
        to_id = next_node_id_++;
        Node to_node;
        to_node.id = to_id;
        to_node.address = to;
        to_node.type = NodeType::ADDRESS;
        nodes_[to_id] = to_node;
        address_to_id_[to] = to_id;
    }
    
    // Create edge
    Edge edge;
    edge.id = next_edge_id_++;
    edge.source = from_id;
    edge.target = to_id;
    edge.type = EdgeType::TRANSFER;
    edge.amount = amount;
    edge.transaction_hash = tx_hash;
    edge.block_number = block;
    edge.timestamp = timestamp;
    
    // Add to graph
    adjacency_out_[from_id].push_back(edge.id);
    adjacency_in_[to_id].push_back(edge.id);
    
    nodes_[from_id].degree_out++;
    nodes_[to_id].degree_in++;
    
    edges_[edge.id] = edge;
    
    // Update indices
    block_index_[block].push_back(edge.id);
    time_index_[timestamp].push_back(edge.id);
    token_index_[token].push_back(edge.id);
}

std::optional<TransferPath> TransferGraph::find_path(
    const Address& from,
    const Address& to,
    uint64_t max_hops
) {
    if (!address_exists(from) || !address_exists(to)) {
        return std::nullopt;
    }
    
    uint64_t start = address_to_id_.at(from);
    uint64_t target = address_to_id_.at(to);
    
    std::vector<uint64_t> visited;
    std::vector<TransferPath> paths = dfs_paths(start, target, visited, max_hops);
    
    if (paths.empty()) {
        return std::nullopt;
    }
    
    // Return shortest path
    return paths[0];
}

std::vector<TransferPath> TransferGraph::find_all_paths(
    const Address& from,
    const Address& to,
    uint64_t max_hops
) {
    if (!address_exists(from) || !address_exists(to)) {
        return {};
    }
    
    uint64_t start = address_to_id_.at(from);
    uint64_t target = address_to_id_.at(to);
    
    std::vector<uint64_t> visited;
    return dfs_paths(start, target, visited, max_hops);
}

std::vector<TransferPath> TransferGraph::dfs_paths(
    uint64_t current,
    uint64_t target,
    std::vector<uint64_t>& visited,
    uint64_t max_hops
) const {
    std::vector<TransferPath> result;
    
    if (visited.size() >= max_hops) {
        return result;
    }
    
    if (current == target) {
        TransferPath path;
        path.nodes = visited;
        path.nodes.push_back(current);
        path.hops = visited.size();
        return result;
    }
    
    visited.push_back(current);
    
    if (adjacency_out_.count(current)) {
        for (const auto& edge_id : adjacency_out_.at(current)) {
            auto edge = get_edge(edge_id);
            if (!edge) continue;
            
            if (std::find(visited.begin(), visited.end(), edge->target) == visited.end()) {
                auto sub_paths = dfs_paths(edge->target, target, visited, max_hops);
                
                for (auto& path : sub_paths) {
                    path.nodes.insert(path.nodes.begin(), visited.begin(), visited.end());
                    path.edges.push_back(*edge);
                    result.push_back(path);
                }
            }
        }
    }
    
    visited.pop_back();
    return result;
}

GraphStats TransferGraph::compute_stats() const {
    std::lock_guard<std::mutex> lock(graph_mutex_);
    
    GraphStats stats;
    stats.total_nodes = nodes_.size();
    stats.total_edges = edges_.size();
    
    uint64_t total_degree = 0;
    stats.max_degree = 0;
    
    for (const auto& [id, node] : nodes_) {
        uint64_t degree = node.degree_in + node.degree_out;
        total_degree += degree;
        if (degree > stats.max_degree) {
            stats.max_degree = degree;
        }
    }
    
    if (stats.total_nodes > 0) {
        stats.average_degree = static_cast<double>(total_degree) / stats.total_nodes;
        stats.density = static_cast<double>(total_degree) / 
            (stats.total_nodes * (stats.total_nodes - 1));
    }
    
    stats.connected_components = find_connected_components().size();
    
    return stats;
}

std::vector<Node> TransferGraph::find_whales(double threshold) const {
    std::lock_guard<std::mutex> lock(graph_mutex_);
    
    std::vector<Node> whales;
    
    for (const auto& [id, node] : nodes_) {
        uint64_t degree = node.degree_in + node.degree_out;
        double centrality = calculate_degree_centrality(id);
        
        if (centrality >= threshold) {
            whales.push_back(node);
        }
    }
    
    return whales;
}

std::vector<ClusterInfo> TransferGraph::detect_clusters(uint64_t min_size) const {
    return label_propagation();
}

TokenFlow TransferGraph::analyze_token_flow(const Address& token) const {
    TokenFlow flow;
    flow.token_address = token;
    
    if (!token_index_.count(token)) {
        return flow;
    }
    
    for (const auto& edge_id : token_index_.at(token)) {
        auto edge = get_edge(edge_id);
        if (!edge) continue;
        
        flow.transaction_count++;
        
        // This is simplified - real implementation would parse amounts
        flow.total_out = "1";  // Simplified
    }
    
    return flow;
}

QueryResult<Edge> TransferGraph::query_transfers(
    const Address& address,
    Timestamp start_time,
    Timestamp end_time,
    uint64_t page,
    uint64_t page_size
) const {
    QueryResult<Edge> result;
    result.page = page;
    result.page_size = page_size;
    
    auto start = std::chrono::high_resolution_clock::now();
    
    if (!address_exists(address)) {
        result.has_next = false;
        result.query_time_ms = 0;
        return result;
    }
    
    uint64_t node_id = address_to_id_.at(address);
    auto edges = get_edges(node_id, GraphDirection::UNDIRECTED);
    
    for (const auto& edge : edges) {
        if (edge.timestamp >= start_time && edge.timestamp <= end_time) {
            result.data.push_back(edge);
        }
    }
    
    result.total_count = result.data.size();
    
    uint64_t start_idx = page * page_size;
    uint64_t end_idx = std::min(start_idx + page_size, result.data.size());
    
    if (start_idx < result.data.size()) {
        result.data = std::vector<Edge>(
            result.data.begin() + start_idx,
            result.data.begin() + end_idx
        );
        result.has_next = end_idx < result.total_count;
    } else {
        result.data = {};
        result.has_next = false;
    }
    
    auto end = std::chrono::high_resolution_clock::now();
    result.query_time_ms = std::chrono::duration<double, std::milli>(end - start).count();
    
    return result;
}

QueryResult<Node> TransferGraph::query_addresses(
    const std::string& search_term,
    uint64_t page,
    uint64_t page_size
) const {
    QueryResult<Node> result;
    result.page = page;
    result.page_size = page_size;
    
    auto start = std::chrono::high_resolution_clock::now();
    
    for (const auto& [id, node] : nodes_) {
        if (node.type == NodeType::ADDRESS) {
            std::string addr_str = node.address_to_string();
            if (addr_str.find(search_term) != std::string::npos ||
                node.label.find(search_term) != std::string::npos) {
                result.data.push_back(node);
            }
        }
    }
    
    result.total_count = result.data.size();
    
    uint64_t start_idx = page * page_size;
    uint64_t end_idx = std::min(start_idx + page_size, result.data.size());
    
    if (start_idx < result.data.size()) {
        result.data = std::vector<Node>(
            result.data.begin() + start_idx,
            result.data.begin() + end_idx
        );
        result.has_next = end_idx < result.total_count;
    } else {
        result.data = {};
        result.has_next = false;
    }
    
    auto end = std::chrono::high_resolution_clock::now();
    result.query_time_ms = std::chrono::duration<double, std::milli>(end - start).count();
    
    return result;
}

std::vector<uint64_t> TransferGraph::bfs(uint64_t start, uint64_t depth) const {
    std::vector<uint64_t> result;
    std::queue<std::pair<uint64_t, uint64_t>> queue;
    
    queue.push({start, 0});
    std::unordered_set<uint64_t> visited;
    visited.insert(start);
    
    while (!queue.empty()) {
        auto [current, current_depth] = queue.front();
        queue.pop();
        
        if (current_depth > depth) break;
        
        result.push_back(current);
        
        if (adjacency_out_.count(current)) {
            for (const auto& edge_id : adjacency_out_.at(current)) {
                auto edge = get_edge(edge_id);
                if (edge && visited.find(edge->target) == visited.end()) {
                    visited.insert(edge->target);
                    queue.push({edge->target, current_depth + 1});
                }
            }
        }
    }
    
    return result;
}

std::vector<uint64_t> TransferGraph::dfs(uint64_t start, uint64_t depth) const {
    std::vector<uint64_t> result;
    std::stack<std::pair<uint64_t, uint64_t>> stack;
    
    stack.push({start, 0});
    std::unordered_set<uint64_t> visited;
    visited.insert(start);
    
    while (!stack.empty()) {
        auto [current, current_depth] = stack.top();
        stack.pop();
        
        if (current_depth > depth) continue;
        
        result.push_back(current);
        
        if (adjacency_out_.count(current)) {
            for (const auto& edge_id : adjacency_out_.at(current)) {
                auto edge = get_edge(edge_id);
                if (edge && visited.find(edge->target) == visited.end()) {
                    visited.insert(edge->target);
                    stack.push({edge->target, current_depth + 1});
                }
            }
        }
    }
    
    return result;
}

std::vector<std::vector<uint64_t>> TransferGraph::find_connected_components() const {
    std::vector<std::vector<uint64_t>> components;
    std::unordered_set<uint64_t> visited;
    
    for (const auto& [id, node] : nodes_) {
        if (visited.find(id) == visited.end()) {
            std::vector<uint64_t> component;
            auto reachable = bfs(id, std::numeric_limits<uint64_t>::max());
            
            for (const auto& node_id : reachable) {
                if (visited.find(node_id) == visited.end()) {
                    visited.insert(node_id);
                    component.push_back(node_id);
                }
            }
            
            components.push_back(component);
        }
    }
    
    return components;
}

std::vector<TransferPath> TransferGraph::find_cycles(uint64_t start) const {
    // Simplified cycle detection
    return {};
}

double TransferGraph::calculate_degree_centrality(uint64_t node_id) const {
    if (nodes_.empty()) return 0.0;
    
    auto node = get_node(node_id);
    if (!node) return 0.0;
    
    uint64_t degree = node->degree_in + node->degree_out;
    return static_cast<double>(degree) / (nodes_.size() - 1);
}

double TransferGraph::calculate_betweenness_centrality(uint64_t node_id) const {
    // Simplified betweenness centrality
    return calculate_degree_centrality(node_id);
}

double TransferGraph::calculate_pagerank(uint64_t node_id, double damping) const {
    // Simplified PageRank
    return 1.0 / nodes_.size();
}

QueryResult<Edge> TransferGraph::get_recent_transfers(uint64_t limit) const {
    QueryResult<Edge> result;
    result.page = 0;
    result.page_size = limit;
    
    auto start = std::chrono::high_resolution_clock::now();
    
    std::vector<std::pair<Timestamp, Edge>> sorted_edges;
    
    for (const auto& [id, edge] : edges_) {
        sorted_edges.push_back({edge.timestamp, edge});
    }
    
    std::sort(sorted_edges.begin(), sorted_edges.end(), 
        [](const auto& a, const auto& b) { return a.first > b.first; }
    );
    
    for (size_t i = 0; i < std::min(limit, sorted_edges.size()); ++i) {
        result.data.push_back(sorted_edges[i].second);
    }
    
    result.total_count = sorted_edges.size();
    result.has_next = limit < sorted_edges.size();
    
    auto end = std::chrono::high_resolution_clock::now();
    result.query_time_ms = std::chrono::duration<double, std::milli>(end - start).count();
    
    return result;
}

QueryResult<Edge> TransferGraph::get_transfers_by_block(BlockNumber block) const {
    QueryResult<Edge> result;
    result.data = get_edges_in_block(block);
    result.total_count = result.data.size();
    result.has_next = false;
    result.query_time_ms = 0;
    return result;
}

std::vector<Node> TransferGraph::get_active_addresses(Timestamp since) const {
    std::vector<Node> active;
    
    for (const auto& [id, node] : nodes_) {
        if (node.last_updated >= since) {
            active.push_back(node);
        }
    }
    
    return active;
}

Amount TransferGraph::calculate_total_volume(const Address& address) const {
    // Simplified implementation
    return "0";
}

Amount TransferGraph::calculate_token_volume(const Address& token) const {
    return "0";
}

std::unordered_map<Address, Amount> TransferGraph::get_top_tokens(uint64_t count) const {
    return {};
}

std::string TransferGraph::to_graphson() const {
    return to_json();
}

std::string TransferGraph::to_gexf() const {
    std::ostringstream oss;
    oss << "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n";
    oss << "<gexf xmlns=\"http://www.gexf.net/1.2draft\" version=\"1.2\">\n";
    oss << "  <graph mode=\"static\" defaultedgetype=\"directed\">\n";
    oss << "    <nodes>\n";
    
    for (const auto& [id, node] : nodes_) {
        oss << "      <node id=\"" << id << "\" label=\"" << node.address_to_string() << "\"/>\n";
    }
    
    oss << "    </nodes>\n";
    oss << "    <edges>\n";
    
    for (const auto& [id, edge] : edges_) {
        oss << "      <edge id=\"" << id << "\" source=\"" << edge.source 
            << "\" target=\"" << edge.target << "\"/>\n";
    }
    
    oss << "    </edges>\n";
    oss << "  </graph>\n";
    oss << "</gexf>";
    
    return oss.str();
}

std::string TransferGraph::to_json() const {
    std::ostringstream oss;
    oss << "{\"nodes\":[";
    
    bool first = true;
    for (const auto& [id, node] : nodes_) {
        if (!first) oss << ",";
        first = false;
        
        oss << "{\"id\":" << id << ",\"address\":\"" << node.address_to_string() << "\"}";
    }
    
    oss << "],\"edges\":[";
    
    first = true;
    for (const auto& [id, edge] : edges_) {
        if (!first) oss << ",";
        first = false;
        
        oss << "{\"id\":" << id << ",\"source\":" << edge.source 
            << ",\"target\":" << edge.target << ",\"amount\":\"" << edge.amount << "\"}";
    }
    
    oss << "]}";
    return oss.str();
}

void TransferGraph::export_to_file(const std::string& filename) const {
    std::ofstream file(filename);
    file << to_json();
    file.close();
}

std::vector<uint8_t> TransferGraph::serialize() const {
    // Simplified serialization
    return {};
}

void TransferGraph::deserialize(const std::vector<uint8_t>& data) {
    // Simplified deserialization
}

void TransferGraph::clear_cache() {
    std::lock_guard<std::mutex> lock(graph_mutex_);
    stats_cache_.clear();
    centrality_cache_.clear();
}

void TransferGraph::optimize() {
    clear_cache();
}

size_t TransferGraph::memory_usage() const {
    size_t size = sizeof(TransferGraph);
    size += nodes_.size() * sizeof(Node);
    size += edges_.size() * sizeof(Edge);
    size += adjacency_in_.size() * sizeof(std::vector<uint64_t>);
    size += adjacency_out_.size() * sizeof(std::vector<uint64_t>);
    return size;
}

// =============================================================================
// Private Methods
// =============================================================================

void TransferGraph::update_indices(const Node& node) {
    address_to_id_[node.address] = node.id;
}

void TransferGraph::update_indices(const Edge& edge) {
    block_index_[edge.block_number].push_back(edge.id);
    time_index_[edge.timestamp].push_back(edge.id);
}

void TransferGraph::remove_from_indices(uint64_t node_id) {
    // Remove from address index
    auto node_it = nodes_.find(node_id);
    if (node_it != nodes_.end()) {
        address_to_id_.erase(node_it->second.address);
    }
    
    // Remove from adjacency indices
    adjacency_in_.erase(node_id);
    adjacency_out_.erase(node_id);
}

void TransferGraph::remove_from_indices(uint64_t edge_id) {
    // Remove from block and time indices
    auto edge_it = edges_.find(edge_id);
    if (edge_it != edges_.end()) {
        const Edge& edge = edge_it->second;
        
        // Remove from block index
        if (block_index_.count(edge.block_number)) {
            auto& vec = block_index_[edge.block_number];
            vec.erase(std::remove(vec.begin(), vec.end(), edge_id), vec.end());
        }
        
        // Remove from time index
        if (time_index_.count(edge.timestamp)) {
            auto& vec = time_index_[edge.timestamp];
            vec.erase(std::remove(vec.begin(), vec.end(), edge_id), vec.end());
        }
    }
}

std::vector<std::vector<uint64_t>> TransferGraph::find_strongly_connected_components() const {
    return find_connected_components();
}

std::vector<ClusterInfo> TransferGraph::label_propagation() const {
    std::vector<ClusterInfo> clusters;
    
    auto components = find_connected_components();
    
    for (size_t i = 0; i < components.size(); ++i) {
        if (components[i].size() >= 3) {
            ClusterInfo cluster;
            cluster.cluster_id = i;
            cluster.members = components[i];
            cluster.transaction_count = 0;
            cluster.cluster_type = "label_propagation";
            
            clusters.push_back(cluster);
        }
    }
    
    return clusters;
}

// =============================================================================
// TransferStreamParser Implementation
// =============================================================================

TransferStreamParser::TransferStreamParser(TransferGraph& graph) : graph_(graph) {}

TransferStreamParser::~TransferStreamParser() {}

void TransferStreamParser::parse_transaction(const std::vector<uint8_t>& tx_data) {
    // Parse transaction data and extract transfers
}

void TransferStreamParser::parse_log(const std::vector<uint8_t>& log_data) {
    // Parse log and extract transfer events
}

void TransferStreamParser::parse_trace(const std::vector<uint8_t>& trace_data) {
    // Parse trace data
}

void TransferStreamParser::process_block(BlockNumber block_number, const std::vector<uint8_t>& block_data) {
    // Process all transfers in a block
}

void TransferStreamParser::process_erc20_transfer(
    const Address& from, 
    const Address& to, 
    const Amount& amount, 
    const Hash& tx_hash
) {
    Address zero_address{};
    zero_address.fill(0);
    
    graph_.add_transfer(from, to, amount, zero_address, tx_hash, 0, 0, TxType::ERC20_TRANSFER);
}

void TransferStreamParser::process_erc721_transfer(
    const Address& from,
    const Address& to,
    const TokenID& token_id,
    const Hash& tx_hash
) {
    Address zero_address{};
    zero_address.fill(0);
    
    graph_.add_transfer(from, to, token_id, zero_address, tx_hash, 0, 0, TxType::ERC721_SAFE_TRANSFER);
}

void TransferStreamParser::process_erc1155_transfer(
    const Address& from,
    const Address& to,
    const TokenID& token_id,
    const Amount& amount,
    const Hash& tx_hash
) {
    Address zero_address{};
    zero_address.fill(0);
    
    graph_.add_transfer(from, to, amount, zero_address, tx_hash, 0, 0, TxType::ERC1155_TRANSFER);
}

void TransferStreamParser::process_native_transfer(
    const Address& from,
    const Address& to,
    const Amount& amount,
    const Hash& tx_hash
) {
    graph_.add_transfer(from, to, amount, from, tx_hash, 0, 0, TxType::NATIVE_TRANSFER);
}

// =============================================================================
// TransferAnalytics Implementation
// =============================================================================

TransferAnalytics::TransferAnalytics(const TransferGraph& graph) : graph_(graph) {}

TransferAnalytics::~TransferAnalytics() {}

Amount TransferAnalytics::get_24h_volume() const {
    return "0";
}

Amount TransferAnalytics::get_weekly_volume() const {
    return "0";
}

Amount TransferAnalytics::get_monthly_volume() const {
    return "0";
}

std::vector<Address> TransferAnalytics::detect_wash_trading(double threshold) const {
    return {};
}

std::vector<std::pair<Address, Address>> TransferAnalytics::detect_cycles(double min_volume) const {
    return {};
}

std::vector<Address> TransferAnalytics::detect_ponzi_schemes() const {
    return {};
}

double TransferAnalytics::calculate_network_density() const {
    auto stats = graph_.compute_stats();
    return stats.density;
}

double TransferAnalytics::calculate_average_clustering() const {
    return 0.0;
}

uint64_t TransferAnalytics::calculate_diameter() const {
    return 0;
}

std::vector<Address> TransferAnalytics::detect_anomalies(double z_score_threshold) const {
    return {};
}

std::vector<Edge> TransferAnalytics::detect_suspicious_transfers() const {
    return {};
}

std::vector<std::pair<Timestamp, Amount>> TransferAnalytics::get_volume_time_series(
    Timestamp start, 
    Timestamp end
) const {
    return {};
}

std::vector<std::pair<Timestamp, uint64_t>> TransferAnalytics::get_transaction_time_series(
    Timestamp start, 
    Timestamp end
) const {
    return {};
}

std::vector<Address> TransferAnalytics::find_common_receivers(const Address& address) const {
    return {};
}

std::vector<Address> TransferAnalytics::find_common_senders(const Address& address) const {
    return {};
}

bool TransferAnalytics::is_wash_trading_pattern(const Address& a, const Address& b) const {
    return false;
}

// =============================================================================
// Utility Functions
// =============================================================================

std::string node_type_to_string(NodeType type) {
    switch (type) {
        case NodeType::ADDRESS: return "address";
        case NodeType::TOKEN: return "token";
        case NodeType::TRANSACTION: return "transaction";
        case NodeType::BLOCK: return "block";
        case NodeType::CONTRACT: return "contract";
        default: return "unknown";
    }
}

NodeType string_to_node_type(const std::string& str) {
    if (str == "address") return NodeType::ADDRESS;
    if (str == "token") return NodeType::TOKEN;
    if (str == "transaction") return NodeType::TRANSACTION;
    if (str == "block") return NodeType::BLOCK;
    if (str == "contract") return NodeType::CONTRACT;
    return NodeType::ADDRESS;
}

std::string edge_type_to_string(EdgeType type) {
    switch (type) {
        case EdgeType::TRANSFER: return "transfer";
        case EdgeType::APPROVAL: return "approval";
        case EdgeType::MINT: return "mint";
        case EdgeType::BURN: return "burn";
        case EdgeType::SWAP: return "swap";
        case EdgeType::BRIDGE: return "bridge";
        default: return "unknown";
    }
}

EdgeType string_to_edge_type(const std::string& str) {
    if (str == "transfer") return EdgeType::TRANSFER;
    if (str == "approval") return EdgeType::APPROVAL;
    if (str == "mint") return EdgeType::MINT;
    if (str == "burn") return EdgeType::BURN;
    if (str == "swap") return EdgeType::SWAP;
    if (str == "bridge") return EdgeType::BRIDGE;
    return EdgeType::TRANSFER;
}

std::string tx_type_to_string(TxType type) {
    switch (type) {
        case TxType::ERC20_TRANSFER: return "erc20_transfer";
        case TxType::ERC20_TRANSFER_FROM: return "erc20_transfer_from";
        case TxType::ERC721_TRANSFER: return "erc721_transfer";
        case TxType::ERC721_SAFE_TRANSFER: return "erc721_safe_transfer";
        case TxType::ERC1155_TRANSFER: return "erc1155_transfer";
        case TxType::NATIVE_TRANSFER: return "native_transfer";
        default: return "unknown";
    }
}

TxType string_to_tx_type(const std::string& str) {
    if (str == "erc20_transfer") return TxType::ERC20_TRANSFER;
    if (str == "erc20_transfer_from") return TxType::ERC20_TRANSFER_FROM;
    if (str == "erc721_transfer") return TxType::ERC721_TRANSFER;
    if (str == "erc721_safe_transfer") return TxType::ERC721_SAFE_TRANSFER;
    if (str == "erc1155_transfer") return TxType::ERC1155_TRANSFER;
    if (str == "native_transfer") return TxType::NATIVE_TRANSFER;
    return TxType::NATIVE_TRANSFER;
}

Address address_from_string(const std::string& str) {
    return Node::string_to_address(str);
}

std::string address_to_string(const Address& addr) {
    std::ostringstream oss;
    oss << "0x";
    for (const auto& byte : addr) {
        oss << std::hex << std::setw(2) << std::setfill('0') << (int)byte;
    }
    return oss.str();
}

Hash hash_from_string(const std::string& str) {
    Hash hash{};
    std::string hex_str = str;
    if (hex_str.substr(0, 2) == "0x") {
        hex_str = hex_str.substr(2);
    }
    for (size_t i = 0; i < hex_str.length() && i < 64; i += 2) {
        hash[i / 2] = static_cast<uint8_t>(std::stoi(hex_str.substr(i, 2), nullptr, 16));
    }
    return hash;
}

std::string hash_to_string(const Hash& hash) {
    std::ostringstream oss;
    oss << "0x";
    for (const auto& byte : hash) {
        oss << std::hex << std::setw(2) << std::setfill('0') << (int)byte;
    }
    return oss.str();
}

} // namespace transfer
} // namespace tigerchain
