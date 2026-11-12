/**
 * @file field_element.cpp
 * @brief BLS12-381 scalar field elements
 *
 * Arithmetic goes through blst so it matches every other client. The one
 * hand-written piece is the canonicality comparison, which is a single
 * big-endian compare against the modulus.
 */

#include "field_element.hpp"

#include <cstring>

#include "blst.h"

namespace tigerchain {
namespace blob {

// r = 0x73eda753299d7d483339d80809a1d80553bda402fffe5bfeffffffff00000001
const std::array<uint8_t, BYTES_PER_FIELD_ELEMENT> kBLS12_381ScalarModulus = {
    0x73, 0xed, 0xa7, 0x53, 0x29, 0x9d, 0x7d, 0x48,
    0x33, 0x39, 0xd8, 0x08, 0x09, 0xa1, 0xd8, 0x05,
    0x53, 0xbd, 0xa4, 0x02, 0xff, 0xfe, 0x5b, 0xfe,
    0xff, 0xff, 0xff, 0xff, 0x00, 0x00, 0x00, 0x01,
};

namespace {

/// Load a big-endian element into blst's internal form.
void to_blst(blst_fr& out, const std::array<uint8_t, BYTES_PER_FIELD_ELEMENT>& bytes) {
    blst_scalar scalar;
    blst_scalar_from_bendian(&scalar, bytes.data());
    blst_fr_from_scalar(&out, &scalar);
}

/// Store blst's internal form back as big-endian bytes.
void from_blst(std::array<uint8_t, BYTES_PER_FIELD_ELEMENT>& out, const blst_fr& value) {
    blst_scalar scalar;
    blst_scalar_from_fr(&scalar, &value);
    blst_bendian_from_scalar(out.data(), &scalar);
}

} // namespace

FieldElement::FieldElement() : bytes_{} {}

FieldElement::FieldElement(const std::array<uint8_t, BYTES_PER_FIELD_ELEMENT>& big_endian)
    : bytes_(big_endian) {}

std::optional<FieldElement> FieldElement::from_bytes_be(const uint8_t* data, size_t length) {
    if (length > BYTES_PER_FIELD_ELEMENT) {
        return std::nullopt;
    }
    FieldElement out;
    std::memcpy(out.bytes_.data() + (BYTES_PER_FIELD_ELEMENT - length), data, length);
    return out;
}

std::optional<FieldElement> FieldElement::from_hex(const std::string& hex) {
    std::string digits = hex;
    if (digits.size() >= 2 && digits[0] == '0' && (digits[1] == 'x' || digits[1] == 'X')) {
        digits = digits.substr(2);
    }
    if (digits.empty() || digits.size() > 64) {
        return std::nullopt;
    }
    if (digits.size() % 2 != 0) {
        digits.insert(digits.begin(), '0');
    }
    FieldElement out;
    const size_t offset = BYTES_PER_FIELD_ELEMENT - digits.size() / 2;
    for (size_t i = 0; i < digits.size(); i += 2) {
        const auto nibble = [](char c) -> int {
            if (c >= '0' && c <= '9') return c - '0';
            if (c >= 'a' && c <= 'f') return c - 'a' + 10;
            if (c >= 'A' && c <= 'F') return c - 'A' + 10;
            return -1;
        };
        const int high = nibble(digits[i]);
        const int low = nibble(digits[i + 1]);
        if (high < 0 || low < 0) {
            return std::nullopt;
        }
        out.bytes_[offset + i / 2] = static_cast<uint8_t>((high << 4) | low);
    }
    return out;
}

std::string FieldElement::to_hex() const {
    static const char* digits = "0123456789abcdef";
    std::string out = "0x";
    out.reserve(66);
    for (uint8_t byte : bytes_) {
        out.push_back(digits[(byte >> 4) & 0x0f]);
        out.push_back(digits[byte & 0x0f]);
    }
    return out;
}

bool FieldElement::is_zero() const {
    for (uint8_t byte : bytes_) {
        if (byte != 0) return false;
    }
    return true;
}

bool FieldElement::is_one() const {
    for (size_t i = 0; i < BYTES_PER_FIELD_ELEMENT - 1; ++i) {
        if (bytes_[i] != 0) return false;
    }
    return bytes_[BYTES_PER_FIELD_ELEMENT - 1] == 1;
}

bool FieldElement::is_canonical() const {
    return is_valid_encoding(bytes_);
}

bool FieldElement::is_valid_encoding(
    const std::array<uint8_t, BYTES_PER_FIELD_ELEMENT>& bytes) {
    // Big-endian compare against the modulus: equal or greater is not a
    // canonical scalar.
    for (size_t i = 0; i < BYTES_PER_FIELD_ELEMENT; ++i) {
        if (bytes[i] < kBLS12_381ScalarModulus[i]) return true;
        if (bytes[i] > kBLS12_381ScalarModulus[i]) return false;
    }
    return false;
}

FieldElement FieldElement::operator+(const FieldElement& other) const {
    blst_fr a, b, sum;
    to_blst(a, bytes_);
    to_blst(b, other.bytes_);
    blst_fr_add(&sum, &a, &b);
    FieldElement out;
    from_blst(out.bytes_, sum);
    return out;
}

FieldElement FieldElement::operator-(const FieldElement& other) const {
    blst_fr a, b, difference;
    to_blst(a, bytes_);
    to_blst(b, other.bytes_);
    blst_fr_sub(&difference, &a, &b);
    FieldElement out;
    from_blst(out.bytes_, difference);
    return out;
}

FieldElement FieldElement::operator*(const FieldElement& other) const {
    blst_fr a, b, product;
    to_blst(a, bytes_);
    to_blst(b, other.bytes_);
    blst_fr_mul(&product, &a, &b);
    FieldElement out;
    from_blst(out.bytes_, product);
    return out;
}

FieldElement FieldElement::operator-() const {
    blst_fr a, negated;
    to_blst(a, bytes_);
    blst_fr_cneg(&negated, &a, true);
    FieldElement out;
    from_blst(out.bytes_, negated);
    return out;
}

std::optional<FieldElement> FieldElement::inverse(const FieldElement& value) {
    if (value.is_zero()) {
        return std::nullopt;
    }
    blst_fr a, inverted;
    to_blst(a, value.bytes_);
    blst_fr_inverse(&inverted, &a);
    FieldElement out;
    from_blst(out.bytes_, inverted);
    return out;
}

} // namespace blob
} // namespace tigerchain
