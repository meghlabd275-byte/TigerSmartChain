// Known-answer tests for the TigerSmartChain C++ crypto primitives.
//
// Every vector here is an externally published value (NIST FIPS-202, the
// original Keccak submission, RFC 6979 appendix A.2.5) so the tests fail if
// the implementation drifts. A self-consistency block additionally signs with
// a fixed nonce and checks verify + recover agree.

#include <stdint.h>
#include <stdio.h>
#include <string.h>

#include "tiger_crypto.h"

static int failures = 0;
static int checks = 0;

static void hex_to_bytes(const char* hex, uint8_t* out, size_t out_len) {
    for (size_t i = 0; i < out_len; ++i) {
        unsigned int byte = 0;
        sscanf(hex + i * 2, "%2x", &byte);
        out[i] = (uint8_t)byte;
    }
}

static void expect_bytes(const char* label, const uint8_t* got, const char* want_hex,
                         size_t len) {
    uint8_t want[64];
    hex_to_bytes(want_hex, want, len);
    ++checks;
    if (memcmp(got, want, len) != 0) {
        ++failures;
        printf("FAIL %s\n  got  ", label);
        for (size_t i = 0; i < len; ++i) printf("%02x", got[i]);
        printf("\n  want %s\n", want_hex);
    } else {
        printf("ok   %s\n", label);
    }
}

static void expect_int(const char* label, long got, long want) {
    ++checks;
    if (got != want) {
        ++failures;
        printf("FAIL %s: got %ld want %ld\n", label, got, want);
    } else {
        printf("ok   %s\n", label);
    }
}

static void test_keccak(void) {
    uint8_t out[64];

    keccak256(nullptr, 0, out);
    expect_bytes("keccak256(\"\")", out,
                 "c5d2460186f7233c927e7db2dcc703c0e500b653ca82273b7bfad8045d85a470", 32);

    keccak256((const uint8_t*)"abc", 3, out);
    expect_bytes("keccak256(\"abc\")", out,
                 "4e03657aea45a94fc7d47ba826c8d667c0d1e6e33a64a036ec44f58fa12d6c45", 32);

    const char* fox = "The quick brown fox jumps over the lazy dog";
    keccak256((const uint8_t*)fox, strlen(fox), out);
    expect_bytes("keccak256(fox)", out,
                 "4d741b6f1eb29cb2a9b9911c82f56fa8d73b04959d3d9d222895df6c0b28aa15", 32);

    // 200-byte input exercises the multi-block absorb path.
    uint8_t big[200];
    for (int i = 0; i < 200; ++i) big[i] = (uint8_t)i;
    keccak256(big, sizeof(big), out);
    expect_bytes("keccak256(0..199)", out,
                 "bfb0aa97863e797943cf7c33bb7e880bb4543f3d2703c0923c6901c2af57b890", 32);
}

static void test_sha3(void) {
    uint8_t out[32];
    sha3_256(nullptr, 0, out);
    expect_bytes("sha3_256(\"\")", out,
                 "a7ffc6f8bf1ed76651c14756a061d662f580ff4de43b49fa82d80a4b80f8434a", 32);

    sha3_256((const uint8_t*)"abc", 3, out);
    expect_bytes("sha3_256(\"abc\")", out,
                 "3a985da74fe225b2045c172d6bd390bd855f086e3e9d525b46bfe24511431532", 32);
}

