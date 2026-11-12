/**
 * @file crypto_utils.hpp
 * @brief Hash and Merkle helpers for blob processing
 *
 * Every function here is a real primitive:
 *
 *  - `keccak256` is the original Keccak submission that Ethereum uses (padding
 *    byte 0x01), implemented once in `crypto_cpp/keccak256.cpp` and shared.
 *  - `sha3_256` is FIPS-202 SHA-3 (padding byte 0x06). The two differ only in
 *    that byte, and mixing them up silently produces wrong hashes, so they
 *    stay separate functions over a shared permutation.
 *  - `sha256` is FIPS-180-4, from blst's vetted implementation.
 *
 * The Merkle helpers hash leaves with a domain separator so a leaf can never
 * be mistaken for an internal node, and an odd node is promoted rather than
 * duplicated, which avoids the duplicate-leaf forgery that naive Merkle trees
 * allow.
 */

#ifndef CRYPTO_UTILS_HPP
#define CRYPTO_UTILS_HPP

#include <array>
#include <cstdint>
#include <string>
#include <vector>

namespace tigerchain {
namespace crypto {

using Hash256 = std::array<uint8_t, 32>;

// =============================================================================
// Hash functions
// =============================================================================

/// Ethereum's Keccak-256.
Hash256 keccak256(const std::vector<uint8_t>& data);

/// FIPS-180-4 SHA-256.
Hash256 sha256(const std::vector<uint8_t>& data);

/// FIPS-202 SHA-3-256.
Hash256 sha3_256(const std::vector<uint8_t>& data);

// =============================================================================
// Hex helpers
// =============================================================================

/// `0x`-prefixed lowercase hex.
std::string bytes_to_hex(const std::vector<uint8_t>& bytes);
std::string bytes_to_hex(const Hash256& bytes);
std::string bytes_to_hex(const uint8_t* data, size_t length);

/// Parse `0x`-prefixed or bare hex. Returns false on a malformed string or an
/// odd number of digits, so a bad input is reported rather than silently
/// becoming a shorter byte string.
bool hex_to_bytes(const std::string& hex, std::vector<uint8_t>& out);

/// Convenience wrapper that returns an empty vector on malformed input.
std::vector<uint8_t> hex_to_bytes(const std::string& hex);

// =============================================================================
// Merkle tree
// =============================================================================

/// One step of a Merkle proof: the sibling hash and which side it is on.
/// Carrying the side explicitly means verification needs no index and no
/// guessing, which is where naive implementations go wrong.
struct MerkleProofNode {
    Hash256 sibling;
    /// True when the sibling is the right-hand child.
    bool sibling_is_right;
};

/// Hash a leaf with its domain separator.
Hash256 merkle_leaf_hash(const std::vector<uint8_t>& leaf);

/// Merkle root of the leaves, or the all-zero hash when there are none.
Hash256 compute_merkle_root(const std::vector<std::vector<uint8_t>>& leaves);

/// Proof that `index` is in the tree. Empty when `index` is out of range.
std::vector<MerkleProofNode> compute_merkle_proof(
    const std::vector<std::vector<uint8_t>>& leaves,
    size_t index
);

/// Verify a proof against a root.
bool verify_merkle_proof(
    const std::vector<uint8_t>& leaf,
    const std::vector<MerkleProofNode>& proof,
    const Hash256& root
);

} // namespace crypto
} // namespace tigerchain

#endif // CRYPTO_UTILS_HPP
