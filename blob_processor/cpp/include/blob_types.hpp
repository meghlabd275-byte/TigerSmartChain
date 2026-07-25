/**
 * @file blob_types.hpp
 * @brief EIP-4844 Blob Transaction Types and Structures
 * @author TigerScan Team
 * @version 1.0.0
 * 
 * Complete EIP-4844 implementation with:
 * - Blob transaction parsing
 * - Blob gas calculation
 * - KZG commitments
 * - Point evaluation precompile
 */

#ifndef BLOB_TYPES_HPP
#define BLOB_TYPES_HPP

#include <cstdint>
#include <vector>
#include <string>
#include <array>
#include <optional>
#include <memory>
#include <variant>

namespace tigerchain {
namespace blob {

// =============================================================================
// Constants
// =============================================================================

constexpr size_t BLOB_SIZE = 4096 * 32;  // 128 KB per blob
constexpr size_t BLOB_FIELD_ELEMENTS = 4096;
constexpr size_t BYTES_PER_FIELD_ELEMENT = 32;
constexpr size_t KZG_COMMITMENT_SIZE = 48;
constexpr size_t KZG_PROOF_SIZE = 48;
constexpr size_t MAX_BLOBS_PER_BLOCK = 6;
constexpr uint64_t BLOB_GAS_PER_BLOB = 131072;

// Blob version constants
constexpr uint8_t BLOB_VERSION = 0;

// Gas calculations
constexpr uint64_t MIN_BLOB_GASPRICE = 1;
constexpr uint64_t BLOB_GASPRICE_UPDATE_FRACTION = 3338477;

// =============================================================================
// Type Definitions
// =============================================================================

using BlobBytes = std::array<uint8_t, BLOB_SIZE>;
using FieldElement = std::array<uint8_t, BYTES_PER_FIELD_ELEMENT>;
using KZGCommitment = std::array<uint8_t, KZG_COMMITMENT_SIZE>;
using KZGProof = std::array<uint8_t, KZG_PROOF_SIZE>;
using VersionedHash = std::array<uint8_t, 32>;
using Address = std::array<uint8_t, 20>;

// =============================================================================
// Blob Transaction Types
// =============================================================================

/**
 * @enum BlobTransactionType
 * @brief Types of blob transactions
 */
enum class BlobTransactionType : uint8_t {
    DEFAULT = 0,
    BLOB = 1,
    BLOB_TX_TYPE = 2  // EIP-4844
};

/**
 * @enum BlobStatus
 * @brief Status of a blob in the network
 */
enum class BlobStatus : uint8_t {
    PENDING = 0,
    INCLUDED = 1,
    FINALIZED = 2,
    REORGED = 3,
    EXPIRED = 4
};

/**
 * @struct FieldElement
 * @brief 32-byte field element for blob data
 */
struct FieldElement {
    std::array<uint8_t, BYTES_PER_FIELD_ELEMENT> value;
    
    FieldElement() : value{} {}
    explicit FieldElement(const std::vector<uint8_t>& data);
    
    std::string to_hex() const;
    static FieldElement from_hex(const std::string& hex);
    bool is_zero() const;
    bool is_one() const;
    
    // Field arithmetic
    FieldElement operator+(const FieldElement& other) const;
    FieldElement operator-(const FieldElement& other) const;
    FieldElement operator*(const FieldElement& other) const;
    bool operator==(const FieldElement& other) const;
};

/**
 * @struct Blob
 * @brief Full blob data structure
 */
struct Blob {
    std::vector<FieldElement> commitments;
    std::vector<FieldElement> data;
    BlobStatus status;
    uint64_t block_number;
    uint64_t timestamp;
    VersionedHash versioned_hash;
    KZGCommitment commitment;
    KZGProof proof;
    
    Blob();
    explicit Blob(const std::vector<uint8_t>& compressed_data);
    
    bool is_valid() const;
    std::vector<uint8_t> serialize() const;
    static Blob deserialize(const std::vector<uint8_t>& data);
    
