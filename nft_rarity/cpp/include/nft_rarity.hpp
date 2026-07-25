/**
 * @file nft_rarity.hpp
 * @brief High-Performance NFT Rarity Calculator
 * @author TigerScan Team
 * @version 1.0.0
 * 
 * C++ implementation for real-time NFT rarity scoring with:
 * - Trait-based rarity calculation
 * - Statistical analysis
 * - Floor price tracking
 * - Collection analytics
 */

#ifndef NFT_RARITY_HPP
#define NFT_RARITY_HPP

#include <cstdint>
#include <vector>
#include <string>
#include <array>
#include <unordered_map>
#include <unordered_set>
#include <map>
#include <set>
#include <algorithm>
#include <cmath>
#include <numeric>
#include <optional>
#include <memory>
#include <variant>
#include <chrono>

namespace tigerchain {
namespace nft {

// =============================================================================
// Constants
// =============================================================================

constexpr double RARITY_SCORE_MAX = 100.0;
constexpr double RARITY_SCORE_MIN = 0.0;
constexpr double TRAIT_WEIGHT_DEFAULT = 1.0;
constexpr size_t MAX_TRAITS = 100;
constexpr size_t MAX_TOKENS = 100000;

// =============================================================================
// Type Definitions
// =============================================================================

using TokenID = std::string;
using ContractAddress = std::array<uint8_t, 20>;
using Timestamp = uint64_t;

// =============================================================================
// Data Structures
// =============================================================================

/**
 * @struct NFTTrait
 * @brief Represents a single trait of an NFT
 */
struct NFTTrait {
    std::string trait_type;
    std::string value;
    double occurrence_rate;
    double rarity_score;
    
    NFTTrait() : occurrence_rate(0.0), rarity_score(0.0) {}
    NFTTrait(const std::string& type, const std::string& val) 
        : trait_type(type), value(val), occurrence_rate(0.0), rarity_score(0.0) {}
};

/**
 * @struct NFTMetadata
 * @brief Full metadata for an NFT
 */
struct NFTMetadata {
    TokenID token_id;
    ContractAddress contract_address;
    std::string name;
    std::string description;
    std::string image_url;
    std::string animation_url;
    std::vector<NFTTrait> traits;
    std::unordered_map<std::string, std::string> attributes;
    Timestamp created_at;
    Timestamp updated_at;
    
    NFTMetadata() : created_at(0), updated_at(0) {}
};

/**
 * @struct RarityScore
 * @brief Calculated rarity score for an NFT
 */
struct RarityScore {
    TokenID token_id;
    double total_score;
    double trait_rarity;
    double statistical_rarity;
    double frequency_rarity;
    double uniqueness_score;
    uint64_t rank;
    uint64_t total_in_collection;
    double percentile;
    std::map<std::string, double> trait_scores;
    Timestamp calculated_at;
    
    RarityScore() 
        : total_score(0.0), trait_rarity(0.0), statistical_rarity(0.0)
        , frequency_rarity(0.0), uniqueness_score(0.0), rank(0)
        , total_in_collection(0), percentile(0.0), calculated_at(0) {}
};

/**
 * @struct CollectionStats
 * @brief Statistics for an entire NFT collection
 */
struct CollectionStats {
    ContractAddress contract_address;
    std::string name;
    std::string symbol;
    uint64_t total_supply;
    uint64_t holders;
    uint64_t transfers_24h;
    double floor_price;
    double floor_price_24h_ago;
    double volume_24h;
    double volume_change_24h;
    double average_price;
    double total_volume;
    Timestamp last_updated;
    
    CollectionStats() 
        : total_supply(0), holders(0), transfers_24h(0)
        , floor_price(0.0), floor_price_24h_ago(0.0), volume_24h(0.0)
        , volume_change_24h(0.0), average_price(0.0), total_volume(0.0), last_updated(0) {}
};

/**
 * @struct TraitDistribution
 * @brief Distribution of trait values in a collection
 */
struct TraitDistribution {
    std::string trait_type;
    std::string value;
    uint64_t count;
    double percentage;
    double rarity_score;
    
    TraitDistribution() : count(0), percentage(0.0), rarity_score(0.0) {}
};

/**
 * @struct SalesData
 * @brief Historical sales data for floor price calculation
 */
struct SalesData {
    TokenID token_id;
    double price;
    Timestamp timestamp;
    std::string buyer;
    std::string seller;
};

/**
 * @struct TraitWeights
 * @brief Custom weights for different trait types
 */
struct TraitWeights {
    std::unordered_map<std::string, double> weights;
    
