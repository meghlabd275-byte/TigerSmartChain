/**
 * TigerScan Holder Graph Service - Implementation
 * 
 * High-performance C++ implementation for holder network analysis
 * with graph algorithms and whale detection.
 */

#include "holder_graph.hpp"
#include <iostream>
#include <sstream>
#include <iomanip>
#include <random>
#include <cmath>

namespace tigerscan {

// GraphAlgorithms Implementation
std::map<std::string, double> GraphAlgorithms::degree_centrality(
    const std::vector<HolderRelationship>& relationships
) {
    std::map<std::string, int> degree_count;
    
    for (const auto& rel : relationships) {
        degree_count[rel.from_address]++;
        degree_count[rel.to_address]++;
    }
    
    int max_degree = 0;
    for (const auto& [_, degree] : degree_count) {
        if (degree > max_degree) max_degree = degree;
    }
    
    std::map<std::string, double> centrality;
    for (const auto& [address, degree] : degree_count) {
        if (max_degree > 0) {
            centrality[address] = static_cast<double>(degree) / max_degree;
        } else {
            centrality[address] = 0;
        }
    }
    
    return centrality;
}

std::vector<std::vector<std::string>> GraphAlgorithms::find_connected_components(
    const std::vector<HolderRelationship>& relationships
) {
    // Build adjacency list
    std::unordered_map<std::string, std::vector<std::string>> adj;
    std::set<std::string> all_nodes;
    
    for (const auto& rel : relationships) {
        adj[rel.from_address].push_back(rel.to_address);
        adj[rel.to_address].push_back(rel.from_address);
        all_nodes.insert(rel.from_address);
        all_nodes.insert(rel.to_address);
    }
    
    std::vector<std::vector<std::string>> components;
    std::set<std::string> visited;
    
    for (const auto& start : all_nodes) {
        if (visited.count(start)) continue;
        
        std::vector<std::string> component;
        std::queue<std::string> q;
        
        q.push(start);
        visited.insert(start);
        
        while (!q.empty()) {
            std::string node = q.front();
            q.pop();
            component.push_back(node);
            
            for (const auto& neighbor : adj[node]) {
                if (!visited.count(neighbor)) {
                    visited.insert(neighbor);
                    q.push(neighbor);
                }
            }
        }
        
        components.push_back(component);
    }
    
    return components;
}

std::map<std::string, int> GraphAlgorithms::community_detection(
    const std::vector<HolderRelationship>& relationships,
    int max_iterations
) {
    // Build adjacency list and initialize labels
    std::unordered_map<std::string, std::vector<std::string>> adj;
    std::unordered_map<std::string, int> labels;
    std::unordered_map<std::string, std::string> original_addresses;
    
    int node_id = 0;
    for (const auto& rel : relationships) {
        if (!labels.count(rel.from_address)) {
            labels[rel.from_address] = node_id++;
            original_addresses[rel.from_address] = rel.from_address;
        }
        if (!labels.count(rel.to_address)) {
            labels[rel.to_address] = node_id++;
            original_addresses[rel.to_address] = rel.to_address;
        }
        
        adj[rel.from_address].push_back(rel.to_address);
        adj[rel.to_address].push_back(rel.from_address);
    }
    
    // Simple label propagation
    for (int iter = 0; iter < max_iterations; iter++) {
        std::unordered_map<std::string, int> new_labels = labels;
        
        for (const auto& [node, neighbors] : adj) {
            if (neighbors.empty()) continue;
            
            std::map<int, int> label_count;
            for (const auto& neighbor : neighbors) {
                if (labels.count(neighbor)) {
                    label_count[labels[neighbor]]++;
                }
            }
            
            if (!label_count.empty()) {
                int max_label = label_count.begin()->first;
                int max_count = label_count.begin()->second;
                
                for (const auto& [label, count] : label_count) {
                    if (count > max_count) {
                        max_count = count;
                        max_label = label;
                    }
                }
                
                new_labels[node] = max_label;
            }
        }
        
        labels = new_labels;
    }
    
    return labels;
}

std::map<std::string, double> GraphAlgorithms::pagerank(
    const std::vector<HolderRelationship>& relationships,
    double damping_factor,
    int max_iterations
) {
    // Build adjacency list
    std::unordered_map<std::string, std::vector<std::string>> adj;
    std::set<std::string> all_nodes;
    std::unordered_map<std::string, int> out_degree;
    
    for (const auto& rel : relationships) {
        adj[rel.from_address].push_back(rel.to_address);
        all_nodes.insert(rel.from_address);
        all_nodes.insert(rel.to_address);
        out_degree[rel.from_address]++;
    }
    
    // Initialize PageRank
    std::unordered_map<std::string, double> pr;
    double init_pr = 1.0 / all_nodes.size();
    
    for (const auto& node : all_nodes) {
        pr[node] = init_pr;
    }
    
    // Iterate
    for (int iter = 0; iter < max_iterations; iter++) {
        std::unordered_map<std::string, double> new_pr;
        
        // Teleport contribution
        double teleport = (1 - damping_factor) / all_nodes.size();
        
        for (const auto& node : all_nodes) {
            double sum = 0;
            
            // Sum contributions from incoming edges
            for (const auto& [from, to_list] : adj) {
                for (const auto& to : to_list) {
                    if (to == node && out_degree[from] > 0) {
                        sum += pr[from] / out_degree[from];
                    }
                }
            }
            
            new_pr[node] = teleport + damping_factor * sum;
        }
        
        pr = new_pr;
    }
    
    return std::map<std::string, double>(pr.begin(), pr.end());
}

double GraphAlgorithms::clustering_coefficient(
    const std::vector<HolderRelationship>& relationships
) {
    // Build adjacency set
    std::unordered_map<std::string, std::set<std::string>> adj;
    
    for (const auto& rel : relationships) {
        adj[rel.from_address].insert(rel.to_address);
        adj[rel.to_address].insert(rel.from_address);
    }
    
    double total_coefficient = 0;
    int node_count = 0;
    
    for (const auto& [node, neighbors] : adj) {
        if (neighbors.size() < 2) continue;
        
        int edges = 0;
        std::vector<std::string> neighbor_vec(neighbors.begin(), neighbors.end());
        
        for (size_t i = 0; i < neighbor_vec.size(); i++) {
            for (size_t j = i + 1; j < neighbor_vec.size(); j++) {
                if (adj[neighbor_vec[i]].count(neighbor_vec[j])) {
                    edges++;
                }
            }
        }
        
        int possible_edges = neighbors.size() * (neighbors.size() - 1) / 2;
        if (possible_edges > 0) {
            total_coefficient += static_cast<double>(edges) / possible_edges;
            node_count++;
        }
    }
    
    return node_count > 0 ? total_coefficient / node_count : 0;
}

// HolderGraphClient Implementation
HolderGraphClient::HolderGraphClient() : curl_(nullptr) {
    curl_global_init(CURL_GLOBAL_DEFAULT);
    curl_ = curl_easy_init();
}

HolderGraphClient::~HolderGraphClient() {
    if (curl_) curl_easy_cleanup(curl_);
    curl_global_cleanup();
}

void HolderGraphClient::set_api_key(const std::string& key) {
    api_key_ = key;
}

size_t HolderGraphClient::write_callback(void* contents, size_t size, size_t nmemb, void* userp) {
    ((std::string*)userp)->append((char*)contents, size * nmemb);
    return size * nmemb;
}

std::string HolderGraphClient::get(const std::string& url,
                                  const std::map<std::string, std::string>& headers,
                                  int timeout_ms) {
    CURL* curl = curl_easy_init();
    if (!curl) return "";
    
    std::string response;
    
    curl_easy_setopt(curl, CURLOPT_URL, url.c_str());
    curl_easy_setopt(curl, CURLOPT_WRITEFUNCTION, write_callback);
    curl_easy_setopt(curl, CURLOPT_WRITEDATA, &response);
    curl_easy_setopt(curl, CURLOPT_FOLLOWLOCATION, 1L);
    curl_easy_setopt(curl, CURLOPT_TIMEOUT_MS, timeout_ms);
    
    struct curl_slist* header_list = nullptr;
    for (const auto& header : headers) {
        header_list = curl_slist_append(header_list, (header.first + ": " + header.second).c_str());
    }
    
    if (!api_key_.empty()) {
        header_list = curl_slist_append(header_list, ("X-API-Key: " + api_key_).c_str());
    }
    
    if (header_list) {
        curl_easy_setopt(curl, CURLOPT_HTTPHEADER, header_list);
    }
    
    curl_easy_perform(curl);
    
    if (header_list) curl_slist_free_all(header_list);
    curl_easy_cleanup(curl);
    
    return response;
}

// HolderGraphService Implementation
HolderGraphService::HolderGraphService() {
    client_ = std::make_unique<HolderGraphClient>();
}

HolderGraphService::~HolderGraphService() {}

void HolderGraphService::initialize(const std::string& ethereum_rpc,
                                   const std::string& redis_url,
                                   const std::string& postgres_url) {
    ethereum_rpc_ = ethereum_rpc;
}

HolderGraph HolderGraphService::generate_graph(
    const std::string& token_address,
    int max_holders,
    bool include_relationships
) {
    HolderGraph graph;
    graph.token_address = token_address;
    graph.generated_at = std::chrono::system_clock::now();
    
    // Fetch holders
    graph.holders = fetch_holders_from_chain(token_address, max_holders);
    graph.total_holders = graph.holders.size();
    
    // Detect whales
    detect_whales(graph.holders);
    
    // Calculate total supply
    double total_supply = 0;
    for (const auto& holder : graph.holders) {
        total_supply += holder.balance;
    }
    graph.total_supply = total_supply;
    
    // Calculate percentages
    for (auto& holder : graph.holders) {
        if (total_supply > 0) {
            holder.percent_supply = (holder.balance / total_supply) * 100;
        }
    }
    
    // Fetch relationships if requested
    if (include_relationships) {
        for (const auto& holder : graph.holders) {
            auto rels = fetch_relationships(token_address, holder.address);
            graph.relationships.insert(graph.relationships.end(), rels.begin(), rels.end());
        }
        
        // Detect clusters
        graph.clusters = detect_clusters(graph.holders, graph.relationships);
    }
    
    return graph;
}

Holder HolderGraphService::get_holder(const std::string& address, const std::string& token) {
    // Check cache first
    std::string cache_key = address + ":" + token;
    
    {
        std::shared_lock lock(cache_mutex_);
        if (holder_cache_.count(cache_key)) {
            return holder_cache_[cache_key];
        }
    }
    
    // Fetch from chain
    Holder holder;
    holder.address = address;
    
    // This would make RPC calls to get holder data
    calculate_holder_metrics(holder, token);
    
    // Cache
    {
        std::unique_lock lock(cache_mutex_);
        holder_cache_[cache_key] = holder;
    }
    
    return holder;
}

std::vector<Holder> HolderGraphService::get_whale_holders(const std::string& token, int limit) {
    auto holders = fetch_holders_from_chain(token, 10000);
    detect_whales(holders);
    
    // Sort by balance descending
    std::sort(holders.begin(), holders.end(),
             [](const Holder& a, const Holder& b) {
                 return a.balance > b.balance;
             });
    
    // Take top holders that are marked as whales
    std::vector<Holder> whales;
    for (const auto& holder : holders) {
        if (holder.is_whale) {
            whales.push_back(holder);
            if ((int)whales.size() >= limit) break;
        }
    }
    
    return whales;
}

std::vector<HolderRelationship> HolderGraphService::get_relationships(
    const std::string& address,
    int limit
) {
    return fetch_relationships("", address);
}

std::vector<HolderCluster> HolderGraphService::get_clusters(const std::string& token) {
    auto holders = fetch_holders_from_chain(token, 1000);
    detect_whales(holders);
    
    std::vector<HolderRelationship> relationships;
    for (const auto& holder : holders) {
        auto rels = fetch_relationships(token, holder.address);
        relationships.insert(relationships.end(), rels.begin(), rels.end());
    }
    
    return detect_clusters(holders, relationships);
}

GraphMetrics HolderGraphService::get_metrics(const std::string& token) {
    auto holders = fetch_holders_from_chain(token, 1000);
    std::vector<HolderRelationship> relationships;
    
    for (const auto& holder : holders) {
        auto rels = fetch_relationships(token, holder.address);
        relationships.insert(relationships.end(), rels.begin(), rels.end());
    }
    
    return calculate_metrics(holders, relationships);
}

std::vector<TransferPattern> HolderGraphService::analyze_patterns(const std::string& token) {
    auto holders = fetch_holders_from_chain(token, 1000);
    std::vector<TransferPattern> patterns;
    
    for (const auto& holder : holders) {
        patterns.push_back(analyze_address_pattern(holder.address, token));
    }
    
    return patterns;
}

std::vector<Holder> HolderGraphService::search_holders(
    const std::string& token,
    const std::string& query,
    int limit
) {
    auto holders = fetch_holders_from_chain(token, 1000);
    std::vector<Holder> results;
    
    // Simple search - check if query is substring of address
    for (const auto& holder : holders) {
        if (holder.address.find(query) != std::string::npos ||
            (holder.tags.size() > 0 && 
             holder.tags[0].find(query) != std::string::npos)) {
            results.push_back(holder);
            if ((int)results.size() >= limit) break;
        }
    }
    
    return results;
}

void HolderGraphService::subscribe(const std::string& token, HolderUpdateCallback callback) {
    std::lock_guard lock(subscriber_mutex_);
    subscribers_[token].push_back(callback);
}

void HolderGraphService::unsubscribe(const std::string& token) {
    std::lock_guard lock(subscriber_mutex_);
    subscribers_.erase(token);
}

// Private methods
std::vector<Holder> HolderGraphService::fetch_holders_from_chain(
    const std::string& token,
    int limit
) {
    std::vector<Holder> holders;
    
    // This would fetch from the blockchain/RPC
    // For now, return empty vector - real implementation would
    // query token contract for holder list
    
    return holders;
}

std::vector<HolderRelationship> HolderGraphService::fetch_relationships(
    const std::string& token,
    const std::string& address
) {
    std::vector<HolderRelationship> relationships;
    
    // This would fetch transaction history and build relationships
    
    return relationships;
}

void HolderGraphService::calculate_holder_metrics(Holder& holder, const std::string& token) {
    // Calculate metrics based on transaction history
    // In production, this would query historical data
    
    holder.transaction_count = 0;
    holder.first_seen = "2020-01-01";
    holder.last_active = "2024-01-01";
}

void HolderGraphService::detect_whales(std::vector<Holder>& holders) {
    if (holders.empty()) return;
    
    // Sort by balance
    std::sort(holders.begin(), holders.end(),
             [](const Holder& a, const Holder& b) {
                 return a.balance > b.balance;
             });
    
    // Top 10% are whales
    int whale_count = std::max(1, static_cast<int>(holders.size() * 0.1));
    
    for (int i = 0; i < (int)holders.size(); i++) {
        holders[i].is_whale = (i < whale_count);
    }
}

std::vector<HolderCluster> HolderGraphService::detect_clusters(
    const std::vector<Holder>& holders,
    const std::vector<HolderRelationship>& relationships
) {
    std::vector<HolderCluster> clusters;
    
    // Use community detection
    auto communities = GraphAlgorithms::community_detection(relationships, 10);
    
    // Group by community
    std::map<int, std::vector<std::string>> community_members;
    for (const auto& holder : holders) {
        if (communities.count(holder.address)) {
            community_members[communities[holder.address]].push_back(holder.address);
        }
    }
    
    // Create clusters
    for (const auto& [community_id, members] : community_members) {
        HolderCluster cluster;
        cluster.cluster_id = community_id;
        cluster.members = members;
        
        // Calculate total balance
        for (const auto& holder : holders) {
            for (const auto& member : members) {
                if (holder.address == member) {
                    cluster.total_balance += holder.balance;
                }
            }
        }
        
        // Determine cluster type based on average balance
        double avg_balance = members.empty() ? 0 : cluster.total_balance / members.size();
        if (avg_balance > 1000000) {
            cluster.cluster_type = "whale";
        } else if (avg_balance > 100000) {
            cluster.cluster_type = "large";
        } else if (avg_balance > 10000) {
            cluster.cluster_type = "medium";
        } else {
            cluster.cluster_type = "small";
        }
        
        clusters.push_back(cluster);
    }
    
    return clusters;
}

GraphMetrics HolderGraphService::calculate_metrics(
    const std::vector<Holder>& holders,
    const std::vector<HolderRelationship>& relationships
) {
    GraphMetrics metrics;
    
    metrics.total_nodes = holders.size();
    metrics.total_edges = relationships.size();
    
    if (metrics.total_nodes > 0) {
        metrics.average_degree = static_cast<double>(2 * metrics.total_edges) / metrics.total_nodes;
    }
    
    // Find max degree
    std::map<std::string, int> degree_count;
    for (const auto& rel : relationships) {
        degree_count[rel.from_address]++;
        degree_count[rel.to_address]++;
    }
    
    for (const auto& [_, degree] : degree_count) {
        if (degree > metrics.max_degree) {
            metrics.max_degree = degree;
        }
    }
    
    // Find largest whale
    double max_balance = 0;
    for (const auto& holder : holders) {
        if (holder.balance > max_balance) {
            max_balance = holder.balance;
            metrics.largest_whale = holder.address;
            metrics.largest_whale_balance = holder.balance;
        }
    }
    
    // Calculate connected components
    auto components = GraphAlgorithms::find_connected_components(relationships);
    metrics.connected_components = components.size();
    
    // Calculate clustering coefficient
    metrics.clustering_coefficient = GraphAlgorithms::clustering_coefficient(relationships);
    
    // Calculate density
    int max_edges = metrics.total_nodes * (metrics.total_nodes - 1) / 2;
    if (max_edges > 0) {
        metrics.density = static_cast<double>(metrics.total_edges) / max_edges;
    }
    
    return metrics;
}

TransferPattern HolderGraphService::analyze_address_pattern(
    const std::string& address,
    const std::string& token
) {
    TransferPattern pattern;
    pattern.address = address;
    
    // This would analyze transaction history to determine pattern
    // For now, return basic pattern
    
    pattern.pattern_type = "normal";
    pattern.avg_inflow = 0;
    pattern.avg_outflow = 0;
    pattern.inflow_count = 0;
    pattern.outflow_count = 0;
    pattern.hold_time_avg = 30;
    pattern.hold_time_std = 0;
    
    return pattern;
}

} // namespace tigerscan
