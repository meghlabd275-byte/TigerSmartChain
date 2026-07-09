#include "connection_pool.hpp"
#include <iostream>
#include <sstream>
#include <thread>

namespace tigersmartchain {

ConnectionPool::ConnectionPool(size_t pool_size, const std::string& base_url) 
    : base_url_(base_url), timeout_ms_(5000) {
    
    for (size_t i = 0; i < pool_size; ++i) {
        auto conn = std::make_unique<CURLConnection>();
        conn->curl = curl_easy_init();
        conn->in_use = false;
        conn->last_used = std::chrono::steady_clock::now();
        connections_.push_back(std::move(conn));
    }
}

ConnectionPool::~ConnectionPool() {
    close_all();
}

void ConnectionPool::set_timeout(uint32_t timeout_ms) {
    timeout_ms_ = timeout_ms;
}

void ConnectionPool::close_all() {
    std::lock_guard<std::mutex> lock(mutex_);
    for (auto& conn : connections_) {
        if (conn->curl) {
            curl_easy_cleanup(conn->curl);
            conn->curl = nullptr;
        }
    }
    connections_.clear();
}

ConnectionPool::CURLConnection* ConnectionPool::get_connection() {
    std::lock_guard<std::mutex> lock(mutex_);
    
    // Find available connection
    for (auto& conn : connections_) {
        if (!conn->in_use) {
            conn->in_use = true;
            conn->last_used = std::chrono::steady_clock::now();
            return conn.get();
        }
    }
    
    // Create new connection if needed
    auto conn = std::make_unique<CURLConnection>();
    conn->curl = curl_easy_init();
    conn->in_use = true;
    conn->last_used = std::chrono::steady_clock::now();
    
    CURLConnection* raw_ptr = conn.get();
    connections_.push_back(std::move(conn));
    return raw_ptr;
}

void ConnectionPool::release_connection(CURLConnection* conn) {
    std::lock_guard<std::mutex> lock(mutex_);
    if (conn) {
        conn->in_use = false;
    }
}

size_t ConnectionPool::write_callback(void* contents, size_t size, size_t nmemb, void* userp) {
    size_t realsize = size * nmemb;
    std::string* str = static_cast<std::string*>(userp);
    str->append(static_cast<char*>(contents), realsize);
    return realsize;
}

std::string ConnectionPool::post(const std::string& json_data) {
    auto* conn = get_connection();
    
    std::string response;
    
    curl_easy_setopt(conn->curl, CURLOPT_URL, base_url_.c_str());
    curl_easy_setopt(conn->curl, CURLOPT_POSTFIELDS, json_data.c_str());
    curl_easy_setopt(conn->curl, CURLOPT_WRITEFUNCTION, write_callback);
    curl_easy_setopt(conn->curl, CURLOPT_WRITEDATA, &response);
    curl_easy_setopt(conn->curl, CURLOPT_TIMEOUT_MS, timeout_ms_);
    curl_easy_setopt(conn->curl, CURLOPT_CONNECTTIMEOUT_MS, timeout_ms_);
    curl_easy_setopt(conn->curl, CURLOPT_HTTPHEADER, "Content-Type: application/json");
    
    // Follow redirects
    curl_easy_setopt(conn->curl, CURLOPT_FOLLOWLOCATION, 1L);
    
    CURLcode res = curl_easy_perform(conn->curl);
    
    if (res != CURLE_OK) {
        std::cerr << "CURL error: " << curl_easy_strerror(res) << std::endl;
    }
    
    long http_code = 0;
    curl_easy_getinfo(conn->curl, CURLINFO_RESPONSE_CODE, &http_code);
    
    release_connection(conn);
    
    if (http_code != 200) {
        std::cerr << "HTTP error: " << http_code << std::endl;
    }
    
    return response;
}

std::string ConnectionPool::get(const std::string& endpoint) {
    auto* conn = get_connection();
    
    std::string response;
    std::string url = base_url_ + endpoint;
    
    curl_easy_setopt(conn->curl, CURLOPT_URL, url.c_str());
    curl_easy_setopt(conn->curl, CURLOPT_HTTPGET, 1);
    curl_easy_setopt(conn->curl, CURLOPT_WRITEFUNCTION, write_callback);
    curl_easy_setopt(conn->curl, CURLOPT_WRITEDATA, &response);
    curl_easy_setopt(conn->curl, CURLOPT_TIMEOUT_MS, timeout_ms_);
    curl_easy_setopt(conn->curl, CURLOPT_CONNECTTIMEOUT_MS, timeout_ms_);
    
    CURLcode res = curl_easy_perform(conn->curl);
    
    if (res != CURLE_OK) {
        std::cerr << "CURL error: " << curl_easy_strerror(res) << std::endl;
    }
    
    release_connection(conn);
    return response;
}

} // namespace tigersmartchain
