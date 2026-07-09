#pragma once

#include <string>
#include <memory>
#include <vector>
#include <mutex>
#include <curl/curl.h>

namespace tigersmartchain {

/**
 * Connection pool for HTTP/WebSocket connections
 * Reuses connections to reduce latency
 */
class ConnectionPool {
public:
    ConnectionPool(size_t pool_size, const std::string& base_url);
    ~ConnectionPool();
    
    // HTTP operations
    std::string post(const std::string& json_data);
    std::string get(const std::string& endpoint);
    
    // Connection management
    void set_timeout(uint32_t timeout_ms);
    void close_all();
    
private:
    struct CURLConnection {
        CURL* curl;
        bool in_use;
        std::chrono::steady_clock::time_point last_used;
    };
    
    std::vector<std::unique_ptr<CURLConnection>> connections_;
    std::string base_url_;
    uint32_t timeout_ms_;
    mutable std::mutex mutex_;
    
    CURLConnection* get_connection();
    void release_connection(CURLConnection* conn);
    
    static size_t write_callback(void* contents, size_t size, size_t nmemb, void* userp);
};

} // namespace tigersmartchain
