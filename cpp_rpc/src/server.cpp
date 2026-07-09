#include "rpc_client.hpp"
#include "connection_pool.hpp"
#include "cache.hpp"
#include "blockchain_parser.hpp"

#include <iostream>
#include <memory>
#include <csignal>
#include <thread>
#include <chrono>
#include <httplib.h>

using namespace tigersmartchain;

// Global flag for graceful shutdown
std::atomic<bool> g_running{true};

void signal_handler(int signal) {
    std::cout << "\nShutting down..." << std::endl;
    g_running = false;
}

int main(int argc, char* argv[]) {
    // Setup signal handlers
    std::signal(SIGINT, signal_handler);
    std::signal(SIGTERM, signal_handler);
    
    // Configuration
    RPCClient::Config config;
    config.rpc_url = "https://bsc-dataseed1.binance.org:443";
    config.timeout_ms = 10000;
    config.max_retries = 3;
    config.pool_size = 20;
    config.enable_cache = true;
    config.cache_ttl_seconds = 30;
    
    // Create RPC client
    auto rpc_client = std::make_unique<RPCClient>(config);
    
    // Create HTTP server
    httplib::Server svr;
    
    // Health check
    svr.Get("/health", [](const httplib::Request& req, httplib::Response& res) {
        res.set_content("{\"status\":\"ok\"}", "application/json");
    });
    
    // Get latest block
    svr.Get("/api/v1/blocks/latest", [&rpc_client](const httplib::Request& req, httplib::Response& res) {
        auto block = rpc_client->get_latest_block();
        if (block) {
            res.set_content("{\"number\":" + std::to_string(block->number) + 
                          ",\"hash\":\"" + block->hash + "\"}", "application/json");
        } else {
            res.status = 500;
            res.set_content("{\"error\":\"Failed to fetch block\"}", "application/json");
        }
    });
    
    // Get block by number
    svr.Get(R"(/api/v1/blocks/(\d+))", [&rpc_client](const httplib::Request& req, httplib::Response& res) {
        uint64_t block_number = std::stoull(req.matches[1]);
        auto block = rpc_client->get_block(block_number);
        if (block) {
            res.set_content("{\"number\":" + std::to_string(block->number) + 
                          ",\"hash\":\"" + block->hash + "\",\"timestamp\":" + 
                          std::to_string(block->timestamp) + "}", "application/json");
        } else {
            res.status = 404;
            res.set_content("{\"error\":\"Block not found\"}", "application/json");
        }
    });
    
    // Get transaction
    svr.Get(R"(/api/v1/txs/([a-fA-F0-9]+))", [&rpc_client](const httplib::Request& req, httplib::Response& res) {
        std::string hash = "0x" + req.matches[1];
        auto tx = rpc_client->get_transaction(hash);
        if (tx) {
            res.set_content("{\"hash\":\"" + tx->hash + "\",\"from\":\"" + tx->from + "\",\"to\":\"" + 
                          (tx->to.value_or("")) + "\",\"value\":\"" + tx->value + "\"}", "application/json");
        } else {
            res.status = 404;
            res.set_content("{\"error\":\"Transaction not found\"}", "application/json");
        }
    });
    
    // Get internal transactions
    svr.Get(R"(/api/v1/txs/([a-fA-F0-9]+)/internal)", [&rpc_client](const httplib::Request& req, httplib::Response& res) {
        std::string hash = "0x" + req.matches[1];
        auto traces = rpc_client->get_internal_transactions(hash);
        res.set_content("{\"count\":" + std::to_string(traces.size()) + "}", "application/json");
    });
    
    // Get token balance
    svr.Get(R"(/api/v1/tokens/([a-fA-F0-9]+)/balance/([a-fA-F0-9]+))", 
            [&rpc_client](const httplib::Request& req, httplib::Response& res) {
        std::string token_address = "0x" + req.matches[1];
        std::string wallet_address = "0x" + req.matches[2];
        
        auto balance = rpc_client->get_balance(wallet_address);
        if (balance) {
            res.set_content("{\"address\":\"" + wallet_address + "\",\"balance\":\"" + *balance + "\"}", 
                          "application/json");
        } else {
            res.status = 500;
            res.set_content("{\"error\":\"Failed to get balance\"}", "application/json");
        }
    });
    
    // Get storage at
    svr.Get(R"(/api/v1/contracts/([a-fA-F0-9]+)/storage/(\d+))", 
            [&rpc_client](const httplib::Request& req, httplib::Response& res) {
        std::string address = "0x" + req.matches[1];
        uint64_t block_number = std::stoull(req.matches[2]);
        
        auto storage = rpc_client->get_storage_at(address, block_number);
        if (storage) {
            res.set_content("{\"address\":\"" + address + "\",\"storage\":\"" + *storage + "\"}", 
                          "application/json");
        } else {
            res.status = 500;
            res.set_content("{\"error\":\"Failed to get storage\"}", "application/json");
        }
    });
    
    // Get code
    svr.Get(R"(/api/v1/contracts/([a-fA-F0-9]+)/code)", [&rpc_client](const httplib::Request& req, httplib::Response& res) {
        std::string address = "0x" + req.matches[1];
        
        auto code = rpc_client->get_code(address);
        if (code) {
            res.set_content("{\"address\":\"" + address + "\",\"bytecode\":\"" + *code + "\"}", 
                          "application/json");
        } else {
            res.status = 500;
            res.set_content("{\"error\":\"Failed to get code\"}", "application/json");
        }
    });
    
    // Get block number
    svr.Get("/api/v1/block-number", [&rpc_client](const httplib::Request& req, httplib::Response& res) {
        auto block_number = rpc_client->get_block_number();
        res.set_content("{\"blockNumber\":" + std::to_string(block_number) + ",\"chainId\":\"" + 
                      rpc_client->get_chain_id() + "\"}", "application/json");
    });
    
    // Get logs
    svr.Get(R"(/api/v1/logs)", [&rpc_client](const httplib::Request& req, httplib::Response& res) {
        std::string address = req.has_param("address") ? req.get_param_value("address") : "";
        uint64_t from_block = req.has_param("fromBlock") ? std::stoull(req.get_param_value("fromBlock")) : 0;
        uint64_t to_block = req.has_param("toBlock") ? std::stoull(req.get_param_value("toBlock")) : 0;
        
        auto logs = rpc_client->get_logs(address, from_block, to_block);
        res.set_content("{\"count\":" + std::to_string(logs.size()) + "}", "application/json");
    });
    
    std::cout << "Starting TigerSmartChain RPC Server on http://0.0.0.0:8080" << std::endl;
    std::cout << "Press Ctrl+C to stop" << std::endl;
    
    // Start server in background thread
    std::thread server_thread([&svr]() {
        svr.listen("0.0.0.0", 8080);
    });
    
    // Main thread - periodic block updates
    while (g_running) {
        std::this_thread::sleep_for(std::chrono::seconds(10));
    }
    
    std::cout << "Stopping server..." << std::endl;
    svr.stop();
    
    if (server_thread.joinable()) {
        server_thread.join();
    }
    
    std::cout << "Server stopped." << std::endl;
    return 0;
}
