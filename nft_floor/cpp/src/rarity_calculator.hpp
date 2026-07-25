/**
 * TigerScan NFT Rarity Calculator
 * 
 * High-performance C++ implementation for calculating NFT rarity scores
 * based on trait frequencies and statistical analysis.
 * 
 * Features:
 * - Trait frequency analysis
 * - Rarity score calculation (OpenRarity-inspired)
 * - Trait correlation detection
 * - Floor price adjusted rarity
 * - Collection statistics
 */

#ifndef TIGERSCAN_NFT_RARITY_CALCULATOR_HPP
#define TIGERSCAN_NFT_RARITY_CALCULATOR_HPP

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
#include <cmath>
#include <functional>
#include <variant>
#include <curl/curl.h>
#include <jansson.h>

namespace tigerscan {

// Trait types
using TraitValue = std::variant<std::string, int, double, bool>;

struct Trait {
    std::string trait_type;
    TraitValue value;
    double frequency;          // Frequency of this trait value in collection
    double rarity_score;       // Calculated rarity score
    
    Trait() : frequency(0), rarity_score(0) {}
    Trait(const std::string& type, const TraitValue& val)
        : trait_type(type), value(val), frequency(0), rarity_score(0) {}
};

struct NFTMetadata {
    std::string token_id;
    std::string collection_address;
    std::string name;
    std::string image_url;
    std::string description;
    std::vector<Trait> traits;
    double rarity_score;
    int rarity_rank;
    std::chrono::system_clock::time_point last_updated;
    
    NFTMetadata() : rarity_score(0), rarity_rank(0) {}
};

struct CollectionRarityStats {
    std::string collection_address;
    std::string collection_name;
    int total_supply;
    int unique_token_count;
    std::map<std::string, int> trait_type_counts;  // Number of unique values per trait type
    std::map<std::string, std::map<std::string, int>> trait_value_counts;  // Count per trait value
    double average_rarity_score;
    double median_rarity_score;
    double std_deviation;
    std::vector<std::string> top_rarest_tokens;
    std::chrono::system_clock::time_point calculated_at;
    
    CollectionRarityStats() : total_supply(0), unique_token_count(0),
        average_rarity_score(0), median_rarity_score(0), std_deviation(0) {}
};

struct RarityResult {
    std::string token_id;
    std::string collection_address;
    double rarity_score;
    int rarity_rank;
    int total_in_collection;
    double percentile;
    std::vector<Trait> traits;
    std::string rarity_tier;  // Common, Uncommon, Rare, Very Rare, Legendary, Mythic
    std::chrono::system_clock::time_point timestamp;
    
    RarityResult() : rarity_score(0), rarity_rank(0), total_in_collection(0), percentile(0) {}
};

// HTTP Client for fetching NFT metadata
class MetadataFetcher {
public:
    MetadataFetcher();
    ~MetadataFetcher();
    
    std::string get(const std::string& url,
                   const std::map<std::string, std::string>& headers = {},
                   int timeout_ms = 5000);
    
    void set_api_key(const std::string& key);
    
private:
    CURL* curl_;
    std::string api_key_;
    int default_timeout_;
    
    static size_t write_callback(void* contents, size_t size, size_t nmemb, void* userp);
    std::string perform_request(const std::string& url, const std::string& method,
                                const std::string& body,
                                const std::map<std::string, std::string>& headers);
};

// Rarity calculator engine
class RarityCalculator {
public:
    RarityCalculator();
    ~RarityCalculator();
    
    // Initialize with configuration
    void initialize(double interpolation_factor = 1.0, bool use_logarithmic = true);
    
    // Calculate rarity for a single NFT
    RarityResult calculate_rarity(
        const std::string& token_id,
        const std::string& collection_address,
        const std::vector<Trait>& traits
    );
    
    // Calculate rarity for entire collection
    CollectionRarityStats calculate_collection_rarity(
        const std::string& collection_address,
        const std::vector<NFTMetadata>& nfts
    );
    
    // Batch calculate rarity for multiple NFTs
    std::vector<RarityResult> calculate_batch_rarity(
        const std::string& collection_address,
        const std::vector<std::pair<std::string, std::vector<Trait>>>& nfts
    );
    
    // Update trait statistics from collection
    void update_collection_stats(
        const std::string& collection_address,
        const std::vector<NFTMetadata>& nfts
    );
    
    // Get cached collection stats
    std::optional<CollectionRarityStats> get_collection_stats(
        const std::string& collection_address
    ) const;
    
    // Get trait importance weights
    std::map<std::string, double> get_trait_importance(
        const std::string& collection_address
    ) const;
    
