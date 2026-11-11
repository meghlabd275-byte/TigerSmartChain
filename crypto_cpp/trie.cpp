/* TigerSmartChain: RLP encoding and the Merkle Patricia Trie.
 *
 * See trie.hpp for the design notes and the specification references.
 */
#include "trie.hpp"

#include <algorithm>

namespace tiger {

/* ------------------------------------------------------------------ */
/* RLP                                                                 */
/* ------------------------------------------------------------------ */

void rlp_encode_length(std::size_t len, uint8_t offset, Bytes& out) {
    if (len < 56) {
        out.push_back(static_cast<uint8_t>(offset + len));
        return;
    }
    /* Long form: the prefix says how many big-endian length bytes follow. */
    uint8_t buf[8];
    std::size_t n = 0;
    std::size_t v = len;
    while (v > 0) {
        buf[n++] = static_cast<uint8_t>(v & 0xff);
        v >>= 8;
    }
    out.push_back(static_cast<uint8_t>(offset + 55 + n));
    for (std::size_t i = n; i > 0; --i) {
        out.push_back(buf[i - 1]);
    }
}

void rlp_encode_bytes(const uint8_t* data, std::size_t len, Bytes& out) {
    if (len == 1 && data[0] < 0x80) {
        out.push_back(data[0]);
        return;
    }
    rlp_encode_length(len, 0x80, out);
    out.insert(out.end(), data, data + len);
}

void rlp_encode_list_payload(const Bytes& payload, Bytes& out) {
    rlp_encode_length(payload.size(), 0xc0, out);
    out.insert(out.end(), payload.begin(), payload.end());
}

/* ------------------------------------------------------------------ */
/* Hex-prefix encoding                                                 */
/* ------------------------------------------------------------------ */

Bytes hex_prefix(const std::vector<uint8_t>& path, bool leaf) {
    const bool odd = (path.size() % 2) != 0;
    uint8_t flag = 0;
    if (leaf) flag |= 2;
    if (odd) flag |= 1;

    Bytes out;
    out.reserve(path.size() / 2 + 1);

    std::size_t i = 0;
    if (odd) {
        out.push_back(static_cast<uint8_t>((flag << 4) | (path[0] & 0x0f)));
        i = 1;
    } else {
        out.push_back(static_cast<uint8_t>(flag << 4));
    }
    for (; i + 1 < path.size(); i += 2) {
        out.push_back(static_cast<uint8_t>(((path[i] & 0x0f) << 4) | (path[i + 1] & 0x0f)));
    }
    return out;
}

bool decode_hex_prefix(const Bytes& encoded, std::vector<uint8_t>& path_out, bool& leaf_out) {
    path_out.clear();
    if (encoded.empty()) {
        return false;
    }
    const uint8_t flag = static_cast<uint8_t>(encoded[0] >> 4);
    if (flag > 3) {
        return false;
    }
    const bool leaf = (flag & 2) != 0;
    const bool odd = (flag & 1) != 0;

    /* On an even-length path the low nibble of the first byte is padding and
     * must be zero; a non-zero value means a malformed encoding. */
    if (!odd && (encoded[0] & 0x0f) != 0) {
        return false;
    }

    if (odd) {
        path_out.push_back(static_cast<uint8_t>(encoded[0] & 0x0f));
    }
    for (std::size_t i = 1; i < encoded.size(); ++i) {
        path_out.push_back(static_cast<uint8_t>(encoded[i] >> 4));
        path_out.push_back(static_cast<uint8_t>(encoded[i] & 0x0f));
    }

    leaf_out = leaf;
    return true;
}

/* ------------------------------------------------------------------ */
/* Trie                                                                */
/* ------------------------------------------------------------------ */

namespace {

/* A nibble path and the value stored at its end. */
struct Entry {
    std::vector<uint8_t> nibbles;
    Bytes value;
};

/* Split a byte key into nibbles, high nibble first. */
std::vector<uint8_t> bytes_to_nibbles(const Bytes& key) {
    std::vector<uint8_t> nibbles;
    nibbles.reserve(key.size() * 2);
    for (uint8_t b : key) {
        nibbles.push_back(static_cast<uint8_t>(b >> 4));
        nibbles.push_back(static_cast<uint8_t>(b & 0x0f));
    }
    return nibbles;
}

/* Length of the common prefix of two nibble ranges. */
std::size_t common_prefix_len(const std::vector<uint8_t>& a, std::size_t a_from,
                              const std::vector<uint8_t>& b, std::size_t b_from) {
    /* A range that starts at or past the end of its key shares nothing: that
     * key has already ended, which is exactly the case that puts a value in a
     * branch's seventeenth slot. Without this check `a.size() - a_from`
     * underflows and the loop reads out of bounds. */
    if (a_from >= a.size() || b_from >= b.size()) {
        return 0;
    }
    const std::size_t max = std::min(a.size() - a_from, b.size() - b_from);
    std::size_t n = 0;
    while (n < max && a[a_from + n] == b[b_from + n]) {
        ++n;
    }
    return n;
}

/* Append the RLP encoding of a child node reference.
 *
 * A reference is inlined when the child's encoding is shorter than 32 bytes and
 * replaced by its keccak256 hash otherwise. This is the rule that makes the
 * root a commitment to the whole trie, and it is applied identically for
 * extension children and for branch children. */
void append_child_ref(const Bytes& child, Bytes& payload) {
    if (child.size() < 32) {
        payload.insert(payload.end(), child.begin(), child.end());
        return;
    }
    uint8_t hash[32];
    keccak256(child.data(), child.size(), hash);
    rlp_encode_bytes(hash, 32, payload);
}

/* Recursively encode the node covering `entries[lo, hi)` after the first
 * `depth` nibbles of every key in that range.
 *
 * The range is sorted, so the longest prefix shared by all of its keys is the
 * prefix shared by the first and the last entry. That property is what makes
 * the recursion exact: it is the only shared prefix to skip, and every entry
 * that does not continue it diverges here.
 *
 * The node is therefore:
 *   - a leaf, when one entry remains;
 *   - an extension wrapping a deeper node, when the range shares more nibbles;
 *   - a branch, otherwise.
 *
 * This derives the canonical form directly. A branch is emitted only where two
 * keys actually diverge, so a single-child branch can never be produced and no
 * post-pass has to collapse one. */
void encode_node(const std::vector<Entry>& entries, std::size_t lo, std::size_t hi,
                 std::size_t depth, Bytes& out) {
    /* One key left: a leaf holding the rest of its path. */
    if (hi - lo == 1) {
        const Entry& e = entries[lo];
        const std::vector<uint8_t> path(
            e.nibbles.begin() + static_cast<std::ptrdiff_t>(depth), e.nibbles.end());

        Bytes payload;
        rlp_encode_bytes(hex_prefix(path, true), payload);
        rlp_encode_bytes(e.value, payload);
        rlp_encode_list_payload(payload, out);
        return;
    }

    const std::size_t shared = common_prefix_len(entries[lo].nibbles, depth,
                                                 entries[hi - 1].nibbles, depth);
    if (shared > 0) {
        const std::vector<uint8_t> path(
            entries[lo].nibbles.begin() + static_cast<std::ptrdiff_t>(depth),
            entries[lo].nibbles.begin() + static_cast<std::ptrdiff_t>(depth + shared));

        Bytes child;
        encode_node(entries, lo, hi, depth + shared, child);

        Bytes payload;
        rlp_encode_bytes(hex_prefix(path, false), payload);
        append_child_ref(child, payload);
        rlp_encode_list_payload(payload, out);
        return;
    }

    /* At least two keys diverge here. The entries are sorted, so each nibble's
     * entries form one contiguous run and a single pass groups them.
     *
     * A key that ends exactly at this depth has no nibble here, so it sorts
     * first within the range and would otherwise block the scan: skip past
     * those entries before grouping, and collect their value separately. */
    std::size_t start = lo;
    while (start < hi && entries[start].nibbles.size() <= depth) {
        ++start;
    }

    Bytes payload;
    Bytes branch_value;
    std::size_t i = start;
    for (std::size_t nibble = 0; nibble < 16; ++nibble) {
        const std::size_t from = i;
        while (i < hi && entries[i].nibbles[depth] == nibble) {
            ++i;
        }

        if (i == from) {
            /* No child for this nibble: the empty byte string. */
            payload.push_back(0x80);
            continue;
        }

        Bytes child;
        encode_node(entries, from, i, depth + 1, child);
        append_child_ref(child, payload);
    }

    /* A key that ends exactly at this depth stores its value in the branch's
     * seventeenth slot, which is the last of the seventeen items. */
    for (std::size_t j = lo; j < start; ++j) {
        if (entries[j].nibbles.size() == depth) {
            branch_value = entries[j].value;
            break;
        }
    }
    rlp_encode_bytes(branch_value, payload);
    rlp_encode_list_payload(payload, out);
}

/* Look up `want` in the subtree covering `entries[lo, hi)` at `depth`.
 *
 * The descent mirrors `encode_node` exactly, so a key is found only on the
 * path the encoding actually contains. */
bool walk(const std::vector<Entry>& entries, std::size_t lo, std::size_t hi, std::size_t depth,
          const std::vector<uint8_t>& want, Bytes& value_out) {
    if (hi - lo == 1) {
        if (entries[lo].nibbles.size() != want.size()) {
            return false;
        }
        if (!std::equal(entries[lo].nibbles.begin(), entries[lo].nibbles.end(), want.begin())) {
            return false;
        }
        value_out = entries[lo].value;
        return true;
    }

    const std::size_t shared = common_prefix_len(entries[lo].nibbles, depth,
                                                 entries[hi - 1].nibbles, depth);
    if (shared > 0) {
        /* The key must continue the whole extension. */
        if (want.size() < depth + shared) {
            return false;
        }
        for (std::size_t k = 0; k < shared; ++k) {
            if (want[depth + k] != entries[lo].nibbles[depth + k]) {
                return false;
            }
        }
        return walk(entries, lo, hi, depth + shared, want, value_out);
    }

    if (want.size() == depth) {
        for (std::size_t j = lo; j < hi; ++j) {
            if (entries[j].nibbles.size() == depth) {
                value_out = entries[j].value;
                return true;
            }
        }
        return false;
    }

    const uint8_t nibble = want[depth];
    for (std::size_t i = lo; i < hi; ++i) {
        const std::vector<uint8_t>& key = entries[i].nibbles;
        if (key.size() <= depth || key[depth] != nibble) {
            continue;
        }
        std::size_t to = i;
        while (to < hi && entries[to].nibbles.size() > depth &&
               entries[to].nibbles[depth] == nibble) {
            ++to;
        }
        return walk(entries, i, to, depth + 1, want, value_out);
    }
    return false;
}

} /* namespace */

void Trie::insert(const uint8_t* key, std::size_t key_len,
                  const uint8_t* value, std::size_t value_len) {
    Bytes k(key, key + key_len);
    entries_[k] = Bytes(value, value + value_len);
}

Bytes Trie::root_encoding() const {
    if (entries_.empty()) {
        /* The empty trie is the empty byte string, which RLP-encodes to 0x80. */
        return Bytes{0x80};
    }

    std::vector<Entry> items;
    items.reserve(entries_.size());
    for (const auto& kv : entries_) {
        items.push_back(Entry{bytes_to_nibbles(kv.first), kv.second});
    }

    Bytes out;
    encode_node(items, 0, items.size(), 0, out);
    return out;
}

void Trie::root_hash(uint8_t out[32]) const {
    const Bytes encoding = root_encoding();
    keccak256(encoding.data(), encoding.size(), out);
}

bool Trie::get(const uint8_t* key, std::size_t key_len, Bytes& value_out) const {
    if (entries_.empty()) {
        return false;
    }

    std::vector<Entry> items;
    items.reserve(entries_.size());
    for (const auto& kv : entries_) {
        items.push_back(Entry{bytes_to_nibbles(kv.first), kv.second});
    }

    const std::vector<uint8_t> want = bytes_to_nibbles(Bytes(key, key + key_len));
    return walk(items, 0, items.size(), 0, want, value_out);
}

/* ------------------------------------------------------------------ */
/* C ABI                                                               */
/* ------------------------------------------------------------------ */

extern "C" {

void* tiger_trie_new(void) {
    /* Not `new (std::nothrow)` alone: a throwing allocation must not escape a
     * C ABI boundary, and the project builds with -fno-exceptions, so a plain
     * `new` here is already infallible-or-abort. Wrap it so a null result is
     * the documented outcome if the allocator returns one. */
    Trie* trie = new Trie();
    return static_cast<void*>(trie);
}

void tiger_trie_free(void* handle) {
    delete static_cast<Trie*>(handle);
}

void tiger_trie_insert(void* handle, const uint8_t* key, std::size_t key_len,
                       const uint8_t* value, std::size_t value_len) {
    if (handle == nullptr) {
        return;
    }
    /* A null data pointer with a non-zero length is a caller bug; treat it as
     * empty rather than dereferencing null. */
    if (key == nullptr) {
        key_len = 0;
    }
    if (value == nullptr) {
        value_len = 0;
    }
    static_cast<Trie*>(handle)->insert(key, key_len, value, value_len);
}

void tiger_trie_root(void* handle, uint8_t* out) {
    if (out == nullptr) {
        return;
    }
    if (handle == nullptr) {
        /* The empty trie, so a caller that never created a handle still gets a
         * defined hash. */
        const uint8_t empty[1] = {0x80};
        keccak256(empty, 1, out);
        return;
    }
    static_cast<Trie*>(handle)->root_hash(out);
}

std::size_t tiger_trie_root_encoding(void* handle, uint8_t* out, std::size_t out_cap) {
    if (handle == nullptr) {
        return 0;
    }
    const Bytes encoding = static_cast<Trie*>(handle)->root_encoding();
    if (out == nullptr || encoding.size() > out_cap) {
        return encoding.size();
    }
    for (std::size_t i = 0; i < encoding.size(); ++i) {
        out[i] = encoding[i];
    }
    return encoding.size();
}

std::size_t tiger_trie_len(void* handle) {
    if (handle == nullptr) {
        return 0;
    }
    return static_cast<Trie*>(handle)->size();
}

std::size_t tiger_trie_get(void* handle, const uint8_t* key, std::size_t key_len, uint8_t* out,
                           std::size_t out_cap) {
    if (handle == nullptr) {
        return static_cast<std::size_t>(-1);
    }
    if (key == nullptr) {
        key_len = 0;
    }
    Bytes value;
    if (!static_cast<Trie*>(handle)->get(key, key_len, value)) {
        return static_cast<std::size_t>(-1);
    }
    if (out == nullptr || value.size() > out_cap) {
        return value.size();
    }
    for (std::size_t i = 0; i < value.size(); ++i) {
        out[i] = value[i];
    }
    return value.size();
}

} /* extern "C" */

} /* namespace tiger */
