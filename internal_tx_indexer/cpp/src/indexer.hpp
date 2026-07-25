/**
 * TigerScan Internal Transaction Indexer
 * 
 * High-performance C++ service for indexing internal transactions
 * and generating call trees from EVM execution traces.
 * 
 * Features:
 * - Real-time internal transaction indexing
 * - Call tree reconstruction
 * - State diff tracking
 * - Gas analysis
 */

#ifndef TIGERSCAN_INTERNAL_TX_INDEXER_HPP
#define TIGERSCAN_INTERNAL_TX_INDEXER_HPP

#include <string>
#include <vector>
#include <map>
#include <unordered_map>
#include <memory>
#include <mutex>
#include <shared_mutex>
#include <atomic>
#include <chrono>
#include <functional>
#include <curl/curl.h>

namespace tigerscan {

// Internal transaction types
enum class CallType {
    Call,
    CallCode,
    DelegateCall,
    StaticCall,
    Create,
    Create2,
    SelfDestruct,
};

struct InternalTransaction {
    std::string transaction_hash;
    uint64_t block_number;
    uint64_t transaction_index;
    uint64_t depth;
    CallType call_type;
    std::string from_address;
    std::string to_address;
    std::string input_data;
    std::string output_data;
    uint64_t gas;
    uint64_t gas_used;
    std::string value;
    std::string status;
    std::string error;
    std::vector<InternalTransaction> subcalls;
};

struct CallTree {
    std::string root_transaction_hash;
    InternalTransaction root_call;
    uint64_t total_gas_used;
    uint64_t total_calls;
    uint64_t max_depth;
    double execution_time_ms;
};

struct StateDiff {
    std::string address;
    std::string key;
    std::string old_value;
    std::string new_value;
    std::string diff_type;
};

struct TransactionTrace {
    std::string transaction_hash;
    uint64_t block_number;
    std::vector<InternalTransaction> calls;
    std::vector<StateDiff> state_diffs;
    uint64_t gas_used;
    std::string return_value;
    std::string error_message;
};

struct IndexerConfig {
    std::string ethereum_rpc;
    std::string redis_url;
    std::string postgres_url;
    int max_concurrent_traces;
    int trace_batch_size;
    int reorg_confirmations;
    bool enable_state_diff;
    bool enable_call_tree;
};

struct IndexerStats {
    std::atomic<uint64_t> total_indexed;
    std::atomic<uint64_t> total_calls;
    std::atomic<uint64_t> total_state_diffs;
    std::atomic<uint64_t> failed_traces;
    std::atomic<uint64_t> reorgs_handled;
    std::chrono::system_clock::time_point last_indexed_block;
    std::atomic<uint64_t> indexing_speed_tps;
};

class RPCClient {
public:
    RPCClient(const std::string& rpc_url);
    ~RPCClient();
    
    std::string eth_call(const std::string& method, const std::string& params);
    std::string debug_traceTransaction(const std::string& tx_hash);
    std::string trace_transaction(const std::string& tx_hash);
    
private:
    std::string rpc_url_;
    CURL* curl_;
    
    std::string post(const std::string& body);
};

class CallTreeBuilder {
public:
    CallTreeBuilder();
    ~CallTreeBuilder();
    
    CallTree build(const std::string& tx_hash, const std::vector<InternalTransaction>& calls);
    InternalTransaction parse_trace_call(void* call_obj);
    void calculate_gas_statistics(CallTree& tree);
    
private:
    InternalTransaction build_tree_recursive(
        void* call_obj,
        uint64_t depth,
        const std::string& tx_hash
    );
};

class StateDiffAnalyzer {
public:
    StateDiffAnalyzer();
    ~StateDiffAnalyzer();
    
    std::vector<StateDiff> analyze(
        const std::string& tx_hash,
        void* state_diffs
    );
    
    StateDiff parse_storage_diff(void* diff);
    std::pair<std::string, std::string> parse_balance_change(
        const std::string& address,
        void* diff
    );
};

class InternalTxIndexer {
public:
    InternalTxIndexer(const IndexerConfig& config);
    ~InternalTxIndexer();
    
    bool initialize();
    void start();
    void stop();
    
    bool index_transaction(const std::string& tx_hash);
    uint64_t index_block_range(uint64_t start_block, uint64_t end_block);
    
    std::optional<TransactionTrace> get_trace(const std::string& tx_hash);
    std::optional<CallTree> get_call_tree(const std::string& tx_hash);
    std::vector<StateDiff> get_state_diffs(const std::string& tx_hash);
    
    IndexerStats get_stats() const;
    
    using TraceCallback = std::function<void(const TransactionTrace&)>;
    void subscribe(TraceCallback callback);
    void handle_reorg(uint64_t block_number);

private:
    IndexerConfig config_;
    std::unique_ptr<RPCClient> rpc_client_;
    std::unique_ptr<CallTreeBuilder> tree_builder_;
    std::unique_ptr<StateDiffAnalyzer> diff_analyzer_;
    
    mutable std::shared_mutex cache_mutex_;
    std::unordered_map<std::string, TransactionTrace> trace_cache_;
    std::unordered_map<std::string, CallTree> tree_cache_;
    
    std::vector<TraceCallback> subscribers_;
    mutable std::mutex subscriber_mutex_;
    
    std::atomic<bool> is_running_;
    std::atomic<uint64_t> current_block_;
    std::atomic<uint64_t> indexed_block_;
    
    IndexerStats stats_;
    
    bool fetch_and_parse_trace(
        const std::string& tx_hash,
        TransactionTrace& trace
    );
    
    void notify_subscribers(const TransactionTrace& trace);
    void cache_trace(const TransactionTrace& trace);
    void update_stats(const TransactionTrace& trace);
};

} // namespace tigerscan

#endif // TIGERSCAN_INTERNAL_TX_INDEXER_HPP