    // Set custom trait weights
    void set_trait_weight(
        const std::string& collection_address,
        const std::string& trait_type,
        double weight
    );
    
    // Statistical calculations
    struct Stats {
        uint64_t total_calculations;
        uint64_t cache_hits;
        double average_calculation_time_ms;
    };
    
    Stats get_stats() const;
    void reset_stats();

private:
    // Calculate rarity score using OpenRarity formula
    double calculate_rarity_score(
        const std::vector<Trait>& traits,
        int total_supply
    );
    
    // Calculate trait rarity score
    double calculate_trait_rarity(
        double frequency,
        int total_supply,
        int trait_count
    );
    
    // Determine rarity tier
    std::string determine_rarity_tier(double score, double max_score);
    
    // Calculate percentile
    double calculate_percentile(int rank, int total);
    
    // Interpolate between linear and logarithmic scoring
    double interpolate_score(double linear_score, double logarithmic_score, double factor);
    
    // Cache management
    struct CacheEntry {
        CollectionRarityStats stats;
        std::chrono::system_clock::time_point timestamp;
        std::chrono::seconds ttl;
    };
    
    mutable std::shared_mutex cache_mutex_;
    std::unordered_map<std::string, CacheEntry> collection_cache_;
    
    // Trait weights
    mutable std::shared_mutex weight_mutex_;
    std::unordered_map<std::string, std::unordered_map<std::string, double>> trait_weights_;
    
    // Configuration
    double interpolation_factor_;
    bool use_logarithmic_;
    
    // Statistics
    std::atomic<uint64_t> total_calculations_{0};
    std::atomic<uint64_t> cache_hits_{0};
    std::atomic<double> total_calculation_time_{0};
    
    static constexpr auto DEFAULT_CACHE_TTL = std::chrono::hours(1);
    static constexpr int MIN_TRAIT_COUNT = 1;
};

// Trait parser for extracting traits from metadata
class TraitParser {
public:
    TraitParser();
    ~TraitParser();
    
    // Parse traits from OpenSea metadata format
    std::vector<Trait> parse_opensea_metadata(const std::string& json) const;
    
    // Parse traits from JSON object
    std::vector<Trait> parse_traits_from_json(json_t* traits_obj) const;
    
    // Parse traits from ERC-721 attributes
    std::vector<Trait> parse_erc721_attributes(
        const std::string& json_attributes
    ) const;
    
    // Detect trait type from value
    static TraitValue parse_trait_value(const std::string& value);
    
private:
    std::map<std::string, std::string> trait_type_mapping_;
};

// Main service class
class NFTRarityService {
public:
    NFTRarityService();
    ~NFTRarityService();
    
    // Initialize service
    bool initialize(const std::string& config_path = "");
    
    // Start service
    void start();
    
    // Stop service
    void stop();
    
    // Calculate rarity for a specific token
    RarityResult get_rarity(
        const std::string& token_id,
        const std::string& collection_address,
        const std::string& chain = "ethereum"
    );
    
    // Get collection rarity statistics
    CollectionRarityStats get_collection_stats(
        const std::string& collection_address
    );
    
    // Batch calculate rarity
    std::vector<RarityResult> get_batch_rarity(
        const std::vector<std::string>& token_ids,
        const std::string& collection_address,
        const std::string& chain = "ethereum"
    );
    
    // Get top rare NFTs in collection
    std::vector<RarityResult> get_top_rare(
        const std::string& collection_address,
        int limit = 10
    );
    
    // Search by trait
    std::vector<RarityResult> search_by_trait(
        const std::string& collection_address,
        const std::string& trait_type,
        const TraitValue& trait_value,
        int limit = 100
    );
    
    // Fetch and calculate rarity for all NFTs in collection
    void calculate_full_collection(
        const std::string& collection_address,
        const std::string& chain = "ethereum"
    );
    
private:
    std::unique_ptr<RarityCalculator> calculator_;
    std::unique_ptr<MetadataFetcher> metadata_fetcher_;
    std::unique_ptr<TraitParser> trait_parser_;
    
    std::unordered_map<std::string, std::vector<NFTMetadata>> collection_nfts_;
    mutable std::mutex nfts_mutex_;
    
    std::atomic<bool> running_{false};
    std::thread update_thread_;
    
    void update_loop();
    
    // Fetch NFT metadata from various sources
    std::optional<NFTMetadata> fetch_nft_metadata(
        const std::string& token_id,
        const std::string& collection_address,
        const std::string& source
    );
    
    std::vector<NFTMetadata> fetch_collection_metadata(
        const std::string& collection_address,
        int limit
    );
};

} // namespace tigerscan

#endif // TIGERSCAN_NFT_RARITY_CALCULATOR_HPP
