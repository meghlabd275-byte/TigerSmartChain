/* TigerSmartChain cryptographic primitives (C ABI).
 *
 * These are the low-level, allocation-free entry points used by the Rust
 * `tiger-crypto` crate and the C++ RPC server. All functions are reentrant
 * and thread-safe: no global mutable state is used.
 */
#ifndef TIGER_CRYPTO_H
#define TIGER_CRYPTO_H

#include <stddef.h>
#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

/* Ethereum Keccak-256 (original Keccak padding 0x01, 1088-bit rate). */
void keccak256(const uint8_t* data, size_t len, uint8_t* hash_out);

/* Ethereum Keccak-512 (padding 0x01, 576-bit rate). */
void keccak512(const uint8_t* data, size_t len, uint8_t* hash_out);

/* FIPS-202 SHA3-256 (padding 0x06, 1088-bit rate). */
void sha3_256(const uint8_t* data, size_t len, uint8_t* hash_out);

/* Verify a secp256k1 ECDSA signature.
 * msg_hash: 32 bytes (the keccak256 digest actually signed)
 * sig:      64 bytes, r || s, each big-endian
 * pubkey:   64 bytes, x || y, each big-endian, without the 0x04 prefix
 * Returns 1 when the signature is valid, 0 otherwise.
 */
int verify_ecdsa(const uint8_t* msg_hash, const uint8_t* sig, const uint8_t* pubkey);

/* Recover the uncompressed public key (64 bytes, x || y) from a signature.
 * recovery_id is 0 or 1 (the parity of R.y).
 * Returns 1 on success, 0 if the signature is not recoverable.
 */
int recover_pubkey(const uint8_t* msg_hash, const uint8_t* sig, uint8_t recovery_id,
                   uint8_t* pubkey_out);

/* Derive the 20-byte Ethereum address from an uncompressed public key:
 * keccak256(x || y)[12..32].
 */
void pubkey_to_address(const uint8_t* pubkey, uint8_t* address_out);

#ifdef __cplusplus
}
#endif

#endif /* TIGER_CRYPTO_H */
