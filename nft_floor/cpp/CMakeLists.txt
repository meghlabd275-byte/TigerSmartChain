cmake_minimum_required(VERSION 3.15)
project(TigerScanNFTFloor)

set(CMAKE_CXX_STANDARD 20)
set(CMAKE_CXX_STANDARD_REQUIRED ON)
# Tuned for a generic x86-64 baseline rather than the build host, so a binary
# built here runs on any deployment target. `-march=native` would let the
# compiler emit instructions the target may not have, which fails at run time
# with an illegal instruction rather than at build time.
set(CMAKE_CXX_FLAGS_RELEASE "-O3 -flto -DNDEBUG")

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

# Both translation units are part of the library. The rarity calculator holds
# the on-chain enumeration the floor service relies on, so leaving it out would
# link a library whose metadata walk is missing.
set(SOURCES
    src/floor_calculator.cpp
    src/rarity_calculator.cpp
)

# Create library
add_library(tigerscan_nft_floor STATIC ${SOURCES})

# Link libraries
find_package(Threads REQUIRED)

target_link_libraries(tigerscan_nft_floor
    ${CURL_LIBRARIES}
    ${JSON_LIBRARIES}
    ${OPENSSL_LIBRARIES}
    Threads::Threads
    ${CMAKE_DL_LIBS}
)

# Installation
install(TARGETS tigerscan_nft_floor
    LIBRARY DESTINATION lib
    ARCHIVE DESTINATION lib
)

install(FILES src/floor_calculator.hpp
              src/rarity_calculator.hpp
    DESTINATION include
)
