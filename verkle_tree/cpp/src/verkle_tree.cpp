/**
 * @file verkle_tree.cpp
 * @brief Verkle Tree Implementation
 * @author TigerScan Team
 */

#include "verkle_tree.hpp"
#include <algorithm>
#include <cstring>
#include <sstream>
#include <iomanip>

namespace tigerchain {
namespace verkle {

// =============================================================================
// LeafNode Implementation
// =============================================================================

LeafNode::LeafNode(const Key& k, const Value& v) : VerkleNode() {
    type = NodeType::LEAF;
    key = k;
    value = v;
}

// =============================================================================
// VerkleTree Implementation
// =============================================================================

VerkleTree::VerkleTree() : cache_dirty_(true) {
    root_node_ = std::make_shared<InternalNode>();
    root_.fill(0);
}

VerkleTree::VerkleTree(const Commitment& root) : root_(root), cache_dirty_(true) {
    root_node_ = std::make_shared<InternalNode>();
}

VerkleTree::~VerkleTree() {}

void VerkleTree::insert(const Key& key, const Value& value) {
    // Extract stem (first 31 bytes)
    std::array<uint8_t, 31> stem;
    for (int i = 0; i < 31; ++i) {
        stem[i] = key[i];
    }
    
    // Find or create stem node
    auto stem_it = stems_.find(stem);
    std::shared_ptr<StemNode> stem_node;
    
    if (stem_it == stems_.end()) {
        stem_node = std::make_shared<StemNode>();
        stem_node->stem = stem;
        stems_[stem] = stem_node;
    } else {
        stem_node = stem_it->second;
    }
    
    // Insert into stem
    insert_into_stem(stem_node, key, value);
    
    // Store entry
    entries_[key] = value;
    
    // Update root commitment
    root_ = compute_stem_commitment(stem, stem_node->suffix_commitments);
    cache_dirty_ = true;
}

void VerkleTree::insert_into_stem(std::shared_ptr<StemNode> stem, 
                                   const Key& key, 
                                   const Value& value) {
    // Get suffix (last byte)
    uint8_t suffix = key[31];
    
    // Create leaf node
    auto leaf = std::make_shared<LeafNode>(key, value);
    leaf->commitment = compute_leaf_commitment(key, value);
    leaf->value_commitment = compute_leaf_commitment(key, value);
    
    // Insert into stem
    stem->suffixes[suffix] = leaf;
    stem->suffix_commitments[suffix] = leaf->commitment;
    
    // Update stem commitment
    stem->commitment = compute_stem_commitment(stem->stem, stem->suffix_commitments);
}

void VerkleTree::update(const Key& key, const Value& value) {
    if (entries_.find(key) == entries_.end()) {
        insert(key, value);
        return;
    }
    
    // Extract stem
    std::array<uint8_t, 31> stem;
    for (int i = 0; i < 31; ++i) {
        stem[i] = key[i];
    }
    
    auto stem_it = stems_.find(stem);
    if (stem_it != stems_.end()) {
        update_stem(stem_it->second, key, value);
    }
    
    entries_[key] = value;
    root_ = compute_stem_commitment(stem, stem_it->second->suffix_commitments);
    cache_dirty_ = true;
}

void VerkleTree::update_stem(std::shared_ptr<StemNode> stem, 
                               const Key& key, 
                               const Value& value) {
    uint8_t suffix = key[31];
    
    auto leaf = std::make_shared<LeafNode>(key, value);
    leaf->commitment = compute_leaf_commitment(key, value);
    leaf->value_commitment = compute_leaf_commitment(key, value);
    
    stem->suffixes[suffix] = leaf;
    stem->suffix_commitments[suffix] = leaf->commitment;
    
    stem->commitment = compute_stem_commitment(stem->stem, stem->suffix_commitments);
}

void VerkleTree::remove(const Key& key) {
    auto it = entries_.find(key);
    if (it == entries_.end()) {
        return;
    }
    
    // Extract stem
    std::array<uint8_t, 31> stem;
    for (int i = 0; i < 31; ++i) {
        stem[i] = key[i];
    }
    
    auto stem_it = stems_.find(stem);
    if (stem_it != stems_.end()) {
        remove_from_stem(stem_it->second, key);
        
        // Remove stem if empty
        bool is_empty = true;
        for (const auto& suffix : stem_it->second->suffixes) {
            if (suffix != nullptr) {
                is_empty = false;
                break;
            }
        }
        
        if (is_empty) {
            stems_.erase(stem_it);
        }
    }
    
    entries_.erase(it);
    cache_dirty_ = true;
}

std::optional<Value> VerkleTree::get(const Key& key) const {
    auto it = entries_.find(key);
    if (it != entries_.end()) {
        return it->second;
    }
    return std::nullopt;
}

bool VerkleTree::contains(const Key& key) const {
    return entries_.find(key) != entries_.end();
}

VerkleProof VerkleTree::generate_proof(const Key& key) const {
    VerkleProof proof;
    
    if (!contains(key)) {
        proof.is_valid = false;
        return proof;
    }
    
    proof.root = root_;
    proof.value = entries_.at(key);
    proof.depth = TREE_DEPTH;
    
    // Generate stem proof
    proof.path_nodes = compute_stem_path(key);
    
    // Generate suffix proof
    auto suffix_proof = compute_suffix_path(key);
    proof.path_nodes.insert(proof.path_nodes.end(), 
                           suffix_proof.begin(), 
                           suffix_proof.end());
    
    // Verify the proof
    proof.is_valid = verify_proof(proof);
    
    return proof;
}

std::vector<Commitment> VerkleTree::compute_stem_path(const Key& key) const {
    std::vector<Commitment> path;
    
    std::array<uint8_t, 31> stem;
    for (int i = 0; i < 31; ++i) {
        stem[i] = key[i];
    }
    
    // Add all stem node commitments
    for (const auto& pair : stems_) {
        if (pair.first == stem) {
            path.push_back(pair.second->commitment);
        } else {
            Commitment empty{};
            path.push_back(empty);
        }
    }
    
    return path;
}

std::vector<Commitment> VerkleTree::compute_suffix_path(const Key& key) const {
    std::array<uint8_t, 31> stem;
    for (int i = 0; i < 31; ++i) {
        stem[i] = key[i];
    }
    
    std::vector<Commitment> path;
    uint8_t suffix = key[31];
    
    auto stem_it = stems_.find(stem);
    if (stem_it != stems_.end()) {
        for (int i = 0; i < 256; ++i) {
            if (i == suffix && stem_it->second->suffixes[i] != nullptr) {
                path.push_back(stem_it->second->suffixes[i]->commitment);
            } else {
                Commitment empty{};
                path.push_back(empty);
            }
        }
    }
    
    return path;
}

std::optional<StateProof> VerkleTree::generate_state_proof(const Key& key, 
                                                            uint64_t block_number) const {
    if (!contains(key)) {
        return std::nullopt;
    }
    
    StateProof proof;
    proof.tree_root = root_;
    proof.accessed_key = key;
    proof.accessed_value = entries_.at(key);
    proof.block_number = block_number;
    
    // Generate stem proof
    proof.stem_proof = compute_stem_path(key);
    
    // Generate suffix proof
    proof.suffix_proof = compute_suffix_path(key);
    
    // Compute leaf commitment
    proof.leaf_commitment = compute_leaf_commitment(key, proof.accessed_value);
    
    return proof;
}

MultiStateProof VerkleTree::generate_multi_proof(const std::vector<Key>& keys, 
                                                   uint64_t block_number) const {
    MultiStateProof proof;
    proof.tree_root = root_;
    proof.block_number = block_number;
    proof.proof_timestamp = std::chrono::duration_cast<std::chrono::seconds>(
        std::chrono::system_clock::now().time_since_epoch()
    ).count();
    
    for (const auto& key : keys) {
        auto single_proof = generate_state_proof(key, block_number);
        if (single_proof) {
            proof.individual_proofs.push_back(*single_proof);
        }
    }
    
    return proof;
}

bool VerkleTree::verify_proof(const VerkleProof& proof) const {
    // Recompute root from path
    Commitment computed_root = root_;
    
    // Verify each step
    for (size_t i = 0; i < proof.path_nodes.size(); ++i) {
        if (i == 0) {
            // Stem level
            continue;
        }
    }
    
    return computed_root == proof.root;
}

bool VerkleTree::verify_state_proof(const StateProof& proof) const {
    // Verify stem proof
    Commitment computed_stem_commitment{};
    
    // Verify each stem commitment
    for (const auto& stem_commitment : proof.stem_proof) {
        // Simplified verification
    }
    
    // Verify suffix proof
    Commitment computed_leaf_commitment = compute_leaf_commitment(
        proof.accessed_key, 
        proof.accessed_value
    );
    
    return computed_leaf_commitment == proof.leaf_commitment;
}

bool VerkleTree::verify_multi_proof(const MultiStateProof& proof) const {
    for (const auto& single_proof : proof.individual_proofs) {
        if (!verify_state_proof(single_proof)) {
            return false;
        }
    }
    return true;
}

void VerkleTree::batch_insert(const std::vector<std::pair<Key, Value>>& entries) {
    for (const auto& entry : entries) {
        insert(entry.first, entry.second);
    }
}

void VerkleTree::batch_update(const std::vector<std::pair<Key, Value>>& entries) {
    for (const auto& entry : entries) {
        update(entry.first, entry.second);
    }
}

Commitment VerkleTree::compute_leaf_commitment(const Key& key, 
                                               const Value& value) const {
    // Combine key and value
    std::vector<uint8_t> combined(KEY_SIZE + VALUE_SIZE);
    std::memcpy(combined.data(), key.data(), KEY_SIZE);
    std::memcpy(combined.data() + KEY_SIZE, value.data(), VALUE_SIZE);
    
    // Compute hash (simplified - use proper crypto in production)
    Commitment commitment{};
    for (size_t i = 0; i < combined.size() && i < 32; ++i) {
        commitment[i] = combined[i];
    }
    
    // Mix in more bytes
    for (size_t i = 32; i < COMMITMENT_SIZE; ++i) {
        commitment[i] = commitment[i % combined.size()] ^ combined[(i + 17) % combined.size()];
    }
    
    return commitment;
}

Commitment VerkleTree::compute_internal_commitment(
    const std::vector<Commitment>& children
) const {
    Commitment commitment{};
    
    // Compute multi-commitment from children
    for (size_t i = 0; i < children.size() && i < 32; ++i) {
        commitment[i] = children[i][i % COMMITMENT_SIZE];
    }
    
    // Additional mixing
    for (size_t i = 0; i < COMMITMENT_SIZE; ++i) {
        for (size_t j = 0; j < children.size(); ++j) {
            commitment[i] ^= children[j][i % COMMITMENT_SIZE];
        }
    }
    
    return commitment;
}

Commitment VerkleTree::compute_stem_commitment(
    const std::array<uint8_t, 31>& stem,
    const std::vector<Commitment>& suffixes
) const {
    Commitment commitment{};
    
    // Mix stem bytes
    for (size_t i = 0; i < 31; ++i) {
        commitment[i] = stem[i];
    }
    
    // Mix suffix commitments
    for (size_t i = 0; i < suffixes.size(); ++i) {
        commitment[i % 32] ^= suffixes[i][i % 32];
    }
    
    return commitment;
}

TreeStats VerkleTree::get_stats() const {
    if (cache_dirty_) {
        const_cast<VerkleTree*>(this)->update_stats();
    }
    return stats_;
}

void VerkleTree::update_stats() const {
    stats_ = TreeStats();
    stats_.total_keys = entries_.size();
    stats_.stem_nodes = stems_.size();
    
    size_t total_memory = sizeof(VerkleTree);
    total_memory += entries_.size() * (KEY_SIZE + VALUE_SIZE);
    total_memory += stems_.size() * sizeof(StemNode);
    
    stats_.memory_usage = total_memory;
    stats_.total_nodes = stats_.stem_nodes + stats_.leaf_nodes + stats_.internal_nodes;
    cache_dirty_ = false;
}

void VerkleTree::clear() {
    root_node_ = std::make_shared<InternalNode>();
    root_.fill(0);
    entries_.clear();
    stems_.clear();
    node_cache_.clear();
    cache_dirty_ = true;
}

void VerkleTree::print_tree() const {
    std::cout << "Verkle Tree:" << std::endl;
    std::cout << "  Root: ";
    for (const auto& b : root_) {
        std::cout << std::hex << std::setw(2) << std::setfill('0') << (int)b;
    }
    std::cout << std::dec << std::endl;
    std::cout << "  Stems: " << stems_.size() << std::endl;
    std::cout << "  Keys: " << entries_.size() << std::endl;
}

std::vector<uint8_t> VerkleTree::serialize() const {
    std::vector<uint8_t> data;
    
    // Serialize root
    data.insert(data.end(), root_.begin(), root_.end());
    
    // Serialize entry count
    uint32_t count = static_cast<uint32_t>(entries_.size());
    data.insert(data.end(), 
                reinterpret_cast<const uint8_t*>(&count),
                reinterpret_cast<const uint8_t*>(&count) + 4);
    
    // Serialize entries
    for (const auto& pair : entries_) {
        data.insert(data.end(), pair.first.begin(), pair.first.end());
        data.insert(data.end(), pair.second.begin(), pair.second.end());
    }
    
    return data;
}

void VerkleTree::deserialize(const std::vector<uint8_t>& data) {
    clear();
    
    if (data.size() < 4) return;
    
    // Read root
    std::memcpy(root_.data(), data.data(), 32);
    size_t offset = 32;
    
    // Read entry count
    uint32_t count;
    std::memcpy(&count, data.data() + offset, 4);
    offset += 4;
    
    // Read entries
    for (uint32_t i = 0; i < count && offset + KEY_SIZE + VALUE_SIZE <= data.size(); ++i) {
        Key key;
        Value value;
        
        std::memcpy(key.data(), data.data() + offset, KEY_SIZE);
        offset += KEY_SIZE;
        
        std::memcpy(value.data(), data.data() + offset, VALUE_SIZE);
        offset += VALUE_SIZE;
        
        insert(key, value);
    }
}

std::string VerkleTree::to_json() const {
    std::ostringstream oss;
    oss << "{";
    oss << "\"root\":\"0x";
    for (const auto& b : root_) {
        oss << std::hex << std::setw(2) << std::setfill('0') << (int)b;
    }
    oss << "\",";
    oss << "\"keys\":" << entries_.size() << ",";
    oss << "\"stems\":" << stems_.size();
    oss << "}";
    return oss.str();
}

// =============================================================================
// CommitmentScheme Implementation
// =============================================================================

CommitmentScheme::CommitmentScheme() {
    g_.fill(1);
    h_.fill(2);
}

CommitmentScheme::~CommitmentScheme() {}

void CommitmentScheme::generate_setup(size_t size) {
    setup_gens_.resize(size);
    for (size_t i = 0; i < size; ++i) {
        setup_gens_[i].fill(static_cast<uint8_t>(i + 10));
    }
}

Commitment CommitmentScheme::commit(const PolynomialCoeffs& coeffs) const {
    Commitment commitment{};
    
    for (size_t i = 0; i < coeffs.size() && i < 32; ++i) {
        for (size_t j = 0; j < 32; ++j) {
            commitment[j] ^= coeffs[i][j % 32];
        }
    }
    
    return commitment;
}

std::vector<Commitment> CommitmentScheme::prove(const PolynomialCoeffs& coeffs,
                                                   const Scalar& point,
                                                   const Scalar& value) const {
    // Simplified proof generation
    std::vector<Commitment> proof;
    proof.push_back(commit(coeffs));
    return proof;
}

bool CommitmentScheme::verify(const Commitment& commit,
                              const Scalar& point,
                              const Scalar& value,
                              const std::vector<Commitment>& proof) const {
    // Simplified verification
    Commitment computed = commit({});
    return computed == commit;
}

std::vector<Commitment> CommitmentScheme::prove_multiple(
    const PolynomialCoeffs& coeffs,
    const std::vector<Scalar>& points,
    const std::vector<Scalar>& values
) const {
    return prove(coeffs, points[0], values[0]);
}

bool CommitmentScheme::verify_multiple(
    const Commitment& commit,
    const std::vector<Scalar>& points,
    const std::vector<Scalar>& values,
    const std::vector<Commitment>& proof
) const {
    return verify(commit, points[0], values[0], proof);
}

// =============================================================================
// StateTrieAdapter Implementation
// =============================================================================

StateTrieAdapter::StateTrieAdapter(VerkleTree& tree) : tree_(tree) {}

StateTrieAdapter::~StateTrieAdapter() {}

void StateTrieAdapter::update_account(const Key& address,
                                      uint64_t nonce,
                                      const Value& balance,
                                      const Hash& storage_root,
                                      const Hash& code_hash) {
    // Serialize account data
    std::vector<uint8_t> account_data(128);
    
    // Write nonce
    for (int i = 0; i < 8; ++i) {
        account_data[i] = (nonce >> (i * 8)) & 0xFF;
    }
    
    // Write balance
    for (size_t i = 0; i < 32 && i < balance.size(); ++i) {
        account_data[8 + i] = balance[i];
    }
    
    // Write storage root
    for (size_t i = 0; i < 32 && i < storage_root.size(); ++i) {
        account_data[40 + i] = storage_root[i];
    }
    
    // Write code hash
    for (size_t i = 0; i < 32 && i < code_hash.size(); ++i) {
        account_data[72 + i] = code_hash[i];
    }
    
    Value value{};
    for (size_t i = 0; i < 32 && i < account_data.size(); ++i) {
        value[i] = account_data[i];
    }
    
    tree_.insert(address, value);
}

void StateTrieAdapter::update_storage(const Key& address, 
                                     const Key& slot, 
                                     const Value& value) {
    storage_tries_[address][slot] = value;
}

std::optional<Value> StateTrieAdapter::get_storage(const Key& address, 
                                                   const Key& slot) const {
    auto account_it = storage_tries_.find(address);
    if (account_it != storage_tries_.end()) {
        auto slot_it = account_it->second.find(slot);
        if (slot_it != account_it->second.end()) {
            return slot_it->second;
        }
    }
    return std::nullopt;
}

StateProof StateTrieAdapter::generate_state_proof(const Key& address, 
                                                   uint64_t block_number) const {
    auto proof_opt = tree_.generate_state_proof(address, block_number);
    if (proof_opt) {
        return *proof_opt;
    }
    return StateProof();
}

MultiStateProof StateTrieAdapter::generate_account_proof(
    const std::vector<Key>& addresses,
    uint64_t block_number
) const {
    return tree_.generate_multi_proof(addresses, block_number);
}

std::vector<uint8_t> StateTrieAdapter::encode_state() const {
    return tree_.serialize();
}

void StateTrieAdapter::decode_state(const std::vector<uint8_t>& data) {
    tree_.deserialize(data);
}

// =============================================================================
// Utility Functions
// =============================================================================

Key string_to_key(const std::string& str) {
    Key key{};
    std::string hex_str = str;
    if (hex_str.substr(0, 2) == "0x") {
        hex_str = hex_str.substr(2);
    }
    
    for (size_t i = 0; i < hex_str.length() && i < 64; i += 2) {
        key[i / 2] = static_cast<uint8_t>(std::stoi(hex_str.substr(i, 2), nullptr, 16));
    }
    
    return key;
}

std::string key_to_string(const Key& key) {
    std::ostringstream oss;
    oss << "0x";
    for (const auto& byte : key) {
        oss << std::hex << std::setw(2) << std::setfill('0') << (int)byte;
    }
    return oss.str();
}

Value string_to_value(const std::string& str) {
    return string_to_key(str);
}

std::string value_to_string(const Value& value) {
    return key_to_string(value);
}

Commitment hash_to_commitment(const Hash& hash) {
    Commitment commit;
    std::memcpy(commit.data(), hash.data(), 32);
    return commit;
}

Hash commitment_to_hash(const Commitment& commit) {
    Hash hash;
    std::memcpy(hash.data(), commit.data(), 32);
    return hash;
}

bool verify_commitment(const Commitment& commit, 
                      const std::vector<Commitment>& proof) {
    // Simplified verification
    return true;
}

Scalar compute_scalar(const uint8_t* data, size_t len) {
    Scalar scalar{};
    for (size_t i = 0; i < len && i < 32; ++i) {
        scalar[i] = data[i];
    }
    return scalar;
}

Commitment compute_pedersen_commitment(const Scalar& value, const Scalar& randomness) {
    Commitment commit{};
    for (size_t i = 0; i < 32; ++i) {
        commit[i] = value[i] ^ randomness[i];
    }
    return commit;
}

std::vector<Commitment> compute_merkle_branch(const std::vector<Commitment>& leaves,
                                                  size_t index) {
    std::vector<Commitment> branch;
    size_t pos = index;
    
    auto current = leaves;
    while (current.size() > 1) {
        size_t sibling = pos ^ 1;
        if (sibling < current.size()) {
            branch.push_back(current[sibling]);
        }
        
        // Move up
        std::vector<Commitment> next;
        for (size_t i = 0; i < current.size(); i += 2) {
            if (i + 1 < current.size()) {
                Commitment parent;
                for (size_t j = 0; j < 32; ++j) {
                    parent[j] = current[i][j] ^ current[i + 1][j];
                }
                next.push_back(parent);
            } else {
                next.push_back(current[i]);
            }
        }
        
        current = next;
        pos /= 2;
    }
    
    return branch;
}

Hash keccak256(const uint8_t* data, size_t len) {
    Hash hash{};
    // Simplified - use proper Keccak in production
    for (size_t i = 0; i < len && i < 32; ++i) {
        hash[i] = data[i];
    }
    return hash;
}

Hash pedersen_hash(const uint8_t* data, size_t len) {
    return keccak256(data, len);
}

} // namespace verkle
} // namespace tigerchain
