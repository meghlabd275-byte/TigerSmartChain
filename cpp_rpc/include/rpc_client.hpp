#pragma once

#include "types.hpp"
#include <string>
#include <vector>
#include <memory>
#include <functional>
#include <atomic>
#include <mutex>

namespace tigersmartchain {

/**
 * High-performance RPC client for blockchain data retrieval
 * Optimized for ultra-low latency with connection pooling and caching
 */
class RPCClient {
public:
    struct Config {
        std::string rpc_url;
        std::string ws_url;
        std::string archive_url;  // For historical data
        uint32_t timeout_ms = 5000;
        uint32_t max_retries = 3;
        uint32_t pool_size = 10;
        bool enable_cache = true;
        uint64_t cache_ttl_seconds = 30;
    };

    explicit RPCClient(const Config& config);
    ~RPCClient();

    // Block operations
    std::optional<Block> get_block(uint64_t block_number, bool include_full_transactions = false);
    std::optional<Block> get_block_by_hash(const std::string& hash, bool include_full_transactions = false);
    std::optional<Block> get_latest_block();
    std::vector<Block> get_blocks(uint64_t start_block, uint64_t end_block);

    // Transaction operations
    std::optional<Transaction> get_transaction(const std::string& hash);
    std::vector<Transaction> get_transactions_by_block(uint64_t block_number);
    std::vector<Transaction> get_transactions_by_address(const std::string& address, uint64_t start_block = 0);
    
    // Internal transactions / traces
    std::vector<InternalTransaction> get_internal_transactions(const std::string& tx_hash);
    std::vector<Trace> get_trace(const std::string& tx_hash);
    
    // State and storage
    std::optional<std::string> get_storage_at(const std::string& address, uint64_t block_number);
    std::optional<std::string> get_code(const std::string& address);
    std::optional<std::string> get_balance(const std::string& address, uint64_t block_number = 0);
    
    // Token operations
    std::optional<Token> get_token(const std::string& address);
    std::vector<TokenHolder> get_token_holders(const std::string& address, uint32_t limit = 100);
    std::vector<TokenTransfer> get_token_transfers(const std::string& address, uint64_t from_block = 0, uint64_t to_block = 0);
    
    // Event logs
    std::vector<Log> get_logs(const std::string& address, uint64_t from_block, uint64_t to_block);
    std::vector<Log> get_logs_with_topics(
        const std::string& address,
        const std::vector<std::string>& topics,
        uint64_t from_block,
        uint64_t to_block
    );

    // Network status
    uint64_t get_block_number();
    std::string get_chain_id();

private:
    Config config_;
    std::unique_ptr<class ConnectionPool> pool_;
    std::unique_ptr<class Cache> cache_;
    
    RPCResponse send_request(const std::string& method, const std::vector<std::string>& params);
    RPCResponse send_request_with_retry(const std::string& method, const std::vector<std::string>& params);
    std::string make_jsonrpc_request(const std::string& method, const std::vector<std::string>& params);
    void parse_block_response(const std::string& json, Block& block);
    void parse_transaction_response(const std::string& json, Transaction& tx);
};

/**
 * Batch RPC client for parallel requests
 */
class BatchRPCClient {
public:
    explicit BatchRPCClient(RPCClient& client);
    
    std::vector<std::optional<Block>> get_blocks_batch(const std::vector<uint64_t>& block_numbers);
    std::vector<std::optional<Transaction>> get_transactions_batch(const std::vector<std::string>& hashes);
    
private:
    RPCClient& client_;
    std::mutex mutex_;
};

/**
 * WebSocket subscription client for real-time data
 */
class WSSubscriptionClient {
public:
    using Callback = std::function<void(const std::string&)>;
    
    WSSubscriptionClient(const std::string& ws_url);
    ~WSSubscriptionClient();
    
    void connect();
    void disconnect();
    
    uint32_t subscribe_new_heads(Callback callback);
    uint32_t subscribe_pending_transactions(Callback callback);
    uint32_t subscribe_logs(const std::string& address, Callback callback);
    
    void unsubscribe(uint32_t subscription_id);
    
    bool is_connected() const;

private:
    class Impl;
    std::unique_ptr<Impl> impl_;
};

} // namespace tigersmartchain
