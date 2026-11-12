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

namespace {

/// Function selectors for the ERC-721 and ERC-1155 metadata surface.
///
/// These are the first four bytes of the Keccak-256 hash of the signature, and
/// they are fixed by the standards, so they are written out rather than hashed
/// at run time.
constexpr const char* SELECTOR_NAME          = "06fdde03"; // name()
constexpr const char* SELECTOR_SYMBOL        = "95d89b41"; // symbol()
constexpr const char* SELECTOR_TOTAL_SUPPLY  = "18160ddd"; // totalSupply()
constexpr const char* SELECTOR_SUPPORTS_IFACE = "01ffc9a7"; // supportsInterface(bytes4)
constexpr const char* SELECTOR_TOKEN_URI     = "c87b56dd"; // tokenURI(uint256)
constexpr const char* SELECTOR_URI           = "0e89341c"; // uri(uint256)

constexpr const char* ERC721_INTERFACE_ID  = "80ac58cd";
constexpr const char* ERC1155_INTERFACE_ID = "d9b67a26";

/// Strip an optional `0x` prefix and return the lower-case hex digits.
std::string strip_hex_prefix(const std::string& value) {
    if (value.size() >= 2 && value[0] == '0' && (value[1] == 'x' || value[1] == 'X')) {
        return value.substr(2);
    }
    return value;
}

int hex_digit(char c) {
    if (c >= '0' && c <= '9') return c - '0';
    if (c >= 'a' && c <= 'f') return c - 'a' + 10;
    if (c >= 'A' && c <= 'F') return c - 'A' + 10;
    return -1;
}

/// Decode a hex string into bytes, ignoring a leading `0x`. A malformed string
/// yields an empty vector rather than a partially decoded one, so a caller
/// cannot mistake garbage for a short value.
std::vector<unsigned char> hex_to_bytes(const std::string& value) {
    std::string digits = strip_hex_prefix(value);
    if (digits.size() % 2 != 0) {
        return {};
    }
    std::vector<unsigned char> out;
    out.reserve(digits.size() / 2);
    for (size_t i = 0; i < digits.size(); i += 2) {
        int high = hex_digit(digits[i]);
        int low = hex_digit(digits[i + 1]);
        if (high < 0 || low < 0) {
            return {};
        }
        out.push_back(static_cast<unsigned char>((high << 4) | low));
    }
    return out;
}

/// A JSON-RPC call that returns the `result` value, or `nullptr` when the node
/// reported an error, the transport failed, or the response was not shaped like
/// a JSON-RPC reply. The caller owns the returned reference.
json_t* rpc_call(HTTPClient& client, const std::string& rpc_url,
                 const std::string& method, json_t* params) {
    json_t* request = json_object();
    json_object_set_new(request, "jsonrpc", json_string("2.0"));
    json_object_set_new(request, "id", json_integer(1));
    json_object_set_new(request, "method", json_string(method.c_str()));
    // `params` is adopted by the object, so a null params becomes an empty list
    // rather than a JSON null, which some nodes reject.
    json_object_set_new(request, "params", params ? params : json_array());

    char* encoded = json_dumps(request, JSON_COMPACT);
    json_decref(request);
    if (!encoded) {
        return nullptr;
    }
    std::string body(encoded);
    free(encoded);

    auto headers = std::map<std::string, std::string>{
        {"Content-Type", "application/json"},
        {"Accept", "application/json"}
    };

    std::string response = client.post(rpc_url, body, headers, 5000);
    if (response.empty()) {
        return nullptr;
    }

    json_error_t error;
    json_t* root = json_loads(response.c_str(), 0, &error);
    if (!root) {
        return nullptr;
    }

    // A node that could not answer sets `error` and leaves `result` out.
    if (json_object_get(root, "error")) {
        json_decref(root);
        return nullptr;
    }

    json_t* result = json_object_get(root, "result");
    if (!result) {
        json_decref(root);
        return nullptr;
    }

    // The caller takes ownership of the result alone.
    json_incref(result);
    json_decref(root);
    return result;
}

/// A JSON-RPC call that returns a hex string, or an empty string on any error.
std::string rpc_hex(HTTPClient& client, const std::string& rpc_url,
                    const std::string& method, json_t* params) {
    json_t* result = rpc_call(client, rpc_url, method, params);
    if (!result) {
        return "";
    }
    std::string value = json_is_string(result) ? json_string_value(result) : "";
    json_decref(result);
    return value;
}

/// An ABI string is offset, length, then UTF-8 bytes padded to a word. A
/// contract that returns something else yields an empty string rather than a
/// decoded fragment.
std::string decode_abi_string(const std::string& hex) {
    std::vector<unsigned char> data = hex_to_bytes(hex);
    // A dynamic string needs the offset word and the length word before any
    // payload can be read.
    if (data.size() < 64) {
        return "";
    }

    // Read the offset, which is the byte position of the length word.
    size_t offset = 0;
    for (size_t i = 0; i < 32; ++i) {
        // A real offset never exceeds the payload, so a high byte is malformed.
        if (i < 24 && data[i] != 0) {
            return "";
        }
        if (i >= 24) {
            offset = (offset << 8) | data[i];
        }
    }
    if (offset + 32 > data.size()) {
        return "";
    }

    size_t length = 0;
    for (size_t i = offset; i < offset + 32; ++i) {
        if (i < offset + 24 && data[i] != 0) {
            return "";
        }
        if (i >= offset + 24) {
            length = (length << 8) | data[i];
        }
    }
    if (length == 0 || offset + 32 + length > data.size()) {
        return "";
    }

    return std::string(reinterpret_cast<const char*>(&data[offset + 32]), length);
}

/// A string that is not length-prefixed: a `bytes32` returned by the older
/// `name`/`symbol` implementations, where the text runs up to the first zero
/// byte. A dynamic return is also accepted, in which case the payload follows
/// the offset and length words.
std::string decode_short_string(const std::string& hex) {
    std::vector<unsigned char> data = hex_to_bytes(hex);
    if (data.size() < 32) {
        return "";
    }

    // When a second word is present this is a dynamic return, so the payload
    // starts after it; otherwise the first word holds the text itself.
    size_t start = data.size() >= 64 ? 32 : 0;
    size_t end = std::min(start + 32, data.size());

    std::string out;
    for (size_t i = start; i < end; ++i) {
        if (data[i] == 0) {
            break;
        }
        out.push_back(static_cast<char>(data[i]));
    }
    return out;
}

/// A uint256 return value, clamped to `int`. A metadata count larger than
/// `INT_MAX` is not representable in the struct, so it saturates.
int decode_abi_uint(const std::string& hex) {
    std::vector<unsigned char> data = hex_to_bytes(hex);
    if (data.size() < 32) {
        return 0;
    }
    // Anything above the low four bytes exceeds `INT_MAX` and saturates.
    for (size_t i = 0; i < 28; ++i) {
        if (data[i] != 0) {
            return INT_MAX;
        }
    }
    long long value = 0;
    for (size_t i = 28; i < 32; ++i) {
        value = (value << 8) | data[i];
    }
    return static_cast<int>(value);
}

/// A left-padded 32-byte argument, as an ABI call encodes an address or an
/// interface id.
std::string encode_word(const std::string& hex_digits) {
    if (hex_digits.size() > 64) {
        return "";
    }
    return std::string(64 - hex_digits.size(), '0') + hex_digits;
}

/// The selector plus one left-padded argument.
std::string encode_call_with_word(const char* selector, const std::string& argument_hex) {
    return std::string(selector) + encode_word(argument_hex);
}

} // namespace

