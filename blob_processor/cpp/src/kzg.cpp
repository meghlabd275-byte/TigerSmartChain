/**
 * @file kzg.cpp
 * @brief EIP-4844 KZG verification over BLS12-381
 *
 * Bridges to the vendored c-kzg-4844 and blst libraries. The trusted setup is
 * compiled in, so the context cannot be pointed at a different setup at
 * runtime.
 *
 * `verify_proof` is the only place a point-evaluation proof is accepted, and
 * it returns whatever the pairing check says. There is deliberately no
 * fallback that reports success when the setup is missing: an unloaded
 * context fails closed, and `is_ready()` lets a caller notice before relying
 * on any verdict.
 */

#include "kzg.hpp"

#include <cstring>
#include <mutex>
#include <utility>

#include "trusted_setup_data.hpp"

extern "C" {
#include "ckzg.h"
#include "blst_aux.h"
}

namespace tigerchain {
namespace blob {

namespace {

// c-kzg declares Bytes32, Bytes48 and Blob at global scope; this component has
// its own types with those names, so the qualified :: form is used throughout.

/// Reinterpret a 48-byte array as c-kzg's G1 point.
const ::Bytes48* as_g1(const std::array<uint8_t, KZG_COMMITMENT_SIZE>& value) {
    static_assert(sizeof(::Bytes48) == KZG_COMMITMENT_SIZE, "G1 point size mismatch");
    return reinterpret_cast<const ::Bytes48*>(value.data());
}

/// Reinterpret a 32-byte array as c-kzg's scalar bytes.
const ::Bytes32* as_scalar(const std::array<uint8_t, BYTES_PER_FIELD_ELEMENT>& value) {
    static_assert(sizeof(::Bytes32) == BYTES_PER_FIELD_ELEMENT, "field element size mismatch");
    return reinterpret_cast<const ::Bytes32*>(value.data());
}

/// Reinterpret a blob as c-kzg's Blob.
const ::Blob* as_ckzg_blob(const BlobData& data) {
    static_assert(sizeof(::Blob) == BLOB_SIZE, "blob size mismatch");
    return reinterpret_cast<const ::Blob*>(data.data());
}

} // namespace

struct KzgContext::Impl {
    KZGSettings settings{};
    bool loaded = false;

