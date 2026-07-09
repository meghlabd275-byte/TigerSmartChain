#pragma once

#include <string>
#include <vector>
#include <unordered_map>
#include <cstdint>
#include <optional>
#include <memory>

namespace tigersmartchain {

// Forward declarations
struct Block;
struct Transaction;
struct Token;
struct TokenTransfer;
struct InternalTransaction;
struct Log;
struct Trace;

// Block data structure
struct Block {
    uint64_t number;
    std::string hash;
    std::string parent_hash;
    uint64_t timestamp;
    std::vector<std::string> transactions;
    uint64_t gas_used;
    uint64_t gas_limit;
    std::string miner;
    std::string difficulty;
    uint64_t size;
    std::string nonce;
    std::string extra_data;
    std::optional<uint64_t> base_fee_per_gas;
    uint32_t transactions_count;
    uint32_t uncles_count;
};

// Transaction data structure
struct Transaction {
    std::string hash;
    uint64_t block_number;
    std::string block_hash;
    uint64_t timestamp;
    std::string from;
    std::optional<std::string> to;
    std::string value;
    std::string gas_price;
    std::string gas_used;
    uint64_t gas_limit;
    uint64_t nonce;
    uint32_t transaction_index;
    std::string input;
    enum class Status { Success, Failure, Pending } status;
    std::vector<Log> logs;
    std::vector<TokenTransfer> token_transfers;
};

// Log event structure
struct Log {
    std::string address;
    std::vector<std::string> topics;
    std::string data;
    uint64_t block_number;
    std::string transaction_hash;
    uint32_t log_index;
};

// Token transfer event
struct TokenTransfer {
    std::string transaction_hash;
    std::string token_address;
    std::string from;
    std::string to;
    std::string value;
    uint64_t timestamp;
    uint64_t block_number;
    uint32_t log_index;
};

// Internal transaction (trace)
struct InternalTransaction {
    std::string transaction_hash;
    uint64_t block_number;
    std::string from;
    std::string to;
    std::string value;
    std::string call_type;
    std::string gas;
    std::string input;
    std::string output;
    std::optional<std::string> error;
    uint32_t depth;
};

// Token data structure
struct Token {
    std::string address;
    std::string name;
    std::string symbol;
    uint8_t decimals;
    std::string total_supply;
    enum class Type { BEP20, BEP721, BEP1155 } type;
    std::optional<double> price;
    std::optional<double> price_change_24h;
    std::optional<double> market_cap;
    std::optional<double> volume_24h;
    uint64_t holders_count;
    uint64_t transfers_count;
    bool is_verified;
    bool is_spam;
    std::optional<std::string> logo_url;
};

// Token holder
struct TokenHolder {
    std::string address;
    std::string balance;
    double percentage;
};

// RPC Request/Response types
struct RPCRequest {
    std::string jsonrpc = "2.0";
    std::string method;
    std::vector<std::string> params;
    uint64_t id = 1;
};

struct RPCResponse {
    std::string jsonrpc = "2.0";
    std::optional<std::string> result;
    std::optional<std::string> error;
    uint64_t id = 1;
};

// Cache entry with TTL
template<typename T>
struct CacheEntry {
    T data;
    uint64_t timestamp;
    uint64_t ttl;
    
    bool is_expired() const {
        auto now = std::chrono::duration_cast<std::chrono::seconds>(
            std::chrono::system_clock::now().time_since_epoch()
        ).count();
        return (now - timestamp) > ttl;
    }
};

} // namespace tigersmartchain