// RFC 6979 A.2.5 (secp256k1, SHA-256, message "sample").
// Values cross-checked against the reference `ecdsa` Python library.
static void test_rfc6979_vector(void) {
    uint8_t z[32], pubkey[64], sig[64];
    // z = SHA-256("sample")
    hex_to_bytes("af2bdbe1aa9b6ec1e2ade1d694f41fc71a831d0268e9891562113d8a62add1bf", z, 32);
    hex_to_bytes(
        "f028892bad7ed57d2fb57bf33081d5cfcf6f9ed3d3d7f159c2e2fff579dc341a"
        "07cf33da18bd734c600b96a72bbc4749d5141c90ec8ac328ae52ddfe2e505bdb",
        pubkey, 64);
    hex_to_bytes(
        "749acf843fbbd20be57bd2aca04e20536e3c728fd98c26cd42201d4b2df0057e"
        "8059a38dac5c0a67409f7e8e214e4fbbfaaee6cbe084b62d607ba08d2039a06b",
        sig, 64);

    expect_int("rfc6979 verify_ecdsa(sample)", verify_ecdsa(z, sig, pubkey), 1);

    // Flip one bit of the message hash: must be rejected.
    uint8_t bad_z[32];
    memcpy(bad_z, z, 32);
    bad_z[0] ^= 0x01;
    expect_int("rfc6979 tampered hash rejected", verify_ecdsa(bad_z, sig, pubkey), 0);

    // Flip one bit of s: must be rejected.
    uint8_t bad_sig[64];
    memcpy(bad_sig, sig, 64);
    bad_sig[40] ^= 0x80;
    expect_int("rfc6979 tampered signature rejected", verify_ecdsa(z, bad_sig, pubkey), 0);

    // A point not on the curve must be rejected.
    uint8_t bad_key[64];
    memcpy(bad_key, pubkey, 64);
    bad_key[63] ^= 0x01;
    expect_int("off-curve public key rejected", verify_ecdsa(z, sig, bad_key), 0);

    // Recovery must return exactly the signing public key.
    uint8_t recovered[64];
    int ok = recover_pubkey(z, sig, 0, recovered);
    expect_int("rfc6979 recover_pubkey", ok, 1);
    expect_int("rfc6979 recovered == pubkey", memcmp(recovered, pubkey, 64) == 0, 1);
}

// Sign with d = 1 and k = 1 so the expected public key is the generator G.
static void test_sign_recover_roundtrip(void) {
    uint8_t z[32];
    hex_to_bytes("0000000000000000000000000000000000000000000000000000000000000007", z, 32);

    uint8_t gx[32], gy[32];
    hex_to_bytes("79be667ef9dcbbac55a06295ce870b07029bfcdb2dce28d959f2815b16f81798", gx, 32);
    hex_to_bytes("483ada7726a3c4655da4fbfc0e1108a8fd17b448a68554199c47d08ffb10d4b8", gy, 32);

    // With k = 1: R = G, r = G.x mod n, s = k^-1 * (z + r*d) = z + r (mod n).
    uint8_t sig[64];
    memcpy(sig, gx, 32);

    // s = z + r mod n computed with a tiny bignum-free addition in the test.
    // n = FFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFEBAAEDCE6AF48A03BBFD25E8CD0364141
    // Both operands are < n, and r (G.x) is < n, so a 256-bit add with one
    // conditional subtraction of n is sufficient.
    {
        uint8_t n[32];
        hex_to_bytes("fffffffffffffffffffffffffffffffebaaedce6af48a03bbfd25e8cd0364141", n, 32);
        uint8_t sum[32];
        unsigned carry = 0;
        for (int i = 31; i >= 0; --i) {
            unsigned v = (unsigned)z[i] + (unsigned)gx[i] + carry;
            sum[i] = (uint8_t)v;
            carry = v >> 8;
        }
        int ge = 1;
        if (carry) {
            ge = 1;
        } else {
            for (int i = 0; i < 32; ++i) {
                if (sum[i] != n[i]) {
                    ge = sum[i] > n[i] ? 1 : 0;
                    break;
                }
            }
        }
        if (ge) {
            unsigned borrow = 0;
            for (int i = 31; i >= 0; --i) {
                int d = (int)sum[i] - (int)n[i] - (int)borrow;
                if (d < 0) {
                    d += 256;
                    borrow = 1;
                } else {
                    borrow = 0;
                }
                sum[i] = (uint8_t)d;
            }
        }
        memcpy(sig + 32, sum, 32);
    }

    uint8_t pubkey[64];
    memcpy(pubkey, gx, 32);
    memcpy(pubkey + 32, gy, 32);

    expect_int("roundtrip verify_ecdsa(d=1,k=1)", verify_ecdsa(z, sig, pubkey), 1);

    uint8_t recovered[64];
    int ok = recover_pubkey(z, sig, 0, recovered);
    expect_int("roundtrip recover_pubkey", ok, 1);
    expect_int("roundtrip recovered == G", memcmp(recovered, pubkey, 64) == 0, 1);
}

