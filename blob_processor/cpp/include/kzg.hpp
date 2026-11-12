/**
 * @file kzg.hpp
 * @brief EIP-4844 KZG commitments and proofs
 *
 * Wraps the vendored c-kzg-4844 / blst pair so the rest of the component sees
 * plain byte arrays instead of C structs. All verification goes through the
 * real BLS12-381 pairing; there is no path that reports success without
 * checking the proof.
 *
 * The trusted setup is embedded in the binary (see `trusted_setup_data.hpp`),
 * so a node cannot be started against a substituted setup.
 */

#ifndef TIGERCHAIN_KZG_HPP
#define TIGERCHAIN_KZG_HPP

#include <array>
#include <cstdint>
#include <optional>
#include <string>
#include <vector>

#include "field_element.hpp"

namespace tigerchain {
namespace blob {

/// Number of field elements in a blob.
constexpr size_t FIELD_ELEMENTS_PER_BLOB = 4096;
/// Number of bytes in a blob.
constexpr size_t BLOB_SIZE = FIELD_ELEMENTS_PER_BLOB * BYTES_PER_FIELD_ELEMENT;
/// Number of bytes in a compressed G1 point, which is what a commitment and a
/// proof both are.
constexpr size_t KZG_COMMITMENT_SIZE = 48;
/// Number of bytes in a compressed G1 point used as a proof.
constexpr size_t KZG_PROOF_SIZE = 48;

/// A blob's 128 KiB of field element data.
using BlobData = std::array<uint8_t, BLOB_SIZE>;
/// A compressed BLS12-381 G1 point.
using Bytes48 = std::array<uint8_t, KZG_COMMITMENT_SIZE>;
/// A 32-byte value, used for the evaluation point and result.
using Bytes32 = std::array<uint8_t, BYTES_PER_FIELD_ELEMENT>;

/// A loaded trusted setup. Loading parses and precomputes the ceremony data,
/// which is expensive, so one instance is created per process and shared.
class KzgContext {
public:
    /// Load the embedded mainnet trusted setup. Construction performs the
    /// precomputation; prefer one long-lived instance over constructing per
    /// verification.
    KzgContext();
    ~KzgContext();

    KzgContext(const KzgContext&) = delete;
    KzgContext& operator=(const KzgContext&) = delete;

    /// True when the setup loaded successfully. A context that failed to load
    /// must not be used; every method then fails rather than returning a
    /// misleading verdict.
    bool is_ready() const { return ready_; }

    /// Why loading failed, for logging.
    const std::string& error() const { return error_; }

    /// The commitment to a blob's polynomial. Returns nothing if the blob
    /// contains a non-canonical field element, which is invalid input.
    std::optional<Bytes48> blob_to_commitment(const BlobData& blob) const;

    /// The KZG proof for the evaluation of a blob's polynomial at `z`, along
    /// with the claimed value `y`.
    std::optional<std::pair<Bytes48, Bytes32>> compute_proof(
        const BlobData& blob, const Bytes32& z) const;

    /// Verify a point-evaluation proof: that the polynomial committed to by
    /// `commitment` evaluates to `y` at `z`, witnessed by `proof`. This is the
    /// relation the EIP-4844 point-evaluation precompile checks.
    bool verify_proof(const Bytes48& commitment, const Bytes32& z,
                      const Bytes32& y, const Bytes48& proof) const;

    /// Verify a blob's commitment and proof directly against the blob data.
    bool verify_blob_proof(const BlobData& blob, const Bytes48& commitment,
                           const Bytes48& proof) const;

    /// Verify many blobs at once. One pairing check covers all of them, which
    /// is markedly cheaper than verifying each blob separately.
    bool verify_blob_proof_batch(const std::vector<BlobData>& blobs,
                                 const std::vector<Bytes48>& commitments,
                                 const std::vector<Bytes48>& proofs) const;

    /// Compute the proof for a whole blob against its commitment.
    std::optional<Bytes48> compute_blob_proof(const BlobData& blob,
                                              const Bytes48& commitment) const;

private:
    /// Opaque so blst and c-kzg types stay out of this header.
    struct Impl;
    Impl* impl_;
    bool ready_;
    std::string error_;
};

/// The process-wide KZG context, constructed on first use.
const KzgContext& kzg_context();

/// A versioned hash is `version || sha256(commitment)[1..32]`, with the first
/// byte replaced by the version. EIP-4844 fixes the version at 0x01.
constexpr uint8_t KZG_COMMITMENT_VERSION = 0x01;

/// Compute the versioned hash for a commitment. Uses SHA-256, not Keccak-256:
/// EIP-4844 specifies SHA-256 here, and using the wrong hash would produce
/// hashes that no other client recognises.
std::array<uint8_t, 32> compute_versioned_hash(const Bytes48& commitment,
                                               uint8_t version = KZG_COMMITMENT_VERSION);

/// True when a versioned hash has the EIP-4844 version byte.
bool is_valid_versioned_hash(const std::array<uint8_t, 32>& hash);

/// SHA-256, exposed because the versioned hash needs it.
std::array<uint8_t, 32> sha256(const uint8_t* data, size_t length);

/// True when every 32-byte field element of a blob is canonical. A blob that
/// fails this is invalid and must be rejected before any commitment work.
bool blob_is_canonical(const BlobData& blob);

} // namespace blob
} // namespace tigerchain

#endif // TIGERCHAIN_KZG_HPP
