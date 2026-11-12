/**
 * @file field_element.hpp
 * @brief A scalar of the BLS12-381 scalar field, as used by EIP-4844 blobs
 *
 * A blob is 4096 of these, each held as 32 big-endian bytes. Values must be
 * strictly less than the field modulus
 *
 *   r = 0x73eda753299d7d483339d80809a1d80553bda402fffe5bfeffffffff00000001
 *
 * and the EIP-4844 consensus rules reject a blob that contains a
 * non-canonical value, so `is_canonical()` has to be checked before a blob is
 * accepted. Arithmetic is delegated to blst rather than done here: a
 * hand-written modular multiply is easy to get subtly wrong and the resulting
 * commitment would disagree with every other client.
 */

#ifndef TIGERCHAIN_FIELD_ELEMENT_HPP
#define TIGERCHAIN_FIELD_ELEMENT_HPP

#include <array>
#include <cstdint>
#include <optional>
#include <string>

namespace tigerchain {
namespace blob {

/// Number of bytes in the big-endian representation of a field element.
constexpr size_t BYTES_PER_FIELD_ELEMENT = 32;

/// An element of the BLS12-381 scalar field, stored big-endian.
class FieldElement {
public:
    /// The zero element.
    FieldElement();

    /// Build from exactly 32 big-endian bytes. Does not require the value to
    /// be canonical; use `is_canonical()` to check.
    explicit FieldElement(const std::array<uint8_t, BYTES_PER_FIELD_ELEMENT>& big_endian);

    /// Build from at most 32 big-endian bytes, left-padded with zeros.
    /// Returns nothing if the input is longer than 32 bytes.
    static std::optional<FieldElement> from_bytes_be(const uint8_t* data, size_t length);

    /// Parse `0x`-prefixed or bare hex, up to 64 digits. Returns nothing on a
    /// malformed string rather than treating it as zero.
    static std::optional<FieldElement> from_hex(const std::string& hex);

    /// The 32 big-endian bytes.
    const std::array<uint8_t, BYTES_PER_FIELD_ELEMENT>& bytes() const { return bytes_; }

    /// `0x`-prefixed, always 64 digits.
    std::string to_hex() const;

    bool is_zero() const;
    bool is_one() const;

    /// True when the value is less than the field modulus, which is what the
    /// EIP-4844 blob validity rules require of every element.
    bool is_canonical() const;

    FieldElement operator+(const FieldElement& other) const;
    FieldElement operator-(const FieldElement& other) const;
    FieldElement operator*(const FieldElement& other) const;
    FieldElement operator-() const;

    bool operator==(const FieldElement& other) const { return bytes_ == other.bytes_; }
    bool operator!=(const FieldElement& other) const { return bytes_ != other.bytes_; }

    /// Multiplicative inverse. The zero element has none, so this returns
    /// nothing for zero instead of producing a bogus value.
    static std::optional<FieldElement> inverse(const FieldElement& value);

    /// True when the value is a valid, canonical field element.
    static bool is_valid_encoding(const std::array<uint8_t, BYTES_PER_FIELD_ELEMENT>& bytes);

private:
    std::array<uint8_t, BYTES_PER_FIELD_ELEMENT> bytes_;
};

/// The field modulus r, big-endian.
extern const std::array<uint8_t, BYTES_PER_FIELD_ELEMENT> kBLS12_381ScalarModulus;

} // namespace blob
} // namespace tigerchain

#endif // TIGERCHAIN_FIELD_ELEMENT_HPP