    double get_weight(const std::string& trait_type) const {
        auto it = weights.find(trait_type);
        return (it != weights.end()) ? it->second : TRAIT_WEIGHT_DEFAULT;
    }
};

/**
 * @struct RarityResult
 * @brief Complete rarity analysis result
 */
struct RarityResult {
    RarityScore score;
    CollectionStats collection;
    std::vector<TraitDistribution> trait_distributions;
    std::vector<RarityScore> top_rarest;
    Timestamp analysis_timestamp;
    
    RarityResult() : analysis_timestamp(0) {}
};

/**
 * @enum RarityAlgorithm
 * @brief Different algorithms for rarity calculation
 */
enum class RarityAlgorithm : uint8_t {
    TRAIT_BASED = 0,
    STATISTICAL = 1,
    FREQUENCY = 2,
    JENKINS = 3,
    CUSTOM = 4
};

/**
 * @struct RarityConfig
 * @brief Configuration for rarity calculation
 */
struct RarityConfig {
    RarityAlgorithm algorithm;
    TraitWeights weights;
    bool include_statistical;
    bool include_frequency;
    bool include_uniqueness;
    double statistical_weight;
    double frequency_weight;
    double uniqueness_weight;
    
    RarityConfig() 
        : algorithm(RarityAlgorithm::TRAIT_BASED)
        , include_statistical(true)
        , include_frequency(true)
        , include_uniqueness(true)
        , statistical_weight(0.3)
        , frequency_weight(0.3)
        , uniqueness_weight(0.2) {}
};

// =============================================================================
// Rarity Calculator Class
// =============================================================================

/**
 * @class NFTRarityCalculator
 * @brief High-performance NFT rarity calculator
 */
class NFTRarityCalculator {
public:
    NFTRarityCalculator();
    explicit NFTRarityCalculator(const RarityConfig& config);
    ~NFTRarityCalculator();
    
    // Configuration
    void set_config(const RarityConfig& config);
    RarityConfig get_config() const;
    
    // Data loading
    void load_collection(const std::vector<NFTMetadata>& nfts);
    void add_nft(const NFTMetadata& nft);
    void remove_nft(const TokenID& token_id);
    void update_nft(const NFTMetadata& nft);
    
    // Rarity calculation
    RarityScore calculate_rarity(const TokenID& token_id);
    RarityScore calculate_rarity(const NFTMetadata& nft);
    std::vector<RarityScore> calculate_all_rarity();
    std::vector<RarityScore> get_top_rarest(size_t count);
    std::vector<RarityScore> get_rarest_by_trait(const std::string& trait_type, const std::string& value);
    
    // Collection analysis
    CollectionStats get_collection_stats() const;
    std::vector<TraitDistribution> get_trait_distribution() const;
    std::map<std::string, std::map<std::string, uint64_t>> get_trait_counts() const;
    
    // Floor price
    void add_sale(const SalesData& sale);
    void add_sales(const std::vector<SalesData>& sales);
    double calculate_floor_price() const;
    double calculate_floor_price_24h() const;
    double calculate_average_price() const;
    double calculate_volume_24h() const;
    
    // Ranking
    uint64_t get_rank(const TokenID& token_id) const;
    std::vector<TokenID> get_tokens_by_rank(uint64_t start, uint64_t end) const;
    
    // Statistical analysis
    double calculate_collection_rarity() const;
    std::map<std::string, double> calculate_trait_importance() const;
    std::vector<TokenID> detect_outliers(double z_threshold = 3.0) const;
    
    // Serialization
    std::string to_json() const;
    void from_json(const std::string& json);
    
    // Utility
    void clear();
    size_t size() const { return nfts_.size(); }
    bool empty() const { return nfts_.empty(); }
    
private:
    // Configuration
    RarityConfig config_;
    
    // NFT storage
    std::unordered_map<TokenID, NFTMetadata> nfts_;
    std::unordered_map<TokenID, RarityScore> rarity_scores_;
    
    // Trait analysis
    std::map<std::string, std::map<std::string, uint64_t>> trait_counts_;
    std::map<std::string, std::map<std::string, double>> trait_occurrences_;
    std::vector<TraitDistribution> trait_distributions_;
    
    // Sales data
    std::vector<SalesData> sales_history_;
    
    // Collection stats
    mutable CollectionStats collection_stats_;
    mutable bool stats_dirty_;
    
