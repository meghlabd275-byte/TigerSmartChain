#pragma once

#include "types.hpp"
#include <string>
#include <vector>
#include <functional>

namespace tigersmartchain {

/**
 * Parses blockchain data from various sources
 */
class BlockchainParser {
public:
    using LogCallback = std::function<void(const Log&)>;
    using TransferCallback = std::function<void(const TokenTransfer&)>;
    using TraceCallback = std::function<void(const InternalTransaction&)>;
    
    BlockchainParser() = default;
    
    // Parse block from JSON
    static Block parse_block(const std::string& json);
    
    // Parse transaction from JSON
    static Transaction parse_transaction(const std::string& json);
    
    // Parse logs from transaction receipt
    static std::vector<Log> parse_logs(const std::string& json);
    
    // Parse internal transactions (traces)
    static std::vector<InternalTransaction> parse_traces(const std::string& json);
    
    // Parse token transfers from logs
    static std::vector<TokenTransfer> parse_token_transfers(const std::string& json, 
                                                             const std::string& token_address);
    
    // Decode contract call data
    static std::string decode_function_selector(const std::string& data);
    
    // Parse ERC20 transfer event
    static bool is_erc20_transfer(const std::vector<std::string>& topics);
    static TokenTransfer decode_erc20_transfer(const Log& log, const std::string& token_address);
    
    // Parse ERC721 transfer event
    static bool is_erc721_transfer(const std::vector<std::string>& topics);
    static TokenTransfer decode_erc721_transfer(const Log& log, const std::string& token_address);
    
    // ABI decoding
    static std::string decode_abi_string(const std::string& data);
    static std::vector<std::string> decode_abi_addresses(const std::string& data);
    
    // Address validation
    static bool is_valid_address(const std::string& address);
    static bool is_contract_address(const std::string& address);
    
    // Hash validation
    static bool is_valid_hash(const std::string& hash);
    static std::string normalize_hash(const std::string& hash);
    
private:
    // Keccak256 for event signatures
    static std::string keccak256(const std::string& input);
    
    // ERC20 Transfer event signature
    static const std::string ERC20_TRANSFER_SIG;
    // ERC721 Transfer event signature  
    static const std::string ERC721_TRANSFER_SIG;
    // ERC1155 Transfer event signature
    static const std::string ERC1155_TRANSFER_SIG;
};

/**
 * Batch parser for processing multiple blocks
 */
class BatchParser {
public:
    BatchParser();
    
    void parse_blocks(const std::vector<std::string>& jsons,
                      std::vector<Block>& blocks);
    
    void parse_transactions(const std::vector<std::string>& jsons,
                           std::vector<Transaction>& transactions);
    
    void process_logs(const std::vector<Log>& logs,
                     BlockchainParser::TransferCallback callback);
    
private:
    size_t batch_size_;
};

} // namespace tigersmartchain
