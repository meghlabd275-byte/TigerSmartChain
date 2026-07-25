/**
 * TigerScan NFT Floor Price Calculator - Implementation
 * 
 * High-performance C++ implementation for NFT floor price calculation
 * with real marketplace API integration.
 */

#include "floor_calculator.hpp"
#include <iostream>
#include <sstream>
#include <iomanip>
#include <thread>
#include <queue>
#include <condition_variable>

namespace tigerscan {

// HTTPClient Implementation
HTTPClient::HTTPClient() : curl_(nullptr), default_timeout_(5000) {
    curl_global_init(CURL_GLOBAL_DEFAULT);
    curl_ = curl_easy_init();
}

HTTPClient::~HTTPClient() {
    if (curl_) {
        curl_easy_cleanup(curl_);
    }
    curl_global_cleanup();
}

void HTTPClient::set_api_key(const std::string& key) {
    api_key_ = key;
}

void HTTPClient::set_timeout(int timeout_ms) {
    default_timeout_ = timeout_ms;
}

size_t HTTPClient::write_callback(void* contents, size_t size, size_t nmemb, void* userp) {
    ((std::string*)userp)->append((char*)contents, size * nmemb);
    return size * nmemb;
}

std::string HTTPClient::perform_request(const std::string& url, const std::string& method,
                                       const std::string& body,
                                       const std::map<std::string, std::string>& headers) {
    CURL* curl = curl_easy_init();
    if (!curl) return "";
    
    std::string response;
    
    curl_easy_setopt(curl, CURLOPT_URL, url.c_str());
    curl_easy_setopt(curl, CURLOPT_WRITEFUNCTION, write_callback);
    curl_easy_setopt(curl, CURLOPT_WRITEDATA, &response);
    curl_easy_setopt(curl, CURLOPT_FOLLOWLOCATION, 1L);
    curl_easy_setopt(curl, CURLOPT_TIMEOUT_MS, default_timeout_);
    curl_easy_setopt(curl, CURLOPT_CONNECTTIMEOUT_MS, 3000);
    
    // Set method
    if (method == "POST") {
        curl_easy_setopt(curl, CURLOPT_POST, 1L);
        if (!body.empty()) {
            curl_easy_setopt(curl, CURLOPT_POSTFIELDS, body.c_str());
        }
    } else if (method == "GET") {
        curl_easy_setopt(curl, CURLOPT_HTTPGET, 1L);
    }
    
    // Set headers
    struct curl_slist* header_list = nullptr;
    for (const auto& header : headers) {
        std::string header_str = header.first + ": " + header.second;
        header_list = curl_slist_append(header_list, header_str.c_str());
    }
    
    // Add API key header if set
    if (!api_key_.empty()) {
        std::string auth_header = "X-API-Key: " + api_key_;
        header_list = curl_slist_append(header_list, auth_header.c_str());
    }
    
    if (header_list) {
        curl_easy_setopt(curl, CURLOPT_HTTPHEADER, header_list);
    }
    
    CURLcode res = curl_easy_perform(curl);
    
    long http_code = 0;
    curl_easy_getinfo(curl, CURLINFO_RESPONSE_CODE, &http_code);
    
    if (header_list) {
        curl_slist_free_all(header_list);
    }
    
    curl_easy_cleanup(curl);
    
    if (res != CURLE_OK || http_code != 200) {
        return "";
    }
    
    return response;
}

std::string HTTPClient::get(const std::string& url, 
                            const std::map<std::string, std::string>& headers,
                            int timeout_ms) {
    int old_timeout = default_timeout_;
    default_timeout_ = timeout_ms;
    std::string result = perform_request(url, "GET", "", headers);
    default_timeout_ = old_timeout;
    return result;
}

std::string HTTPClient::post(const std::string& url, 
                             const std::string& body,
                             const std::map<std::string, std::string>& headers,
                             int timeout_ms) {
    int old_timeout = default_timeout_;
    default_timeout_ = timeout_ms;
    std::string result = perform_request(url, "POST", body, headers);
    default_timeout_ = old_timeout;
    return result;
}

// FloorPriceCalculator Implementation
FloorPriceCalculator::FloorPriceCalculator() {}

FloorPriceCalculator::~FloorPriceCalculator() {}

void FloorPriceCalculator::initialize(const std::vector<MarketplaceConfig>& configs) {
    std::unique_lock lock(marketplace_mutex_);
    
    for (const auto& config : configs) {
        marketplaces_[config.name] = config;
        
        // Create HTTP client for this marketplace
        auto client = std::make_unique<HTTPClient>();
        if (!config.api_key.empty()) {
            client->set_api_key(config.api_key);
        }
        client->set_timeout(config.timeout_ms);
        
        std::unique_lock client_lock(clients_mutex_);
        clients_[config.name] = std::move(client);
    }
}

void FloorPriceCalculator::add_marketplace(const MarketplaceConfig& config) {
    std::unique_lock lock(marketplace_mutex_);
    marketplaces_[config.name] = config;
    
    auto client = std::make_unique<HTTPClient>();
    if (!config.api_key.empty()) {
        client->set_api_key(config.api_key);
    }
    client->set_timeout(config.timeout_ms);
    
    std::unique_lock client_lock(clients_mutex_);
    clients_[config.name] = std::move(client);
}

void FloorPriceCalculator::remove_marketplace(const std::string& name) {
    std::unique_lock lock(marketplace_mutex_);
    marketplaces_.erase(name);
    
    std::unique_lock client_lock(clients_mutex_);
    clients_.erase(name);
}

std::vector<SaleData> FloorPriceCalculator::fetch_opensea_sales(
    const std::string& collection_address,
    int limit
) {
    std::vector<SaleData> sales;
    
    std::shared_lock client_lock(clients_mutex_);
    auto it = clients_.find("opensea");
    if (it == clients_.end()) {
        return sales;
    }
    auto& client = it->second;
    client_lock.unlock();
    
    std::string url = "https://api.opensea.io/api/v2/collections/" + 
                      collection_address + "/floor-price";
    
    auto headers = std::map<std::string, std::string>{
        {"Accept", "application/json"}
    };
    
    std::string response = client->get(url, headers, 5000);
    
    if (response.empty()) {
        return sales;
    }
    
    // Parse JSON response
    json_error_t error;
    json_t* root = json_loads(response.c_str(), 0, &error);
    if (!root) {
        return sales;
    }
    
    json_t* floor_price = json_object_get(root, "floor_price");
    if (floor_price) {
        SaleData sale;
        sale.price_wei = json_number_value(floor_price) * PriceConverter::WEI_PER_ETH;
        sale.marketplace = "opensea";
        sale.timestamp = std::chrono::duration_cast<std::chrono::seconds>(
            std::chrono::system_clock::now().time_since_epoch()
        ).count();
        sales.push_back(sale);
    }
    
    json_decref(root);
    
    // Also fetch recent sales for more data
    url = "https://api.opensea.io/api/v2/events/collection/" + collection_address +
          "?event_type=successful&limit=" + std::to_string(limit);
    
    response = client->get(url, headers, 5000);
    
    if (!response.empty()) {
        root = json_loads(response.c_str(), 0, &error);
        if (root) {
            json_t* asset_events = json_object_get(root, "asset_events");
            if (asset_events && json_is_array(asset_events)) {
                size_t index;
                json_t* event;
                
                json_array_foreach(asset_events, index, event) {
                    json_t* closing_price = json_object_get(event, "closing_price");
                    json_t* token_id = json_object_get(event, "token_id");
                    
                    if (closing_price && token_id) {
                        SaleData sale;
                        sale.token_id = json_string_value(token_id);
                        sale.price_wei = json_number_value(closing_price);
                        sale.marketplace = "opensea";
                        
                        json_t* timestamp = json_object_get(event, "timestamp");
                        if (timestamp) {
                            sale.timestamp = static_cast<uint64_t>(json_number_value(timestamp));
                        }
                        
                        sales.push_back(sale);
                    }
                }
            }
            json_decref(root);
        }
    }
    
    return sales;
}

std::vector<SaleData> FloorPriceCalculator::fetch_blur_sales(
    const std::string& collection_address,
    int limit
) {
    std::vector<SaleData> sales;
    
    std::shared_lock client_lock(clients_mutex_);
    auto it = clients_.find("blur");
    if (it == clients_.end()) {
        return sales;
    }
    auto& client = it->second;
    client_lock.unlock();
    
    // Blur API - fetch collection stats
    std::string url = "https://api.blur.io/collections/" + collection_address;
    
    auto headers = std::map<std::string, std::string>{
        {"Accept", "application/json"}
    };
    
    std::string response = client->get(url, headers, 5000);
    
    if (response.empty()) {
        return sales;
    }
    
    json_error_t error;
    json_t* root = json_loads(response.c_str(), 0, &error);
    if (!root) {
        return sales;
    }
    
    json_t* floor_price = json_object_get(root, "floorPrice");
    if (floor_price) {
        SaleData sale;
        sale.price_wei = json_number_value(floor_price) * PriceConverter::WEI_PER_ETH;
        sale.marketplace = "blur";
        sale.timestamp = std::chrono::duration_cast<std::chrono::seconds>(
            std::chrono::system_clock::now().time_since_epoch()
        ).count();
        sales.push_back(sale);
    }
    
    json_decref(root);
    return sales;
}

std::vector<SaleData> FloorPriceCalculator::fetch_uniswap_sales(
    const std::string& collection_address,
    int limit
) {
    std::vector<SaleData> sales;
    
    std::shared_lock client_lock(clients_mutex_);
    auto it = clients_.find("uniswap");
    if (it == clients_.end()) {
        return sales;
    }
    auto& client = it->second;
    client_lock.unlock();
    
    // Uniswap NFT API
    std::string url = "https://api.uniswap.org/v1/nfts/collection/" + 
                      collection_address + "/stats";
    
    auto headers = std::map<std::string, std::string>{
        {"Accept", "application/json"}
    };
    
    std::string response = client->get(url, headers, 5000);
    
    if (response.empty()) {
        return sales;
    }
    
    json_error_t error;
    json_t* root = json_loads(response.c_str(), 0, &error);
    if (!root) {
        return sales;
    }
    
    json_t* data = json_object_get(root, "data");
    if (data) {
        json_t* floor_price = json_object_get(data, "floorPrice");
        if (floor_price) {
            SaleData sale;
            sale.price_wei = json_number_value(floor_price) * PriceConverter::WEI_PER_ETH;
            sale.marketplace = "uniswap";
            sale.timestamp = std::chrono::duration_cast<std::chrono::seconds>(
                std::chrono::system_clock::now().time_since_epoch()
            ).count();
            sales.push_back(sale);
        }
    }
    
    json_decref(root);
    return sales;
}

std::vector<SaleData> FloorPriceCalculator::fetch_looksrare_sales(
    const std::string& collection_address,
    int limit
) {
    std::vector<SaleData> sales;
    
    std::shared_lock client_lock(clients_mutex_);
    auto it = clients_.find("looksrare");
    if (it == clients_.end()) {
        return sales;
    }
    auto& client = it->second;
    client_lock.unlock();
    
    // LooksRare API
    std::string url = "https://api.looksrare.org/api/v1/collections/" + 
                      collection_address + "/stats";
    
    auto headers = std::map<std::string, std::string>{
        {"Accept", "application/json"}
    };
    
    std::string response = client->get(url, headers, 5000);
    
    if (response.empty()) {
        return sales;
    }
    
    json_error_t error;
    json_t* root = json_loads(response.c_str(), 0, &error);
    if (!root) {
        return sales;
    }
    
    json_t* data = json_object_get(root, "data");
    if (data) {
        json_t* floor_price = json_object_get(data, "floorPrice");
        if (floor_price) {
            SaleData sale;
            sale.price_wei = json_number_value(floor_price) * PriceConverter::WEI_PER_ETH;
            sale.marketplace = "looksrare";
            sale.timestamp = std::chrono::duration_cast<std::chrono::seconds>(
                std::chrono::system_clock::now().time_since_epoch()
            ).count();
            sales.push_back(sale);
        }
    }
    
    json_decref(root);
    return sales;
}

double FloorPriceCalculator::calculate_weighted_average(
    const std::map<std::string, std::vector<double>>& prices,
    const std::map<std::string, double>& weights
) {
    double weighted_sum = 0;
    double total_weight = 0;
    
    for (const auto& [marketplace, price_list] : prices) {
        if (price_list.empty()) continue;
        
        double marketplace_floor = price_list.front(); // Use floor price
        double weight = 1.0;
        
        auto it = weights.find(marketplace);
        if (it != weights.end()) {
            weight = it->second;
        }
        
        weighted_sum += marketplace_floor * weight;
        total_weight += weight;
    }
    
    return total_weight > 0 ? weighted_sum / total_weight : 0;
}

double FloorPriceCalculator::calculate_median(std::vector<double>& prices) {
    if (prices.empty()) return 0;
    
    std::sort(prices.begin(), prices.end());
    
    size_t n = prices.size();
    if (n % 2 == 0) {
        return (prices[n/2 - 1] + prices[n/2]) / 2.0;
    } else {
        return prices[n/2];
    }
}

double FloorPriceCalculator::calculate_std_dev(const std::vector<double>& prices, double mean) {
    if (prices.size() < 2) return 0;
    
    double sum_squared_diff = 0;
    for (double price : prices) {
        double diff = price - mean;
        sum_squared_diff += diff * diff;
    }
    
    return std::sqrt(sum_squared_diff / (prices.size() - 1));
}

FloorPriceResult FloorPriceCalculator::calculate_floor(
    const std::string& collection_address,
    const std::string& chain,
    int sample_size
) {
    FloorPriceResult result;
    result.collection_address = collection_address;
    result.timestamp = std::chrono::system_clock::now();
    result.is_estimated = false;
    
    total_requests_++;
    
    auto start_time = std::chrono::high_resolution_clock::now();
    
    // Fetch sales from all configured marketplaces in parallel
    std::vector<std::future<std::vector<SaleData>>> futures;
    
    {
        std::shared_lock lock(marketplace_mutex_);
        if (marketplaces_.find("opensea") != marketplaces_.end()) {
            futures.push_back(std::async(std::launch::async, [this, &collection_address, sample_size]() {
                return fetch_opensea_sales(collection_address, sample_size);
            }));
        }
        
        if (marketplaces_.find("blur") != marketplaces_.end()) {
            futures.push_back(std::async(std::launch::async, [this, &collection_address, sample_size]() {
                return fetch_blur_sales(collection_address, sample_size);
            }));
        }
        
        if (marketplaces_.find("uniswap") != marketplaces_.end()) {
            futures.push_back(std::async(std::launch::async, [this, &collection_address, sample_size]() {
                return fetch_uniswap_sales(collection_address, sample_size);
            }));
        }
        
        if (marketplaces_.find("looksrare") != marketplaces_.end()) {
            futures.push_back(std::async(std::launch::async, [this, &collection_address, sample_size]() {
                return fetch_looksrare_sales(collection_address, sample_size);
            }));
        }
    }
    
    // Collect all sales
    std::vector<SaleData> all_sales;
    for (auto& future : futures) {
        try {
            auto sales = future.get();
            all_sales.insert(all_sales.end(), sales.begin(), sales.end());
        } catch (...) {
            failed_requests_++;
        }
    }
    
    // Calculate statistics
    if (all_sales.empty()) {
        result.is_estimated = true;
        
        // Try to get from cache
        return calculate_from_cache(collection_address);
    }
    
    // Group prices by marketplace
    std::map<std::string, std::vector<double>> prices_by_marketplace;
    std::map<std::string, double> weights;
    
    for (const auto& sale : all_sales) {
        prices_by_marketplace[sale.marketplace].push_back(sale.price_wei);
    }
    
    // Get weights
    {
        std::shared_lock lock(marketplace_mutex_);
        for (const auto& [name, config] : marketplaces_) {
            weights[name] = config.weight;
        }
    }
    
    // Calculate floor from each marketplace
    for (const auto& [marketplace, prices] : prices_by_marketplace) {
        if (!prices.empty()) {
            std::vector<double> sorted_prices = prices;
            result.marketplace_floors[marketplace] = calculate_median(sorted_prices);
        }
    }
    
    // Calculate overall statistics
    std::vector<double> all_prices;
    for (const auto& sale : all_sales) {
        all_prices.push_back(sale.price_wei);
    }
    
    if (!all_prices.empty()) {
        result.floor_price_wei = calculate_median(all_prices);
        result.floor_price_eth = PriceConverter::wei_to_eth(result.floor_price_wei);
        
        result.weighted_average = calculate_weighted_average(prices_by_marketplace, weights);
        result.median = result.floor_price_eth;
        result.std_deviation = PriceConverter::wei_to_eth(
            calculate_std_dev(all_prices, result.floor_price_wei)
        );
        result.sample_size = static_cast<int>(all_sales.size());
    }
    
    // Update cache
    update_cache(collection_address, all_sales);
    
    auto end_time = std::chrono::high_resolution_clock::now();
    auto duration = std::chrono::duration_cast<std::chrono::milliseconds>(end_time - start_time);
    
    {
        std::lock_guard lock(stats_mutex_);
        total_response_time_ += duration.count();
        successful_requests_++;
    }
    
    return result;
}

FloorPriceResult FloorPriceCalculator::calculate_from_cache(
    const std::string& collection_address
) {
    std::shared_lock lock(cache_mutex_);
    
    auto it = cache_.find(collection_address);
    if (it != cache_.end()) {
        auto& entry = it->second;
        auto age = std::chrono::system_clock::now() - entry.timestamp;
        
        if (age < entry.ttl) {
            return entry.result;
        }
    }
    
    FloorPriceResult result;
    result.collection_address = collection_address;
    result.is_estimated = true;
    
    return result;
}

void FloorPriceCalculator::update_cache(
    const std::string& collection_address,
    const std::vector<SaleData>& sales
) {
    std::unique_lock lock(cache_mutex_);
    
    CacheEntry& entry = cache_[collection_address];
    entry.sales = sales;
    entry.timestamp = std::chrono::system_clock::now();
    entry.ttl = DEFAULT_CACHE_TTL;
    
    // Calculate result
    if (!sales.empty()) {
        std::vector<double> all_prices;
        for (const auto& sale : sales) {
            all_prices.push_back(sale.price_wei);
        }
        
        entry.result.floor_price_wei = calculate_median(all_prices);
        entry.result.floor_price_eth = PriceConverter::wei_to_eth(entry.result.floor_price_wei);
        entry.result.sample_size = sales.size();
        entry.result.timestamp = entry.timestamp;
        entry.result.collection_address = collection_address;
    }
}

std::vector<std::string> FloorPriceCalculator::get_supported_marketplaces() const {
    std::shared_lock lock(marketplace_mutex_);
    
    std::vector<std::string> result;
    for (const auto& [name, _] : marketplaces_) {
        result.push_back(name);
    }
    return result;
}

FloorPriceCalculator::Stats FloorPriceCalculator::get_stats() const {
    Stats stats;
    stats.total_requests = total_requests_;
    stats.successful_requests = successful_requests_;
    stats.failed_requests = failed_requests_;
    
    uint64_t total = successful_requests_;
    if (total > 0) {
        stats.average_response_time_ms = total_response_time_.load() / total;
    }
    
    std::lock_guard lock(stats_mutex_);
    stats.last_update = std::chrono::system_clock::now();
    
    return stats;
}

void FloorPriceCalculator::reset_stats() {
    total_requests_ = 0;
    successful_requests_ = 0;
    failed_requests_ = 0;
    total_response_time_ = 0;
}

// CollectionMetadataFetcher Implementation
CollectionMetadataFetcher::CollectionMetadataFetcher() {
    client_ = std::make_unique<HTTPClient>();
}

CollectionMetadataFetcher::~CollectionMetadataFetcher() {}

std::optional<CollectionMetadata> CollectionMetadataFetcher::fetch(
    const std::string& collection_address,
    const std::string& chain
) {
    // Try OpenSea first
    auto metadata = fetch_from_opensea(collection_address);
    if (metadata) {
        return metadata;
    }
    
    // Fall back to contract fetch
    return std::nullopt;
}

std::optional<CollectionMetadata> CollectionMetadataFetcher::fetch_from_opensea(
    const std::string& collection_address
) {
    if (opensea_api_key_.empty()) {
        return std::nullopt;
    }
    
    client_->set_api_key(opensea_api_key_);
    
    std::string url = "https://api.opensea.io/api/v2/collections/" + collection_address;
    
    auto headers = std::map<std::string, std::string>{
        {"Accept", "application/json"}
    };
    
    std::string response = client_->get(url, headers, 5000);
    
    if (response.empty()) {
        return std::nullopt;
    }
    
    json_error_t error;
    json_t* root = json_loads(response.c_str(), 0, &error);
    if (!root) {
        return std::nullopt;
    }
    
    CollectionMetadata metadata;
    metadata.address = collection_address;
    
    json_t* collection = json_object_get(root, "collection");
    if (!collection) {
        json_decref(root);
        return std::nullopt;
    }
    
    json_t* name = json_object_get(collection, "name");
    if (name) {
        metadata.name = json_string_value(name);
    }
    
    json_t* description = json_object_get(collection, "description");
    if (description) {
        metadata.description = json_string_value(description);
    }
    
    json_t* image_url = json_object_get(collection, "image_url");
    if (image_url) {
        metadata.image_url = json_string_value(image_url);
    }
    
    json_t* external_url = json_object_get(collection, "external_url");
    if (external_url) {
        metadata.external_url = json_string_value(external_url);
    }
    
    json_t* stats = json_object_get(collection, "stats");
    if (stats) {
        json_t* total_supply = json_object_get(stats, "total_supply");
        if (total_supply) {
            metadata.total_supply = static_cast<int>(json_number_value(total_supply));
        }
        
        json_t* num_owners = json_object_get(stats, "num_owners");
        if (num_owners) {
            metadata.num_owners = static_cast<int>(json_number_value(num_owners));
        }
    }
    
    json_decref(root);
    return metadata;
}

std::optional<CollectionMetadata> CollectionMetadataFetcher::fetch_from_contract(
    const std::string& collection_address,
    const std::string& rpc_url
) {
    // This would require an RPC client - placeholder for now
    return std::nullopt;
}

// NFTFloorService Implementation
NFTFloorService::NFTFloorService() {
    calculator_ = std::make_unique<FloorPriceCalculator>();
    metadata_fetcher_ = std::make_unique<CollectionMetadataFetcher>();
}

NFTFloorService::~NFTFloorService() {
    stop();
}

bool NFTFloorService::initialize(const std::string& config_path) {
    // Default marketplace configurations
    std::vector<MarketplaceConfig> configs = {
        MarketplaceConfig("opensea", "https://api.opensea.io", "", "", 5000, 3, 1.0),
        MarketplaceConfig("blur", "https://api.blur.io", "", "", 5000, 3, 0.9),
        MarketplaceConfig("uniswap", "https://api.uniswap.org", "", "", 5000, 3, 0.8),
        MarketplaceConfig("looksrare", "https://api.looksrare.org", "", "", 5000, 3, 0.7)
    };
    
    calculator_->initialize(configs);
    return true;
}

void NFTFloorService::start() {
    running_ = true;
    update_thread_ = std::thread([this]() { update_loop(); });
}

void NFTFloorService::stop() {
    running_ = false;
    if (update_thread_.joinable()) {
        update_thread_.join();
    }
}

FloorPriceResult NFTFloorService::get_floor_price(
    const std::string& collection_address,
    const std::string& chain,
    bool use_cache
) {
    if (use_cache) {
        auto cached = calculator_->calculate_from_cache(collection_address);
        if (!cached.is_estimated) {
            return cached;
        }
    }
    
    return calculator_->calculate_floor(collection_address, chain);
}

std::optional<CollectionMetadata> NFTFloorService::get_collection_metadata(
    const std::string& collection_address,
    const std::string& chain
) {
    return metadata_fetcher_->fetch(collection_address, chain);
}

void NFTFloorService::subscribe(const std::string& collection_address, 
                                FloorUpdateCallback callback) {
    std::lock_guard lock(subscriber_mutex_);
    subscribers_[collection_address].push_back(callback);
}

void NFTFloorService::unsubscribe(const std::string& collection_address) {
    std::lock_guard lock(subscriber_mutex_);
    subscribers_.erase(collection_address);
}

std::map<std::string, FloorPriceResult> NFTFloorService::get_floor_prices_batch(
    const std::vector<std::string>& collection_addresses,
    const std::string& chain
) {
    std::map<std::string, FloorPriceResult> results;
    
    std::vector<std::future<std::pair<std::string, FloorPriceResult>>> futures;
    
    for (const auto& address : collection_addresses) {
        futures.push_back(std::async(std::launch::async, [this, &address, &chain]() {
            return std::make_pair(address, get_floor_price(address, chain));
        }));
    }
    
    for (auto& future : futures) {
        try {
            auto [address, result] = future.get();
            results[address] = result;
        } catch (...) {
            // Continue with other results
        }
    }
    
    return results;
}

void NFTFloorService::update_loop() {
    while (running_) {
        std::this_thread::sleep_for(std::chrono::minutes(1));
        
        // Notify subscribers of updates
        std::lock_guard lock(subscriber_mutex_);
        for (const auto& [address, callbacks] : subscribers_) {
            auto result = get_floor_price(address, "ethereum", true);
            for (const auto& callback : callbacks) {
                callback(result);
            }
        }
    }
}

} // namespace tigerscan
