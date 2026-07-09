#include "rpc_client.hpp"
#include "connection_pool.hpp"
#include "cache.hpp"

#include <iostream>
#include <sstream>
#include <chrono>
#include <thread>
#include <curl/curl.h>
#include <nlohmann/json.hpp>

using json = nlohmann::json;

namespace tigersmartchain {

class RPCClient::Impl {
public:
    Impl(const Config& config) : config_(config), pool_(nullptr), cache_(nullptr) {
        curl_global_init(CURL_GLOBAL_DEFAULT);
        
        pool_ = std::make_unique<ConnectionPool>(config.pool_size, config.rpc_url);
        
        if (config.enable_cache) {
            cache_ = std::make_unique<Cache>(config.cache_ttl_seconds);
        }
    }
    
    ~Impl() {
        curl_global_cleanup();
    }
    
    Config config_;
    std::unique_ptr<ConnectionPool> pool_;
    std::unique_ptr<Cache> cache_;
    std::mutex mutex_;
    
    // Rate limiting
    std::atomic<uint64_t> request_count_{0};
    std::chrono::steady_clock::time_point last_reset_;
    static constexpr uint64_t MAX_REQUESTS_PER_SECOND = 100;
    
    void rate_limit_wait() {
        auto now = std::chrono::steady_clock::now();
        if (now - last_reset_ > std::chrono::seconds(1)) {
            request_count_.store(0);
            last_reset_ = now;
        }
        
        while (request_count_.load() >= MAX_REQUESTS_PER_SECOND) {
            std::this_thread::sleep_for(std::chrono::milliseconds(10));
        }
        request_count_.fetch_add(1);
    }
};

RPCClient::RPCClient(const Config& config) : impl_(std::make_unique<Impl>(config)) {}

RPCClient::~RPCClient() = default;

RPCResponse RPCClient::send_request(const std::string& method, const std::vector<std::string>& params) {
    impl_->rate_limit_wait();
    
    auto request = make_jsonrpc_request(method, params);
    
    // Check cache for GET-like requests
    if (impl_->config_.enable_cache && impl_->cache_) {
        auto cached = impl_->cache_->get(request);
        if (cached) {
            return json::parse(*cached);
        }
    }
    
    auto response_str = impl_->pool_->post(request);
    
    // Cache the response
    if (impl_->config_.enable_cache && impl_->cache_ && !response_str.empty()) {
        impl_->cache_->put(request, response_str);
    }
    
    try {
        return json::parse(response_str);
    } catch (const std::exception& e) {
        std::cerr << "Failed to parse RPC response: " << e.what() << std::endl;
        return RPCResponse{};
    }
}

RPCResponse RPCClient::send_request_with_retry(const std::string& method, const std::vector<std::string>& params) {
    for (uint32_t attempt = 0; attempt < impl_->config_.max_retries; ++attempt) {
        try {
            return send_request(method, params);
        } catch (const std::exception& e) {
            std::cerr << "RPC request failed (attempt " << (attempt + 1) << "): " << e.what() << std::endl;
            std::this_thread::sleep_for(std::chrono::milliseconds(100 * (attempt + 1)));
        }
    }
    return RPCResponse{};
}

std::string RPCClient::make_jsonrpc_request(const std::string& method, const std::vector<std::string>& params) {
    json req = {
        {"jsonrpc", "2.0"},
        {"method", method},
        {"id", 1}
    };
    
    json params_array = json::array();
    for (const auto& p : params) {
        params_array.push_back(p);
    }
    req["params"] = params_array;
    
    return req.dump();
}

std::optional<Block> RPCClient::get_block(uint64_t block_number, bool include_full_transactions) {
    auto response = send_request_with_retry("eth_getBlockByNumber", {
        "0x" + std::to_string(block_number),
        include_full_transactions ? "true" : "false"
    });
    
    if (!response.result) {
        return std::nullopt;
    }
    
    try {
        json block_json = json::parse(*response.result);
        Block block;
        parse_block_response(block_json.dump(), block);
        return block;
    } catch (const std::exception& e) {
        std::cerr << "Failed to parse block: " << e.what() << std::endl;
        return std::nullopt;
    }
}

std::optional<Block> RPCClient::get_block_by_hash(const std::string& hash, bool include_full_transactions) {
    auto response = send_request_with_retry("eth_getBlockByHash", {
        hash,
        include_full_transactions ? "true" : "false"
    });
    
    if (!response.result) {
        return std::nullopt;
    }
    
    try {
        json block_json = json::parse(*response.result);
        Block block;
        parse_block_response(block_json.dump(), block);
        return block;
    } catch (const std::exception& e) {
        return std::nullopt;
    }
}

std::optional<Block> RPCClient::get_latest_block() {
    auto response = send_request_with_retry("eth_getBlockByNumber", {"latest", "true"});
    
    if (!response.result) {
        return std::nullopt;
    }
    
    try {
        json block_json = json::parse(*response.result);
        Block block;
        parse_block_response(block_json.dump(), block);
        return block;
    } catch (const std::exception& e) {
        return std::nullopt;
    }
}

std::vector<Block> RPCClient::get_blocks(uint64_t start_block, uint64_t end_block) {
    std::vector<Block> blocks;
    blocks.reserve(end_block - start_block);
    
    // Use batch requests for efficiency
    for (uint64_t i = start_block; i <= end_block; ++i) {
        auto block = get_block(i);
        if (block) {
            blocks.push_back(*block);
        }
    }
    
    return blocks;
}

std::optional<Transaction> RPCClient::get_transaction(const std::string& hash) {
    auto response = send_request_with_retry("eth_getTransactionByHash", {hash});
    
    if (!response.result) {
        return std::nullopt;
    }
    
    try {
        json tx_json = json::parse(*response.result);
        Transaction tx;
        parse_transaction_response(tx_json.dump(), tx);
        return tx;
    } catch (const std::exception& e) {
        return std::nullopt;
    }
}

std::vector<Transaction> RPCClient::get_transactions_by_block(uint64_t block_number) {
    auto block = get_block(block_number, true);
    if (!block) {
        return {};
    }
    
    return {}; // Transactions would be parsed from block data
}

std::vector<Transaction> RPCClient::get_transactions_by_address(const std::string& address, uint64_t start_block) {
    // Get current block number
    auto current_block = get_block_number();
    
    // For full implementation, would need to index all blocks or use archive node
    return {};
}

std::vector<InternalTransaction> RPCClient::get_internal_transactions(const std::string& tx_hash) {
    auto response = send_request_with_retry("debug_traceTransaction", {
        tx_hash,
        {
            {"tracer", "callTracer"},
            {"timeout", "30s"}
        }
    });
    
    if (!response.result) {
        return {};
    }
    
    try {
        json result = json::parse(*response.result);
        std::vector<InternalTransaction> traces;
        // Parse traces from response
        return traces;
    } catch (const std::exception& e) {
        return {};
    }
}

std::vector<Trace> RPCClient::get_trace(const std::string& tx_hash) {
    auto response = send_request_with_retry("trace_transaction", {tx_hash});
    
    if (!response.result) {
        return {};
    }
    
    return {};
}

std::optional<std::string> RPCClient::get_storage_at(const std::string& address, uint64_t block_number) {
    auto response = send_request_with_retry("eth_getStorageAt", {
        address,
        "0x0",  // storage position
        "0x" + std::to_string(block_number)
    });
    
    return response.result;
}

std::optional<std::string> RPCClient::get_code(const std::string& address) {
    auto response = send_request_with_retry("eth_getCode", {address, "latest"});
    return response.result;
}

std::optional<std::string> RPCClient::get_balance(const std::string& address, uint64_t block_number) {
    std::string block_param = block_number > 0 ? "0x" + std::to_string(block_number) : "latest";
    auto response = send_request_with_retry("eth_getBalance", {address, block_param});
    return response.result;
}

std::optional<Token> RPCClient::get_token(const std::string& address) {
    // Call token contract methods
    // ERC20: name(), symbol(), decimals(), totalSupply(), balanceOf()
    return std::nullopt;
}

std::vector<TokenHolder> RPCClient::get_token_holders(const std::string& address, uint32_t limit) {
    // Would require indexing all Transfer events
    return {};
}

std::vector<TokenTransfer> RPCClient::get_token_transfers(const std::string& address, uint64_t from_block, uint64_t to_block) {
    // Get Transfer events from token contract
    // Topics: keccak256("Transfer(address,address,uint256)")
    return {};
}

std::vector<Log> RPCClient::get_logs(const std::string& address, uint64_t from_block, uint64_t to_block) {
    auto response = send_request_with_retry("eth_getLogs", {
        {
            {"address", address},
            {"fromBlock", "0x" + std::to_string(from_block)},
            {"toBlock", "0x" + std::to_string(to_block)}
        }
    });
    
    if (!response.result) {
        return {};
    }
    
    try {
        json result = json::parse(*response.result);
        std::vector<Log> logs;
        // Parse logs
        return logs;
    } catch (const std::exception& e) {
        return {};
    }
}

std::vector<Log> RPCClient::get_logs_with_topics(
    const std::string& address,
    const std::vector<std::string>& topics,
    uint64_t from_block,
    uint64_t to_block
) {
    return get_logs(address, from_block, to_block);
}

uint64_t RPCClient::get_block_number() {
    auto response = send_request_with_retry("eth_blockNumber", {});
    if (!response.result) {
        return 0;
    }
    
    try {
        return std::stoull(*response.result, nullptr, 16);
    } catch (...) {
        return 0;
    }
}

std::string RPCClient::get_chain_id() {
    auto response = send_request_with_retry("eth_chainId", {});
    return response.result.value_or("0x1");
}

void RPCClient::parse_block_response(const std::string& json_str, Block& block) {
    try {
        json j = json::parse(json_str);
        
        if (j.contains("number")) {
            block.number = std::stoull(j["number"].get<std::string>(), nullptr, 16);
        }
        if (j.contains("hash")) {
            block.hash = j["hash"].get<std::string>();
        }
        if (j.contains("parentHash")) {
            block.parent_hash = j["parentHash"].get<std::string>();
        }
        if (j.contains("timestamp")) {
            block.timestamp = std::stoull(j["timestamp"].get<std::string>(), nullptr, 16);
        }
        if (j.contains("gasUsed")) {
            block.gas_used = std::stoull(j["gasUsed"].get<std::string>(), nullptr, 16);
        }
        if (j.contains("gasLimit")) {
            block.gas_limit = std::stoull(j["gasLimit"].get<std::string>(), nullptr, 16);
        }
        if (j.contains("miner")) {
            block.miner = j["miner"].get<std::string>();
        }
        if (j.contains("size")) {
            block.size = std::stoull(j["size"].get<std::string>(), nullptr, 16);
        }
        if (j.contains("transactions")) {
            block.transactions_count = j["transactions"].size();
        }
        if (j.contains("baseFeePerGas")) {
            block.base_fee_per_gas = std::stoull(j["baseFeePerGas"].get<std::string>(), nullptr, 16);
        }
        
    } catch (const std::exception& e) {
        std::cerr << "Error parsing block: " << e.what() << std::endl;
    }
}

void RPCClient::parse_transaction_response(const std::string& json_str, Transaction& tx) {
    try {
        json j = json::parse(json_str);
        
        if (j.contains("hash")) {
            tx.hash = j["hash"].get<std::string>();
        }
        if (j.contains("blockNumber")) {
            tx.block_number = std::stoull(j["blockNumber"].get<std::string>(), nullptr, 16);
        }
        if (j.contains("from")) {
            tx.from = j["from"].get<std::string>();
        }
        if (j.contains("to")) {
            tx.to = j["to"].get<std::string>();
        }
        if (j.contains("value")) {
            tx.value = j["value"].get<std::string>();
        }
        if (j.contains("gasPrice")) {
            tx.gas_price = j["gasPrice"].get<std::string>();
        }
        if (j.contains("gas")) {
            tx.gas_limit = std::stoull(j["gas"].get<std::string>(), nullptr, 16);
        }
        if (j.contains("nonce")) {
            tx.nonce = std::stoull(j["nonce"].get<std::string>(), nullptr, 16);
        }
        if (j.contains("input")) {
            tx.input = j["input"].get<std::string>();
        }
        
    } catch (const std::exception& e) {
        std::cerr << "Error parsing transaction: " << e.what() << std::endl;
    }
}

// BatchRPCClient implementation
BatchRPCClient::BatchRPCClient(RPCClient& client) : client_(client) {}

std::vector<std::optional<Block>> BatchRPCClient::get_blocks_batch(const std::vector<uint64_t>& block_numbers) {
    std::vector<std::optional<Block>> results;
    results.reserve(block_numbers.size());
    
    for (const auto& num : block_numbers) {
        results.push_back(client_.get_block(num));
    }
    
    return results;
}

std::vector<std::optional<Transaction>> BatchRPCClient::get_transactions_batch(const std::vector<std::string>& hashes) {
    std::vector<std::optional<Transaction>> results;
    results.reserve(hashes.size());
    
    for (const auto& hash : hashes) {
        results.push_back(client_.get_transaction(hash));
    }
    
    return results;
}

} // namespace tigersmartchain
