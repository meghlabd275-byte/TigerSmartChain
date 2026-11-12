/**
 * @file uint256.hpp
 * @brief Unsigned 256-bit integer for the EVM's native word size
 *
 * The EVM works in 256-bit words, so a 256-bit type is needed for blob
 * evaluation points, RLP-encoded quantities and gas fields. A fixed-size
 * `std::array<uint8_t, 32>` cannot express these directly because the two
 * byte orders matter: JSON-RPC and the ABI are big-endian, the EVM's own
 * memory layout is little-endian.
 *
 * Values are stored big-endian, which is the form the ABI and RLP use, so
 * encoding needs no reversal.
 */

#ifndef TIGERCHAIN_UINT256_HPP
#define TIGERCHAIN_UINT256_HPP

#include <array>
#include <cstdint>
#include <string>
#include <optional>
#include <stdexcept>

namespace tigerchain {

/// An unsigned 256-bit integer, stored big-endian in `words`.
class uint256 {
public:
    /// All-zero value.
    uint256() : words_{} {}

    /// Build from a 64-bit value.
    uint256(uint64_t value) : words_{} {
        for (int i = 0; i < 8; ++i) {
            words_[31 - i] = static_cast<uint8_t>((value >> (8 * i)) & 0xff);
        }
    }

    /// Build from exactly 32 big-endian bytes. A different length is a
    /// programming error, so it throws rather than silently truncating or
    /// zero-extending a caller's data.
    explicit uint256(const std::array<uint8_t, 32>& big_endian) : words_(big_endian) {}

    /// Build from a byte range that is at most 32 bytes, treated as
    /// big-endian and left-padded with zeros. Longer input throws.
    static uint256 from_bytes(const uint8_t* data, size_t length) {
        if (length > 32) {
            throw std::invalid_argument("uint256: more than 32 bytes");
        }
        uint256 out;
        for (size_t i = 0; i < length; ++i) {
            out.words_[32 - length + i] = data[i];
        }
        return out;
    }

    /// Parse a `0x`-prefixed or bare hexadecimal string. Returns nothing when
    /// the string is empty, over 64 digits, or contains a non-hex character,
    /// so a malformed input can never be mistaken for a value.
    static std::optional<uint256> from_hex(const std::string& text) {
        std::string digits = text;
        if (digits.size() >= 2 && digits[0] == '0' && (digits[1] == 'x' || digits[1] == 'X')) {
            digits = digits.substr(2);
        }
        if (digits.empty() || digits.size() > 64) {
            return std::nullopt;
        }
        // Pad to an even number of digits so the string splits into whole bytes.
        if (digits.size() % 2 != 0) {
            digits.insert(digits.begin(), '0');
        }
        uint256 out;
        size_t byte_index = 32 - digits.size() / 2;
        for (size_t i = 0; i < digits.size(); i += 2) {
            int high = hex_nibble(digits[i]);
            int low = hex_nibble(digits[i + 1]);
            if (high < 0 || low < 0) {
                return std::nullopt;
            }
            out.words_[byte_index++] = static_cast<uint8_t>((high << 4) | low);
        }
        return out;
    }

    /// The 32 big-endian bytes.
    const std::array<uint8_t, 32>& bytes() const { return words_; }

    /// True when every byte is zero.
    bool is_zero() const {
        for (uint8_t byte : words_) {
            if (byte != 0) return false;
        }
        return true;
    }

    /// True when the value is exactly one.
    bool is_one() const {
        for (size_t i = 0; i < 31; ++i) {
            if (words_[i] != 0) return false;
        }
        return words_[31] == 1;
    }

    /// The low 64 bits, for lengths and gas counts that fit.
    uint64_t low64() const {
        uint64_t value = 0;
        for (size_t i = 24; i < 32; ++i) {
            value = (value << 8) | words_[i];
        }
        return value;
    }

    /// True when the value fits in 64 bits, so `low64` loses nothing.
    bool fits_u64() const {
        for (size_t i = 0; i < 24; ++i) {
            if (words_[i] != 0) return false;
        }
        return true;
    }

    /// Big-endian hex with a `0x` prefix and no leading zeros, the form
    /// JSON-RPC quantities use. Zero renders as `0x0`.
    std::string to_hex() const {
        const std::string full = to_hex_fixed();
        const size_t first = full.find_first_not_of('0');
        if (first == std::string::npos) {
            return "0x0";
        }
        return "0x" + full.substr(first);
    }

    /// Big-endian hex, always 64 digits, with no prefix.
    std::string to_hex_fixed() const {
        static const char* digits = "0123456789abcdef";
        std::string out;
        out.reserve(64);
        for (uint8_t byte : words_) {
            out.push_back(digits[(byte >> 4) & 0x0f]);
            out.push_back(digits[byte & 0x0f]);
        }
        return out;
    }

    bool operator==(const uint256& other) const { return words_ == other.words_; }
    bool operator!=(const uint256& other) const { return words_ != other.words_; }
    /// Lexicographic order on the big-endian bytes is numeric order.
    bool operator<(const uint256& other) const { return words_ < other.words_; }

private:
    std::array<uint8_t, 32> words_;

    static int hex_nibble(char c) {
        if (c >= '0' && c <= '9') return c - '0';
        if (c >= 'a' && c <= 'f') return c - 'a' + 10;
        if (c >= 'A' && c <= 'F') return c - 'A' + 10;
        return -1;
    }
};

} // namespace tigerchain

/// The EVM's 256-bit word type, under the name the rest of the codebase uses.
using uint256_t = tigerchain::uint256;

#endif // TIGERCHAIN_UINT256_HPP
