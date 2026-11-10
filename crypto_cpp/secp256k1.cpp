// Real secp256k1 ECDSA verification and public-key recovery for TigerSmartChain.
//
// Implemented from scratch (no libsecp256k1 available in the build image):
//   * 256-bit field arithmetic modulo p = 2^256 - 2^32 - 977, using the
//     pseudo-Mersenne special reduction (no general division needed).
//   * Generic 256-bit modular arithmetic modulo the group order n, used for
//     scalar inversions (n is prime, so inversion is Fermat's little theorem).
//   * Jacobian-coordinate point addition/doubling on y^2 = x^3 + 7.
//   * ECDSA verify and public-key recovery, matching Ethereum semantics
//     (z = keccak256 digest interpreted as a 256-bit big-endian integer).
//
// Variable-time on purpose: every operation here consumes only public data
// (message hash, signature, public key). Do NOT reuse these routines for
// signing with a private key.

#include <stddef.h>
#include <stdint.h>
#include <string.h>

#include "tiger_crypto.h"

namespace {

struct U256 {
    uint64_t v[4];
};

inline void u256_zero(U256* a) { a->v[0] = a->v[1] = a->v[2] = a->v[3] = 0; }

inline bool u256_is_zero(const U256* a) {
    return (a->v[0] | a->v[1] | a->v[2] | a->v[3]) == 0;
}

inline bool u256_bit(const U256* a, int i) {
    return ((a->v[i >> 6] >> (i & 63)) & 1ULL) != 0;
}

inline void u256_from_bytes_be(U256* out, const uint8_t* b) {
    for (int i = 0; i < 4; ++i) {
        const uint8_t* p = b + (3 - i) * 8;
        out->v[i] = ((uint64_t)p[0] << 56) | ((uint64_t)p[1] << 48) |
                    ((uint64_t)p[2] << 40) | ((uint64_t)p[3] << 32) |
                    ((uint64_t)p[4] << 24) | ((uint64_t)p[5] << 16) |
                    ((uint64_t)p[6] << 8) | ((uint64_t)p[7]);
    }
}

inline void u256_to_bytes_be(uint8_t* b, const U256* a) {
    for (int i = 0; i < 4; ++i) {
        uint8_t* p = b + (3 - i) * 8;
        const uint64_t x = a->v[i];
        p[0] = (uint8_t)(x >> 56);
        p[1] = (uint8_t)(x >> 48);
        p[2] = (uint8_t)(x >> 40);
        p[3] = (uint8_t)(x >> 32);
        p[4] = (uint8_t)(x >> 24);
        p[5] = (uint8_t)(x >> 16);
        p[6] = (uint8_t)(x >> 8);
        p[7] = (uint8_t)(x);
    }
}

// -1, 0, 1 for a < b, a == b, a > b.
inline int u256_cmp(const U256* a, const U256* b) {
    for (int i = 3; i >= 0; --i) {
        if (a->v[i] != b->v[i]) {
            return a->v[i] > b->v[i] ? 1 : -1;
        }
    }
    return 0;
}

// out = a + b; returns the carry out of bit 255.
inline uint64_t u256_add(U256* out, const U256* a, const U256* b) {
    __uint128_t c = 0;
    for (int i = 0; i < 4; ++i) {
        const __uint128_t s = (__uint128_t)a->v[i] + b->v[i] + c;
        out->v[i] = (uint64_t)s;
        c = s >> 64;
    }
    return (uint64_t)c;
}

// out = a - b; returns the borrow out of bit 255.
inline uint64_t u256_sub(U256* out, const U256* a, const U256* b) {
    __uint128_t borrow = 0;
    for (int i = 0; i < 4; ++i) {
        const __uint128_t d = (__uint128_t)a->v[i] - b->v[i] - borrow;
        out->v[i] = (uint64_t)d;
        borrow = (d >> 64) & 1;
    }
    return (uint64_t)borrow;
}

inline void u256_shr1(U256* a) {
    for (int i = 0; i < 3; ++i) {
        a->v[i] = (a->v[i] >> 1) | (a->v[i + 1] << 63);
    }
    a->v[3] >>= 1;
}

// p = 2^256 - 2^32 - 977
const U256 P = {{0xFFFFFFFEFFFFFC2FULL, 0xFFFFFFFFFFFFFFFFULL, 0xFFFFFFFFFFFFFFFFULL,
                 0xFFFFFFFFFFFFFFFFULL}};

// n, the secp256k1 group order.
const U256 N = {{0xBFD25E8CD0364141ULL, 0xBAAEDCE6AF48A03BULL, 0xFFFFFFFFFFFFFFFEULL,
                 0xFFFFFFFFFFFFFFFFULL}};

const U256 GX = {{0x59F2815B16F81798ULL, 0x029BFCDB2DCE28D9ULL, 0x55A06295CE870B07ULL,
                  0x79BE667EF9DCBBACULL}};

const U256 GY = {{0x9C47D08FFB10D4B8ULL, 0xFD17B448A6855419ULL, 0x5DA4FBFC0E1108A8ULL,
                  0x483ADA7726A3C465ULL}};

// C = 2^256 - p = 2^32 + 977
constexpr uint64_t C_P = 0x1000003D1ULL;

void fe_reduce(U256* a) {
    while (u256_cmp(a, &P) >= 0) {
        u256_sub(a, a, &P);
    }
}

void fe_add(U256* out, const U256* a, const U256* b) {
    const uint64_t carry = u256_add(out, a, b);
    if (carry) {
        U256 c;
        c.v[0] = C_P;
        c.v[1] = c.v[2] = c.v[3] = 0;
        u256_add(out, out, &c);
    }
    fe_reduce(out);
}

void fe_sub(U256* out, const U256* a, const U256* b) {
    const uint64_t borrow = u256_sub(out, a, b);
    if (borrow) {
        // The register now holds a - b + 2^256. Because 2^256 == C_P (mod p),
        // that value is congruent to (a - b) + C_P, so undo the extra C_P to
        // land on a - b + p, which is the canonical representative.
        U256 c;
        c.v[0] = C_P;
        c.v[1] = c.v[2] = c.v[3] = 0;
        u256_sub(out, out, &c);
    }
}

void fe_mul(U256* out, const U256* a, const U256* b) {
    uint64_t t[8] = {0, 0, 0, 0, 0, 0, 0, 0};

    for (int i = 0; i < 4; ++i) {
        __uint128_t carry = 0;
        for (int j = 0; j < 4; ++j) {
            const __uint128_t cur = (__uint128_t)a->v[i] * b->v[j] + t[i + j] + carry;
            t[i + j] = (uint64_t)cur;
            carry = cur >> 64;
        }
        t[i + 4] += (uint64_t)carry;
    }

    // Fold the upper 256 bits: T == lo + hi * C (mod p).
    const uint64_t hi[4] = {t[4], t[5], t[6], t[7]};

    uint64_t m[5] = {0, 0, 0, 0, 0};
    {
        __uint128_t carry = 0;
        for (int i = 0; i < 4; ++i) {
            const __uint128_t cur = (__uint128_t)hi[i] * C_P + carry;
            m[i] = (uint64_t)cur;
            carry = cur >> 64;
        }
        m[4] = (uint64_t)carry;
    }

    uint64_t r[5] = {0, 0, 0, 0, 0};
    {
        __uint128_t carry = 0;
        for (int i = 0; i < 4; ++i) {
            const __uint128_t cur = (__uint128_t)t[i] + m[i] + carry;
            r[i] = (uint64_t)cur;
            carry = cur >> 64;
        }
        r[4] = (uint64_t)((__uint128_t)m[4] + carry);
    }

    {
        __uint128_t carry = (__uint128_t)r[4] * C_P;
        for (int i = 0; i < 4; ++i) {
            const __uint128_t cur = (__uint128_t)r[i] + (uint64_t)carry;
            out->v[i] = (uint64_t)cur;
            carry = (carry >> 64) + (cur >> 64);
        }
        uint64_t extra = (uint64_t)carry;
        while (extra) {
            __uint128_t c2 = (__uint128_t)extra * C_P;
            for (int i = 0; i < 4; ++i) {
                const __uint128_t cur = (__uint128_t)out->v[i] + (uint64_t)c2;
                out->v[i] = (uint64_t)cur;
                c2 = (c2 >> 64) + (cur >> 64);
            }
            extra = (uint64_t)c2;
        }
    }
    fe_reduce(out);
}

void fe_sqr(U256* out, const U256* a) { fe_mul(out, a, a); }

// Fermat inversion: a^(p-2) mod p. Valid because p is prime.
void fe_inv(U256* out, const U256* a) {
    if (u256_is_zero(a)) {
        u256_zero(out);
        return;
    }
    U256 e = P;
    U256 two;
    two.v[0] = 2;
    two.v[1] = two.v[2] = two.v[3] = 0;
    u256_sub(&e, &e, &two);

    U256 result;
    result.v[0] = 1;
    result.v[1] = result.v[2] = result.v[3] = 0;
    U256 base = *a;

    for (int i = 0; i < 256; ++i) {
        if (u256_bit(&e, i)) {
            fe_mul(&result, &result, &base);
        }
        fe_sqr(&base, &base);
    }
    *out = result;
}

// Square root modulo p (p == 3 mod 4): a^((p+1)/4). False when non-residue.
bool fe_sqrt(U256* out, const U256* a) {
    U256 e = P;
    U256 one;
    one.v[0] = 1;
    one.v[1] = one.v[2] = one.v[3] = 0;
    u256_add(&e, &e, &one);
    u256_shr1(&e);
    u256_shr1(&e);

    U256 result;
    result.v[0] = 1;
    result.v[1] = result.v[2] = result.v[3] = 0;
    U256 base = *a;

    for (int i = 0; i < 256; ++i) {
        if (u256_bit(&e, i)) {
            fe_mul(&result, &result, &base);
        }
        fe_sqr(&base, &base);
    }

    U256 check;
    fe_sqr(&check, &result);
    if (u256_cmp(&check, a) != 0) {
        return false;
    }
    *out = result;
    return true;
}

void scalar_add(U256* out, const U256* a, const U256* b) {
    const uint64_t carry = u256_add(out, a, b);
    if (carry || u256_cmp(out, &N) >= 0) {
        u256_sub(out, out, &N);
    }
}

// Double-and-add modular multiplication. Called only a handful of times per
// verification, so clarity beats raw speed here.
void scalar_mul(U256* out, const U256* a, const U256* b) {
    U256 acc;
    u256_zero(&acc);
    U256 base = *a;
    if (u256_cmp(&base, &N) >= 0) {
        u256_sub(&base, &base, &N);
    }

    for (int i = 0; i < 256; ++i) {
        if (u256_bit(b, i)) {
            scalar_add(&acc, &acc, &base);
        }
        scalar_add(&base, &base, &base);
    }
    *out = acc;
}

void scalar_inv(U256* out, const U256* a) {
    U256 e = N;
    U256 two;
    two.v[0] = 2;
    two.v[1] = two.v[2] = two.v[3] = 0;
    u256_sub(&e, &e, &two);

    U256 result;
    result.v[0] = 1;
    result.v[1] = result.v[2] = result.v[3] = 0;
    U256 base = *a;

    for (int i = 0; i < 256; ++i) {
        if (u256_bit(&e, i)) {
            scalar_mul(&result, &result, &base);
        }
        scalar_mul(&base, &base, &base);
    }
    *out = result;
}

struct Jacobian {
    U256 x, y, z;
    bool infinity;
};

inline void jac_infinity(Jacobian* p) {
    u256_zero(&p->x);
    u256_zero(&p->y);
    u256_zero(&p->z);
    p->infinity = true;
}

inline void jac_from_affine(Jacobian* p, const U256* x, const U256* y) {
    p->x = *x;
    p->y = *y;
    p->z.v[0] = 1;
    p->z.v[1] = p->z.v[2] = p->z.v[3] = 0;
    p->infinity = false;
}

// Doubling for curves with a = 0 (dbl-2007-bl).
void jac_double(Jacobian* out, const Jacobian* p) {
    if (p->infinity || u256_is_zero(&p->y)) {
        jac_infinity(out);
        return;
    }

    U256 A, B, C, D, E, F, t1, t2, X3, Y3, Z3;

    fe_sqr(&A, &p->x);
    fe_sqr(&B, &p->y);
    fe_sqr(&C, &B);

    fe_add(&t1, &p->x, &B);
    fe_sqr(&t1, &t1);
    fe_sub(&t1, &t1, &A);
    fe_sub(&t1, &t1, &C);
    fe_add(&D, &t1, &t1);

    fe_add(&E, &A, &A);
    fe_add(&E, &E, &A);

    fe_sqr(&F, &E);

    fe_add(&t1, &D, &D);
    fe_sub(&X3, &F, &t1);

    fe_sub(&t1, &D, &X3);
    fe_mul(&t1, &E, &t1);
    fe_add(&t2, &C, &C);
    fe_add(&t2, &t2, &t2);
    fe_add(&t2, &t2, &t2);
    fe_sub(&Y3, &t1, &t2);

    fe_mul(&t1, &p->y, &p->z);
    fe_add(&Z3, &t1, &t1);

    out->x = X3;
    out->y = Y3;
    out->z = Z3;
    out->infinity = false;
}

// Full addition (add-2007-bl).
void jac_add(Jacobian* out, const Jacobian* p, const Jacobian* q) {
    if (p->infinity) {
        *out = *q;
        return;
    }
    if (q->infinity) {
        *out = *p;
        return;
    }

    U256 Z1Z1, Z2Z2, U1, U2, S1, S2, t1, t2;
    fe_sqr(&Z1Z1, &p->z);
    fe_sqr(&Z2Z2, &q->z);
    fe_mul(&U1, &p->x, &Z2Z2);
    fe_mul(&U2, &q->x, &Z1Z1);
    fe_mul(&t1, &q->z, &Z2Z2);
    fe_mul(&S1, &p->y, &t1);
    fe_mul(&t1, &p->z, &Z1Z1);
    fe_mul(&S2, &q->y, &t1);

    if (u256_cmp(&U1, &U2) == 0) {
        if (u256_cmp(&S1, &S2) != 0) {
            jac_infinity(out);
            return;
        }
        jac_double(out, p);
        return;
    }

    U256 H, I, J, r, V, X3, Y3, Z3;
    fe_sub(&H, &U2, &U1);
    fe_add(&t1, &H, &H);
    fe_sqr(&I, &t1);
    fe_mul(&J, &H, &I);
    fe_sub(&t1, &S2, &S1);
    fe_add(&r, &t1, &t1);
    fe_mul(&V, &U1, &I);

    fe_sqr(&t1, &r);
    fe_sub(&t1, &t1, &J);
    fe_add(&t2, &V, &V);
    fe_sub(&X3, &t1, &t2);

    fe_sub(&t1, &V, &X3);
    fe_mul(&t1, &r, &t1);
    fe_mul(&t2, &S1, &J);
    fe_add(&t2, &t2, &t2);
    fe_sub(&Y3, &t1, &t2);

    fe_add(&t1, &p->z, &q->z);
    fe_sqr(&t1, &t1);
    fe_sub(&t1, &t1, &Z1Z1);
    fe_sub(&t1, &t1, &Z2Z2);
    fe_mul(&Z3, &t1, &H);

    out->x = X3;
    out->y = Y3;
    out->z = Z3;
    out->infinity = false;
}

void jac_neg(Jacobian* out, const Jacobian* p) {
    *out = *p;
    if (!p->infinity) {
        U256 zero;
        u256_zero(&zero);
        fe_sub(&out->y, &zero, &p->y);
    }
}

bool jac_to_affine(U256* x, U256* y, const Jacobian* p) {
    if (p->infinity || u256_is_zero(&p->z)) {
        return false;
    }
    U256 zinv, zinv2, zinv3;
    fe_inv(&zinv, &p->z);
    fe_sqr(&zinv2, &zinv);
    fe_mul(&zinv3, &zinv2, &zinv);
    fe_mul(x, &p->x, &zinv2);
    fe_mul(y, &p->y, &zinv3);
    return true;
}

bool point_mul(U256* out_x, U256* out_y, const U256* px, const U256* py,
               const U256* scalar) {
    Jacobian acc;
    jac_infinity(&acc);

    Jacobian base;
    jac_from_affine(&base, px, py);

    for (int i = 255; i >= 0; --i) {
        jac_double(&acc, &acc);
        if (u256_bit(scalar, i)) {
            jac_add(&acc, &acc, &base);
        }
    }
    return jac_to_affine(out_x, out_y, &acc);
}

// Multiply a Jacobian point by a scalar, staying in Jacobian coordinates.
void jac_scalar_mul(Jacobian* out, const Jacobian* p, const U256* scalar) {
    Jacobian acc;
    jac_infinity(&acc);
    for (int i = 255; i >= 0; --i) {
        jac_double(&acc, &acc);
        if (u256_bit(scalar, i)) {
            jac_add(&acc, &acc, p);
        }
    }
    *out = acc;
}

}  // namespace