    VersionedHash compute_versioned_hash() const;
};

/**
 * @struct BlobTransaction
 * @brief EIP-4844 blob transaction structure
 */
struct BlobTransaction {
    // Common transaction fields
    uint64_t chain_id;
    uint64_t nonce;
    uint64_t gas_limit;
    uint64_t gas_price;
    uint64_t max_priority_fee_per_gas;
    uint64_t max_fee_per_gas;
    Address from;
    std::optional<Address> to;
    std::vector<uint8_t> input_data;
    std::vector<uint8_t> value;
    uint8_t v;
    std::vector<uint8_t> r;
    std::vector<uint8_t> s;
    
    // EIP-4844 specific fields
    uint64_t max_fee_per_blob_gas;
    std::vector<VersionedHash> blob_versioned_hashes;
    std::vector<Blob> blobs;
    std::vector<KZGProof> proofs;
    std::vector<KZGCommitment> commitments;
    
    // Metadata
    std::string hash;
    std::string block_hash;
    uint64_t block_number;
    uint64_t transaction_index;
    uint64_t timestamp;
    bool is_executed;
    bool is_verified;
    
    BlobTransaction();
    
    uint64_t get_blob_gas() const;
    uint64_t get_total_gas() const;
    bool verify_blobs() const;
    std::string compute_hash() const;
    
    // Serialization
    std::vector<uint8_t> encode() const;
    static BlobTransaction decode(const std::vector<uint8_t>& data);
};

/**
 * @struct BlobBlock
 * @brief Block containing blob transactions
 */
struct BlobBlock {
    uint64_t block_number;
    std::string block_hash;
    std::string parent_hash;
    uint64_t timestamp;
    std::vector<BlobTransaction> blob_txs;
    std::vector<Blob> blobs;
    std::vector<VersionedHash> excess_blob_gas;
    uint64_t blob_gas_used;
    uint64_t gas_used;
    Address miner;
    std::string difficulty;
    std::string total_difficulty;
    
    BlobBlock();
    
    bool has_blobs() const { return !blobs.empty(); }
    size_t blob_count() const { return blobs.size(); }
    uint64_t get_blob_gas_used() const;
};

/**
 * @struct BlobTransactionReceipt
 * @brief Receipt for blob transactions
 */
struct BlobTransactionReceipt {
    std::string transaction_hash;
    std::string block_hash;
    uint64_t block_number;
    uint64_t transaction_index;
    Address contract_address;
    uint64_t cumulative_gas_used;
    uint64_t gas_used;
    std::vector<Log> logs;
    std::vector<uint8_t> logs_bloom;
    uint8_t status;
    std::string type;
    
    // EIP-4844 specific
    std::vector<VersionedHash> blob_versioned_hashes;
    uint64_t blob_gas_used;
    uint64_t blob_gas_price;
    
    BlobTransactionReceipt();
};

/**
 * @struct PointEvaluationPrecompile
 * @brief Point evaluation precompile (EIP-4844)
 */
struct PointEvaluationPrecompile {
    // Input
    VersionedHash versioned_hash;
    uint256_t z;
    uint256_t y;
    KZGCommitment commitment;
    KZGProof proof;
    
    // Output
    bool success;
    std::string error_message;
    uint64_t gas_used;
    
    PointEvaluationPrecompile();
    std::vector<uint8_t> encode_input() const;
    std::vector<uint8_t> execute() const;
    static PointEvaluationPrecompile decode_output(const std::vector<uint8_t>& output);
};

/**
 * @struct BlobGasInfo
 * @brief Blob gas pricing information
 */
struct BlobGasInfo {
    uint64_t blob_gas_price;
    uint64_t excess_blob_gas;
    uint64_t blob_base_fee;
    uint64_t min_blob_gas_price;
    uint64_t max_blob_gas_price;
    uint64_t target_blobs_per_block;
    uint64_t max_blobs_per_block;
    
    BlobGasInfo();
    
    static BlobGasInfo calculate(
        uint64_t excess_blob_gas,
        uint64_t timestamp,
        uint64_t parent_excess_blob_gas,
        uint64_t parent_timestamp
    );
    
