cmake_minimum_required(VERSION 3.15)
project(TigerScanHolderGraph)

set(CMAKE_CXX_STANDARD 20)
set(CMAKE_CXX_STANDARD_REQUIRED ON)
set(CMAKE_CXX_FLAGS "${CMAKE_CXX_FLAGS} -O3 -march=native -flto")

find_package(PkgConfig REQUIRED)
pkg_check_modules(CURL REQUIRED libcurl)
pkg_check_modules(JSON REQUIRED jansson)

include_directories(${CMAKE_SOURCE_DIR}/src)
include_directories(${CURL_INCLUDE_DIRS})
include_directories(${JSON_INCLUDE_DIRS})

set(SOURCES
    src/holder_graph.cpp
)

add_library(tigerscan_holder_graph STATIC ${SOURCES})

target_link_libraries(tigerscan_holder_graph
    ${CURL_LIBRARIES}
    ${JSON_LIBRARIES}
    pthread
    dl
)

install(TARGETS tigerscan_holder_graph LIBRARY DESTINATION lib)
install(FILES src/holder_graph.hpp DESTINATION include)
