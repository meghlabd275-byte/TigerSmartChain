/**
 * @file blob_types.hpp
 * @brief EIP-4844 blob transaction types
 *
 * The data model follows the consensus spec:
 *
 *  - A blob is 4096 canonical scalar-field elements (128 KiB), not "a
 *    commitment plus some data". Commitments and proofs are separate objects
 *    that accompany the blob, because they are what a verifier checks the blob
 *    against.
 *  - A blob transaction carries a *list* of blobs, one commitment per blob, and
 *    the versioned hashes that the execution layer sees. The blob data itself
 *    never reaches the execution layer.
 *  - Gas is counted in blob gas: 131072 per blob, priced by the
 *    `fake_exponential` schedule from EIP-4844 rather than a linear function.
 */

#ifndef BLOB_TYPES_HPP
#define BLOB_TYPES_HPP

#include <array>
#include <cstdint>
#include <optional>
#include <string>
#include <vector>

#include "field_element.hpp"
#include "kzg.hpp"
#include "uint256.hpp"

namespace tigerchain {
namespace blob {

// =============================================================================
// Constants
// =============================================================================

/// Blob gas consumed by one blob.
constexpr uint64_t BLOB_GAS_PER_BLOB = 131072;
/// Target number of blobs per block, which the pricing schedule aims at.
constexpr uint64_t TARGET_BLOBS_PER_BLOCK = 3;
/// Maximum number of blobs in a block.
constexpr uint64_t MAX_BLOBS_PER_BLOCK = 6;
/// Minimum blob gas price, in wei.
constexpr uint64_t MIN_BLOB_GASPRICE = 1;
/// Controls how fast the blob base fee responds to demand.
constexpr uint64_t BLOB_GASPRICE_UPDATE_FRACTION = 3338477;

using VersionedHash = std::array<uint8_t, 32>;
using Address = std::array<uint8_t, 20>;

// =============================================================================
// Enumerations
// =============================================================================

enum class BlobStatus : uint8_t {
    PENDING = 0,
    INCLUDED = 1,
    FINALIZED = 2,
    REORGED = 3,
    EXPIRED = 4
};

// =============================================================================
// Blob
// =============================================================================

/// A blob with its accompanying commitment and proof.
struct Blob {
    /// The 4096 field elements. Zero-initialised: an indeterminate 128 KiB
    /// buffer would be read as field elements by the commitment routine.
    BlobData data{};
    /// The KZG commitment to the blob's polynomial.
    Bytes48 commitment{};
    /// A KZG proof, when one has been computed.
    std::optional<Bytes48> proof;

    BlobStatus status = BlobStatus::PENDING;
    uint64_t block_number = 0;
    uint64_t timestamp = 0;

    /// True when every field element is canonical and the commitment matches
    /// the data. Verifying the commitment costs a pairing, so callers that
    /// have already checked it can use `blob_is_canonical` instead.
    bool is_valid() const;

    /// The versioned hash the execution layer will see.
    VersionedHash versioned_hash() const;
};

// =============================================================================
// Blob transaction
// =============================================================================

/// An EIP-4844 blob transaction (type 3).
struct BlobTransaction {
    uint64_t chain_id = 0;
    uint64_t nonce = 0;
    uint64_t gas_limit = 0;
    uint64_t max_priority_fee_per_gas = 0;
    uint64_t max_fee_per_gas = 0;
    Address from{};
    /// Empty for a contract-creating transaction.
    std::optional<Address> to;
    /// The calldata. For a blob-carrying transaction this is usually empty.
    std::vector<uint8_t> input_data;
    uint256_t value;
    std::vector<uint8_t> access_list_rlp;

    /// The blob gas price the sender is willing to pay.
    uint256_t max_fee_per_blob_gas;
    /// One versioned hash per blob, in order. This is what goes on chain.
    std::vector<VersionedHash> blob_versioned_hashes;

    /// Signature. `y_parity` is 0 or 1 for a type-3 transaction; the legacy
    /// 27/28 encoding does not apply.
    uint256_t r;
    uint256_t s;
    uint8_t y_parity = 0;

