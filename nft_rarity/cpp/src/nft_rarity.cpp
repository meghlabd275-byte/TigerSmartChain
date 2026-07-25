/**
 * @file nft_rarity.cpp
 * @brief NFT Rarity Calculator Implementation
 * @author TigerScan Team
 */

#include "nft_rarity.hpp"
#include <algorithm>
#include <sstream>
#include <iomanip>
#include <cassert>

namespace tigerchain {
namespace nft {

// =============================================================================
// NFTRarityCalculator Implementation
// =============================================================================

NFTRarityCalculator::NFTRarityCalculator() 
    : stats_dirty_(true) {
    config_ = RarityConfig();
}

NFTRarityCalculator::NFTRarityCalculator(const RarityConfig& config)
    : config_(config), stats_dirty_(true) {}

NFTRarityCalculator::~NFTRarityCalculator() {}

void NFTRarityCalculator::set_config(const RarityConfig& config) {
    config_ = config;
    stats_dirty_ = true;
}

RarityConfig NFTRarityCalculator::get_config() const {
    return config_;
}

void NFTRarityCalculator::load_collection(const std::vector<NFTMetadata>& nfts) {
    nfts_.clear();
    rarity_scores_.clear();
    trait_counts_.clear();
    trait_occurrences_.clear();
    
    for (const auto& nft : nfts) {
        add_nft(nft);
    }
    
    recalculate_trait_stats();
    calculate_all_rarity();
    update_rankings();
    stats_dirty_ = true;
}

void NFTRarityCalculator::add_nft(const NFTMetadata& nft) {
    nfts_[nft.token_id] = nft;
    
    // Update trait counts
    for (const auto& trait : nft.traits) {
        trait_counts_[trait.trait_type][trait.value]++;
    }
    
    stats_dirty_ = true;
}

void NFTRarityCalculator::remove_nft(const TokenID& token_id) {
    auto it = nfts_.find(token_id);
    if (it != nfts_.end()) {
        // Update trait counts
        for (const auto& trait : it->second.traits) {
            auto& type_map = trait_counts_[trait.trait_type];
            auto val_it = type_map.find(trait.value);
            if (val_it != type_map.end()) {
                if (val_it->second > 1) {
                    val_it->second--;
                } else {
                    type_map.erase(val_it);
                }
            }
        }
        
        nfts_.erase(it);
        rarity_scores_.erase(token_id);
        stats_dirty_ = true;
    }
}

void NFTRarityCalculator::update_nft(const NFTMetadata& nft) {
    if (nfts_.find(nft.token_id) != nfts_.end()) {
        remove_nft(nft.token_id);
    }
    add_nft(nft);
    
    recalculate_trait_stats();
    auto score = calculate_rarity(nft.token_id);
    rarity_scores_[nft.token_id] = score;
    update_rankings();
}

void NFTRarityCalculator::recalculate_trait_stats() {
    trait_occurrences_.clear();
    trait_distributions_.clear();
    
    uint64_t total = nfts_.size();
    if (total == 0) return;
    
    for (const auto& type_pair : trait_counts_) {
        for (const auto& value_pair : type_pair.second) {
            double occurrence = static_cast<double>(value_pair.second) / total;
            
            TraitDistribution dist;
            dist.trait_type = type_pair.first;
            dist.value = value_pair.first;
            dist.count = value_pair.second;
            dist.percentage = occurrence * 100.0;
            dist.rarity_score = calculate_trait_rarity(
                NFTTrait(type_pair.first, value_pair.first)
            );
            
            trait_occurrences_[type_pair.first][value_pair.first] = occurrence;
            trait_distributions_.push_back(dist);
        }
    }
    
    // Sort by rarity score (descending)
    std::sort(trait_distributions_.begin(), trait_distributions_.end(),
        [](const TraitDistribution& a, const TraitDistribution& b) {
            return a.rarity_score > b.rarity_score;
        });
}

double NFTRarityCalculator::calculate_trait_rarity(const NFTTrait& trait) const {
    auto type_it = trait_occurrences_.find(trait.trait_type);
    if (type_it == trait_occurrences_.end()) {
        return 0.0;
    }
    
    auto value_it = type_it->second.find(trait.value);
    if (value_it == type_it->second.end()) {
        return 0.0;
    }
    
    double occurrence = value_it->second;
    return trait_rarity_to_score(occurrence);
}

double NFTRarityCalculator::calculate_statistical_rarity(const NFTMetadata& nft) const {
    if (nfts_.empty()) return 0.0;
    
    std::vector<double> all_scores;
    for (const auto& pair : rarity_scores_) {
        all_scores.push_back(pair.second.total_score);
    }
    
    if (all_scores.empty()) return 0.0;
    
    double mean = calculate_mean(all_scores);
    double std_dev = calculate_std_dev(all_scores, mean);
    
    // Calculate this NFT's score
    double score = calculate_rarity(nft.token_id).total_score;
    
    if (std_dev == 0.0) return 0.5;
    
    double z_score = (score - mean) / std_dev;
    
    // Convert z-score to probability-like value
    return 1.0 / (1.0 + std::exp(-z_score));
}

double NFTRarityCalculator::calculate_frequency_rarity(const NFTMetadata& nft) const {
    if (nfts_.empty()) return 0.0;
    
    double total_rarity = 0.0;
    double total_traits = 0.0;
    
    for (const auto& trait : nft.traits) {
        double weight = config_.weights.get_weight(trait.trait_type);
        double rarity = calculate_trait_rarity(trait);
        total_rarity += rarity * weight;
        total_traits += weight;
    }
    
    if (total_traits == 0.0) return 0.0;
    
    // Average weighted rarity
    return total_rarity / total_traits;
}

double NFTRarityCalculator::calculate_uniqueness_rarity(const NFTMetadata& nft) const {
    // Check for rare combinations
    double uniqueness = 0.0;
    
    // Count unique traits
    std::set<std::string> unique_trait_types;
    for (const auto& trait : nft.traits) {
        unique_trait_types.insert(trait.trait_type);
    }
    
    // More unique trait types = higher uniqueness
    double type_ratio = static_cast<double>(unique_trait_types.size()) / 
        std::max(nft.traits.size(), static_cast<size_t>(1));
    
    // Check for rare values
    double rare_value_count = 0.0;
    for (const auto& trait : nft.traits) {
        auto type_it = trait_occurrences_.find(trait.trait_type);
        if (type_it != trait_occurrences_.end()) {
            auto val_it = type_it->second.find(trait.value);
            if (val_it != type_it->second.end() && val_it->second < 10) {
                rare_value_count += 1.0;
            }
        }
    }
    
    uniqueness = (type_ratio + rare_value_count) / 2.0;
    return std::min(uniqueness, 1.0);
}

RarityScore NFTRarityCalculator::calculate_rarity(const TokenID& token_id) {
    auto it = nfts_.find(token_id);
    if (it == nfts_.end()) {
        return RarityScore();
    }
    
    return calculate_rarity(it->second);
}

RarityScore NFTRarityCalculator::calculate_rarity(const NFTMetadata& nft) {
    RarityScore score;
    score.token_id = nft.token_id;
    score.calculated_at = std::chrono::duration_cast<std::chrono::seconds>(
        std::chrono::system_clock::now().time_since_epoch()
    ).count();
    
    // Calculate individual components
    score.trait_rarity = calculate_frequency_rarity(nft);
    
    if (config_.include_statistical) {
        score.statistical_rarity = calculate_statistical_rarity(nft);
    }
    
    if (config_.include_frequency) {
        score.frequency_rarity = score.trait_rarity;
    }
    
    if (config_.include_uniqueness) {
        score.uniqueness_score = calculate_uniqueness_rarity(nft);
    }
    
    // Calculate weighted total
    double total_weight = 1.0;
    if (config_.include_statistical) total_weight += config_.statistical_weight;
    if (config_.include_frequency) total_weight += config_.frequency_weight;
    if (config_.include_uniqueness) total_weight += config_.uniqueness_weight;
    
    score.total_score = score.trait_rarity;
    if (config_.include_statistical) {
        score.total_score += score.statistical_rarity * config_.statistical_weight;
    }
    if (config_.include_frequency) {
        score.total_score += score.frequency_rarity * config_.frequency_weight;
    }
    if (config_.include_uniqueness) {
        score.total_score += score.uniqueness_score * config_.uniqueness_weight;
    }
    
    score.total_score /= total_weight;
    score.total_score = std::max(RARITY_SCORE_MIN, std::min(RARITY_SCORE_MAX, score.total_score));
    
    // Store trait scores
    for (const auto& trait : nft.traits) {
        score.trait_scores[trait.trait_type + ":" + trait.value] = 
            calculate_trait_rarity(trait);
    }
    
    score.total_in_collection = nfts_.size();
    
    // Calculate percentile
    score.percentile = calculate_percentile(score.total_score);
    
    rarity_scores_[nft.token_id] = score;
    
    return score;
}

std::vector<RarityScore> NFTRarityCalculator::calculate_all_rarity() {
    std::vector<RarityScore> scores;
    
    for (const auto& pair : nfts_) {
        scores.push_back(calculate_rarity(pair.first));
    }
    
    update_rankings();
    
    return scores;
}

void NFTRarityCalculator::update_rankings() {
    // Sort by total score (descending)
    std::vector<std::pair<TokenID, RarityScore>> sorted;
    for (const auto& pair : rarity_scores_) {
        sorted.push_back(pair);
    }
    
    std::sort(sorted.begin(), sorted.end(),
        [](const auto& a, const auto& b) {
            return a.second.total_score > b.second.total_score;
        });
    
    // Assign ranks
    for (size_t i = 0; i < sorted.size(); ++i) {
        rarity_scores_[sorted[i].first].rank = i + 1;
    }
}

std::vector<RarityScore> NFTRarityCalculator::get_top_rarest(size_t count) {
    std::vector<RarityScore> scores;
    
    for (const auto& pair : rarity_scores_) {
        scores.push_back(pair.second);
    }
    
    std::sort(scores.begin(), scores.end(),
        [](const RarityScore& a, const RarityScore& b) {
            return a.total_score > b.total_score;
        });
    
    if (count > scores.size()) {
        count = scores.size();
    }
    
    return std::vector<RarityScore>(scores.begin(), scores.begin() + count);
}

std::vector<RarityScore> NFTRarityCalculator::get_rarest_by_trait(
    const std::string& trait_type, 
    const std::string& value
) {
    std::vector<RarityScore> matching_scores;
    
    for (const auto& pair : nfts_) {
        const auto& nft = pair.second;
        
        // Check if NFT has this trait
        bool has_trait = false;
        for (const auto& trait : nft.traits) {
            if (trait.trait_type == trait_type && trait.value == value) {
                has_trait = true;
                break;
            }
        }
        
        if (has_trait) {
            auto score_it = rarity_scores_.find(nft.token_id);
            if (score_it != rarity_scores_.end()) {
                matching_scores.push_back(score_it->second);
            }
        }
    }
    
    std::sort(matching_scores.begin(), matching_scores.end(),
        [](const RarityScore& a, const RarityScore& b) {
            return a.total_score > b.total_score;
        });
    
    return matching_scores;
}

CollectionStats NFTRarityCalculator::get_collection_stats() const {
    if (stats_dirty_) {
        const_cast<NFTRarityCalculator*>(this)->update_collection_stats();
    }
    return collection_stats_;
}

std::vector<TraitDistribution> NFTRarityCalculator::get_trait_distribution() const {
    return trait_distributions_;
}

std::map<std::string, std::map<std::string, uint64_t>> 
NFTRarityCalculator::get_trait_counts() const {
    return trait_counts_;
}

void NFTRarityCalculator::add_sale(const SalesData& sale) {
    sales_history_.push_back(sale);
    stats_dirty_ = true;
}

void NFTRarityCalculator::add_sales(const std::vector<SalesData>& sales) {
    sales_history_.insert(sales_history_.end(), sales.begin(), sales.end());
    stats_dirty_ = true;
}

void NFTRarityCalculator::update_collection_stats() const {
    collection_stats_.total_supply = nfts_.size();
    collection_stats_.last_updated = std::chrono::duration_cast<std::chrono::seconds>(
        std::chrono::system_clock::now().time_since_epoch()
    ).count();
    
    if (!sales_history_.empty()) {
        collection_stats_.floor_price = calculate_floor_price();
        collection_stats_.average_price = calculate_average_price();
        collection_stats_.volume_24h = calculate_volume_24h();
    }
    
    stats_dirty_ = false;
}

double NFTRarityCalculator::calculate_floor_price() const {
    if (sales_history_.empty()) return 0.0;
    
    std::vector<double> prices;
    for (const auto& sale : sales_history_) {
        prices.push_back(sale.price);
    }
    
    std::sort(prices.begin(), prices.end());
    return prices.front();
}

double NFTRarityCalculator::calculate_floor_price_24h() const {
    auto now = std::chrono::duration_cast<std::chrono::seconds>(
        std::chrono::system_clock::now().time_since_epoch()
    ).count();
    
    uint64_t day_ago = now - 86400;
    
    std::vector<double> prices;
    for (const auto& sale : sales_history_) {
        if (sale.timestamp >= day_ago) {
            prices.push_back(sale.price);
        }
    }
    
    if (prices.empty()) return 0.0;
    
    std::sort(prices.begin(), prices.end());
    return prices.front();
}

double NFTRarityCalculator::calculate_average_price() const {
    if (sales_history_.empty()) return 0.0;
    
    double total = 0.0;
    for (const auto& sale : sales_history_) {
        total += sale.price;
    }
    
    return total / sales_history_.size();
}

double NFTRarityCalculator::calculate_volume_24h() const {
    auto now = std::chrono::duration_cast<std::chrono::seconds>(
        std::chrono::system_clock::now().time_since_epoch()
    ).count();
    
    uint64_t day_ago = now - 86400;
    
    double volume = 0.0;
    for (const auto& sale : sales_history_) {
        if (sale.timestamp >= day_ago) {
            volume += sale.price;
        }
    }
    
    return volume;
}

uint64_t NFTRarityCalculator::get_rank(const TokenID& token_id) const {
    auto it = rarity_scores_.find(token_id);
    if (it != rarity_scores_.end()) {
        return it->second.rank;
    }
    return 0;
}

std::vector<TokenID> NFTRarityCalculator::get_tokens_by_rank(
    uint64_t start, 
    uint64_t end
) const {
    std::vector<std::pair<TokenID, uint64_t>> ranked;
    
    for (const auto& pair : rarity_scores_) {
        ranked.push_back({pair.first, pair.second.rank});
    }
    
    std::sort(ranked.begin(), ranked.end(),
        [](const auto& a, const auto& b) {
            return a.second < b.second;
        });
    
    std::vector<TokenID> result;
    for (const auto& pair : ranked) {
        if (pair.second >= start && pair.second <= end) {
            result.push_back(pair.first);
        }
    }
    
    return result;
}

double NFTRarityCalculator::calculate_collection_rarity() const {
    if (rarity_scores_.empty()) return 0.0;
    
    double total = 0.0;
    for (const auto& pair : rarity_scores_) {
        total += pair.second.total_score;
    }
    
    return total / rarity_scores_.size();
}

std::map<std::string, double> NFTRarityCalculator::calculate_trait_importance() const {
    std::map<std::string, double> importance;
    
    // Calculate average rarity for each trait type
    std::map<std::string, double> total_rarity;
    std::map<std::string, uint64_t> count;
    
    for (const auto& pair : rarity_scores_) {
        for (const auto& trait_score : pair.second.trait_scores) {
            std::string trait_type = trait_score.first.substr(0, trait_score.first.find(':'));
            total_rarity[trait_type] += trait_score.second;
            count[trait_type]++;
        }
    }
    
    for (const auto& pair : total_rarity) {
        if (count[pair.first] > 0) {
            importance[pair.first] = pair.second / count[pair.first];
        }
    }
    
    return importance;
}

std::vector<TokenID> NFTRarityCalculator::detect_outliers(double z_threshold) const {
    if (rarity_scores_.empty()) return {};
    
    std::vector<double> all_scores;
    for (const auto& pair : rarity_scores_) {
        all_scores.push_back(pair.second.total_score);
    }
    
    double mean = calculate_mean(all_scores);
    double std_dev = calculate_std_dev(all_scores, mean);
    
    std::vector<TokenID> outliers;
    for (const auto& pair : rarity_scores_) {
        double z_score = (pair.second.total_score - mean) / std_dev;
        if (std::abs(z_score) > z_threshold) {
            outliers.push_back(pair.first);
        }
    }
    
    return outliers;
}

double NFTRarityCalculator::calculate_mean(const std::vector<double>& values) const {
    if (values.empty()) return 0.0;
    
    double sum = std::accumulate(values.begin(), values.end(), 0.0);
    return sum / values.size();
}

double NFTRarityCalculator::calculate_std_dev(
    const std::vector<double>& values, 
    double mean
) const {
    if (values.empty()) return 0.0;
    
    double sum_sq = 0.0;
    for (double v : values) {
        double diff = v - mean;
        sum_sq += diff * diff;
    }
    
    return std::sqrt(sum_sq / values.size());
}

double NFTRarityCalculator::calculate_percentile(double score) const {
    if (rarity_scores_.empty()) return 0.0;
    
    uint64_t below = 0;
    for (const auto& pair : rarity_scores_) {
        if (pair.second.total_score < score) {
            below++;
        }
    }
    
    return (static_cast<double>(below) / rarity_scores_.size()) * 100.0;
}

void NFTRarityCalculator::clear() {
    nfts_.clear();
    rarity_scores_.clear();
    trait_counts_.clear();
    trait_occurrences_.clear();
    trait_distributions_.clear();
    sales_history_.clear();
    collection_stats_ = CollectionStats();
    stats_dirty_ = true;
}

std::string NFTRarityCalculator::to_json() const {
    std::ostringstream oss;
    oss << "{";
    oss << "\"collection_stats\":" << collection_stats_to_json() << ",";
    oss << "\"trait_distributions\":[";
    
    bool first = true;
    for (const auto& dist : trait_distributions_) {
        if (!first) oss << ",";
        first = false;
        
        oss << "{\"trait_type\":\"" << dist.trait_type << "\",";
        oss << "\"value\":\"" << dist.value << "\",";
        oss << "\"count\":" << dist.count << ",";
        oss << "\"percentage\":" << dist.percentage << ",";
        oss << "\"rarity_score\":" << dist.rarity_score << "}";
    }
    
    oss << "],\"rarity_scores\":[";
    
    first = true;
    for (const auto& pair : rarity_scores_) {
        if (!first) oss << ",";
        first = false;
        oss << rarity_score_to_json(pair.second);
    }
    
    oss << "]}";
    return oss.str();
}

void NFTRarityCalculator::from_json(const std::string& json) {
    // Simplified - full JSON parsing would require a proper JSON library
    clear();
}

std::string NFTRarityCalculator::rarity_score_to_json(const RarityScore& score) const {
    std::ostringstream oss;
    oss << "{\"token_id\":\"" << score.token_id << "\",";
    oss << "\"total_score\":" << score.total_score << ",";
    oss << "\"rank\":" << score.rank << ",";
    oss << "\"percentile\":" << score.percentile << "}";
    return oss.str();
}

std::string NFTRarityCalculator::collection_stats_to_json() const {
    std::ostringstream oss;
    oss << "{\"total_supply\":" << collection_stats_.total_supply << ",";
    oss << "\"floor_price\":" << collection_stats_.floor_price << ",";
    oss << "\"average_price\":" << collection_stats_.average_price << ",";
    oss << "\"volume_24h\":" << collection_stats_.volume_24h << "}";
    return oss.str();
}

// =============================================================================
// Utility Functions
// =============================================================================

double trait_rarity_to_score(double occurrence) {
    if (occurrence <= 0.0 || occurrence > 1.0) {
        return 0.0;
    }
    
    // Use logarithmic scale for rarity
    // Lower occurrence = higher rarity
    double score = -std::log10(occurrence + 1e-10);
    
    // Normalize to 0-100 range
    score = (score / 4.0) * 100.0;
    
    return std::max(RARITY_SCORE_MIN, std::min(RARITY_SCORE_MAX, score));
}

double jensens_inequality_correction(double rarity_score, double alpha) {
    // Apply Jensen's inequality correction for skewed distributions
    double x = rarity_score / RARITY_SCORE_MAX;
    double corrected = std::pow(x, 1.0 - alpha * (1.0 - x));
    return corrected * RARITY_SCORE_MAX;
}

std::string token_id_to_string(const TokenID& id) {
    return id;
}

TokenID string_to_token_id(const std::string& str) {
    return str;
}

std::string contract_address_to_string(const ContractAddress& addr) {
    std::ostringstream oss;
    oss << "0x";
    for (const auto& byte : addr) {
        oss << std::hex << std::setw(2) << std::setfill('0') << (int)byte;
    }
    return oss.str();
}

ContractAddress string_to_contract_address(const std::string& str) {
    ContractAddress addr{};
    std::string hex_str = str;
    if (hex_str.substr(0, 2) == "0x") {
        hex_str = hex_str.substr(2);
    }
    
    for (size_t i = 0; i < hex_str.length() && i < 40; i += 2) {
        addr[i / 2] = static_cast<uint8_t>(std::stoi(hex_str.substr(i, 2), nullptr, 16));
    }
    
    return addr;
}

std::string rarity_score_to_string(const RarityScore& score) {
    std::ostringstream oss;
    oss << "Token: " << score.token_id << "\n";
    oss << "Total Score: " << score.total_score << "\n";
    oss << "Rank: " << score.rank << " / " << score.total_in_collection << "\n";
    oss << "Percentile: " << score.percentile << "%";
    return oss.str();
}

} // namespace nft
} // namespace tigerchain
