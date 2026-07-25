cmake_minimum_required(VERSION 3.15)
project(tiger_blob_processor VERSION 1.0.0 LANGUAGES CXX)

set(CMAKE_CXX_STANDARD 20)
set(CMAKE_CXX_STANDARD_REQUIRED ON)
set(CMAKE_CXX_EXTENSIONS OFF)

# Compiler flags for optimization
set(CMAKE_CXX_FLAGS_RELEASE "${CMAKE_CXX_FLAGS_RELEASE} -O3 -march=native -mtune=native")
set(CMAKE_CXX_FLAGS_DEBUG "${CMAKE_CXX_FLAGS_DEBUG} -g -O0 -Wall -Wextra")

# Find packages
find_package(Threads REQUIRED)
find_package(OpenSSL REQUIRED)

# Include directories
include_directories(include)

# Source files
set(SOURCE_FILES
    src/blob_processor.cpp
    src/crypto_utils.cpp
    src/field_element.cpp
    src/kzg.cpp
)

# Create library
add_library(tiger_blob STATIC ${SOURCE_FILES})

# Link libraries
target_link_libraries(tiger_blob PUBLIC
    Threads::Threads
    OpenSSL::Crypto
)

# Enable LTO for release builds
if(CMAKE_BUILD_TYPE STREQUAL "Release")
    include(CheckIPOSupported)
    check_ipo_supported(RESULT ipo_supported)
    if(ipo_supported)
        set_target_properties(tiger_blob PROPERTIES
            INTERPROCEDURAL_OPTIMIZATION TRUE
        )
    endif()
endif()

# Install targets
install(TARGETS tiger_blob
    ARCHIVE DESTINATION lib
    LIBRARY DESTINATION lib
)

install(DIRECTORY include/
    DESTINATION include/tiger_blob
)
