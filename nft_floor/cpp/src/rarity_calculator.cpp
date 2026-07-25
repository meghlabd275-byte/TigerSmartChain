/**
 * TigerScan NFT Rarity Calculator - Implementation
 * 
 * High-performance C++ implementation for NFT rarity calculation
 * using OpenRarity-inspired algorithm with trait frequency analysis.
 */

#include "rarity_calculator.hpp"
#include <iostream>
#include <sstream>
#include <iomanip>
#include <thread>
#include <future>

namespace tigerscan {

// MetadataFetcher Implementation
MetadataFetcher::MetadataFetcher() : curl_(nullptr), default_timeout_(5000) {
    curl_global_init(CURL_GLOBAL_DEFAULT);
    curl_ = curl_easy_init();
}

MetadataFetcher::~MetadataFetcher() {
    if (curl_) {
        curl_easy_cleanup(curl_);
    }
    curl_global_cleanup();
}

void MetadataFetcher::set_api_key(const std::string& key) {
    api_key_ = key;
}

size_t MetadataFetcher::write_callback(void* contents, size_t size, size_t nmemb, void* userp) {
    ((std::string*)userp)->append((char*)contents, size * nmemb);
    return size * nmemb;
}

std::string MetadataFetcher::perform_request(const std::string& url, const std::string& method,
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
    
    if (method == "POST") {
        curl_easy_setopt(curl, CURLOPT_POST, 1L);
        if (!body.empty()) {
            curl_easy_setopt(curl, CURLOPT_POSTFIELDS, body.c_str());
        }
    }
    
    struct curl_slist* header_list = nullptr;
    for (const auto& header : headers) {
        std::string header_str = header.first + ": " + header.second;
        header_list = curl_slist_append(header_list, header_str.c_str());
    }
    
    if (!api_key_.empty()) {
        header_list = curl_slist_append(header_list, ("X-API-Key: " + api_key_).c_str());
    }
    
    if (header_list) {
        curl_easy_setopt(curl, CURLOPT_HTTPHEADER, header_list);
    }
    
    curl_easy_perform(curl);
    
    if (header_list) {
        curl_slist_free_all(header_list);
    }
    
    curl_easy_cleanup(curl);
    return response;
}

std::string MetadataFetcher::get(const std::string& url,
                                 const std::map<std::string, std::string>& headers,
                                 int timeout_ms) {
    int old_timeout = default_timeout_;
    default_timeout_ = timeout_ms;
    std::string result = perform_request(url, "GET", "", headers);
    default_timeout_ = old_timeout;
    return result;
}

// RarityCalculator Implementation
RarityCalculator::RarityCalculator() 
    : interpolation_factor_(1.0), use_logarithmic_(true) {}

RarityCalculator::~RarityCalculator() {}

void RarityCalculator::initialize(double interpolation_factor, bool use_logarithmic) {
    interpolation_factor_ = interpolation_factor;
    use_logarithmic_ = use_logarithmic;
}

double RarityCalculator::calculate_rarity_score(
    const std::vector<Trait>& traits,
    int total_supply
) {
    if (traits.empty() || total_supply <= 0) {
        return 0.0;
    }
    
    double linear_score = 0.0;
    double logarithmic_score = 0.0;
    
    for (const auto& trait : traits) {
        double frequency = trait.frequency;
        
        if (frequency <= 0 || frequency > 1.0) {
            frequency = 1.0 / static_cast<double>(total_supply);
        }
        
        // Linear score: inverse of frequency
        double linear_trait_score = (1.0 - frequency) * 100.0;
        linear_score += linear_trait_score;
        
        // Logarithmic score: natural log based
        double log_trait_score = -std::log(frequency);
        logarithmic_score += log_trait_score;
    }
    
    // Normalize by number of traits
    double trait_count = static_cast<double>(traits.size());
    linear_score /= trait_count;
    logarithmic_score /= trait_count;
    
    // Normalize logarithmic score to 0-100 range
    double max_log_score = -std::log(1.0 / static_cast<double>(total_supply));
    if (max_log_score > 0) {
        logarithmic_score = (logarithmic_score / max_log_score) * 100.0;
    }
    
    // Interpolate if configured
    if (use_logarithmic_ && interpolation_factor_ > 0) {
        return interpolate_score(linear_score, logarithmic_score, interpolation_factor_);
    }
    
    return linear_score;
}

double RarityCalculator::calculate_trait_rarity(
    double frequency,
    int total_supply,
    int trait_count
) {
    if (frequency <= 0) {
        frequency = 1.0 / static_cast<double>(total_supply);
    }
    
    // OpenRarity formula: -ln(frequency)
    double rarity = -std::log(frequency);
    
    // Normalize by trait count
    return rarity / static_cast<double>(trait_count);
}

double RarityCalculator::interpolate_score(
    double linear_score,
    double logarithmic_score,
    double factor
) {
    return (1.0 - factor) * linear_score + factor * logarithmic_score;
}

std::string RarityCalculator::determine_rarity_tier(double score, double max_score) {
    if (max_score <= 0) return "Common";
    
    double percentile = (score / max_score) * 100.0;
    
    if (percentile >= 99.0) return "Mythic";
    if (percentile >= 95.0) return "Legendary";
    if (percentile >= 80.0) return "Very Rare";
    if (percentile >= 50.0) return "Rare";
    if (percentile >= 20.0) return "Uncommon";
    return "Common";
}

double RarityCalculator::calculate_percentile(int rank, int total) {
    if (total <= 0 || rank <= 0) return 0.0;
    return (static_cast<double>(total - rank + 1) / static_cast<double>(total)) * 100.0;
}

RarityResult RarityCalculator::calculate_rarity(
    const std::string& token_id,
    const std::string& collection_address,
    const std::vector<Trait>& traits
) {
    auto start_time = std::chrono::high_resolution_clock::now();
    
    RarityResult result;
    result.token_id = token_id;
    result.collection_address = collection_address;
    result.traits = traits;
    result.timestamp = std::chrono::system_clock::now();
    
    // Get collection stats
    std::optional<CollectionRarityStats> stats_opt;
    {
        std::shared_lock lock(cache_mutex_);
        auto it = collection_cache_.find(collection_address);
        if (it != collection_cache_.end()) {
            stats_opt = it->second.stats;
            cache_hits_++;
        }
    }
    
    if (!stats_opt) {
        // Return with basic calculation
        result.rarity_score = calculate_rarity_score(traits, 10000);
        result.rarity_tier = determine_rarity_tier(result.rarity_score, 100.0);
        return result;
    }
    
    const auto& stats = *stats_opt;
    result.total_in_collection = stats.total_supply;
    
    // Calculate rarity score
    result.rarity_score = calculate_rarity_score(traits, stats.total_supply);
    
    // Determine tier
    result.rarity_tier = determine_rarity_tier(
        result.rarity_score,
        stats.average_rarity_score + stats.std_deviation * 3
    );
    
    // Calculate rank
    result.rarity_rank = 1;
    for (const auto& token_id_str : stats.top_rarest_tokens) {
        result.rarity_rank++;
        if (token_id_str == token_id) break;
    }
    
    // Calculate percentile
    result.percentile = calculate_percentile(result.rarity_rank, stats.total_supply);
    
    auto end_time = std::chrono::high_resolution_clock::now();
    auto duration = std::chrono::duration_cast<std::chrono::microseconds>(end_time - start_time);
    total_calculation_time_ += duration.count() / 1000.0;
    total_calculations_++;
    
    return result;
}

CollectionRarityStats RarityCalculator::calculate_collection_rarity(
    const std::string& collection_address,
    const std::vector<NFTMetadata>& nfts
) {
    CollectionRarityStats stats;
    stats.collection_address = collection_address;
    stats.calculated_at = std::chrono::system_clock::now();
    
    if (nfts.empty()) {
        return stats;
    }
    
    stats.total_supply = static_cast<int>(nfts.size());
    stats.unique_token_count = stats.total_supply;
    
    // Calculate trait frequencies
    std::map<std::string, std::map<std::string, int>> trait_counts;
    std::map<std::string, int> trait_type_counts;
    
    for (const auto& nft : nfts) {
        std::set<std::string> seen_traits;
        for (const auto& trait : nft.traits) {
            std::string trait_str = std::get<std::string>(trait.value);
            trait_counts[trait.trait_type][trait_str]++;
            if (seen_traits.find(trait.trait_type) == seen_traits.end()) {
                trait_type_counts[trait.trait_type]++;
                seen_traits.insert(trait.trait_type);
            }
        }
    }
    
    stats.trait_type_counts = trait_type_counts;
    stats.trait_value_counts = trait_counts;
    
    // Calculate rarity for each NFT
    std::vector<double> rarity_scores;
    std::vector<std::pair<std::string, double>> token_scores;
    
    for (const auto& nft : nfts) {
        // Update trait frequencies
        std::vector<Trait> updated_traits = nft.traits;
        for (auto& trait : updated_traits) {
            auto type_it = trait_counts.find(trait.trait_type);
            if (type_it != trait_counts.end()) {
                auto value_it = type_it->second.find(std::get<std::string>(trait.value));
                if (value_it != type_it->second.end()) {
                    trait.frequency = static_cast<double>(value_it->second) / 
                                     static_cast<double>(nfts.size());
                }
            }
        }
        
        double score = calculate_rarity_score(updated_traits, nfts.size());
        rarity_scores.push_back(score);
        token_scores.push_back({nft.token_id, score});
    }
    
    // Calculate statistics
    if (!rarity_scores.empty()) {
        double sum = std::accumulate(rarity_scores.begin(), rarity_scores.end(), 0.0);
        stats.average_rarity_score = sum / rarity_scores.size();
        
        // Median
        std::vector<double> sorted_scores = rarity_scores;
        std::sort(sorted_scores.begin(), sorted_scores.end());
        size_t n = sorted_scores.size();
        if (n % 2 == 0) {
            stats.median_rarity_score = (sorted_scores[n/2 - 1] + sorted_scores[n/2]) / 2.0;
        } else {
            stats.median_rarity_score = sorted_scores[n/2];
        }
        
        // Standard deviation
        double sq_sum = 0;
        for (double score : rarity_scores) {
            sq_sum += (score - stats.average_rarity_score) * (score - stats.average_rarity_score);
        }
        stats.std_deviation = std::sqrt(sq_sum / rarity_scores.size());
        
        // Top rarest tokens
        std::sort(token_scores.begin(), token_scores.end(),
                 [](const auto& a, const auto& b) { return a.second > b.second; });
        
        for (size_t i = 0; i < std::min(size_t(100), token_scores.size()); i++) {
            stats.top_rarest_tokens.push_back(token_scores[i].first);
        }
    }
    
    // Update cache
    update_collection_stats(collection_address, nfts);
    
    return stats;
}

std::vector<RarityResult> RarityCalculator::calculate_batch_rarity(
    const std::string& collection_address,
    const std::vector<std::pair<std::string, std::vector<Trait>>>& nfts
) {
    std::vector<std::future<RarityResult>> futures;
    
    for (const auto& [token_id, traits] : nfts) {
        futures.push_back(std::async(std::launch::async, [this, &collection_address, &token_id, &traits]() {
            return calculate_rarity(token_id, collection_address, traits);
        }));
    }
    
    std::vector<RarityResult> results;
    for (auto& future : futures) {
        try {
            results.push_back(future.get());
        } catch (...) {
            // Skip failed calculations
        }
    }
    
    // Sort by rarity score descending
    std::sort(results.begin(), results.end(),
             [](const RarityResult& a, const RarityResult& b) {
                 return a.rarity_score > b.rarity_score;
             });
    
    // Update ranks
    for (size_t i = 0; i < results.size(); i++) {
        results[i].rarity_rank = static_cast<int>(i + 1);
    }
    
    return results;
}

void RarityCalculator::update_collection_stats(
    const std::string& collection_address,
    const std::vector<NFTMetadata>& nfts
) {
    std::unique_lock lock(cache_mutex_);
    
    CacheEntry& entry = collection_cache_[collection_address];
    entry.stats = calculate_collection_rarity(collection_address, nfts);
    entry.timestamp = std::chrono::system_clock::now();
    entry.ttl = DEFAULT_CACHE_TTL;
}

std::optional<CollectionRarityStats> RarityCalculator::get_collection_stats(
    const std::string& collection_address
) const {
    std::shared_lock lock(cache_mutex_);
    
    auto it = collection_cache_.find(collection_address);
    if (it != collection_cache_.end()) {
        auto age = std::chrono::system_clock::now() - it->second.timestamp;
        if (age < it->second.ttl) {
            return it->second.stats;
        }
    }
    
    return std::nullopt;
}

std::map<std::string, double> RarityCalculator::get_trait_importance(
    const std::string& collection_address
) const {
    std::shared_lock lock(weight_mutex_);
    
    auto it = trait_weights_.find(collection_address);
    if (it != trait_weights_.end()) {
        return std::map<std::string, double>(it->second.begin(), it->second.end());
    }
    
    // Return default equal weights
    auto stats = get_collection_stats(collection_address);
    if (stats) {
        std::map<std::string, double> weights;
        for (const auto& [trait_type, _] : stats->trait_type_counts) {
            weights[trait_type] = 1.0;
        }
        return weights;
    }
    
    return {};
}

void RarityCalculator::set_trait_weight(
    const std::string& collection_address,
    const std::string& trait_type,
    double weight
) {
    std::unique_lock lock(weight_mutex_);
    trait_weights_[collection_address][trait_type] = weight;
}

RarityCalculator::Stats RarityCalculator::get_stats() const {
    Stats stats;
    stats.total_calculations = total_calculations_;
    stats.cache_hits = cache_hits_;
    
    uint64_t total = total_calculations_;
    if (total > 0) {
        stats.average_calculation_time_ms = total_calculation_time_ / total;
    }
    
    return stats;
}

void RarityCalculator::reset_stats() {
    total_calculations_ = 0;
    cache_hits_ = 0;
    total_calculation_time_ = 0;
}

// TraitParser Implementation
TraitParser::TraitParser() {}

TraitParser::~TraitParser() {}

std::vector<Trait> TraitParser::parse_opensea_metadata(const std::string& json) const {
    std::vector<Trait> traits;
    
    json_error_t error;
    json_t* root = json_loads(json.c_str(), 0, &error);
    if (!root) {
        return traits;
    }
    
    json_t* attributes = json_object_get(root, "attributes");
    if (!attributes || !json_is_array(attributes)) {
        json_decref(root);
        return traits;
    }
    
    traits = parse_traits_from_json(attributes);
    json_decref(root);
    
    return traits;
}

std::vector<Trait> TraitParser::parse_traits_from_json(json_t* traits_obj) const {
    std::vector<Trait> traits;
    
    if (!json_is_array(traits_obj)) {
        return traits;
    }
    
    size_t index;
    json_t* trait_obj;
    
    json_array_foreach(traits_obj, index, trait_obj) {
        json_t* trait_type = json_object_get(trait_obj, "trait_type");
        json_t* value = json_object_get(trait_obj, "value");
        json_t* display_type = json_object_get(trait_obj, "display_type");
        
        if (trait_type && value) {
            std::string type_str = json_string_value(trait_type);
            TraitValue value_var = parse_trait_value(json_string_value(value));
            
            Trait trait(type_str, value_var);
            
            // Parse numeric display types
            if (display_type) {
                std::string display_str = json_string_value(display_type);
                if (display_str == "number" || display_str == "boost_number") {
                    value_var = json_number_value(value);
                } else if (display_str == "date") {
                    // Convert timestamp
                }
            }
            
            traits.push_back(trait);
        }
    }
    
    return traits;
}

std::vector<Trait> TraitParser::parse_erc721_attributes(
    const std::string& json_attributes
) const {
    return parse_opensea_metadata(json_attributes);
}

TraitValue TraitParser::parse_trait_value(const std::string& value) {
    // Try to parse as number
    try {
        size_t pos;
        double num = std::stod(value, &pos);
        if (pos == value.size()) {
            if (num == static_cast<int>(num)) {
                return static_cast<int>(num);
            }
            return num;
        }
    } catch (...) {
        // Not a number
    }
    
    // Check for boolean
    if (value == "true" || value == "false") {
        return value == "true";
    }
    
    // Default to string
    return value;
}

// NFTRarityService Implementation
NFTRarityService::NFTRarityService() {
    calculator_ = std::make_unique<RarityCalculator>();
    metadata_fetcher_ = std::make_unique<MetadataFetcher>();
    trait_parser_ = std::make_unique<TraitParser>();
}

NFTRarityService::~NFTRarityService() {
    stop();
}

bool NFTRarityService::initialize(const std::string& config_path) {
    calculator_->initialize(1.0, true);  // Use logarithmic scoring
    return true;
}

void NFTRarityService::start() {
    running_ = true;
    update_thread_ = std::thread([this]() { update_loop(); });
}

void NFTRarityService::stop() {
    running_ = false;
    if (update_thread_.joinable()) {
        update_thread_.join();
    }
}

RarityResult NFTRarityService::get_rarity(
    const std::string& token_id,
    const std::string& collection_address,
    const std::string& chain
) {
    // Try to fetch from cache first
    auto stats = calculator_->get_collection_stats(collection_address);
    if (!stats) {
        // Need to fetch collection metadata first
        calculate_full_collection(collection_address, chain);
        stats = calculator_->get_collection_stats(collection_address);
    }
    
    if (!stats) {
        RarityResult result;
        result.token_id = token_id;
        result.collection_address = collection_address;
        return result;
    }
    
    // Fetch NFT metadata
    auto metadata = fetch_nft_metadata(token_id, collection_address, "opensea");
    if (!metadata) {
        RarityResult result;
        result.token_id = token_id;
        result.collection_address = collection_address;
        result.rarity_score = 0;
        return result;
    }
    
    return calculator_->calculate_rarity(token_id, collection_address, metadata->traits);
}

CollectionRarityStats NFTRarityService::get_collection_stats(
    const std::string& collection_address
) {
    auto stats = calculator_->get_collection_stats(collection_address);
    if (stats) {
        return *stats;
    }
    
    CollectionRarityStats empty_stats;
    empty_stats.collection_address = collection_address;
    return empty_stats;
}

std::vector<RarityResult> NFTRarityService::get_batch_rarity(
    const std::vector<std::string>& token_ids,
    const std::string& collection_address,
    const std::string& chain
) {
    std::vector<std::pair<std::string, std::vector<Trait>>> nfts;
    
    for (const auto& token_id : token_ids) {
        auto metadata = fetch_nft_metadata(token_id, collection_address, "opensea");
        if (metadata) {
            nfts.push_back({token_id, metadata->traits});
        }
    }
    
    return calculator_->calculate_batch_rarity(collection_address, nfts);
}

std::vector<RarityResult> NFTRarityService::get_top_rare(
    const std::string& collection_address,
    int limit
) {
    std::lock_guard lock(nfts_mutex_);
    
    auto it = collection_nfts_.find(collection_address);
    if (it == collection_nfts_.end()) {
        return {};
    }
    
    const auto& nfts = it->second;
    std::vector<std::pair<std::string, std::vector<Trait>>> nft_data;
    
    for (const auto& nft : nfts) {
        nft_data.push_back({nft.token_id, nft.traits});
    }
    
    auto results = calculator_->calculate_batch_rarity(collection_address, nft_data);
    
    return std::vector<RarityResult>(results.begin(), 
                                    results.begin() + std::min(limit, (int)results.size()));
}

std::vector<RarityResult> NFTRarityService::search_by_trait(
    const std::string& collection_address,
    const std::string& trait_type,
    const TraitValue& trait_value,
    int limit
) {
    std::lock_guard lock(nfts_mutex_);
    
    auto it = collection_nfts_.find(collection_address);
    if (it == collection_nfts_.end()) {
        return {};
    }
    
    std::vector<RarityResult> results;
    
    for (const auto& nft : it->second) {
        for (const auto& trait : nft.traits) {
            if (trait.trait_type == trait_type && trait.value == trait_value) {
                auto result = calculator_->calculate_rarity(
                    nft.token_id, 
                    collection_address, 
                    nft.traits
                );
                results.push_back(result);
                break;
            }
        }
    }
    
    // Sort by rarity
    std::sort(results.begin(), results.end(),
             [](const RarityResult& a, const RarityResult& b) {
                 return a.rarity_score > b.rarity_score;
             });
    
    return std::vector<RarityResult>(results.begin(),
                                    results.begin() + std::min(limit, (int)results.size()));
}

void NFTRarityService::calculate_full_collection(
    const std::string& collection_address,
    const std::string& chain
) {
    auto nfts = fetch_collection_metadata(collection_address, 1000);
    
    if (!nfts.empty()) {
        {
            std::lock_guard lock(nfts_mutex_);
            collection_nfts_[collection_address] = nfts;
        }
        
        calculator_->update_collection_stats(collection_address, nfts);
    }
}

std::optional<NFTMetadata> NFTRarityService::fetch_nft_metadata(
    const std::string& token_id,
    const std::string& collection_address,
    const std::string& source
) {
    NFTMetadata metadata;
    metadata.token_id = token_id;
    metadata.collection_address = collection_address;
    
    std::string url;
    
    if (source == "opensea") {
        url = "https://api.opensea.io/api/v2/tokens/" + collection_address +
              "/" + token_id + "?format=json";
    } else {
        return std::nullopt;
    }
    
    auto headers = std::map<std::string, std::string>{
        {"Accept", "application/json"}
    };
    
    std::string response = metadata_fetcher_->get(url, headers);
    
    if (response.empty()) {
        return std::nullopt;
    }
    
    json_error_t error;
    json_t* root = json_loads(response.c_str(), 0, &error);
    if (!root) {
        return std::nullopt;
    }
    
    // Parse name
    json_t* name = json_object_get(root, "name");
    if (name) {
        metadata.name = json_string_value(name);
    }
    
    // Parse image
    json_t* image_url = json_object_get(root, "image_url");
    if (image_url) {
        metadata.image_url = json_string_value(image_url);
    }
    
    // Parse description
    json_t* description = json_object_get(root, "description");
    if (description) {
        metadata.description = json_string_value(description);
    }
    
    // Parse traits
    json_t* traits_obj = json_object_get(root, "traits");
    if (traits_obj) {
        metadata.traits = trait_parser_->parse_traits_from_json(traits_obj);
    }
    
    metadata.last_updated = std::chrono::system_clock::now();
    
    json_decref(root);
    return metadata;
}

std::vector<NFTMetadata> NFTRarityService::fetch_collection_metadata(
    const std::string& collection_address,
    int limit
) {
    std::vector<NFTMetadata> nfts;
    
    // This would fetch all NFTs in a collection - placeholder
    // In production, this would iterate through all token IDs or use pagination
    
    return nfts;
}

void NFTRarityService::update_loop() {
    while (running_) {
        std::this_thread::sleep_for(std::chrono::hours(1));
        
        // Periodically update collection statistics
        std::lock_guard lock(nfts_mutex_);
        for (const auto& [address, _] : collection_nfts_) {
            auto nfts = fetch_collection_metadata(address, 1000);
            if (!nfts.empty()) {
                calculator_->update_collection_stats(address, nfts);
            }
        }
    }
}

} // namespace tigerscan
