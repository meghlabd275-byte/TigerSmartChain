/* TigerSmartChain: RLP encoding and the Merkle Patricia Trie.
 *
 * This is the consensus-critical hot path. Every block has to agree on
 * `stateRoot`, `transactionsRoot` and `receiptsRoot`, and every one of those is
 * a keccak256 hash of an RLP-encoded trie root. A single non-canonical node
 * encoding yields a different root and the chain forks, so the encoding rules
 * are implemented literally from the Yellow Paper (Appendix B for RLP,
 * Appendix C for the hex-prefix encoding, Appendix D for the trie).
 *
 * The trie is built from a sorted key/value set and hashed in one recursive
 * pass. That construction is canonical by itself: for a given key/value set the
 * Merkle Patricia Trie has exactly one canonical form, so no separate
 * normalisation pass is needed to collapse single-child branches or merge
 * chained extensions. The recursion derives the form directly.
 *
 * Thread safety: a `tiger_trie` handle is not shared between threads. Distinct
 * handles are independent, and no global mutable state is used, so one handle
 * per thread is safe.
 */
#ifndef TIGER_TRIE_HPP
#define TIGER_TRIE_HPP

#include "tiger_crypto.h"

#include <cstddef>
#include <cstdint>
#include <map>
#include <vector>

namespace tiger {

using Bytes = std::vector<uint8_t>;

/* The largest RLP encoding this module will produce for a single trie node.
 * A branch node holds sixteen 32-byte hashes plus its own value, which is
 * well under this bound; the limit exists so a corrupt or hostile input is
 * rejected rather than overrunning a buffer. */
constexpr std::size_t MAX_NODE_ENCODING = 1024;

/* ------------------------------------------------------------------ */
/* RLP                                                                 */
/* ------------------------------------------------------------------ */

/* Append the RLP length prefix for a payload of `len` bytes. */
void rlp_encode_length(std::size_t len, uint8_t offset, Bytes& out);

/* Append the RLP encoding of a byte string. A single byte below 0x80 is its
 * own encoding; everything else is a length prefix followed by the bytes. */
void rlp_encode_bytes(const uint8_t* data, std::size_t len, Bytes& out);

inline void rlp_encode_bytes(const Bytes& data, Bytes& out) {
    rlp_encode_bytes(data.data(), data.size(), out);
}

/* Append the RLP encoding of a list whose payload has already been encoded. */
void rlp_encode_list_payload(const Bytes& payload, Bytes& out);

/* ------------------------------------------------------------------ */
/* Trie                                                                */
/* ------------------------------------------------------------------ */

/* The compact ("hex-prefix") encoding of a nibble path: Yellow Paper
 * Appendix C. Bit 1 of the first nibble flags a leaf, bit 0 flags an
 * odd-length path. An even-length path gets a leading zero nibble so the
 * result is a whole number of bytes. */
Bytes hex_prefix(const std::vector<uint8_t>& path, bool leaf);

/* Decode a compact-encoded path back to `(nibbles, is_leaf)`.
 * Returns false when the encoding is not valid. */
bool decode_hex_prefix(const Bytes& encoded, std::vector<uint8_t>& path_out, bool& leaf_out);

/* A Merkle Patricia Trie over byte keys.
 *
 * Keys are used as given. The Ethereum state trie is keyed by
 * keccak256(address) and the transactions trie by RLP(index); callers hash the
 * key themselves, exactly as the reference implementations do. */
class Trie {
public:
    /* Insert or overwrite `value` at `key`. */
    void insert(const uint8_t* key, std::size_t key_len,
                const uint8_t* value, std::size_t value_len);

    /* Number of stored entries. */
    std::size_t size() const { return entries_.size(); }

    /* The RLP encoding of the root node. The empty trie encodes to 0x80. */
    Bytes root_encoding() const;

    /* keccak256 of the root encoding. The empty trie hashes to
     * 56e81f171bcc55a6ff8345e692c0f86e5b48e01b996cadc001622fb5e363b421. */
    void root_hash(uint8_t out[32]) const;

    /* Look up a key. Returns false when the key is absent. */
    bool get(const uint8_t* key, std::size_t key_len, Bytes& value_out) const;

private:
    /* A sorted key/value map. Iteration order is byte order, which is also
     * nibble order, so the recursion below sees keys in trie order. */
    std::map<Bytes, Bytes> entries_;
};

/* ------------------------------------------------------------------ */
/* C ABI                                                               */
/* ------------------------------------------------------------------ */
/* The trie is an opaque heap object rather than a fixed-size struct, so the
 * ABI carries no layout assumptions across the language boundary. Callers
 * create one handle, insert into it, read the root, and destroy it. */
using tiger_trie_handle = void*;

extern "C" {

/* Create an empty trie. Returns null only if allocation fails. */
tiger_trie_handle tiger_trie_new(void);

/* Destroy a trie created by `tiger_trie_new`. Passing null is a no-op. */
void tiger_trie_free(tiger_trie_handle trie);

/* Insert or overwrite a key/value pair. A null handle is ignored. */
void tiger_trie_insert(tiger_trie_handle trie, const uint8_t* key, std::size_t key_len,
                       const uint8_t* value, std::size_t value_len);

/* Write the keccak256 root hash into `out` (32 bytes). A null handle yields
 * the canonical empty-trie root, so a caller that skipped `tiger_trie_new`
 * gets a defined answer instead of uninitialised memory. */
void tiger_trie_root(tiger_trie_handle trie, uint8_t* out);

/* Write the RLP encoding of the root node into `out`, which must hold
 * `out_cap` bytes. Returns the number of bytes written, or the required
 * capacity when it exceeds `out_cap`, in which case nothing is written.
 * A null handle returns 0. */
std::size_t tiger_trie_root_encoding(tiger_trie_handle trie, uint8_t* out, std::size_t out_cap);

/* Number of stored entries. A null handle is empty. */
std::size_t tiger_trie_len(tiger_trie_handle trie);

/* Look up a key. On success writes the value into `out` (which must hold
 * `out_cap` bytes) and returns its length. Returns `(std::size_t)-1` when the
 * key is absent, and the required capacity when it does not fit in `out_cap`. */
std::size_t tiger_trie_get(tiger_trie_handle trie, const uint8_t* key, std::size_t key_len,
                           uint8_t* out, std::size_t out_cap);

} /* extern "C" */

} /* namespace tiger */

#endif /* TIGER_TRIE_HPP */
