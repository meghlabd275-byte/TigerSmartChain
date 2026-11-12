/**
 * @file crypto_utils.cpp
 * @brief Hash and Merkle helpers
 *
 * Keccak-256 and SHA3-256 come from `crypto_cpp`, the repository's own
 * dependency-free implementation, which is already covered by NIST and
 * Ethereum test vectors. SHA-256 comes from blst. Nothing here reimplements a
 * hash, because a second implementation is a second chance to get a padding
 * byte wrong.
 */

#include "crypto_utils.hpp"

#include <cstring>

#include "tiger_crypto.h"

extern "C" {
// blst_aux.h uses types from blst.h, so it must come first.
#include "blst.h"
#include "blst_aux.h"
}

namespace tigerchain {
namespace crypto {

namespace {

constexpr uint8_t MERKLE_LEAF_PREFIX = 0x00;
constexpr uint8_t MERKLE_NODE_PREFIX = 0x01;

Hash256 hash_pair(const Hash256& left, const Hash256& right) {
    std::vector<uint8_t> buffer;
    buffer.reserve(1 + 64);
    buffer.push_back(MERKLE_NODE_PREFIX);
    buffer.insert(buffer.end(), left.begin(), left.end());
    buffer.insert(buffer.end(), right.begin(), right.end());
    return keccak256(buffer);
}

} // namespace

Hash256 keccak256(const std::vector<uint8_t>& data) {
    Hash256 out{};
    ::keccak256(data.data(), data.size(), out.data());
    return out;
}

Hash256 sha256(const std::vector<uint8_t>& data) {
    Hash256 out{};
    blst_sha256(out.data(), data.data(), data.size());
    return out;
}

Hash256 sha3_256(const std::vector<uint8_t>& data) {
    Hash256 out{};
    ::sha3_256(data.data(), data.size(), out.data());
    return out;
}

std::string bytes_to_hex(const uint8_t* data, size_t length) {
    static const char* digits = "0123456789abcdef";
    std::string out = "0x";
    out.reserve(2 + length * 2);
    for (size_t i = 0; i < length; ++i) {
        out.push_back(digits[(data[i] >> 4) & 0x0f]);
        out.push_back(digits[data[i] & 0x0f]);
    }
    return out;
}

std::string bytes_to_hex(const std::vector<uint8_t>& bytes) {
    return bytes_to_hex(bytes.data(), bytes.size());
}

std::string bytes_to_hex(const Hash256& bytes) {
    return bytes_to_hex(bytes.data(), bytes.size());
}

bool hex_to_bytes(const std::string& hex, std::vector<uint8_t>& out) {
    std::string digits = hex;
    if (digits.size() >= 2 && digits[0] == '0' && (digits[1] == 'x' || digits[1] == 'X')) {
        digits = digits.substr(2);
    }
    if (digits.size() % 2 != 0) {
        return false;
    }
    const auto nibble = [](char c) -> int {
        if (c >= '0' && c <= '9') return c - '0';
        if (c >= 'a' && c <= 'f') return c - 'a' + 10;
        if (c >= 'A' && c <= 'F') return c - 'A' + 10;
        return -1;
    };
    out.clear();
    out.reserve(digits.size() / 2);
    for (size_t i = 0; i < digits.size(); i += 2) {
        const int high = nibble(digits[i]);
        const int low = nibble(digits[i + 1]);
        if (high < 0 || low < 0) {
            out.clear();
            return false;
        }
        out.push_back(static_cast<uint8_t>((high << 4) | low));
    }
    return true;
}

std::vector<uint8_t> hex_to_bytes(const std::string& hex) {
    std::vector<uint8_t> out;
    if (!hex_to_bytes(hex, out)) {
        return {};
    }
    return out;
}

Hash256 merkle_leaf_hash(const std::vector<uint8_t>& leaf) {
    std::vector<uint8_t> buffer;
    buffer.reserve(1 + leaf.size());
    buffer.push_back(MERKLE_LEAF_PREFIX);
    buffer.insert(buffer.end(), leaf.begin(), leaf.end());
    return keccak256(buffer);
}

Hash256 compute_merkle_root(const std::vector<std::vector<uint8_t>>& leaves) {
    if (leaves.empty()) {
        return Hash256{};
    }
    std::vector<Hash256> level;
    level.reserve(leaves.size());
    for (const auto& leaf : leaves) {
        level.push_back(merkle_leaf_hash(leaf));
    }
    while (level.size() > 1) {
        std::vector<Hash256> next;
        next.reserve((level.size() + 1) / 2);
        for (size_t i = 0; i + 1 < level.size(); i += 2) {
            next.push_back(hash_pair(level[i], level[i + 1]));
        }
        // An odd node is promoted unchanged rather than duplicated. Duplicating
        // would let an attacker present a tree of 2n leaves as if it had 2n+1,
        // because the last pair would hash the same value twice.
        if (level.size() % 2 == 1) {
            next.push_back(level.back());
        }
        level = std::move(next);
    }
    return level[0];
}

std::vector<MerkleProofNode> compute_merkle_proof(
    const std::vector<std::vector<uint8_t>>& leaves, size_t index) {
    std::vector<MerkleProofNode> proof;
    if (leaves.empty() || index >= leaves.size()) {
        return proof;
    }
    std::vector<Hash256> level;
    level.reserve(leaves.size());
    for (const auto& leaf : leaves) {
        level.push_back(merkle_leaf_hash(leaf));
    }
    size_t position = index;
    while (level.size() > 1) {
        if (position % 2 == 0) {
            // The sibling is on the right when it exists.
            if (position + 1 < level.size()) {
                proof.push_back({level[position + 1], true});
            }
        } else {
            proof.push_back({level[position - 1], false});
        }
        std::vector<Hash256> next;
        next.reserve((level.size() + 1) / 2);
        for (size_t i = 0; i + 1 < level.size(); i += 2) {
            next.push_back(hash_pair(level[i], level[i + 1]));
        }
        if (level.size() % 2 == 1) {
            next.push_back(level.back());
        }
        position /= 2;
        level = std::move(next);
    }
    return proof;
}

bool verify_merkle_proof(const std::vector<uint8_t>& leaf,
                         const std::vector<MerkleProofNode>& proof,
                         const Hash256& root) {
    Hash256 current = merkle_leaf_hash(leaf);
    for (const auto& node : proof) {
        current = node.sibling_is_right ? hash_pair(current, node.sibling)
                                        : hash_pair(node.sibling, current);
    }
    return current == root;
}

} // namespace crypto
} // namespace tigerchain