    // Helper methods
    void recalculate_trait_stats();
    double calculate_trait_rarity(const NFTTrait& trait) const;
    double calculate_statistical_rarity(const NFTMetadata& nft) const;
    double calculate_frequency_rarity(const NFTMetadata& nft) const;
    double calculate_uniqueness_rarity(const NFTMetadata& nft) const;
    void update_rankings();
    void update_collection_stats() const;
    
    // Statistical helpers
    double calculate_mean(const std::vector<double>& values) const;
    double calculate_std_dev(const std::vector<double>& values, double mean) const;
    double calculate_percentile(double score) const;
    
    // JSON helpers
    std::string rarity_score_to_json(const RarityScore& score) const;
    std::string collection_stats_to_json() const;
};

// =============================================================================
// Advanced Analytics
// =============================================================================

/**
 * @class NFTRarityAnalytics
 * @brief Advanced analytics for NFT rarity
 */
class NFTRarityAnalytics {
public:
    explicit NFTRarityAnalytics(const NFTRarityCalculator& calculator);
    ~NFTRarityAnalytics();
    
    // Pattern detection
    std::vector<std::vector<TokenID>> detect_trait_clusters(size_t min_cluster_size = 3) const;
    std::vector<TokenID> detect_wash_trading(double volume_threshold = 100.0) const;
    std::vector<TokenID> detect_pump_and_dump(double price_change_threshold = 5.0) const;
    
    // Price analysis
    std::map<std::string, double> predict_floor_price(double confidence = 0.95) const;
    std::vector<std::pair<Timestamp, double>> get_price_history(size_t days = 30) const;
    std::map<std::string, double> calculate_trait_premium() const;
    
    // Collection comparison
    double compare_collections(const NFTRarityCalculator& other) const;
    std::map<std::string, double> get_collection_metrics() const;
    
    // Market analysis
    double calculate_market_cap() const;
    double calculate_holder_concentration() const;
    std::vector<TokenID> get_undervalued_tokens(double threshold = 0.2) const;
    
    // Investment scoring
    std::map<TokenID, double> calculate_investment_scores() const;
    
private:
    const NFTRarityCalculator& calculator_;
    
    // Helper methods
    std::vector<double> extract_trait_numeric_values(const std::string& trait_type) const;
    std::map<std::string, double> calculate_correlation_matrix() const;
};

// =============================================================================
// Wash Trading Detector
// =============================================================================

/**
 * @class WashTradingDetector
 * @brief Detects wash trading in NFT collections
 */
class WashTradingDetector {
public:
    WashTradingDetector();
    ~WashTradingDetector();
    
    void add_transaction(const std::string& from, const std::string& to, double price, Timestamp time);
    std::vector<TokenID> detect_wash_trades(double min_volume = 100.0) const;
    std::vector<std::pair<std::string, std::string>> detect_cycles() const;
    
private:
    struct Transaction {
        std::string from;
        std::string to;
        double price;
        Timestamp time;
    };
    
    std::vector<Transaction> transactions_;
    
    bool is_same_wallet(const std::string& a, const std::string& b) const;
    double calculate_trading_volume(const std::string& wallet) const;
};

// =============================================================================
// Floor Price Calculator
// =============================================================================

/**
 * @class FloorPriceCalculator
 * @brief Calculates floor prices with multiple methods
 */
class FloorPriceCalculator {
public:
    FloorPriceCalculator();
    ~FloorPriceCalculator();
    
    void add_sale(const SalesData& sale);
    void add_sales(const std::vector<SalesData>& sales);
    
    double calculate_floor() const;
    double calculate_floor_weighted(size_t window = 10) const;
    double calculate_floor_moving_average(size_t periods = 7) const;
    double calculate_floor_median() const;
    double calculate_floor_percentile(double percentile = 0.1) const;
    
    void clear();
    
private:
    std::vector<SalesData> sales_;
    
    static bool compare_by_price(const SalesData& a, const SalesData& b);
};

// =============================================================================
// Utility Functions
// =============================================================================

std::string token_id_to_string(const TokenID& id);
TokenID string_to_token_id(const std::string& str);
std::string contract_address_to_string(const ContractAddress& addr);
ContractAddress string_to_contract_address(const std::string& str);
std::string rarity_score_to_string(const RarityScore& score);

double jensens_inequality_correction(double rarity_score, double alpha = 0.5);
double trait_rarity_to_score(double occurrence);

} // namespace nft
} // namespace tigerchain

#endif // NFT_RARITY_HPP