    ~Impl() {
        if (loaded) {
            free_trusted_setup(&settings);
        }
    }
};

KzgContext::KzgContext() : impl_(new Impl()), ready_(false) {
    const auto status = load_trusted_setup(
        &impl_->settings,
        detail::kG1Monomial, detail::kG1Monomial_size,
        detail::kG1Lagrange, detail::kG1Lagrange_size,
        detail::kG2Monomial, detail::kG2Monomial_size,
        /*precompute=*/0);
    if (status != C_KZG_OK) {
        error_ = "failed to load the embedded EIP-4844 trusted setup";
        return;
    }
    impl_->loaded = true;
    ready_ = true;
}

KzgContext::~KzgContext() {
    delete impl_;
}

bool blob_is_canonical(const BlobData& blob) {
    for (size_t element = 0; element < FIELD_ELEMENTS_PER_BLOB; ++element) {
        std::array<uint8_t, BYTES_PER_FIELD_ELEMENT> bytes{};
        std::memcpy(bytes.data(), blob.data() + element * BYTES_PER_FIELD_ELEMENT,
                    BYTES_PER_FIELD_ELEMENT);
        if (!FieldElement::is_valid_encoding(bytes)) {
            return false;
        }
    }
    return true;
}

std::optional<Bytes48> KzgContext::blob_to_commitment(const BlobData& blob) const {
    if (!ready_ || !blob_is_canonical(blob)) {
        return std::nullopt;
    }
    Bytes48 commitment{};
    if (blob_to_kzg_commitment(reinterpret_cast<::KZGCommitment*>(commitment.data()),
                               as_ckzg_blob(blob), &impl_->settings) != C_KZG_OK) {
        return std::nullopt;
    }
    return commitment;
}

std::optional<std::pair<Bytes48, Bytes32>> KzgContext::compute_proof(
    const BlobData& blob, const Bytes32& z) const {
    if (!ready_ || !blob_is_canonical(blob)) {
        return std::nullopt;
    }
    Bytes48 proof{};
    Bytes32 y{};
    if (compute_kzg_proof(reinterpret_cast<::KZGProof*>(proof.data()),
                          reinterpret_cast<::Bytes32*>(y.data()),
                          as_ckzg_blob(blob), as_scalar(z),
                          &impl_->settings) != C_KZG_OK) {
        return std::nullopt;
    }
    return std::make_pair(proof, y);
}

bool KzgContext::verify_proof(const Bytes48& commitment, const Bytes32& z,
                              const Bytes32& y, const Bytes48& proof) const {
    if (!ready_) {
        return false;
    }
    bool ok = false;
    if (verify_kzg_proof(&ok, as_g1(commitment), as_scalar(z), as_scalar(y),
                         as_g1(proof), &impl_->settings) != C_KZG_OK) {
        return false;
    }
    return ok;
}

bool KzgContext::verify_blob_proof(const BlobData& blob, const Bytes48& commitment,
                                   const Bytes48& proof) const {
    if (!ready_ || !blob_is_canonical(blob)) {
        return false;
    }
    bool ok = false;
    if (verify_blob_kzg_proof(&ok, as_ckzg_blob(blob), as_g1(commitment),
                              as_g1(proof), &impl_->settings) != C_KZG_OK) {
        return false;
    }
    return ok;
}

bool KzgContext::verify_blob_proof_batch(const std::vector<BlobData>& blobs,
                                         const std::vector<Bytes48>& commitments,
                                         const std::vector<Bytes48>& proofs) const {
    if (!ready_ || blobs.empty()) {
        return false;
    }
    if (blobs.size() != commitments.size() || blobs.size() != proofs.size()) {
        return false;
    }
    for (const auto& blob : blobs) {
        if (!blob_is_canonical(blob)) {
            return false;
        }
    }
    // The three arrays are contiguous and identically laid out to c-kzg's, so
    // one call can check them all with a single multi-pairing.
    static_assert(sizeof(::Blob) == BLOB_SIZE, "blob size mismatch");
    static_assert(sizeof(::Bytes48) == KZG_COMMITMENT_SIZE, "G1 point size mismatch");
    bool ok = false;
    if (verify_blob_kzg_proof_batch(
            &ok, reinterpret_cast<const ::Blob*>(blobs.data()),
            reinterpret_cast<const ::Bytes48*>(commitments.data()),
            reinterpret_cast<const ::Bytes48*>(proofs.data()),
            blobs.size(), &impl_->settings) != C_KZG_OK) {
        return false;
    }
    return ok;
}

std::optional<Bytes48> KzgContext::compute_blob_proof(const BlobData& blob,
                                                      const Bytes48& commitment) const {
    if (!ready_ || !blob_is_canonical(blob)) {
        return std::nullopt;
    }
    Bytes48 proof{};
    if (compute_blob_kzg_proof(reinterpret_cast<::KZGProof*>(proof.data()),
                               as_ckzg_blob(blob), as_g1(commitment),
                               &impl_->settings) != C_KZG_OK) {
        return std::nullopt;
    }
    return proof;
}

const KzgContext& kzg_context() {
    // Constructed once; loading the setup is expensive and the context is
    // immutable afterwards, so sharing it is safe.
    static const KzgContext context;
    return context;
}

std::array<uint8_t, 32> sha256(const uint8_t* data, size_t length) {
    std::array<uint8_t, 32> out{};
    blst_sha256(out.data(), data, length);
    return out;
}

std::array<uint8_t, 32> compute_versioned_hash(const Bytes48& commitment,
                                               uint8_t version) {
    // EIP-4844: version byte, then sha256(commitment) with its first byte
    // replaced by the version.
    const auto digest = sha256(commitment.data(), commitment.size());
    std::array<uint8_t, 32> hash{};
    std::memcpy(hash.data(), digest.data(), 32);
    hash[0] = version;
    return hash;
}

bool is_valid_versioned_hash(const std::array<uint8_t, 32>& hash) {
    return hash[0] == KZG_COMMITMENT_VERSION;
}

} // namespace blob
} // namespace tigerchain
