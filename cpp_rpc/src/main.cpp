#include "rpc_client.hpp"
#include <iostream>

int main() {
    tigersmartchain::RPCClient::Config config;
    config.rpc_url = "https://bsc-dataseed1.binance.org:443";
    
    auto client = std::make_unique<tigersmartchain::RPCClient>(config);
    
    auto block = client->get_latest_block();
    if (block) {
        std::cout << "Latest block: " << block->number << std::endl;
    }
    
    return 0;
}
