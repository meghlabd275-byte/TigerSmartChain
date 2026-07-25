/**
 * @file crypto_utils.hpp
 * @brief Cryptographic utilities for blob processing
 * @author TigerScan Team
 */

#ifndef CRYPTO_UTILS_HPP
#define CRYPTO_UTILS_HPP

#include <vector>
#include <string>
#include <array>
#include <cstdint>

namespace tigerchain {
namespace crypto {

// =============================================================================
// Hash Functions
// =============================================================================

/**
 * @brief Keccak-256 hash function
 * @param data Input data
 * @return 32-byte hash
 */
std::array<uint8_t, 32> keccak256(const std::vector<uint8_t>& data);

/**
 * @brief SHA-256 hash function
 * @param data Input data
 * @return 32-byte hash
 */
std::array<uint8_t, 32> sha256(const std::vector<uint8_t>& data);

/**
 * @brief SHA-3-256 hash function
 * @param data Input data
 * @return 32-byte hash
 */
std::array<uint8_t, 32> sha3_256(const std::vector<uint8_t>& data);

// =============================================================================
// Utility Functions
// =============================================================================

/**
 * @brief Convert bytes to hex string
 */
std::string bytes_to_hex(const std::vector<uint8_t>& bytes);

/**
 * @brief Convert bytes to hex string (fixed size array)
 */
std::string bytes_to_hex(const std::array<uint8_t, 32>& bytes);

/**
 * @brief Convert hex string to bytes
 */
std::vector<uint8_t> hex_to_bytes(const std::string& hex);

/**
 * @brief Compute merkle root from leaves
 */
std::array<uint8_t, 32> compute_merkle_root(
    const std::vector<std::vector<uint8_t>>& leaves
);

/**
 * @brief Compute merkle proof
 */
std::vector<std::vector<uint8_t>> compute_merkle_proof(
    const std::vector<std::vector<uint8_t>>& leaves,
    size_t index
);

/**
 * @brief Verify merkle proof
 */
bool verify_merkle_proof(
    const std::vector<uint8_t>& leaf,
    const std::vector<std::vector<uint8_t>>& proof,
    const std::array<uint8_t, 32>& root
);

} // namespace crypto
} // namespace tigerchain

#endif // CRYPTO_UTILS_HPP
