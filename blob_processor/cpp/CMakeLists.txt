cmake_minimum_required(VERSION 3.16)
project(tiger_blob_processor VERSION 2.0.0 LANGUAGES C CXX ASM)

set(CMAKE_CXX_STANDARD 20)
set(CMAKE_CXX_STANDARD_REQUIRED ON)
set(CMAKE_CXX_EXTENSIONS OFF)
set(CMAKE_C_STANDARD 11)

if(NOT CMAKE_BUILD_TYPE)
    set(CMAKE_BUILD_TYPE Release CACHE STRING "Build type" FORCE)
endif()

# Deliberately not -march=native. The library is built once and run on whatever
# machine serves traffic, so tuning for the build host's CPU would crash with an
# illegal instruction on any older one. TIGER_NATIVE_BUILD opts in for a
# self-hosted deployment that knows its hardware.
option(TIGER_NATIVE_BUILD "Tune for the building CPU" OFF)
if(TIGER_NATIVE_BUILD)
    include(CheckCXXCompilerFlag)
    check_cxx_compiler_flag("-march=native" HAS_MARCH_NATIVE)
    if(HAS_MARCH_NATIVE)
        add_compile_options(-march=native)
    endif()
endif()

# blst ships x86-64 assembly that assumes BMI2/ADX. Enabling those globally
# would make the C++ objects require them too, so only blst is compiled with
# them, and only when the toolchain accepts the flags.
include(CheckCCompilerFlag)
check_c_compiler_flag("-mbmi2" HAS_BMI2)
check_c_compiler_flag("-madx" HAS_ADX)

set(BLST_FLAGS "")
if(HAS_BMI2 AND HAS_ADX)
    set(BLST_FLAGS -mbmi2 -madx -D__ADX__)
endif()

find_package(Threads REQUIRED)

# ---------------------------------------------------------------------------
# Vendored dependencies: blst (BLS12-381) and c-kzg-4844 (EIP-4844 KZG)
# ---------------------------------------------------------------------------
set(BLST_DIR ${CMAKE_CURRENT_SOURCE_DIR}/third_party/blst)
set(CKZG_DIR ${CMAKE_CURRENT_SOURCE_DIR}/third_party/ckzg)

# blst is two translation units: server.c pulls in the C sources, and
# build/assembly.S selects the right per-format assembly. See
# third_party/README.md for the note about build/ being force-added to git.
add_library(blst STATIC
    ${BLST_DIR}/src/server.c
    ${BLST_DIR}/build/assembly.S
)
set_source_files_properties(${BLST_DIR}/build/assembly.S PROPERTIES LANGUAGE ASM)
target_include_directories(blst PUBLIC ${BLST_DIR}/bindings)
target_compile_options(blst PRIVATE ${BLST_FLAGS} -fno-builtin)
target_compile_definitions(blst PRIVATE ${BLST_DEFINES})

# c-kzg-4844 is a single translation unit that includes the rest.
add_library(ckzg STATIC ${CKZG_DIR}/src/ckzg.c)
target_include_directories(ckzg PUBLIC ${CKZG_DIR}/src)
target_link_libraries(ckzg PUBLIC blst)

# ---------------------------------------------------------------------------
# The shared Keccak/SHA3 implementation
# ---------------------------------------------------------------------------
set(CRYPTO_CPP_DIR ${CMAKE_CURRENT_SOURCE_DIR}/../../crypto_cpp)
add_library(tiger_crypto_primitives STATIC
    ${CRYPTO_CPP_DIR}/keccak256.cpp
)
target_include_directories(tiger_crypto_primitives PUBLIC ${CRYPTO_CPP_DIR})

# ---------------------------------------------------------------------------
# The blob processor
# ---------------------------------------------------------------------------
add_library(tiger_blob STATIC
    src/field_element.cpp
    src/kzg.cpp
    src/crypto_utils.cpp
    src/blob_processor.cpp
    src/trusted_setup_data.cpp
)

target_include_directories(tiger_blob PUBLIC
    ${CMAKE_CURRENT_SOURCE_DIR}/include
    ${BLST_DIR}/bindings
    ${CKZG_DIR}/src
)

target_link_libraries(tiger_blob PUBLIC
    ckzg
    tiger_crypto_primitives
    Threads::Threads
)

target_compile_options(tiger_blob PRIVATE -Wall -Wextra)

if(CMAKE_BUILD_TYPE STREQUAL "Release")
    include(CheckIPOSupported)
    check_ipo_supported(RESULT ipo_supported OUTPUT ipo_error)
    if(ipo_supported)
        set_target_properties(tiger_blob PROPERTIES INTERPROCEDURAL_OPTIMIZATION TRUE)
    else()
        message(STATUS "LTO not available: ${ipo_error}")
    endif()
endif()

# ---------------------------------------------------------------------------
# Tests
# ---------------------------------------------------------------------------
enable_testing()
add_executable(kzg_vectors tests/kzg_vectors.cpp)
target_link_libraries(kzg_vectors PRIVATE tiger_blob)
add_test(NAME kzg_vectors COMMAND kzg_vectors)

install(TARGETS tiger_blob ARCHIVE DESTINATION lib LIBRARY DESTINATION lib)
install(DIRECTORY include/ DESTINATION include/tiger_blob)
