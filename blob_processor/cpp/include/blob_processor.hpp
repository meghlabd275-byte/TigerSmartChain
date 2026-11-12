/**
 * @file blob_processor.hpp
 * @brief Blob transaction processing entry point
 *
 * Parses EIP-4844 blob transactions, checks their blob commitments and
 * proofs, prices blob gas, and answers the point-evaluation precompile. Every
 * verdict comes from a real KZG check; nothing here reports success by
 * default.
 */

#ifndef BLOB_PROCESSOR_HPP
#define BLOB_PROCESSOR_HPP

#include <cstdint>
#include <optional>
#include <string>
#include <vector>

#include "blob_types.hpp"
#include "kzg.hpp"

namespace tigerchain {
namespace blob {

/// Result of a blob transaction check, with the reason for a rejection so a
/// caller can log or report it.
struct BlobVerification {
    bool valid = false;
    /// Empty when valid; otherwise why it was rejected.
    std::string reason;
};

/// Blob gas pricing for a block.
struct BlobGasInfo {
    uint64_t excess_blob_gas = 0;
    uint256_t blob_gas_price;
    uint64_t blob_gas_used = 0;
    uint64_t target_blob_gas = TARGET_BLOBS_PER_BLOCK * BLOB_GAS_PER_BLOB;
    uint64_t max_blob_gas = MAX_BLOBS_PER_BLOCK * BLOB_GAS_PER_BLOB;

    /// Price the given number of blobs.
    uint256_t fee_for(size_t num_blobs) const;
};

/**
 * @class BlobProcessor
 * @brief Parses and verifies EIP-4844 blob transactions
 *
 * Holds no mutable per-transaction state, so one instance can be shared and
 * used concurrently. The KZG context it uses is process-wide.
 */
class BlobProcessor {
public:
    BlobProcessor();

    /// Parse a blob transaction from its canonical RLP encoding. Returns
    /// nothing when the encoding is malformed or the field counts disagree.
    std::optional<BlobTransaction> parse_transaction(const std::vector<uint8_t>& rlp) const;

    /// Verify a transaction's blobs, commitments, proofs and versioned
    /// hashes.
    BlobVerification verify_transaction(const BlobTransaction& tx) const;

    /// Verify a single blob against its commitment and proof.
    BlobVerification verify_blob(const Blob& blob, const Bytes48& commitment) const;

    /// Blob gas a transaction consumes.
    uint64_t calculate_blob_gas(const BlobTransaction& tx) const;

    /// Price for a block with the given excess blob gas.
    BlobGasInfo calculate_blob_gas_price(uint64_t excess_blob_gas) const;

    /// Run the point-evaluation precompile on raw input.
    std::optional<std::vector<uint8_t>> evaluate_point(
        const std::vector<uint8_t>& input) const;

    /// True when the KZG context loaded, and so verification can be trusted.
    bool is_ready() const;
};

} // namespace blob
} // namespace tigerchain

#endif // BLOB_PROCESSOR_HPP
