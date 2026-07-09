cmake_minimum_required(VERSION 3.16)
project(TigerSmartChainRPC)

set(CMAKE_CXX_STANDARD 20)
set(CMAKE_CXX_STANDARD_REQUIRED ON)

# Find required packages
find_package(Threads REQUIRED)
find_package(OpenSSL REQUIRED)

# Include directories
include_directories(include)

# Source files
set(SOURCES
    src/main.cpp
    src/rpc_client.cpp
    src/blockchain_parser.cpp
    src/cache.cpp
    src/connection_pool.cpp
)

# Headers
set(HEADERS
    include/rpc_client.hpp
    include/blockchain_parser.hpp
    include/cache.hpp
    include/connection_pool.hpp
    include/types.hpp
)

# Create library
add_library(tiger_rpc STATIC ${SOURCES} ${HEADERS})

# Create executable
add_executable(rpc_server src/server.cpp)

target_link_libraries(rpc_server tiger_rpc Threads::Threads OpenSSL::SSL OpenSSL::Crypto)
