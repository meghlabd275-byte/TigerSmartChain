cmake_minimum_required(VERSION 3.15)
project(TigerScanNFTFloor)

set(CMAKE_CXX_STANDARD 20)
set(CMAKE_CXX_STANDARD_REQUIRED ON)
set(CMAKE_CXX_FLAGS "${CMAKE_CXX_FLAGS} -O3 -march=native -flto")

# Find required packages
find_package(PkgConfig REQUIRED)
pkg_check_modules(CURL REQUIRED libcurl)
pkg_check_modules(JSON REQUIRED jansson)
pkg_check_modules(OPENSSL REQUIRED openssl)

# Include directories
include_directories(${CMAKE_SOURCE_DIR}/src)
include_directories(${CURL_INCLUDE_DIRS})
include_directories(${JSON_INCLUDE_DIRS})
include_directories(${OPENSSL_INCLUDE_DIRS})

# Source files
set(SOURCES
    src/floor_calculator.cpp
)

# Create library
add_library(tigerscan_nft_floor STATIC ${SOURCES})

# Link libraries
target_link_libraries(tigerscan_nft_floor
    ${CURL_LIBRARIES}
    ${JSON_LIBRARIES}
    ${OPENSSL_LIBRARIES}
    pthread
    dl
)

# Installation
install(TARGETS tigerscan_nft_floor
    LIBRARY DESTINATION lib
    ARCHIVE DESTINATION lib
)

install(FILES src/floor_calculator.hpp
    DESTINATION include
)
