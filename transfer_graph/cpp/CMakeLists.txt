cmake_minimum_required(VERSION 3.15)
project(TigerScanTransferGraph)

set(CMAKE_CXX_STANDARD 20)
set(CMAKE_CXX_STANDARD_REQUIRED ON)
set(CMAKE_CXX_FLAGS "${CMAKE_CXX_FLAGS} -O3 -march=native -flto")

find_package(PkgConfig REQUIRED)
pkg_check_modules(CURL REQUIRED libcurl)

include_directories(${CMAKE_SOURCE_DIR}/src)
include_directories(${CURL_INCLUDE_DIRS})

set(SOURCES
    src/transfer_graph.cpp
)

add_library(tigerscan_transfer_graph STATIC ${SOURCES})

target_link_libraries(tigerscan_transfer_graph
    ${CURL_LIBRARIES}
    pthread
    dl
)

install(TARGETS tigerscan_transfer_graph LIBRARY DESTINATION lib)
install(FILES src/transfer_graph.hpp DESTINATION include)
