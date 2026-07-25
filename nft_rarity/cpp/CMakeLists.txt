cmake_minimum_required(VERSION 3.15)
project(tiger_nft_rarity VERSION 1.0.0 LANGUAGES CXX)

set(CMAKE_CXX_STANDARD 20)
set(CMAKE_CXX_STANDARD_REQUIRED ON)
set(CMAKE_CXX_EXTENSIONS OFF)

# Optimization flags
set(CMAKE_CXX_FLAGS_RELEASE "${CMAKE_CXX_FLAGS_RELEASE} -O3 -march=native -mtune=native -ffast-math")
set(CMAKE_CXX_FLAGS_DEBUG "${CMAKE_CXX_FLAGS_DEBUG} -g -O0 -Wall -Wextra")

find_package(Threads REQUIRED)

include_directories(include)

set(SOURCE_FILES
    src/nft_rarity.cpp
)

add_library(tiger_nft_rarity STATIC ${SOURCE_FILES})

target_link_libraries(tiger_nft_rarity PUBLIC Threads::Threads)

# Enable LTO
if(CMAKE_BUILD_TYPE STREQUAL "Release")
    include(CheckIPOSupported)
    check_ipo_supported(RESULT ipo_supported)
    if(ipo_supported)
        set_target_properties(tiger_nft_rarity PROPERTIES
            INTERPROCEDURAL_OPTIMIZATION TRUE
        )
    endif()
endif()

install(TARGETS tiger_nft_rarity ARCHIVE DESTINATION lib LIBRARY DESTINATION lib)
install(DIRECTORY include/ DESTINATION include/tiger_nft_rarity)