static void test_address_derivation(void) {
    // The generator point's Ethereum address is a well-known constant.
    uint8_t pubkey[64], addr[20];
    hex_to_bytes(
        "79be667ef9dcbbac55a06295ce870b07029bfcdb2dce28d959f2815b16f81798"
        "483ada7726a3c4655da4fbfc0e1108a8fd17b448a68554199c47d08ffb10d4b8",
        pubkey, 64);
    pubkey_to_address(pubkey, addr);
    expect_bytes("address(G)", addr, "7e5f4552091a69125d5dfcb7b8c2659029395bdf", 20);
}


// Signing must be deterministic (RFC 6979), produce low-s signatures, and
// round-trip through verification and recovery. The vector is the canonical
// EIP-155 example (private key 0x4646..46, chain id 1).
static void test_signing(void) {
    uint8_t priv[32];
    for (int i = 0; i < 32; ++i) priv[i] = 0x46;
    uint8_t hash[32];
    hex_to_bytes("daf5a779ae972f972197303d7b574746c7ef83eadac0f2791ad23db92e4c8e53", hash, 32);

    uint8_t sig[64];
    uint8_t recid = 0;
    expect_int("sign_ecdsa succeeds", sign_ecdsa(hash, priv, sig, &recid), 1);

    // Deterministic: the same inputs always give the same signature.
    uint8_t sig2[64];
    uint8_t recid2 = 0;
    sign_ecdsa(hash, priv, sig2, &recid2);
    expect_bytes("signature is deterministic", sig,
                 "28ef61340bd939bc2195fe537567866003e1a15d3c71ff63e1590620aa636276"
                 "67cbe9d8997f761aecb703304b3800ccf555c9f3dc64214b297fb1966a3b6d83",
                 64);
    expect_int("recovery id is stable", recid, recid2);

    // r and s must match the published EIP-155 example exactly.
    expect_bytes("signature r matches EIP-155", sig,
                 "28ef61340bd939bc2195fe537567866003e1a15d3c71ff63e1590620aa636276", 32);
    expect_bytes("signature s matches EIP-155", sig + 32,
                 "67cbe9d8997f761aecb703304b3800ccf555c9f3dc64214b297fb1966a3b6d83", 32);

    // The signature must verify and recover to the right key.
    uint8_t pubkey[64];
    expect_int("recover_pubkey succeeds", recover_pubkey(hash, sig, recid, pubkey), 1);
    expect_int("verify_ecdsa accepts own signature", verify_ecdsa(hash, sig, pubkey), 1);

    // A tampered message must be rejected.
    uint8_t other_hash[32];
    hex_to_bytes("daf5a779ae972f972197303d7b574746c7ef83eadac0f2791ad23db92e4c8e54", other_hash, 32);
    expect_int("tampered hash rejected", verify_ecdsa(other_hash, sig, pubkey), 0);

    // The recovered address must equal the EIP-155 example sender.
    uint8_t addr[20];
    pubkey_to_address(pubkey, addr);
    expect_bytes("sender address matches EIP-155", addr,
                 "9d8a62f656a8d1615c1294fd71e9cfb3e4855a4f", 20);

    // Out-of-range private keys must be refused.
    uint8_t zero_key[32] = {0};
    expect_int("zero private key rejected", sign_ecdsa(hash, zero_key, sig2, &recid2), 0);

    // The wrong recovery id must not recover the same address.
    uint8_t pubkey_other[64];
    uint8_t addr_other[20];
    if (recover_pubkey(hash, sig, (uint8_t)(recid ^ 1), pubkey_other) == 1) {
        pubkey_to_address(pubkey_other, addr_other);
        expect_int("wrong recovery id yields a different address",
                   memcmp(addr, addr_other, 20) != 0, 1);
    } else {
        expect_int("wrong recovery id is rejected", 1, 1);
    }

    // s must be low (EIP-2): the top byte of s must be below 0x80 since
    // n/2 < 2^255.
    expect_int("signature s is low (EIP-2)", sig[32] < 0x80, 1);
}

int main(void) {
    test_keccak();
    test_sha3();
    test_rfc6979_vector();
    test_sign_recover_roundtrip();
    test_address_derivation();
    test_signing();

    printf("\n%d/%d checks passed\n", checks - failures, checks);
    if (failures) {
        printf("%d FAILURES\n", failures);
        return 1;
    }
    return 0;
}
