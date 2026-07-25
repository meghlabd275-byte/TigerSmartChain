/**
 * TigerScan NFT Floor Price Calculator
 * 
 * High-performance C++ implementation for calculating NFT floor prices
 * from multiple marketplace APIs with real-time aggregation.
 * 
 * Supported marketplaces:
 * - OpenSea API
 * - Blur API
 * - Uniswap NFT API
 * - LooksRare API
 * - X2Y2 API
 */

#ifndef TIGERSCAN_NFT_FLOOR_CALCULATOR_HPP
#define TIGERSCAN_NFT_FLOOR_CALCULATOR_HPP

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
#include <cmath>
#include <numeric>
#include <functional>
#include <curl/curl.h>
#include <openssl/hmac.h>
#include <openssl/sha.h>
#include <jansson.h>

namespace tigerscan {

// Configuration structures
struct MarketplaceConfig {
    std::string name;
    std::string api_base_url;
    std::string api_key;
    std::string secret_key;
    int timeout_ms;
    int max_retries;
    double weight;  // Weight in floor calculation
    
    MarketplaceConfig(const std::string& n, const std::string& url, 
                      const std::string& key = "", const std::string& secret = "",
                      int timeout = 5000, int retries = 3, double w = 1.0)
        : name(n), api_base_url(url), api_key(key), secret_key(secret),
          timeout_ms(timeout), max_retries(retries), weight(w) {}
};

struct FloorPriceResult {
    std::string collection_address;
    std::string collection_name;
    double floor_price_wei;
    double floor_price_eth;
    double weighted_average;
    double median;
    double std_deviation;
    int sample_size;
    std::chrono::system_clock::time_point timestamp;
    std::map<std::string, double> marketplace_floors;
    std::string source;
    bool is_estimated;
    
    FloorPriceResult() : floor_price_wei(0), floor_price_eth(0),
        weighted_average(0), median(0), std_deviation(0),
        sample_size(0), is_estimated(false) {}
};

struct SaleData {
    std::string token_id;
    std::string seller_address;
    std::string buyer_address;
    double price_wei;
    uint64_t timestamp;
    std::string marketplace;
    std::string transaction_hash;
};

// HTTP Client for marketplace APIs
class HTTPClient {
public:
    HTTPClient();
    ~HTTPClient();
    
    std::string get(const std::string& url, 
                    const std::map<std::string, std::string>& headers = {},
                    int timeout_ms = 5000);
    
    std::string post(const std::string& url, 
                     const std::string& body,
                     const std::map<std::string, std::string>& headers = {},
                     int timeout_ms = 5000);
    
    void set_api_key(const std::string& key);
    void set_timeout(int timeout_ms);
    
private:
    CURL* curl_;
    std::string api_key_;
    int default_timeout_;
    
    static size_t write_callback(void* contents, size_t size, size_t nmemb, void* userp);
    std::string perform_request(const std::string& url, const std::string& method,
                                const std::string& body,
                                const std::map<std::string, std::string>& headers);
};

// Floor price calculator engine
class FloorPriceCalculator {
public:
    FloorPriceCalculator();
    ~FloorPriceCalculator();
    
    // Initialize with marketplace configurations
    void initialize(const std::vector<MarketplaceConfig>& configs);
    
    // Add a marketplace dynamically
    void add_marketplace(const MarketplaceConfig& config);
    
    // Remove a marketplace
    void remove_marketplace(const std::string& name);
    
    // Calculate floor price for a collection
    FloorPriceResult calculate_floor(
        const std::string& collection_address,
        const std::string& chain = "ethereum",
        int sample_size = 100
    );
    
    // Calculate floor from cached data
    FloorPriceResult calculate_from_cache(
        const std::string& collection_address
    );
    
    // Update cache with new sale data
    void update_cache(
        const std::string& collection_address,
        const std::vector<SaleData>& sales
    );
    
    // Get supported marketplaces
    std::vector<std::string> get_supported_marketplaces() const;
    
    // Statistics
    struct Stats {
        uint64_t total_requests;
        uint64_t successful_requests;
        uint64_t failed_requests;
        double average_response_time_ms;
        std::chrono::system_clock::time_point last_update;
    };
    
    Stats get_stats() const;
    void reset_stats();

private:
    // Fetch sales from individual marketplaces
    std::vector<SaleData> fetch_opensea_sales(
        const std::string& collection_address,
        int limit
    );
    
    std::vector<SaleData> fetch_blur_sales(
        const std::string& collection_address,
        int limit
    );
    
    std::vector<SaleData> fetch_uniswap_sales(
        const std::string& collection_address,
        int limit
    );
    
    std::vector<SaleData> fetch_looksrare_sales(
        const std::string& collection_address,
        int limit
    );
    
    // Statistical calculations
    double calculate_weighted_average(
        const std::map<std::string, std::vector<double>>& prices,
        const std::map<std::string, double>& weights
    );
    