extern "C" {

int verify_ecdsa(const uint8_t* msg_hash, const uint8_t* sig, const uint8_t* pubkey) {
    if (msg_hash == nullptr || sig == nullptr || pubkey == nullptr) {
        return 0;
    }

    U256 r, s, z, qx, qy;
    u256_from_bytes_be(&r, sig);
    u256_from_bytes_be(&s, sig + 32);
    u256_from_bytes_be(&z, msg_hash);
    u256_from_bytes_be(&qx, pubkey);
    u256_from_bytes_be(&qy, pubkey + 32);

    if (u256_is_zero(&r) || u256_is_zero(&s)) {
        return 0;
    }
    if (u256_cmp(&r, &N) >= 0 || u256_cmp(&s, &N) >= 0) {
        return 0;
    }

    // The public key must be a real curve point: y^2 == x^3 + 7 (mod p).
    {
        U256 lhs, rhs, x3, seven;
        fe_sqr(&lhs, &qy);
        fe_sqr(&x3, &qx);
        fe_mul(&x3, &x3, &qx);
        seven.v[0] = 7;
        seven.v[1] = seven.v[2] = seven.v[3] = 0;
        fe_add(&rhs, &x3, &seven);
        if (u256_cmp(&lhs, &rhs) != 0) {
            return 0;
        }
    }

    U256 s_inv, u1, u2;
    scalar_inv(&s_inv, &s);
    scalar_mul(&u1, &z, &s_inv);
    scalar_mul(&u2, &r, &s_inv);

    U256 gx, gy, qrx, qry;
    if (!point_mul(&gx, &gy, &GX, &GY, &u1)) {
        return 0;
    }
    if (!point_mul(&qrx, &qry, &qx, &qy, &u2)) {
        return 0;
    }

    Jacobian jg, jq, jr;
    jac_from_affine(&jg, &gx, &gy);
    jac_from_affine(&jq, &qrx, &qry);
    jac_add(&jr, &jg, &jq);

    U256 rx, ry;
    if (!jac_to_affine(&rx, &ry, &jr)) {
        return 0;
    }

    if (u256_cmp(&rx, &N) >= 0) {
        u256_sub(&rx, &rx, &N);
    }
    return u256_cmp(&rx, &r) == 0 ? 1 : 0;
}

int recover_pubkey(const uint8_t* msg_hash, const uint8_t* sig, uint8_t recovery_id,
                   uint8_t* pubkey_out) {
    if (msg_hash == nullptr || sig == nullptr || pubkey_out == nullptr || recovery_id > 1) {
        return 0;
    }

    U256 r, s, z;
    u256_from_bytes_be(&r, sig);
    u256_from_bytes_be(&s, sig + 32);
    u256_from_bytes_be(&z, msg_hash);

    if (u256_is_zero(&r) || u256_is_zero(&s)) {
        return 0;
    }
    if (u256_cmp(&r, &N) >= 0 || u256_cmp(&s, &N) >= 0) {
        return 0;
    }

    // Reconstruct R.x = r (Ethereum's recovery id never needs r + n because
    // the id is only 0/1; the 2 bit is rejected above).
    const U256 rx = r;

    U256 seven, x3, y2, ry;
    seven.v[0] = 7;
    seven.v[1] = seven.v[2] = seven.v[3] = 0;
    fe_sqr(&x3, &rx);
    fe_mul(&x3, &x3, &rx);
    fe_add(&y2, &x3, &seven);
    if (!fe_sqrt(&ry, &y2)) {
        return 0;
    }
    if ((ry.v[0] & 1ULL) != (uint64_t)(recovery_id & 1)) {
        U256 zero;
        u256_zero(&zero);
        fe_sub(&ry, &zero, &ry);
    }

    // Q = r^-1 * (s*R - z*G)
    U256 r_inv;
    scalar_inv(&r_inv, &r);

    Jacobian jR, jG, jS, jZ, jDiff, jQ;
    jac_from_affine(&jR, &rx, &ry);
    jac_from_affine(&jG, &GX, &GY);

    jac_scalar_mul(&jS, &jR, &s);
    jac_scalar_mul(&jZ, &jG, &z);
    jac_neg(&jZ, &jZ);
    jac_add(&jDiff, &jS, &jZ);
    jac_scalar_mul(&jQ, &jDiff, &r_inv);

    U256 qx, qy;
    if (!jac_to_affine(&qx, &qy, &jQ)) {
        return 0;
    }

    u256_to_bytes_be(pubkey_out, &qx);
    u256_to_bytes_be(pubkey_out + 32, &qy);
    return 1;
}


// =============================================================================
// DETERMINISTIC SIGNING (RFC 6979)
// =============================================================================
//
// Signing is implemented here because the recovery/verification routines above
// deliberately do not sign: reusing a nonce across two messages leaks the
// private key, so the nonce must come from a deterministic, standardised
// derivation. RFC 6979 derives k from HMAC-SHA256 over the private key and the
// message hash, which removes any dependence on a random source and makes
// signing reproducible and testable.

// Minimal SHA-256 (FIPS 180-4) used only for the RFC 6979 HMAC.
struct Sha256Ctx {
    uint32_t state[8];
    uint64_t bitlen;
    uint8_t buf[64];
    size_t buflen;
};

inline uint32_t rotr32(uint32_t x, uint32_t n) { return (x >> n) | (x << (32 - n)); }

const uint32_t SHA256_K[64] = {
    0x428a2f98u, 0x71374491u, 0xb5c0fbcfu, 0xe9b5dba5u, 0x3956c25bu, 0x59f111f1u,
    0x923f82a4u, 0xab1c5ed5u, 0xd807aa98u, 0x12835b01u, 0x243185beu, 0x550c7dc3u,
    0x72be5d74u, 0x80deb1feu, 0x9bdc06a7u, 0xc19bf174u, 0xe49b69c1u, 0xefbe4786u,
    0x0fc19dc6u, 0x240ca1ccu, 0x2de92c6fu, 0x4a7484aau, 0x5cb0a9dcu, 0x76f988dau,
    0x983e5152u, 0xa831c66du, 0xb00327c8u, 0xbf597fc7u, 0xc6e00bf3u, 0xd5a79147u,
    0x06ca6351u, 0x14292967u, 0x27b70a85u, 0x2e1b2138u, 0x4d2c6dfcu, 0x53380d13u,
    0x650a7354u, 0x766a0abbu, 0x81c2c92eu, 0x92722c85u, 0xa2bfe8a1u, 0xa81a664bu,
    0xc24b8b70u, 0xc76c51a3u, 0xd192e819u, 0xd6990624u, 0xf40e3585u, 0x106aa070u,
    0x19a4c116u, 0x1e376c08u, 0x2748774cu, 0x34b0bcb5u, 0x391c0cb3u, 0x4ed8aa4au,
    0x5b9cca4fu, 0x682e6ff3u, 0x748f82eeu, 0x78a5636fu, 0x84c87814u, 0x8cc70208u,
    0x90befffau, 0xa4506cebu, 0xbef9a3f7u, 0xc67178f2u};

void sha256_compress(uint32_t* state, const uint8_t* block) {
    uint32_t w[64];
    for (int i = 0; i < 16; ++i) {
        w[i] = (uint32_t(block[i * 4]) << 24) | (uint32_t(block[i * 4 + 1]) << 16) |
               (uint32_t(block[i * 4 + 2]) << 8) | uint32_t(block[i * 4 + 3]);
    }
    for (int i = 16; i < 64; ++i) {
        uint32_t s0 = rotr32(w[i - 15], 7) ^ rotr32(w[i - 15], 18) ^ (w[i - 15] >> 3);
        uint32_t s1 = rotr32(w[i - 2], 17) ^ rotr32(w[i - 2], 19) ^ (w[i - 2] >> 10);
        w[i] = w[i - 16] + s0 + w[i - 7] + s1;
    }
    uint32_t a = state[0], b = state[1], c = state[2], d = state[3];
    uint32_t e = state[4], f = state[5], g = state[6], h = state[7];
    for (int i = 0; i < 64; ++i) {
        uint32_t S1 = rotr32(e, 6) ^ rotr32(e, 11) ^ rotr32(e, 25);
        uint32_t ch = (e & f) ^ (~e & g);
        uint32_t t1 = h + S1 + ch + SHA256_K[i] + w[i];
        uint32_t S0 = rotr32(a, 2) ^ rotr32(a, 13) ^ rotr32(a, 22);
        uint32_t maj = (a & b) ^ (a & c) ^ (b & c);
        uint32_t t2 = S0 + maj;
        h = g; g = f; f = e; e = d + t1;
        d = c; c = b; b = a; a = t1 + t2;
    }
    state[0] += a; state[1] += b; state[2] += c; state[3] += d;
    state[4] += e; state[5] += f; state[6] += g; state[7] += h;
}

void sha256_init(Sha256Ctx* ctx) {
    ctx->state[0] = 0x6a09e667u; ctx->state[1] = 0xbb67ae85u;
    ctx->state[2] = 0x3c6ef372u; ctx->state[3] = 0xa54ff53au;
    ctx->state[4] = 0x510e527fu; ctx->state[5] = 0x9b05688cu;
    ctx->state[6] = 0x1f83d9abu; ctx->state[7] = 0x5be0cd19u;
    ctx->bitlen = 0;
    ctx->buflen = 0;
}

void sha256_update(Sha256Ctx* ctx, const uint8_t* data, size_t len) {
    ctx->bitlen += uint64_t(len) * 8;
    for (size_t i = 0; i < len; ++i) {
        ctx->buf[ctx->buflen++] = data[i];
        if (ctx->buflen == 64) {
            sha256_compress(ctx->state, ctx->buf);
            ctx->buflen = 0;
        }
    }
}

void sha256_final(Sha256Ctx* ctx, uint8_t* out) {
    uint64_t bitlen = ctx->bitlen;
    uint8_t pad = 0x80;
    sha256_update(ctx, &pad, 1);
    uint8_t zero = 0x00;
    while (ctx->buflen != 56) {
        sha256_update(ctx, &zero, 1);
    }
    // Append the length in bits, big-endian. Writing directly avoids the
    // accounting in sha256_update.
    for (int i = 7; i >= 0; --i) {
        ctx->buf[ctx->buflen++] = uint8_t((bitlen >> (i * 8)) & 0xff);
    }
    sha256_compress(ctx->state, ctx->buf);
    for (int i = 0; i < 8; ++i) {
        out[i * 4] = uint8_t(ctx->state[i] >> 24);
        out[i * 4 + 1] = uint8_t(ctx->state[i] >> 16);
        out[i * 4 + 2] = uint8_t(ctx->state[i] >> 8);
        out[i * 4 + 3] = uint8_t(ctx->state[i]);
    }
}

void sha256(const uint8_t* data, size_t len, uint8_t* out) {
    Sha256Ctx ctx;
    sha256_init(&ctx);
    sha256_update(&ctx, data, len);
    sha256_final(&ctx, out);
}

void hmac_sha256(const uint8_t* key, size_t keylen, const uint8_t* msg, size_t msglen,
                 uint8_t* out) {
    uint8_t k[64];
    memset(k, 0, sizeof(k));
    if (keylen > 64) {
        sha256(key, keylen, k);
    } else {
        memcpy(k, key, keylen);
    }
    uint8_t ipad[64], opad[64];
    for (int i = 0; i < 64; ++i) {
        ipad[i] = k[i] ^ 0x36;
        opad[i] = k[i] ^ 0x5c;
    }
    uint8_t inner[32];
    Sha256Ctx ctx;
    sha256_init(&ctx);
    sha256_update(&ctx, ipad, 64);
    sha256_update(&ctx, msg, msglen);
    sha256_final(&ctx, inner);

    sha256_init(&ctx);
    sha256_update(&ctx, opad, 64);
    sha256_update(&ctx, inner, 32);
    sha256_final(&ctx, out);
}

// RFC 6979: derive the candidate nonce for a given attempt.
//
// T = HMAC_K(V || 0x00) (and so on) with the optional extra entropy byte. The
// caller retries with a higher `attempt` when the candidate is not usable.
void rfc6979_nonce(const U256* priv, const uint8_t* hash, uint32_t attempt, U256* k_out) {
    uint8_t V[32], K[32];
    memset(V, 0x01, 32);
    memset(K, 0x00, 32);

    // int2octets(x) || bits2octets(h1)
    uint8_t x_oct[32];
    u256_to_bytes_be(x_oct, priv);
    uint8_t h_oct[32];
    U256 h_val;
    u256_from_bytes_be(&h_val, hash);
    // bits2octets reduces the hash modulo n when it is >= n.
    if (u256_cmp(&h_val, &N) >= 0) {
        U256 reduced;
        u256_from_bytes_be(&reduced, hash);
        // Subtract n once; a 256-bit hash is always < 2n.
        uint64_t borrow = 0;
        for (int i = 0; i < 4; ++i) {
            uint64_t a = reduced.v[i];
            uint64_t b = N.v[i];
            uint64_t sub = a - b - borrow;
            borrow = (a < b + borrow) ? 1 : 0;
            reduced.v[i] = sub;
        }
        u256_to_bytes_be(h_oct, &reduced);
    } else {
        memcpy(h_oct, hash, 32);
    }

    uint8_t seed[64];
    memcpy(seed, x_oct, 32);
    memcpy(seed + 32, h_oct, 32);

    uint8_t buf[97];
    // K = HMAC_K(V || 0x00 || seed)
    memcpy(buf, V, 32);
    buf[32] = 0x00;
    memcpy(buf + 33, seed, 64);
    hmac_sha256(K, 32, buf, 97, K);
    // V = HMAC_K(V)
    hmac_sha256(K, 32, V, 32, V);
    // K = HMAC_K(V || 0x01 || seed)
    memcpy(buf, V, 32);
    buf[32] = 0x01;
    memcpy(buf + 33, seed, 64);
    hmac_sha256(K, 32, buf, 97, K);
    hmac_sha256(K, 32, V, 32, V);

    // Each attempt advances V once more through the HMAC chain.
    for (uint32_t i = 0; i <= attempt; ++i) {
        hmac_sha256(K, 32, V, 32, V);
    }

    u256_from_bytes_be(k_out, V);
}


/* Sign a 32-byte digest with a 32-byte private key using deterministic
 * RFC 6979 nonces. Produces a 64-byte (r || s) signature with low-s
 * normalisation, plus the recovery id (0 or 1).
 * Returns 1 on success, 0 if the key or digest is unusable. */
int sign_ecdsa(const uint8_t* msg_hash, const uint8_t* privkey, uint8_t* sig_out,
               uint8_t* recovery_id_out) {
    if (msg_hash == nullptr || privkey == nullptr || sig_out == nullptr) {
        return 0;
    }

    U256 d;
    u256_from_bytes_be(&d, privkey);
    // The private key must be in [1, n-1].
    if (u256_is_zero(&d) || u256_cmp(&d, &N) >= 0) {
        return 0;
    }

    // z is the message hash reduced modulo n.
    U256 z;
    u256_from_bytes_be(&z, msg_hash);
    if (u256_cmp(&z, &N) >= 0) {
        uint64_t borrow = 0;
        for (int i = 0; i < 4; ++i) {
            uint64_t a = z.v[i];
            uint64_t b = N.v[i];
            uint64_t sub = a - b - borrow;
            borrow = (a < b + borrow) ? 1 : 0;
            z.v[i] = sub;
        }
    }

    // Retry with the next RFC 6979 candidate if a nonce is unusable. In
    // practice the first candidate is essentially always valid.
    for (uint32_t attempt = 0; attempt < 16; ++attempt) {
        U256 k;
        rfc6979_nonce(&d, msg_hash, attempt, &k);
        if (u256_is_zero(&k) || u256_cmp(&k, &N) >= 0) {
            continue;
        }

        // R = k * G.
        U256 rx, ry;
        if (!point_mul(&rx, &ry, &GX, &GY, &k)) {
            continue;
        }

        // r = R.x mod n; reject r == 0.
        U256 r = rx;
        if (u256_cmp(&r, &N) >= 0) {
            u256_sub(&r, &r, &N);
        }
        if (u256_is_zero(&r)) {
            continue;
        }

        // recovery id is the parity of R.y, flipped when R.x was reduced.
        uint8_t recid = uint8_t(ry.v[0] & 1);
        if (u256_cmp(&rx, &N) >= 0) {
            recid ^= 1;
        }

        // s = k^-1 (z + r*d) mod n; reject s == 0.
        U256 k_inv;
        scalar_inv(&k_inv, &k);
        U256 rd;
        scalar_mul(&rd, &r, &d);
        U256 sum;
        scalar_add(&sum, &z, &rd);
        U256 sig_s;
        scalar_mul(&sig_s, &k_inv, &sum);
        if (u256_is_zero(&sig_s)) {
            continue;
        }

        // Low-s normalisation (EIP-2): if s > n/2, use n - s and flip recid.
        U256 half_n = N;
        {
            // half = n >> 1, computed with a shift.
            uint64_t carry = 0;
            for (int i = 3; i >= 0; --i) {
                uint64_t next_carry = half_n.v[i] & 1ULL;
                half_n.v[i] = (half_n.v[i] >> 1) | (carry << 63);
                carry = next_carry;
            }
        }
        if (u256_cmp(&sig_s, &half_n) > 0) {
            u256_sub(&sig_s, &N, &sig_s);
            recid ^= 1;
        }

        u256_to_bytes_be(sig_out, &r);
        u256_to_bytes_be(sig_out + 32, &sig_s);
        if (recovery_id_out != nullptr) {
            *recovery_id_out = recid;
        }
        return 1;
    }

    return 0;
}

void pubkey_to_address(const uint8_t* pubkey, uint8_t* address_out) {
    if (pubkey == nullptr || address_out == nullptr) {
        return;
    }
    uint8_t digest[32];
    keccak256(pubkey, 64, digest);
    memcpy(address_out, digest + 12, 20);
}


}  // extern "C"