    uint64_t calculate_blob_fee(size_t num_blobs) const;
};

/**
 * @struct BlobIndexEntry
 * @brief Indexed blob for fast querying
 */
struct BlobIndexEntry {
    VersionedHash versioned_hash;
    std::string transaction_hash;
    uint64_t block_number;
    uint64_t blob_index;
    BlobStatus status;
    uint64_t timestamp;
    std::string blob_data_cid;  // IPFS CID
    std::vector<KZGCommitment> commitments;
    
    BlobIndexEntry();
};

/**
 * @struct BlobQueryResult
 * @brief Query result for blobs
 */
struct BlobQueryResult {
    std::vector<BlobIndexEntry> blobs;
    uint64_t total_count;
    uint64_t page;
    uint64_t page_size;
    bool has_next;
};

/**
 * @class BlobProcessor
 * @brief High-performance blob transaction processor
 */
class BlobProcessor {
public:
    BlobProcessor();
    ~BlobProcessor();
    
    // Core processing
    std::optional<BlobTransaction> parse_blob_transaction(
        const std::vector<uint8_t>& rlp_data
    );
    
    std::optional<Blob> parse_blob(const std::vector<uint8_t>& blob_data);
    
    bool verify_blob(const Blob& blob);
    bool verify_kzg_proof(
        const Blob& blob,
        const KZGCommitment& commitment,
        const KZGProof& proof
    );
    
    // Gas calculations
    uint64_t calculate_blob_gas(const BlobTransaction& tx);
    BlobGasInfo calculate_blob_gas_price(
        uint64_t excess_blob_gas,
        uint64_t timestamp
    );
    
    // Point evaluation precompile
    PointEvaluationPrecompile evaluate_point(
        const VersionedHash& versioned_hash,
        const uint256_t& z,
        const uint256_t& y,
        const KZGCommitment& commitment,
        const KZGProof& proof
    );
    
    // Blob verification
    bool verify_versioned_hash(
        const VersionedHash& hash,
        const KZGCommitment& commitment
    );
    
    // Serialization
    std::vector<uint8_t> encode_blob_transaction(
        const BlobTransaction& tx
    );
    
private:
    // Internal state
    std::vector<Blob> blob_cache_;
    uint64_t excess_blob_gas_;
    uint64_t last_update_timestamp_;
    
    // KZG utilities
    KZGCommitment compute_kzg_commitment(const Blob& blob);
    KZGProof compute_kzg_proof(const Blob& blob, uint256_t z);
    bool verify_kzg_commitment_proof(
        const Blob& blob,
        const KZGCommitment& commitment,
        const KZGProof& proof,
        uint256_t z,
        uint256_t y
    );
    
    // Field element operations
    FieldElement bls12_381_mul(const FieldElement& a, const FieldElement& b);
    FieldElement bls12_381_add(const FieldElement& a, const FieldElement& b);
    FieldElement bls12_381_sub(const FieldElement& a, const FieldElement& b);
    FieldElement bls12_381_neg(const FieldElement& a);
    bool bls12_381_equal(const FieldElement& a, const FieldElement& b);
    
    // Compression
    std::vector<uint8_t> compress_blob(const Blob& blob);
    Blob decompress_blob(const std::vector<uint8_t>& compressed);
};

// =============================================================================
// Utility Functions
// =============================================================================

std::string blob_status_to_string(BlobStatus status);
BlobStatus string_to_blob_status(const std::string& str);

bool is_valid_versioned_hash(const VersionedHash& hash);
VersionedHash compute_versioned_hash(
    const KZGCommitment& commitment,
    uint8_t version
);

uint64_t compute_excess_blob_gas(
    uint64_t parent_excess_blob_gas,
    uint64_t parent_blob_gas_used
);

// =============================================================================
// Serialization
// =============================================================================

std::vector<uint8_t> serialize_blob(const Blob& blob);
Blob deserialize_blob(const std::vector<uint8_t>& data);

std::vector<uint8_t> serialize_blob_transaction(const BlobTransaction& tx);
BlobTransaction deserialize_blob_transaction(const std::vector<uint8_t>& data);

std::string blob_to_json(const Blob& blob);
std::string blob_transaction_to_json(const BlobTransaction& tx);
std::string blob_block_to_json(const BlobBlock& block);

} // namespace blob
} // namespace tigerchain

#endif // BLOB_TYPES_HPP