    double calculate_median(std::vector<double>& prices);
    double calculate_std_dev(const std::vector<double>& prices, double mean);
    
    // Cache management
    struct CacheEntry {
        std::vector<SaleData> sales;
        FloorPriceResult result;
        std::chrono::system_clock::time_point timestamp;
        std::chrono::seconds ttl;
    };
    
    std::unordered_map<std::string, CacheEntry> cache_;
    mutable std::shared_mutex cache_mutex_;
    
    // Marketplace configurations
    std::unordered_map<std::string, MarketplaceConfig> marketplaces_;
    mutable std::shared_mutex marketplace_mutex_;
    
    // HTTP clients per marketplace
    std::unordered_map<std::string, std::unique_ptr<HTTPClient>> clients_;
    mutable std::shared_mutex clients_mutex_;
    
    // Statistics
    std::atomic<uint64_t> total_requests_{0};
    std::atomic<uint64_t> successful_requests_{0};
    std::atomic<uint64_t> failed_requests_{0};
    std::atomic<double> total_response_time_{0};
    mutable std::mutex stats_mutex_;
    
    // Configuration
    static constexpr auto DEFAULT_CACHE_TTL = std::chrono::minutes(5);
    static constexpr int MAX_CONCURRENT_REQUESTS = 10;
    static constexpr double MIN_FLOOR_SAMPLE_SIZE = 5;
};

// Price converter utilities
class PriceConverter {
public:
    static constexpr double WEI_PER_ETH = 1e18;
    static constexpr double WEI_PER_GWEI = 1e9;
    
    static double wei_to_eth(double wei) { return wei / WEI_PER_ETH; }
    static double eth_to_wei(double eth) { return eth * WEI_PER_ETH; }
    static double gwei_to_wei(double gwei) { return gwei * WEI_PER_GWEI; }
    static double wei_to_gwei(double wei) { return wei / WEI_PER_GWEI; }
    
    // Convert from various tokens to wei
    static double token_to_wei(double amount, int decimals) {
        return amount * std::pow(10, decimals);
    }
    
    static double wei_to_token(double wei, int decimals) {
        return wei / std::pow(10, decimals);
    }
};

// Collection metadata
struct CollectionMetadata {
    std::string address;
    std::string name;
    std::string symbol;
    std::string description;
    std::string image_url;
    std::string external_url;
    std::string contract_type;
    std::string schema_name;
    int total_supply;
    int num_owners;
    std::map<std::string, std::string> traits;
    
    CollectionMetadata() : total_supply(0), num_owners(0) {}
};

class CollectionMetadataFetcher {
public:
    CollectionMetadataFetcher();
    ~CollectionMetadataFetcher();
    
    // Fetch metadata from multiple sources
    std::optional<CollectionMetadata> fetch(
        const std::string& collection_address,
        const std::string& chain = "ethereum"
    );
    
    // Fetch from OpenSea
    std::optional<CollectionMetadata> fetch_from_opensea(
        const std::string& collection_address
    );
    
    // Fetch from contract directly (ERC-721/ERC-1155)
    std::optional<CollectionMetadata> fetch_from_contract(
        const std::string& collection_address,
        const std::string& rpc_url
    );
    
private:
    std::unique_ptr<HTTPClient> client_;
    std::string opensea_api_key_;
};

// Main service class
class NFTFloorService {
public:
    NFTFloorService();
    ~NFTFloorService();
    
    // Initialize service with configuration
    bool initialize(const std::string& config_path);
    
    // Start the service
    void start();
    
    // Stop the service
    void stop();
    
    // Get floor price for collection
    FloorPriceResult get_floor_price(
        const std::string& collection_address,
        const std::string& chain = "ethereum",
        bool use_cache = true
    );
    
    // Get collection metadata
    std::optional<CollectionMetadata> get_collection_metadata(
        const std::string& collection_address,
        const std::string& chain = "ethereum"
    );
    
    // Subscribe to floor price updates
    using FloorUpdateCallback = std::function<void(const FloorPriceResult&)>;
    void subscribe(const std::string& collection_address, FloorUpdateCallback callback);
    void unsubscribe(const std::string& collection_address);
    
    // Batch query
    std::map<std::string, FloorPriceResult> get_floor_prices_batch(
        const std::vector<std::string>& collection_addresses,
        const std::string& chain = "ethereum"
    );
    
private:
    std::unique_ptr<FloorPriceCalculator> calculator_;
    std::unique_ptr<CollectionMetadataFetcher> metadata_fetcher_;
    
    std::unordered_map<std::string, std::vector<FloorUpdateCallback>> subscribers_;
    mutable std::mutex subscriber_mutex_;
    
    std::atomic<bool> running_{false};
    std::thread update_thread_;
    
    void update_loop();
};

} // namespace tigerscan

#endif // TIGERSCAN_NFT_FLOOR_CALCULATOR_HPP