std::optional<CollectionMetadata> CollectionMetadataFetcher::fetch_from_contract(
    const std::string& collection_address,
    const std::string& rpc_url
) {
    if (rpc_url.empty()) {
        return std::nullopt;
    }

    // A JSON-RPC `to` field must be a 20-byte address. Anything else would be
    // sent to the node as a malformed request, so it is rejected here.
    std::string address_digits = strip_hex_prefix(collection_address);
    if (address_digits.size() != 40) {
        return std::nullopt;
    }
    for (char c : address_digits) {
        if (hex_digit(c) < 0) {
            return std::nullopt;
        }
    }
    std::string address_field = "0x" + address_digits;

    // Build the request once: `eth_call` against the collection address.
    auto make_params = [&](const std::string& data) {
        json_t* call = json_object();
        json_object_set_new(call, "to", json_string(address_field.c_str()));
        json_object_set_new(call, "data", json_string(("0x" + data).c_str()));
        json_t* params = json_array();
        json_array_append_new(params, call);
        json_array_append_new(params, json_string("latest"));
        return params;
    };

    auto eth_call = [&](const std::string& data) {
        return rpc_hex(*client_, rpc_url, "eth_call", make_params(data));
    };

    CollectionMetadata metadata;
    metadata.address = address_field;

    // The contract type decides how the rest of the surface is read, so it is
    // probed first. A contract that answers neither probe is not a token.
    bool is_721 = false;
    bool is_1155 = false;
    std::string iface_721 = eth_call(
        encode_call_with_word(SELECTOR_SUPPORTS_IFACE, ERC721_INTERFACE_ID));
    if (!iface_721.empty() && decode_abi_uint(iface_721) == 1) {
        is_721 = true;
    }
    std::string iface_1155 = eth_call(
        encode_call_with_word(SELECTOR_SUPPORTS_IFACE, ERC1155_INTERFACE_ID));
    if (!iface_1155.empty() && decode_abi_uint(iface_1155) == 1) {
        is_1155 = true;
    }

    if (!is_721 && !is_1155) {
        // `supportsInterface` is optional on ERC-721. A contract that does not
        // implement it is still a token if it answers `totalSupply`, so that is
        // the fallback rather than rejecting it outright.
        std::string supply = eth_call(SELECTOR_TOTAL_SUPPLY);
        if (supply.empty()) {
            return std::nullopt;
        }
        is_721 = true;
    }

    metadata.contract_type = is_1155 ? "ERC1155" : "ERC721";
    metadata.schema_name = metadata.contract_type;

    std::string name = decode_abi_string(eth_call(SELECTOR_NAME));
    if (name.empty()) {
        // Some early contracts returned a `bytes32` rather than a string.
        name = decode_short_string(eth_call(SELECTOR_NAME));
    }
    metadata.name = name;

    std::string symbol = decode_abi_string(eth_call(SELECTOR_SYMBOL));
    if (symbol.empty()) {
        symbol = decode_short_string(eth_call(SELECTOR_SYMBOL));
    }
    metadata.symbol = symbol;

    // `totalSupply` is not part of the ERC-721 standard, so a contract that
    // omits it leaves the count at zero rather than failing the whole fetch.
    std::string supply_hex = eth_call(SELECTOR_TOTAL_SUPPLY);
    if (!supply_hex.empty()) {
        metadata.total_supply = decode_abi_uint(supply_hex);
    }

    // The collection's own metadata URI. ERC-721 exposes it per token, so token
    // one is the representative; ERC-1155 exposes a collection-wide template.
    std::string uri_hex;
    if (is_1155) {
        uri_hex = eth_call(SELECTOR_URI);
    } else {
        std::string token_one = encode_word("1");
        uri_hex = eth_call(std::string(SELECTOR_TOKEN_URI) + token_one);
    }
    if (!uri_hex.empty()) {
        std::string uri = decode_abi_string(uri_hex);
        if (uri.empty()) {
            uri = decode_short_string(uri_hex);
        }
        metadata.external_url = uri;
    }

    return metadata;
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
