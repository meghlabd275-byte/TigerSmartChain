/**
 * @file blob_processor.cpp
 * @brief EIP-4844 blob transaction processing
 *
 * Contains the pieces the consensus rules define exactly:
 *
 *  - canonical RLP, with the strictness the spec requires: non-minimal length
 *    forms and leading zero bytes are rejected, so two encodings can never
 *    decode to the same transaction and change its hash;
 *  - `fake_exponential`, the integer fee curve, evaluated with
 *    arbitrary-precision intermediates so it agrees with every other client;
 *  - the point-evaluation precompile, which accepts a proof only when the
 *    pairing check passes and the versioned hash matches the commitment.
 */

#include "blob_processor.hpp"

#include <algorithm>
#include <cstring>

#include "crypto_utils.hpp"
#include "kzg.hpp"

namespace tigerchain {
namespace blob {

namespace {

// =============================================================================
// Minimal big integers
// =============================================================================
//
// fake_exponential multiplies its accumulator by the numerator on every
// iteration, so intermediate values can exceed 64 bits even when the final fee
// fits in 256. Limbs are base 2^32, least significant first.

using BigInt = std::vector<uint32_t>;

bool big_is_zero(const BigInt& value) {
    for (uint32_t limb : value) {
        if (limb != 0) return false;
    }
    return true;
}

void big_trim(BigInt& value) {
    while (!value.empty() && value.back() == 0) {
        value.pop_back();
    }
}

void big_mul_small(BigInt& value, uint64_t factor) {
    uint64_t carry = 0;
    for (uint32_t& limb : value) {
        const uint64_t product = static_cast<uint64_t>(limb) * factor + carry;
        limb = static_cast<uint32_t>(product & 0xffffffffu);
        carry = product >> 32;
    }
    while (carry != 0) {
        value.push_back(static_cast<uint32_t>(carry & 0xffffffffu));
        carry >>= 32;
    }
}

uint64_t big_div_small(BigInt& value, uint64_t divisor) {
    uint64_t remainder = 0;
    for (size_t i = value.size(); i-- > 0;) {
        const uint64_t current = (remainder << 32) | value[i];
        value[i] = static_cast<uint32_t>(current / divisor);
        remainder = current % divisor;
    }
    big_trim(value);
    return remainder;
}

void big_add(BigInt& target, const BigInt& addend) {
    const size_t size = std::max(target.size(), addend.size());
    target.resize(size, 0);
    uint64_t carry = 0;
    for (size_t i = 0; i < size; ++i) {
        const uint64_t sum = static_cast<uint64_t>(target[i]) +
                             (i < addend.size() ? addend[i] : 0) + carry;
        target[i] = static_cast<uint32_t>(sum & 0xffffffffu);
        carry = sum >> 32;
    }
    if (carry != 0) {
        target.push_back(static_cast<uint32_t>(carry));
    }
}

std::array<uint8_t, 32> all_ones() {
    std::array<uint8_t, 32> max_bytes;
    max_bytes.fill(0xff);
    return max_bytes;
}

/// Convert to 256 bits, saturating at the maximum when the value is larger. A
/// saturated fee is unpayable, which is the safe direction: it cannot
/// undercharge for blob space.
uint256_t big_to_uint256_saturating(const BigInt& value) {
    if (value.size() > 8) {
        return uint256_t(all_ones());
    }
    std::array<uint8_t, 32> bytes{};
    for (size_t limb = 0; limb < value.size(); ++limb) {
        for (int byte = 0; byte < 4; ++byte) {
            bytes[31 - (limb * 4 + byte)] =
                static_cast<uint8_t>((value[limb] >> (8 * byte)) & 0xff);
        }
    }
    return uint256_t(bytes);
}

/// Multiply a 256-bit value by a 64-bit value, saturating at 2^256 - 1.
uint256_t multiply_saturating(const uint256_t& value, uint64_t factor) {
    BigInt product(1, 0);
    product[0] = static_cast<uint32_t>(factor & 0xffffffffu);
    if ((factor >> 32) != 0) {
        product.push_back(static_cast<uint32_t>(factor >> 32));
    }
    const auto& bytes = value.bytes();
    BigInt result(1, 0);
    for (size_t limb = 0; limb < 8; ++limb) {
        uint32_t value_limb = 0;
        for (int byte = 0; byte < 4; ++byte) {
            value_limb = (value_limb << 8) | bytes[28 - limb * 4 + byte];
        }
        if (value_limb == 0) {
            continue;
        }
        BigInt shifted(product.size() + limb, 0);
        std::copy(product.begin(), product.end(), shifted.begin() + limb);
        big_mul_small(shifted, value_limb);
        big_add(result, shifted);
    }
    return big_to_uint256_saturating(result);
}

// =============================================================================
// RLP
// =============================================================================

std::vector<uint8_t> to_minimal_big_endian(uint64_t value) {
    std::vector<uint8_t> out;
    while (value != 0) {
        out.insert(out.begin(), static_cast<uint8_t>(value & 0xff));
        value >>= 8;
    }
    return out;
}

std::vector<uint8_t> to_minimal_big_endian(const uint256_t& value) {
    const auto& bytes = value.bytes();
    size_t first = 0;
    while (first < bytes.size() && bytes[first] == 0) {
        ++first;
    }
    return std::vector<uint8_t>(bytes.begin() + first, bytes.end());
}

std::vector<uint8_t> rlp_length_header(size_t length, uint8_t short_offset,
                                       uint8_t long_offset) {
    if (length <= 55) {
        return {static_cast<uint8_t>(short_offset + length)};
    }
    const auto length_bytes = to_minimal_big_endian(static_cast<uint64_t>(length));
    std::vector<uint8_t> header;
    header.push_back(static_cast<uint8_t>(long_offset + length_bytes.size()));
    header.insert(header.end(), length_bytes.begin(), length_bytes.end());
    return header;
}

std::vector<uint8_t> rlp_encode_byte_string(const std::vector<uint8_t>& data) {
    if (data.size() == 1 && data[0] < 0x80) {
        return data;
    }
    auto out = rlp_length_header(data.size(), 0x80, 0xb7);
    out.insert(out.end(), data.begin(), data.end());
    return out;
}

/// Decode one RLP item, reporting how many bytes it used and whether it was a
/// list. Rejects every non-canonical form.
bool rlp_decode_item(const uint8_t* data, size_t length, std::vector<uint8_t>& payload,
                     bool& is_list, size_t& consumed) {
    if (length == 0) {
        return false;
    }
    const uint8_t prefix = data[0];
    if (prefix < 0x80) {
        payload.assign(1, prefix);
        is_list = false;
        consumed = 1;
        return true;
    }
    if (prefix <= 0xb7) {
        const size_t payload_length = prefix - 0x80;
        if (length < 1 + payload_length) {
            return false;
        }
        payload.assign(data + 1, data + 1 + payload_length);
        // A single byte below 0x80 encodes as itself; using a length prefix
        // would give that byte two encodings.
        if (payload_length == 1 && payload[0] < 0x80) {
            return false;
        }
        is_list = false;
        consumed = 1 + payload_length;
        return true;
    }
    if (prefix <= 0xbf) {
        const size_t length_of_length = prefix - 0xb7;
        if (length_of_length > 8 || length < 1 + length_of_length) {
            return false;
        }
        uint64_t payload_length = 0;
        for (size_t i = 0; i < length_of_length; ++i) {
            payload_length = (payload_length << 8) | data[1 + i];
        }
        // The long form must be used only when it is needed, and the length
        // must not carry a leading zero byte.
        if (data[1] == 0 || payload_length <= 55) {
            return false;
        }
        if (length < 1 + length_of_length + payload_length) {
            return false;
        }
        payload.assign(data + 1 + length_of_length,
                       data + 1 + length_of_length + payload_length);
        is_list = false;
        consumed = 1 + length_of_length + payload_length;
        return true;
    }
    if (prefix <= 0xf7) {
        const size_t payload_length = prefix - 0xc0;
        if (length < 1 + payload_length) {
            return false;
        }
        payload.assign(data + 1, data + 1 + payload_length);
        is_list = true;
        consumed = 1 + payload_length;
        return true;
    }
    const size_t length_of_length = prefix - 0xf7;
    if (length_of_length > 8 || length < 1 + length_of_length) {
        return false;
    }
    uint64_t payload_length = 0;
    for (size_t i = 0; i < length_of_length; ++i) {
        payload_length = (payload_length << 8) | data[1 + i];
    }
    if (data[1] == 0 || payload_length <= 55) {
        return false;
    }
    if (length < 1 + length_of_length + payload_length) {
        return false;
    }
    payload.assign(data + 1 + length_of_length,
                   data + 1 + length_of_length + payload_length);
    is_list = true;
    consumed = 1 + length_of_length + payload_length;
    return true;
}

/// Split an RLP list payload into its items, each stored in canonical encoded
/// form so it can be unwrapped again.
bool split_list_payload(const std::vector<uint8_t>& payload,
                        std::vector<std::vector<uint8_t>>& out) {
    out.clear();
    size_t offset = 0;
    while (offset < payload.size()) {
        std::vector<uint8_t> item;
        bool item_is_list = false;
        size_t item_consumed = 0;
        if (!rlp_decode_item(payload.data() + offset, payload.size() - offset, item,
                             item_is_list, item_consumed)) {
            return false;
        }
        if (item_is_list) {
            out.emplace_back(payload.begin() + offset,
                             payload.begin() + offset + item_consumed);
        } else {
            out.push_back(rlp_encode_byte_string(item));
        }
        offset += item_consumed;
    }
    return true;
}

/// Interpret an RLP byte string as a big-endian integer. A leading zero byte is
/// rejected, since it would be a second encoding of the same value.
bool decode_uint256(const std::vector<uint8_t>& bytes, uint256_t& out) {
    if (bytes.size() > 32 || (!bytes.empty() && bytes[0] == 0)) {
        return false;
    }
    out = uint256_t::from_bytes(bytes.data(), bytes.size());
    return true;
}

bool decode_uint64(const std::vector<uint8_t>& bytes, uint64_t& out) {
    if (bytes.size() > 8 || (!bytes.empty() && bytes[0] == 0)) {
        return false;
    }
    uint64_t value = 0;
    for (uint8_t byte : bytes) {
        value = (value << 8) | byte;
    }
    out = value;
    return true;
}

std::vector<uint8_t> encode_uint256(const uint256_t& value) {
    return rlp_encode_byte_string(to_minimal_big_endian(value));
}

std::vector<uint8_t> encode_uint64(uint64_t value) {
    return rlp_encode_byte_string(to_minimal_big_endian(value));
}

/// The precompile's fixed success output: the field-elements-per-blob count and
/// the BLS modulus, each as a 32-byte big-endian word.
std::array<uint8_t, 64> make_point_evaluation_output() {
    std::array<uint8_t, 64> out{};
    out[30] = 0x10;  // FIELD_ELEMENTS_PER_BLOB = 4096 = 0x1000
    out[31] = 0x00;
    std::memcpy(out.data() + 32, kBLS12_381ScalarModulus.data(), 32);
    return out;
}

} // namespace

const std::array<uint8_t, 64> POINT_EVALUATION_OUTPUT = make_point_evaluation_output();

// =============================================================================
// RLP public helpers
// =============================================================================

std::vector<uint8_t> rlp_encode_bytes(const std::vector<uint8_t>& data) {
    return rlp_encode_byte_string(data);
}

std::vector<uint8_t> rlp_encode_list(const std::vector<std::vector<uint8_t>>& items) {
    std::vector<uint8_t> payload;
    for (const auto& item : items) {
        payload.insert(payload.end(), item.begin(), item.end());
    }
    auto out = rlp_length_header(payload.size(), 0xc0, 0xf7);
    out.insert(out.end(), payload.begin(), payload.end());
    return out;
}

bool rlp_decode_bytes(const uint8_t* data, size_t length, std::vector<uint8_t>& out,
                      size_t& consumed) {
    bool is_list = false;
    if (!rlp_decode_item(data, length, out, is_list, consumed)) {
        return false;
    }
    return !is_list;
}

bool rlp_decode_list(const std::vector<uint8_t>& data,
                     std::vector<std::vector<uint8_t>>& out) {
    bool is_list = false;
    size_t consumed = 0;
    std::vector<uint8_t> payload;
    if (!rlp_decode_item(data.data(), data.size(), payload, is_list, consumed)) {
        return false;
    }
    if (!is_list || consumed != data.size()) {
        return false;
    }
    return split_list_payload(payload, out);
}

// =============================================================================
// Blob
// =============================================================================

bool Blob::is_valid() const {
    if (!blob_is_canonical(data)) {
        return false;
    }
    const auto& context = kzg_context();
    if (!context.is_ready()) {
        return false;
    }
    if (proof.has_value()) {
        return context.verify_blob_proof(data, commitment, *proof);
    }
    const auto expected = context.blob_to_commitment(data);
    return expected.has_value() && *expected == commitment;
}

VersionedHash Blob::versioned_hash() const {
    return compute_versioned_hash(commitment);
}

// =============================================================================
// Blob gas pricing
// =============================================================================

uint256_t fake_exponential(uint64_t factor, uint64_t numerator, uint64_t denominator) {
    if (denominator == 0) {
        return uint256_t(0);
    }
    // output = sum over i >= 1 of factor*denominator * (numerator/denominator)^i / i!
    // Each term comes from the previous one by multiplying by `numerator` and
    // dividing by `denominator * i`, which keeps every intermediate exact.
    BigInt numerator_accum(1, 0);
    numerator_accum[0] = static_cast<uint32_t>(factor & 0xffffffffu);
    if ((factor >> 32) != 0) {
        numerator_accum.push_back(static_cast<uint32_t>(factor >> 32));
    }
    big_mul_small(numerator_accum, denominator);

    BigInt output(1, 0);
    uint64_t i = 1;
    while (!big_is_zero(numerator_accum)) {
        big_add(output, numerator_accum);
        big_mul_small(numerator_accum, numerator);
        const uint64_t divisor = denominator * i;
        if (divisor == 0) {
            // The divisor has wrapped, so the series is far past any
            // representable value; saturate instead of looping forever.
            return uint256_t(all_ones());
        }
        big_div_small(numerator_accum, divisor);
        ++i;
    }

    if (big_is_zero(output)) {
        return uint256_t(0);
    }
    big_div_small(output, denominator);
    return big_to_uint256_saturating(output);
}

uint64_t compute_excess_blob_gas(uint64_t parent_excess_blob_gas,
                                 uint64_t parent_blob_gas_used) {
    const uint64_t target = TARGET_BLOBS_PER_BLOCK * BLOB_GAS_PER_BLOB;
    if (parent_blob_gas_used < target) {
        return parent_excess_blob_gas;
    }
    return parent_excess_blob_gas + (parent_blob_gas_used - target);
}

uint256_t compute_blob_gas_price(uint64_t excess_blob_gas) {
    return fake_exponential(1, excess_blob_gas, BLOB_GASPRICE_UPDATE_FRACTION);
}

uint256_t BlobGasInfo::fee_for(size_t num_blobs) const {
    return multiply_saturating(blob_gas_price,
                              static_cast<uint64_t>(num_blobs) * BLOB_GAS_PER_BLOB);
}

// =============================================================================
// BlobTransaction
// =============================================================================

uint64_t BlobTransaction::blob_gas() const {
    return static_cast<uint64_t>(blob_versioned_hashes.size()) * BLOB_GAS_PER_BLOB;
}

uint256_t BlobTransaction::blob_fee() const {
    return multiply_saturating(max_fee_per_blob_gas, blob_gas());
}

uint64_t BlobTransaction::total_gas() const {
    return gas_limit + blob_gas();
}

bool BlobTransaction::verify() const {
    const size_t blob_count = blob_versioned_hashes.size();
    if (blob_count == 0 || blob_count > MAX_BLOBS_PER_BLOCK) {
        return false;
    }
    // The blobs travel beside the transaction, so all three lists must line up
    // with the on-chain versioned hashes.
    if (blobs.size() != blob_count || commitments.size() != blob_count ||
        proofs.size() != blob_count) {
        return false;
    }

    std::vector<BlobData> blob_data;
    blob_data.reserve(blob_count);
    for (size_t i = 0; i < blob_count; ++i) {
        if (!is_valid_versioned_hash(blob_versioned_hashes[i])) {
            return false;
        }
        if (compute_versioned_hash(commitments[i]) != blob_versioned_hashes[i]) {
            return false;
        }
        blob_data.push_back(blobs[i].data);
    }

    const auto& context = kzg_context();
    if (!context.is_ready()) {
        return false;
    }
    // One multi-pairing covers every blob in the transaction.
    return context.verify_blob_proof_batch(blob_data, commitments, proofs);
}

std::vector<uint8_t> BlobTransaction::encode() const {
    std::vector<uint8_t> to_bytes;
    if (to.has_value()) {
        to_bytes.assign(to->begin(), to->end());
    }

    std::vector<std::vector<uint8_t>> versioned_hashes;
    versioned_hashes.reserve(blob_versioned_hashes.size());
    for (const auto& hash : blob_versioned_hashes) {
        versioned_hashes.push_back(
            rlp_encode_byte_string(std::vector<uint8_t>(hash.begin(), hash.end())));
    }

    const std::vector<std::vector<uint8_t>> fields = {
        encode_uint64(chain_id),
        encode_uint64(nonce),
        encode_uint256(uint256_t(max_priority_fee_per_gas)),
        encode_uint256(uint256_t(max_fee_per_gas)),
        encode_uint64(gas_limit),
        rlp_encode_byte_string(to_bytes),
        encode_uint256(value),
        rlp_encode_byte_string(input_data),
        access_list_rlp.empty() ? rlp_encode_list({}) : access_list_rlp,
        encode_uint256(max_fee_per_blob_gas),
        rlp_encode_list(versioned_hashes),
        encode_uint64(y_parity),
        encode_uint256(r),
        encode_uint256(s),
    };

    auto out = rlp_encode_list(fields);
    out.insert(out.begin(), 0x03);  // EIP-2718 typed transaction envelope
    return out;
}

std::string BlobTransaction::compute_hash() const {
    return crypto::bytes_to_hex(crypto::keccak256(encode()));
}

std::optional<BlobTransaction> BlobTransaction::decode(const std::vector<uint8_t>& data) {
    if (data.size() < 2 || data[0] != 0x03) {
        return std::nullopt;
    }
    std::vector<std::vector<uint8_t>> fields;
    if (!rlp_decode_list(std::vector<uint8_t>(data.begin() + 1, data.end()), fields)) {
        return std::nullopt;
    }
    if (fields.size() != 14) {
        return std::nullopt;
    }

    // Each field is stored in canonical encoded form, so unwrap it back to the
    // raw value before interpreting.
    const auto unwrap = [](const std::vector<uint8_t>& encoded)
        -> std::optional<std::vector<uint8_t>> {
        std::vector<uint8_t> value;
        size_t consumed = 0;
        if (!rlp_decode_bytes(encoded.data(), encoded.size(), value, consumed) ||
            consumed != encoded.size()) {
            return std::nullopt;
        }
        return value;
    };

    BlobTransaction tx;
    std::vector<uint8_t> field;

    if (auto got = unwrap(fields[0])) { if (!decode_uint64(*got, tx.chain_id)) return std::nullopt; } else return std::nullopt;
    if (auto got = unwrap(fields[1])) { if (!decode_uint64(*got, tx.nonce)) return std::nullopt; } else return std::nullopt;
    if (auto got = unwrap(fields[2])) {
        uint256_t v;
        if (!decode_uint256(*got, v) || !v.fits_u64()) return std::nullopt;
        tx.max_priority_fee_per_gas = v.low64();
    } else return std::nullopt;
    if (auto got = unwrap(fields[3])) {
        uint256_t v;
        if (!decode_uint256(*got, v) || !v.fits_u64()) return std::nullopt;
        tx.max_fee_per_gas = v.low64();
    } else return std::nullopt;
    if (auto got = unwrap(fields[4])) { if (!decode_uint64(*got, tx.gas_limit)) return std::nullopt; } else return std::nullopt;

    if (auto got = unwrap(fields[5])) {
        if (!got->empty()) {
            if (got->size() != 20) return std::nullopt;
            Address address{};
            std::copy(got->begin(), got->end(), address.begin());
            tx.to = address;
        }
    } else return std::nullopt;

    if (auto got = unwrap(fields[6])) { if (!decode_uint256(*got, tx.value)) return std::nullopt; } else return std::nullopt;
    if (auto got = unwrap(fields[7])) { tx.input_data = *got; } else return std::nullopt;

    tx.access_list_rlp = fields[8];

    if (auto got = unwrap(fields[9])) {
        if (!decode_uint256(*got, tx.max_fee_per_blob_gas)) return std::nullopt;
    } else return std::nullopt;

    {
        std::vector<std::vector<uint8_t>> hashes;
        if (!rlp_decode_list(fields[10], hashes)) {
            return std::nullopt;
        }
        for (const auto& encoded : hashes) {
            auto value = unwrap(encoded);
            if (!value || value->size() != 32) {
                return std::nullopt;
            }
            VersionedHash hash{};
            std::copy(value->begin(), value->end(), hash.begin());
            tx.blob_versioned_hashes.push_back(hash);
        }
    }

    if (auto got = unwrap(fields[11])) {
        uint64_t y_parity = 0;
        if (!decode_uint64(*got, y_parity) || y_parity > 1) return std::nullopt;
        tx.y_parity = static_cast<uint8_t>(y_parity);
    } else return std::nullopt;
    if (auto got = unwrap(fields[12])) { if (!decode_uint256(*got, tx.r)) return std::nullopt; } else return std::nullopt;
    if (auto got = unwrap(fields[13])) { if (!decode_uint256(*got, tx.s)) return std::nullopt; } else return std::nullopt;

    return tx;
}

// =============================================================================
// Point evaluation precompile
// =============================================================================

std::optional<PointEvaluationInput> PointEvaluationInput::decode(
    const std::vector<uint8_t>& input) {
    // versioned hash (32) + z (32) + y (32) + commitment (48) + proof (48)
    if (input.size() != 192) {
        return std::nullopt;
    }
    PointEvaluationInput out;
    std::copy(input.begin(), input.begin() + 32, out.versioned_hash.begin());
    std::copy(input.begin() + 32, input.begin() + 64, out.z.begin());
    std::copy(input.begin() + 64, input.begin() + 96, out.y.begin());
    std::copy(input.begin() + 96, input.begin() + 144, out.commitment.begin());
    std::copy(input.begin() + 144, input.begin() + 192, out.proof.begin());
    return out;
}

std::vector<uint8_t> PointEvaluationInput::encode() const {
    std::vector<uint8_t> out;
    out.reserve(192);
    out.insert(out.end(), versioned_hash.begin(), versioned_hash.end());
    out.insert(out.end(), z.begin(), z.end());
    out.insert(out.end(), y.begin(), y.end());
    out.insert(out.end(), commitment.begin(), commitment.end());
    out.insert(out.end(), proof.begin(), proof.end());
    return out;
}

std::optional<std::vector<uint8_t>> evaluate_point(const PointEvaluationInput& input) {
    // The versioned hash must come from this commitment, otherwise the caller
    // is proving a statement about a different blob.
    if (compute_versioned_hash(input.commitment) != input.versioned_hash) {
        return std::nullopt;
    }
    const auto& context = kzg_context();
    if (!context.is_ready()) {
        return std::nullopt;
    }
    if (!context.verify_proof(input.commitment, input.z, input.y, input.proof)) {
        return std::nullopt;
    }
    return std::vector<uint8_t>(POINT_EVALUATION_OUTPUT.begin(),
                                POINT_EVALUATION_OUTPUT.end());
}

// =============================================================================
// Status helpers
// =============================================================================

std::string blob_status_to_string(BlobStatus status) {
    switch (status) {
        case BlobStatus::PENDING: return "pending";
        case BlobStatus::INCLUDED: return "included";
        case BlobStatus::FINALIZED: return "finalized";
        case BlobStatus::REORGED: return "reorged";
        case BlobStatus::EXPIRED: return "expired";
    }
    return "unknown";
}

BlobStatus string_to_blob_status(const std::string& text) {
    if (text == "pending") return BlobStatus::PENDING;
    if (text == "included") return BlobStatus::INCLUDED;
    if (text == "finalized") return BlobStatus::FINALIZED;
    if (text == "reorged") return BlobStatus::REORGED;
    if (text == "expired") return BlobStatus::EXPIRED;
    return BlobStatus::PENDING;
}

// =============================================================================
// BlobProcessor
// =============================================================================

BlobProcessor::BlobProcessor() = default;

std::optional<BlobTransaction> BlobProcessor::parse_transaction(
    const std::vector<uint8_t>& rlp) const {
    return BlobTransaction::decode(rlp);
}

BlobVerification BlobProcessor::verify_transaction(const BlobTransaction& tx) const {
    if (tx.blob_versioned_hashes.empty()) {
        return {false, "transaction carries no blobs"};
    }
    if (tx.blob_versioned_hashes.size() > MAX_BLOBS_PER_BLOCK) {
        return {false, "more blobs than the block limit allows"};
    }
    if (tx.blobs.size() != tx.blob_versioned_hashes.size() ||
        tx.commitments.size() != tx.blob_versioned_hashes.size() ||
        tx.proofs.size() != tx.blob_versioned_hashes.size()) {
        return {false, "blob, commitment and proof counts disagree"};
    }
    const auto& context = kzg_context();
    if (!context.is_ready()) {
        return {false, "KZG trusted setup is not loaded"};
    }
    for (size_t i = 0; i < tx.blob_versioned_hashes.size(); ++i) {
        if (!is_valid_versioned_hash(tx.blob_versioned_hashes[i])) {
            return {false, "versioned hash has the wrong version byte"};
        }
        if (compute_versioned_hash(tx.commitments[i]) != tx.blob_versioned_hashes[i]) {
            return {false, "versioned hash does not match its commitment"};
        }
    }
    std::vector<BlobData> blobs;
    blobs.reserve(tx.blobs.size());
    for (const auto& blob : tx.blobs) {
        blobs.push_back(blob.data);
    }
    if (!context.verify_blob_proof_batch(blobs, tx.commitments, tx.proofs)) {
        return {false, "a blob commitment proof did not verify"};
    }
    return {true, ""};
}

BlobVerification BlobProcessor::verify_blob(const Blob& blob,
                                            const Bytes48& commitment) const {
    if (!blob_is_canonical(blob.data)) {
        return {false, "blob contains a non-canonical field element"};
    }
    const auto& context = kzg_context();
    if (!context.is_ready()) {
        return {false, "KZG trusted setup is not loaded"};
    }
    if (blob.proof.has_value()) {
        if (!context.verify_blob_proof(blob.data, commitment, *blob.proof)) {
            return {false, "blob proof did not verify"};
        }
        return {true, ""};
    }
    const auto expected = context.blob_to_commitment(blob.data);
    if (!expected.has_value()) {
        return {false, "could not compute the blob commitment"};
    }
    if (*expected != commitment) {
        return {false, "commitment does not match the blob"};
    }
    return {true, ""};
}

uint64_t BlobProcessor::calculate_blob_gas(const BlobTransaction& tx) const {
    return tx.blob_gas();
}

BlobGasInfo BlobProcessor::calculate_blob_gas_price(uint64_t excess_blob_gas) const {
    BlobGasInfo info;
    info.excess_blob_gas = excess_blob_gas;
    info.blob_gas_price = compute_blob_gas_price(excess_blob_gas);
    return info;
}

std::optional<std::vector<uint8_t>> BlobProcessor::evaluate_point(
    const std::vector<uint8_t>& input) const {
    const auto decoded = PointEvaluationInput::decode(input);
    if (!decoded.has_value()) {
        return std::nullopt;
    }
    return blob::evaluate_point(*decoded);
}

bool BlobProcessor::is_ready() const {
    return kzg_context().is_ready();
}

} // namespace blob
} // namespace tigerchain
