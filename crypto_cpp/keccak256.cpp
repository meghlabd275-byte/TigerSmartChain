// Real Keccak-256 / Keccak-512 / SHA3-256 for TigerSmartChain.
//
// Ethereum uses the ORIGINAL Keccak submission (domain padding byte 0x01),
// not the finalised FIPS-202 SHA-3 (which pads with 0x06). Both are provided
// here; callers must pick the right one. Do not "unify" the padding byte.
//
// The permutation is Keccak-f[1600] driven by the compact reference loop with
// a compile-time round-constant table, so the optimiser can unroll it.

#include <stddef.h>
#include <stdint.h>
#include <string.h>

#include "tiger_crypto.h"

namespace {

constexpr int KECCAK_ROUNDS = 24;
constexpr int KECCAK_RATE_256 = 136;  // (1600 - 2*256) / 8 bytes
constexpr int KECCAK_RATE_512 = 72;   // (1600 - 2*512) / 8 bytes

const uint64_t RC[KECCAK_ROUNDS] = {
    0x0000000000000001ULL, 0x0000000000008082ULL, 0x800000000000808aULL,
    0x8000000080008000ULL, 0x000000000000808bULL, 0x0000000080000001ULL,
    0x8000000080008081ULL, 0x8000000000008009ULL, 0x000000000000008aULL,
    0x0000000000000088ULL, 0x0000000080008009ULL, 0x000000008000000aULL,
    0x000000008000808bULL, 0x800000000000008bULL, 0x8000000000008089ULL,
    0x8000000000008003ULL, 0x8000000000008002ULL, 0x8000000000000080ULL,
    0x000000000000800aULL, 0x800000008000000aULL, 0x8000000080008081ULL,
    0x8000000000008080ULL, 0x0000000080000001ULL, 0x8000000080008008ULL,
};

const int ROTC[24] = {1,  3,  6,  10, 15, 21, 28, 36, 45, 55, 2,  14,
                      27, 41, 56, 8,  25, 43, 62, 18, 39, 61, 20, 44};

const int PILN[24] = {10, 7,  11, 17, 18, 3,  5,  16, 8,  21, 24, 4,
                      15, 23, 19, 13, 12, 2,  20, 14, 22, 9,  6,  1};

inline uint64_t rotl64(uint64_t x, int n) {
    return (x << n) | (x >> (64 - n));
}

// Portable little-endian load/store. Compilers lower these to single moves.
inline uint64_t load64_le(const uint8_t* p) {
    return (uint64_t)p[0] | ((uint64_t)p[1] << 8) | ((uint64_t)p[2] << 16) |
           ((uint64_t)p[3] << 24) | ((uint64_t)p[4] << 32) | ((uint64_t)p[5] << 40) |
           ((uint64_t)p[6] << 48) | ((uint64_t)p[7] << 56);
}

inline void store64_le(uint8_t* p, uint64_t v) {
    p[0] = (uint8_t)(v);
    p[1] = (uint8_t)(v >> 8);
    p[2] = (uint8_t)(v >> 16);
    p[3] = (uint8_t)(v >> 24);
    p[4] = (uint8_t)(v >> 32);
    p[5] = (uint8_t)(v >> 40);
    p[6] = (uint8_t)(v >> 48);
    p[7] = (uint8_t)(v >> 56);
}

void keccakf(uint64_t st[25]) {
    uint64_t bc[5];
    for (int round = 0; round < KECCAK_ROUNDS; ++round) {
        // Theta
        for (int i = 0; i < 5; ++i) {
            bc[i] = st[i] ^ st[i + 5] ^ st[i + 10] ^ st[i + 15] ^ st[i + 20];
        }
        for (int i = 0; i < 5; ++i) {
            const uint64_t t = bc[(i + 4) % 5] ^ rotl64(bc[(i + 1) % 5], 1);
            for (int j = 0; j < 25; j += 5) {
                st[j + i] ^= t;
            }
        }
        // Rho + Pi
        uint64_t t = st[1];
        for (int i = 0; i < 24; ++i) {
            const int j = PILN[i];
            const uint64_t save = st[j];
            st[j] = rotl64(t, ROTC[i]);
            t = save;
        }
        // Chi
        for (int j = 0; j < 25; j += 5) {
            for (int i = 0; i < 5; ++i) {
                bc[i] = st[j + i];
            }
            for (int i = 0; i < 5; ++i) {
                st[j + i] = bc[i] ^ ((~bc[(i + 1) % 5]) & bc[(i + 2) % 5]);
            }
        }
        // Iota
        st[0] ^= RC[round];
    }
}

// Sponge with configurable rate and domain-separation padding byte.
void sponge(const uint8_t* data, size_t len, int rate, uint8_t pad, uint8_t* out,
            size_t out_len) {
    uint64_t st[25];
    memset(st, 0, sizeof(st));

    size_t offset = 0;
    while (len - offset >= (size_t)rate) {
        for (int i = 0; i < rate / 8; ++i) {
            st[i] ^= load64_le(data + offset + (size_t)i * 8);
        }
        keccakf(st);
        offset += (size_t)rate;
    }

    uint8_t block[KECCAK_RATE_256];
    memset(block, 0, sizeof(block));
    const size_t rem = len - offset;
    if (rem > 0) {
        memcpy(block, data + offset, rem);
    }
    block[rem] = pad;
    block[rate - 1] |= 0x80;

    for (int i = 0; i < rate / 8; ++i) {
        st[i] ^= load64_le(block + (size_t)i * 8);
    }
    keccakf(st);

    size_t written = 0;
    uint8_t lane[KECCAK_RATE_256];
    while (written < out_len) {
        for (int i = 0; i < rate / 8; ++i) {
            store64_le(lane + (size_t)i * 8, st[i]);
        }
        size_t take = out_len - written;
        if (take > (size_t)rate) {
            take = (size_t)rate;
        }
        memcpy(out + written, lane, take);
        written += take;
        if (written < out_len) {
            keccakf(st);
        }
    }
}

}  // namespace

extern "C" {

void keccak256(const uint8_t* data, size_t len, uint8_t* hash_out) {
    if (hash_out == nullptr || (data == nullptr && len != 0)) {
        return;
    }
    static const uint8_t empty = 0;
    sponge(len == 0 ? &empty : data, len, KECCAK_RATE_256, 0x01, hash_out, 32);
}

void keccak512(const uint8_t* data, size_t len, uint8_t* hash_out) {
    if (hash_out == nullptr || (data == nullptr && len != 0)) {
        return;
    }
    static const uint8_t empty = 0;
    sponge(len == 0 ? &empty : data, len, KECCAK_RATE_512, 0x01, hash_out, 64);
}

void sha3_256(const uint8_t* data, size_t len, uint8_t* hash_out) {
    if (hash_out == nullptr || (data == nullptr && len != 0)) {
        return;
    }
    static const uint8_t empty = 0;
    sponge(len == 0 ? &empty : data, len, KECCAK_RATE_256, 0x06, hash_out, 32);
}

}  // extern "C"