    /// The blobs themselves, and the commitments and proofs that accompany
    /// them. These travel in the network wrapper, not in the consensus
    /// transaction, but a verifier needs them to check the transaction.
    std::vector<Blob> blobs;
    std::vector<Bytes48> commitments;
    std::vector<Bytes48> proofs;

    /// Blob gas this transaction consumes.
    uint64_t blob_gas() const;
    /// Blob gas multiplied by the price the sender offered.
    uint256_t blob_fee() const;
    /// Total gas, execution plus blob.
    uint64_t total_gas() const;

    /// True when the counts agree, each versioned hash matches its commitment,
    /// and every blob, commitment and proof is valid.
    bool verify() const;

    /// The transaction hash: Keccak-256 of the canonical RLP encoding.
    std::string compute_hash() const;

    /// Canonical RLP encoding, including the type byte.
    std::vector<uint8_t> encode() const;

    /// Parse a canonical RLP encoding. Returns nothing on malformed input.
    static std::optional<BlobTransaction> decode(const std::vector<uint8_t>& data);
};

// =============================================================================
// Blob gas pricing
// =============================================================================

/// EIP-4844's `fake_exponential(factor, numerator, denominator)`: a smooth
/// approximation of `factor * e ** (numerator / denominator)` built from
/// integer arithmetic, so every client computes exactly the same fee.
uint256_t fake_exponential(uint64_t factor, uint64_t numerator, uint64_t denominator);

/// Excess blob gas after a block, from EIP-4844:
/// `max(parent_excess + parent_blob_gas_used - target, 0)`.
uint64_t compute_excess_blob_gas(uint64_t parent_excess_blob_gas,
                                 uint64_t parent_blob_gas_used);

/// The blob gas price implied by an excess: `fake_exponential(1, excess,
/// BLOB_GASPRICE_UPDATE_FRACTION)`.
uint256_t compute_blob_gas_price(uint64_t excess_blob_gas);

// =============================================================================
// Point evaluation precompile
// =============================================================================

/// Input to the EIP-4844 point-evaluation precompile at address 0x0a.
struct PointEvaluationInput {
    VersionedHash versioned_hash;
    Bytes32 z;
    Bytes32 y;
    Bytes48 commitment;
    Bytes48 proof;

    /// Decode the precompile's 192-byte input. Returns nothing when the length
    /// is wrong.
    static std::optional<PointEvaluationInput> decode(const std::vector<uint8_t>& input);

    /// Encode to 192 bytes.
    std::vector<uint8_t> encode() const;
};

/// The precompile's fixed gas cost.
constexpr uint64_t POINT_EVALUATION_GAS = 50000;
/// The precompile's success output: the field-elements-per-blob constant,
/// followed by the BLS modulus, both 32 bytes.
extern const std::array<uint8_t, 64> POINT_EVALUATION_OUTPUT;

/// Run the point-evaluation precompile. Returns the 64-byte output on success,
/// or nothing when the versioned hash does not match the commitment or the
/// proof does not verify. A failure here is a precompile failure, so the
/// caller must consume all gas and revert.
std::optional<std::vector<uint8_t>> evaluate_point(const PointEvaluationInput& input);

// =============================================================================
// Serialisation helpers
// =============================================================================

std::string blob_status_to_string(BlobStatus status);
BlobStatus string_to_blob_status(const std::string& text);

/// Canonical RLP helpers shared by the encoders.
std::vector<uint8_t> rlp_encode_bytes(const std::vector<uint8_t>& data);
std::vector<uint8_t> rlp_encode_list(const std::vector<std::vector<uint8_t>>& items);

/// Decode one RLP item. On success `consumed` receives the number of bytes
/// read. Returns false when the encoding is not well formed, which includes
/// non-canonical length forms.
bool rlp_decode_bytes(const uint8_t* data, size_t length, std::vector<uint8_t>& out,
                      size_t& consumed);

/// Decode an RLP list into its items. Returns false on a malformed encoding.
bool rlp_decode_list(const std::vector<uint8_t>& data,
                     std::vector<std::vector<uint8_t>>& out);

} // namespace blob
} // namespace tigerchain

#endif // BLOB_TYPES_HPP
