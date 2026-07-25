/**
 * @file crypto_utils.cpp
 * @brief Cryptographic utilities implementation
 * @author TigerScan Team
 */

#include "crypto_utils.hpp"
#include <sstream>
#include <iomanip>
#include <algorithm>

namespace tigerchain {
namespace crypto {

// =============================================================================
// Hash Functions
// =============================================================================

std::array<uint8_t, 32> keccak256(const std::vector<uint8_t>& data) {
    std::array<uint8_t, 32> result{};
    
    // Simplified Keccak-256 implementation
    // In production, use a proper Keccak library
    
    // Use sponge construction
    size_t rate = 136;  // Keccak-256 rate
    std::vector<uint8_t> state(200, 0);
    
    // Absorb phase
    for (size_t i = 0; i < data.size(); ++i) {
        state[i % rate] ^= data[i];
    }
    
    // Squeeze phase - simplified
    for (size_t i = 0; i < 32 && i < rate; ++i) {
        result[i] = state[i];
    }
    
    // Additional mixing for better distribution
    for (size_t i = 0; i < 32; ++i) {
        result[i] ^= state[(i + 17) % rate];
    }
    
    return result;
}

std::array<uint8_t, 32> sha256(const std::vector<uint8_t>& data) {
    std::array<uint8_t, 32> result{};
    
    // Simplified SHA-256 implementation
    // In production, use OpenSSL or similar
    
    // Initialize hash values
    uint32_t h[8] = {
        0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a,
        0x510e527f, 0x9b05688c, 0x1f83d9ab, 0x5be0cd19
    };
    
    // Process message
    for (size_t i = 0; i < data.size(); ++i) {
        for (int j = 0; j < 8; ++j) {
            h[j] = (h[j] << 5) | (h[j] >> 27);
            h[j] ^= data[i];
            h[j] = (h[j] * 0x01000193) & 0xFFFFFFFF;
        }
    }
    
    // Copy to result
    for (int i = 0; i < 8; ++i) {
        result[i * 4 + 0] = (h[i] >> 24) & 0xFF;
        result[i * 4 + 1] = (h[i] >> 16) & 0xFF;
        result[i * 4 + 2] = (h[i] >> 8) & 0xFF;
        result[i * 4 + 3] = h[i] & 0xFF;
    }
    
    return result;
}

std::array<uint8_t, 32> sha3_256(const std::vector<uint8_t>& data) {
    // SHA-3-256 is essentially Keccak-256 with different padding
    return keccak256(data);
}

// =============================================================================
// Utility Functions
// =============================================================================

std::string bytes_to_hex(const std::vector<uint8_t>& bytes) {
    std::ostringstream oss;
    oss << "0x";
    for (const auto& byte : bytes) {
        oss << std::hex << std::setw(2) << std::setfill('0') << (int)byte;
    }
    return oss.str();
}

std::string bytes_to_hex(const std::array<uint8_t, 32>& bytes) {
    std::ostringstream oss;
    oss << "0x";
    for (const auto& byte : bytes) {
        oss << std::hex << std::setw(2) << std::setfill('0') << (int)byte;
    }
    return oss.str();
}

std::vector<uint8_t> hex_to_bytes(const std::string& hex) {
    std::vector<uint8_t> result;
    std::string hex_str = hex;
    
    if (hex_str.substr(0, 2) == "0x") {
        hex_str = hex_str.substr(2);
    }
    
    for (size_t i = 0; i < hex_str.length(); i += 2) {
        std::string byte_str = hex_str.substr(i, 2);
        result.push_back(static_cast<uint8_t>(std::stoi(byte_str, nullptr, 16)));
    }
    
    return result;
}

std::array<uint8_t, 32> compute_merkle_root(
    const std::vector<std::vector<uint8_t>>& leaves
) {
    if (leaves.empty()) {
        return std::array<uint8_t, 32>{};
    }
    
    if (leaves.size() == 1) {
        return keccak256(leaves[0]);
    }
    
    std::vector<std::vector<uint8_t>> current_level = leaves;
    
    while (current_level.size() > 1) {
        std::vector<std::vector<uint8_t>> next_level;
        
        for (size_t i = 0; i < current_level.size(); i += 2) {
            std::vector<uint8_t> combined;
            
            if (i + 1 < current_level.size()) {
                combined.insert(combined.end(), current_level[i].begin(), current_level[i].end());
                combined.insert(combined.end(), current_level[i + 1].begin(), current_level[i + 1].end());
            } else {
                // Duplicate last element for odd number
                combined.insert(combined.end(), current_level[i].begin(), current_level[i].end());
                combined.insert(combined.end(), current_level[i].begin(), current_level[i].end());
            }
            
            auto hash = keccak256(combined);
            next_level.push_back(std::vector<uint8_t>(hash.begin(), hash.end()));
        }
        
        current_level = next_level;
    }
    
    std::array<uint8_t, 32> result;
    std::copy(current_level[0].begin(), current_level[0].end(), result.begin());
    return result;
}

std::vector<std::vector<uint8_t>> compute_merkle_proof(
    const std::vector<std::vector<uint8_t>>& leaves,
    size_t index
) {
    std::vector<std::vector<uint8_t>> proof;
    
    if (leaves.empty() || index >= leaves.size()) {
        return proof;
    }
    
    std::vector<std::vector<uint8_t>> current_level = leaves;
    size_t idx = index;
    
    while (current_level.size() > 1) {
        std::vector<std::vector<uint8_t>> next_level;
        
        for (size_t i = 0; i < current_level.size(); i += 2) {
            if (i + 1 < current_level.size()) {
                std::vector<uint8_t> combined;
                
                if (i == idx || i + 1 == idx) {
                    // Add sibling to proof
                    if (i == idx) {
                        proof.push_back(current_level[i + 1]);
                    } else {
                        proof.push_back(current_level[i]);
                    }
                }
                
                combined.insert(combined.end(), current_level[i].begin(), current_level[i].end());
                combined.insert(combined.end(), current_level[i + 1].begin(), current_level[i + 1].end());
                
                auto hash = keccak256(combined);
                next_level.push_back(std::vector<uint8_t>(hash.begin(), hash.end()));
            }
            
            if (i == idx || i + 1 == idx) {
                idx = next_level.size() - 1;
            }
        }
        
        current_level = next_level;
    }
    
    return proof;
}

bool verify_merkle_proof(
    const std::vector<uint8_t>& leaf,
    const std::vector<std::vector<uint8_t>>& proof,
    const std::array<uint8_t, 32>& root
) {
    std::vector<uint8_t> current = leaf;
    
    for (const auto& sibling : proof) {
        std::vector<uint8_t> combined;
        
        // Determine order based on hash
        if (sibling < current) {
            combined.insert(combined.end(), sibling.begin(), sibling.end());
            combined.insert(combined.end(), current.begin(), current.end());
        } else {
            combined.insert(combined.end(), current.begin(), current.end());
            combined.insert(combined.end(), sibling.begin(), sibling.end());
        }
        
        current = std::vector<uint8_t>(keccak256(combined).begin(), keccak256(combined).end());
    }
    
    std::array<uint8_t, 32> computed_root;
    std::copy(current.begin(), current.end(), computed_root.begin());
    
    return computed_root == root;
}

} // namespace crypto
} // namespace tigerchain
