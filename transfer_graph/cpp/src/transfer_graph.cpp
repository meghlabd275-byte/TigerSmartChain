/**
 * TigerScan Transfer Graph Service - Implementation
 * 
 * High-performance C++ implementation for transfer tracking
 * and flow analysis.
 */

#include "transfer_graph.hpp"
#include <iostream>
#include <sstream>
#include <algorithm>
#include <numeric>
#include <cmath>

namespace tigerscan {

// TransferAnalyzer Implementation
TransferAnalyzer::TransferAnalyzer() {}

TransferAnalyzer::~TransferAnalyzer() {}

std::vector<TransferFlow> TransferAnalyzer::analyze_flows(
    const std::string& address,
    const std::string& token,
    int limit
) {
    auto transfers = fetch_transfers(token, address, limit);
    
    std::map<std::string, TransferFlow> flows;
    
    for (const auto& transfer : transfers) {
        std::string counterparty;
        
        if (transfer.from_address == address) {
            counterparty = transfer.to_address;
        } else if (transfer.to_address == address) {
            counterparty = transfer.from_address;
        } else {
            continue;
        }
        
        if (!flows.count(counterparty)) {
            flows[counterparty] = TransferFlow();
            flows[counterparty].from_address = address;
            flows[counterparty].to_address = counterparty;
            flows[counterparty].first_transfer = std::to_string(transfer.timestamp);
        }
        
        if (transfer.from_address == address) {
            flows[counterparty].amount += transfer.amount;
            flows[counterparty].amount_usd += transfer.usd_value;
            flows[counterparty].outgoing = true;
        } else {
            flows[counterparty].amount += transfer.amount;
            flows[counterparty].amount_usd += transfer.usd_value;
            flows[counterparty].incoming = true;
        }
        
        flows[counterparty].transfer_count++;
        flows[counterparty].last_transfer = std::to_string(transfer.timestamp);
    }
    
    // Determine flow type
    for (auto& [_, flow] : flows) {
        if (flow.outgoing && flow.incoming) {
            flow.flow_type = "swap";
        } else if (flow.outgoing) {
            flow.flow_type = "withdrawal";
        } else {
            flow.flow_type = "deposit";
        }
    }
    
    std::vector<TransferFlow> result;
    for (auto& [_, flow] : flows) {
        result.push_back(flow);
    }
    
    return result;
}

std::vector<Transfer> TransferAnalyzer::detect_large_transfers(
    const std::string& token,
    double threshold_usd,
    int limit
) {
    auto transfers = fetch_transfers(token, "", 10000);
    
    std::vector<Transfer> large;
    for (const auto& transfer : transfers) {
        if (transfer.usd_value >= threshold_usd) {
            transfer.is_large_transfer = true;
            large.push_back(transfer);
            
            if ((int)large.size() >= limit) break;
        }
    }
    
    // Sort by USD value descending
    std::sort(large.begin(), large.end(),
             [](const Transfer& a, const Transfer& b) {
                 return a.usd_value > b.usd_value;
             });
    
    return large;
}

std::map<std::string, double> TransferAnalyzer::analyze_timing_patterns(
    const std::string& address
) {
    auto transfers = fetch_transfers("", address, 1000);
    
    std::map<std::string, int> hour_counts;
    std::map<std::string, double> hour_volumes;
    
    for (const auto& transfer : transfers) {
        // Extract hour from timestamp (simplified)
        std::string hour = "00"; // Would parse actual timestamp
        
        hour_counts[hour]++;
        hour_volumes[hour] += transfer.usd_value;
    }
    
    std::map<std::string, double> patterns;
    for (const auto& [hour, count] : hour_counts) {
        patterns[hour + "_count"] = count;
    }
    for (const auto& [hour, volume] : hour_volumes) {
        patterns[hour + "_volume"] = volume;
    }
    
    return patterns;
}

double TransferAnalyzer::calculate_velocity(
    const std::string& address,
    const std::string& token,
    int days
) {
    auto transfers = fetch_transfers(token, address, 10000);
    
    if (transfers.empty()) return 0;
    
    // Calculate days since first transfer
    // In production, would use actual timestamps
    
    return static_cast<double>(transfers.size()) / days;
}

bool TransferAnalyzer::detect_wash_trading(const std::string& address) {
    auto transfers = fetch_transfers("", address, 1000);
    
    if (transfers.size() < 10) return false;
    
    // Check for circular transfers (A -> B -> A)
    std::set<std::string> counterparties;
    for (const auto& t : transfers) {
        if (t.from_address == address) {
            counterparties.insert(t.to_address);
        }
    }
    
    int circular = 0;
    for (const auto& t : transfers) {
        if (t.to_address == address && 
            counterparties.count(t.from_address)) {
            circular++;
        }
    }
    
    // If more than 30% are circular, likely wash trading
    return (static_cast<double>(circular) / transfers.size()) > 0.3;
}

std::vector<std::pair<std::string, double>> TransferAnalyzer::get_top_senders(
    const std::string& token,
    int limit
) {
    auto transfers = fetch_transfers(token, "", 10000);
    
    std::map<std::string, double> sender_amounts;
    for (const auto& t : transfers) {
        sender_amounts[t.from_address] += t.usd_value;
    }
    
    std::vector<std::pair<std::string, double>> result(sender_amounts.begin(), 
                                                        sender_amounts.end());
    
    std::sort(result.begin(), result.end(),
             [](const auto& a, const auto& b) {
                 return a.second > b.second;
             });
    
    if ((int)result.size() > limit) {
        result.resize(limit);
    }
    
    return result;
}

std::vector<std::pair<std::string, double>> TransferAnalyzer::get_top_receivers(
    const std::string& token,
    int limit
) {
    auto transfers = fetch_transfers(token, "", 10000);
    
    std::map<std::string, double> receiver_amounts;
    for (const auto& t : transfers) {
        receiver_amounts[t.to_address] += t.usd_value;
    }
    
    std::vector<std::pair<std::string, double>> result(receiver_amounts.begin(), 
                                                        receiver_amounts.end());
    
    std::sort(result.begin(), result.end(),
             [](const auto& a, const auto& b) {
                 return a.second > b.second;
             });
    
    if ((int)result.size() > limit) {
        result.resize(limit);
    }
    
    return result;
}

std::vector<Transfer> TransferAnalyzer::fetch_transfers(
    const std::string& token,
    const std::string& address,
    int limit
) {
    // This would fetch from blockchain/RPC
    return {};
}

// TransferGraphBuilder Implementation
TransferGraphBuilder::TransferGraphBuilder() {}

TransferGraphBuilder::~TransferGraphBuilder() {}

TransferGraph TransferGraphBuilder::build_graph(
    const std::string& token_address,
    uint64_t start_block,
    uint64_t end_block
) {
    TransferGraph graph;
    graph.token_address = token_address;
    graph.start_block = start_block;
    graph.end_block = end_block;
    graph.generated_at = std::chrono::system_clock::now();
    
    graph.transfers = fetch_transfers_in_range(token_address, start_block, end_block);
    
    // Build address index
    for (const auto& t : graph.transfers) {
        graph.address_transfers[t.from_address].push_back(t);
        graph.address_transfers[t.to_address].push_back(t);
    }
    
    return graph;
}

std::map<std::string, std::vector<Transfer>> TransferGraphBuilder::build_address_graph(
    const std::string& token_address,
    uint64_t start_block,
    uint64_t end_block
) {
    auto graph = build_graph(token_address, start_block, end_block);
    return graph.address_transfers;
}

std::vector<TimelineEntry> TransferGraphBuilder::generate_timeline(
    const std::string& token_address,
    const std::string& address,
    int limit
) {
    std::vector<TimelineEntry> timeline;
    
    auto transfers = fetch_transfers_in_range(token_address, 0, UINT64_MAX);
    
    for (const auto& t : transfers) {
        if (!address.empty() && 
            t.from_address != address && 
            t.to_address != address) {
            continue;
        }
        
        TimelineEntry entry;
        entry.block_number = t.block_number;
        entry.timestamp = t.timestamp;
        entry.hash = t.transaction_hash;
        entry.from = t.from_address;
        entry.to = t.to_address;
        entry.amount = t.amount;
        entry.amount_usd = t.usd_value;
        entry.type = "transfer";
        
        timeline.push_back(entry);
        
        if ((int)timeline.size() >= limit) break;
    }
    
    // Sort by timestamp descending (most recent first)
    std::sort(timeline.begin(), timeline.end(),
             [](const TimelineEntry& a, const TimelineEntry& b) {
                 return a.timestamp > b.timestamp;
             });
    
    return timeline;
}

std::vector<Transfer> TransferGraphBuilder::fetch_transfers_in_range(
    const std::string& token,
    uint64_t start_block,
    uint64_t end_block
) {
    // This would fetch from blockchain
    return {};
}

// TransferGraphService Implementation
TransferGraphService::TransferGraphService() {
    analyzer_ = std::make_unique<TransferAnalyzer>();
    builder_ = std::make_unique<TransferGraphBuilder>();
}

TransferGraphService::~TransferGraphService() {}

void TransferGraphService::initialize(const std::string& ethereum_rpc,
                                    const std::string& redis_url) {
    ethereum_rpc_ = ethereum_rpc;
}

TransferStats TransferGraphService::get_stats(const std::string& token) {
    // Check cache
    {
        std::lock_guard lock(cache_mutex_);
        if (stats_cache_.count(token)) {
            return stats_cache_[token];
        }
    }
    
    auto stats = calculate_stats(token);
    
    {
        std::lock_guard lock(cache_mutex_);
        stats_cache_[token] = stats;
    }
    
    return stats;
}

std::vector<Transfer> TransferGraphService::get_transfers(
    const std::string& address,
    const std::string& token,
    int limit
) {
    return fetch_from_rpc(address, token, limit);
}

std::vector<TransferFlow> TransferGraphService::get_flows(
    const std::string& address,
    const std::string& token,
    int limit
) {
    return analyzer_->analyze_flows(address, token, limit);
}

std::vector<TimelineEntry> TransferGraphService::get_timeline(
    const std::string& address,
    const std::string& token,
    int limit
) {
    return builder_->generate_timeline(token, address, limit);
}

std::vector<Transfer> TransferGraphService::get_large_transfers(
    const std::string& token,
    double min_usd,
    int limit
) {
    return analyzer_->detect_large_transfers(token, min_usd, limit);
}

std::vector<Transfer> TransferGraphService::search(
    const std::string& query,
    const std::string& token,
    int limit
) {
    // Simple search - in production would use more sophisticated matching
    auto transfers = fetch_from_rpc("", token, 1000);
    
    std::vector<Transfer> results;
    for (const auto& t : transfers) {
        if (t.transaction_hash.find(query) != std::string::npos ||
            t.from_address.find(query) != std::string::npos ||
            t.to_address.find(query) != std::string::npos) {
            results.push_back(t);
            
            if ((int)results.size() >= limit) break;
        }
    }
    
    return results;
}

void TransferGraphService::subscribe(const std::string& token, TransferCallback callback) {
    std::lock_guard lock(subscriber_mutex_);
    subscribers_[token].push_back(callback);
}

void TransferGraphService::unsubscribe(const std::string& token) {
    std::lock_guard lock(subscriber_mutex_);
    subscribers_.erase(token);
}

TransferStats TransferGraphService::calculate_stats(const std::string& token) {
    TransferStats stats;
    stats.token_address = token;
    
    auto transfers = fetch_from_rpc("", token, 10000);
    
    stats.total_transfers = transfers.size();
    
    std::set<std::string> senders;
    std::set<std::string> receivers;
    double total_volume = 0;
    
    std::vector<double> amounts;
    
    for (const auto& t : transfers) {
        senders.insert(t.from_address);
        receivers.insert(t.to_address);
        total_volume += t.usd_value;
        amounts.push_back(t.amount);
        
        if (t.usd_value > 10000) {
            stats.large_transfers++;
        }
        
        if (t.is_suspicious) {
            stats.suspicious_transfers++;
        }
    }
    
    stats.unique_senders = senders.size();
    stats.unique_receivers = receivers.size();
    stats.total_volume = total_volume;
    stats.total_volume_usd = total_volume;
    
    if (!amounts.empty()) {
        double sum = std::accumulate(amounts.begin(), amounts.end(), 0.0);
        stats.avg_transfer_size = sum / amounts.size();
        
        std::sort(amounts.begin(), amounts.end());
        size_t n = amounts.size();
        if (n % 2 == 0) {
            stats.median_transfer_size = (amounts[n/2 - 1] + amounts[n/2]) / 2.0;
        } else {
            stats.median_transfer_size = amounts[n/2];
        }
    }
    
    return stats;
}

std::vector<Transfer> TransferGraphService::fetch_from_rpc(
    const std::string& address,
    const std::string& token,
    int limit
) {
    // This would make RPC calls to fetch transfers
    return {};
}

} // namespace tigerscan
