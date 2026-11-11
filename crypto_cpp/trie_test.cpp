/* Tests for the C++ RLP and Merkle Patricia Trie.
 *
 * The expected roots are the canonical values published for the Ethereum
 * reference implementations, so a passing run means this implementation agrees
 * with every other client on the consensus-critical encoding. */
#include "trie.hpp"

#include <cstdio>
#include <cstring>
#include <string>
#include <vector>

namespace {

int checks = 0;
int failures = 0;

void check(bool ok, const char* what) {
    ++checks;
    if (!ok) {
        ++failures;
        std::printf("FAIL %s\n", what);
    } else {
        std::printf("ok   %s\n", what);
    }
}

std::string to_hex(const uint8_t* data, std::size_t len) {
    static const char* digits = "0123456789abcdef";
    std::string out;
    out.reserve(len * 2);
    for (std::size_t i = 0; i < len; ++i) {
        out.push_back(digits[data[i] >> 4]);
        out.push_back(digits[data[i] & 0x0f]);
    }
    return out;
}

std::string to_hex(const tiger::Bytes& b) {
    return to_hex(b.data(), b.size());
}

tiger::Bytes bytes(const std::string& s) {
    return tiger::Bytes(s.begin(), s.end());
}

void insert(tiger::Trie& trie, const std::string& key, const std::string& value) {
    trie.insert(reinterpret_cast<const uint8_t*>(key.data()), key.size(),
                reinterpret_cast<const uint8_t*>(value.data()), value.size());
}

std::string root_hex(const tiger::Trie& trie) {
    uint8_t hash[32];
    trie.root_hash(hash);
    return to_hex(hash, 32);
}

/* ------------------------------------------------------------------ */
/* RLP: the Ethereum wiki's reference examples.                        */
/* ------------------------------------------------------------------ */

void test_rlp_wiki_vectors() {
    tiger::Bytes out;

    out.clear();
    tiger::rlp_encode_bytes(bytes("dog"), out);
    check(to_hex(out) == "83646f67", "rlp: \"dog\" -> 83646f67");

    out.clear();
    tiger::Bytes payload;
    tiger::rlp_encode_bytes(bytes("cat"), payload);
    tiger::rlp_encode_bytes(bytes("dog"), payload);
    tiger::rlp_encode_list_payload(payload, out);
    check(to_hex(out) == "c88363617483646f67", "rlp: [\"cat\",\"dog\"] -> c88363617483646f67");

    out.clear();
    tiger::rlp_encode_bytes(tiger::Bytes{}, out);
    check(to_hex(out) == "80", "rlp: empty string -> 80");

    out.clear();
    tiger::rlp_encode_list_payload(tiger::Bytes{}, out);
    check(to_hex(out) == "c0", "rlp: empty list -> c0");

    /* A single byte below 0x80 is its own encoding. */
    out.clear();
    tiger::rlp_encode_bytes(tiger::Bytes{0x0f}, out);
    check(to_hex(out) == "0f", "rlp: 0x0f -> 0f");

    /* 1024 needs two payload bytes, so the long-form prefix 0x82 appears. */
    out.clear();
    tiger::rlp_encode_bytes(tiger::Bytes{0x04, 0x00}, out);
    check(to_hex(out) == "820400", "rlp: 0x0400 -> 820400");

    /* The set-theoretic representation of three: [[], [[]], [[], [[]]]]. */
    const tiger::Bytes empty_list = tiger::Bytes{0xc0};          /* []        */
    const tiger::Bytes list_of_empty = tiger::Bytes{0xc1, 0xc0}; /* [[]]      */
    tiger::Bytes third;
    tiger::rlp_encode_list_payload(tiger::Bytes{0xc0, 0xc1, 0xc0}, third);
    tiger::Bytes three;
    tiger::rlp_encode_list_payload(
        tiger::Bytes{0xc0, 0xc1, 0xc0, 0xc3, 0xc0, 0xc1, 0xc0}, three);
    check(to_hex(three) == "c7c0c1c0c3c0c1c0", "rlp: three -> c7c0c1c0c3c0c1c0");
    check(to_hex(empty_list) == "c0" && to_hex(list_of_empty) == "c1c0" &&
              to_hex(third) == "c3c0c1c0",
          "rlp: the nested components of three encode as expected");

    /* A 56-byte string must use the long form. */
    out.clear();
    tiger::rlp_encode_bytes(bytes("Lorem ipsum dolor sit amet, consectetur adipisicing elit"), out);
    check(out.size() == 58 && out[0] == 0xb8, "rlp: 56-byte string uses the long form");
}

/* ------------------------------------------------------------------ */
/* Hex-prefix encoding: Yellow Paper Appendix C examples.              */
/* ------------------------------------------------------------------ */

void test_hex_prefix() {
    check(tiger::hex_prefix({}, false) == tiger::Bytes{0x00}, "hex-prefix: empty extension -> 00");
    check(tiger::hex_prefix({}, true) == tiger::Bytes{0x20}, "hex-prefix: empty leaf -> 20");
    check(tiger::hex_prefix({0x1}, false) == tiger::Bytes{0x11}, "hex-prefix: [1] extension -> 11");
    check(tiger::hex_prefix({0x1, 0x2}, false) == tiger::Bytes({0x00, 0x12}),
          "hex-prefix: [1,2] extension -> 0012");
    check(tiger::hex_prefix({0x1}, true) == tiger::Bytes{0x31}, "hex-prefix: [1] leaf -> 31");
    check(tiger::hex_prefix({0x1, 0x2}, true) == tiger::Bytes({0x20, 0x12}),
          "hex-prefix: [1,2] leaf -> 2012");

    /* Round trips. */
    for (const auto& tc : std::vector<std::pair<std::vector<uint8_t>, bool>>{
             {{}, false}, {{}, true}, {{1}, true}, {{1, 2, 3}, true}, {{15, 0, 7, 8, 9}, false}}) {
        const tiger::Bytes encoded = tiger::hex_prefix(tc.first, tc.second);
        std::vector<uint8_t> path;
        bool leaf = false;
        const bool ok = tiger::decode_hex_prefix(encoded, path, leaf);
        check(ok && path == tc.first && leaf == tc.second, "hex-prefix: round trip");
    }

    /* A non-zero low nibble on an even-length path is invalid. */
    std::vector<uint8_t> path;
    bool leaf = false;
    check(!tiger::decode_hex_prefix(tiger::Bytes({0x01, 0x23}), path, leaf),
          "hex-prefix: rejects a non-zero padding nibble");
}

/* ------------------------------------------------------------------ */
/* Trie: canonical roots from the reference implementations.           */
/* ------------------------------------------------------------------ */

void test_empty_trie() {
    tiger::Trie trie;
    check(root_hex(trie) ==
              "56e81f171bcc55a6ff8345e692c0f86e5b48e01b996cadc001622fb5e363b421",
          "trie: the empty trie has the canonical root");
    check(to_hex(trie.root_encoding()) == "80", "trie: the empty root encodes to 0x80");
}

/* The canonical example from the Ethereum wiki's trie page, cross-checked
 * against ethereum/py-trie. */
void test_wiki_example() {
    tiger::Trie trie;
    insert(trie, "do", "verb");
    insert(trie, "dog", "puppy");
    insert(trie, "doge", "coin");
    insert(trie, "horse", "stallion");

    check(root_hex(trie) ==
              "5991bb8c6514148a29db676a14ac506cd2cd5775ace63c30a4fe457715e9ac84",
          "trie: the Ethereum wiki example matches the reference root");
    check(trie.size() == 4, "trie: size counts the inserted keys");
}

/* Write `value` as a 32-byte big-endian word, the encoding a trie key uses. */
void word32(uint32_t value, uint8_t out[32]) {
    std::memset(out, 0, 32);
    for (int i = 0; i < 4; ++i) {
        out[31 - i] = static_cast<uint8_t>((value >> (8 * i)) & 0xff);
    }
}

/* 256 keys exercising branch and extension splits at depth, cross-checked
 * against ethereum/py-trie. Keys are the 32-byte big-endian words 0..255 and
 * values the 32-byte words i*7+1. */
void test_large_keyset() {
    tiger::Trie trie;
    for (uint32_t i = 0; i < 256; ++i) {
        uint8_t key[32];
        uint8_t value[32];
        word32(i, key);
        word32(i * 7 + 1, value);
        trie.insert(key, 32, value, 32);
    }

    check(root_hex(trie) ==
              "5643a50bb43d87e2552644da07ccf5f8f63b657eb244f7a94f67cfde62771fb3",
          "trie: a 256-key keyset matches the reference root");

    /* Every key must read back through the branch and extension structure the
     * root was computed from. */
    bool all_found = true;
    for (uint32_t i = 0; i < 256; ++i) {
        uint8_t key[32];
        uint8_t expected[32];
        word32(i, key);
        word32(i * 7 + 1, expected);
        tiger::Bytes value;
        if (!trie.get(key, 32, value) || value != tiger::Bytes(expected, expected + 32)) {
            all_found = false;
            break;
        }
    }
    check(all_found, "trie: every key in the 256-key keyset reads back");
}

void test_insert_get_delete_semantics() {
    tiger::Trie trie;
    insert(trie, "do", "verb");
    insert(trie, "dog", "puppy");
    insert(trie, "doge", "coin");
    insert(trie, "horse", "stallion");

    tiger::Bytes value;
    check(trie.get(reinterpret_cast<const uint8_t*>("do"), 2, value) && value == bytes("verb"),
          "trie: get returns the stored value");
    check(trie.get(reinterpret_cast<const uint8_t*>("dog"), 3, value) && value == bytes("puppy"),
          "trie: get distinguishes a prefix of a longer key");
    check(trie.get(reinterpret_cast<const uint8_t*>("doge"), 4, value) && value == bytes("coin"),
          "trie: get returns the value of a longer key");
    check(!trie.get(reinterpret_cast<const uint8_t*>("d"), 1, value),
          "trie: get rejects a missing prefix");
    check(!trie.get(reinterpret_cast<const uint8_t*>("doge2"), 5, value),
          "trie: get rejects a missing extension");
    check(!trie.get(reinterpret_cast<const uint8_t*>("zebra"), 5, value),
          "trie: get rejects an absent key");
}

void test_overwrite_changes_the_root() {
    tiger::Trie a;
    insert(a, "dog", "puppy");
    const std::string before = root_hex(a);

    insert(a, "dog", "cat");
    tiger::Bytes value;
    check(a.get(reinterpret_cast<const uint8_t*>("dog"), 3, value) && value == bytes("cat"),
          "trie: overwriting replaces the value");
    check(root_hex(a) != before, "trie: overwriting changes the root");

    /* A single-entry trie hashes the same regardless of insertion order. */
    tiger::Trie b;
    insert(b, "dog", "cat");
    check(root_hex(a) == root_hex(b), "trie: a single-entry trie is order independent");
}

/* A key that is a strict prefix of another stores its value in the branch's
 * seventeenth slot. This is the case that separates a correct branch encoding
 * from one that drops the value. */
void test_branch_value_slot() {
    tiger::Trie trie;
    insert(trie, "a", "1");
    insert(trie, "ab", "2");

    tiger::Bytes value;
    check(trie.get(reinterpret_cast<const uint8_t*>("a"), 1, value) && value == bytes("1"),
          "trie: a key that prefixes another keeps its value");
    check(trie.get(reinterpret_cast<const uint8_t*>("ab"), 2, value) && value == bytes("2"),
          "trie: the longer key keeps its own value");

    /* The root must differ from a trie holding only the longer key. */
    tiger::Trie longer_only;
    insert(longer_only, "ab", "2");
    check(root_hex(trie) != root_hex(longer_only),
          "trie: the prefix key contributes to the root");
}

void test_order_independence() {
    tiger::Trie forward;
    insert(forward, "do", "verb");
    insert(forward, "dog", "puppy");
    insert(forward, "doge", "coin");
    insert(forward, "horse", "stallion");

    tiger::Trie reverse;
    insert(reverse, "horse", "stallion");
    insert(reverse, "doge", "coin");
    insert(reverse, "dog", "puppy");
    insert(reverse, "do", "verb");

    check(root_hex(forward) == root_hex(reverse),
          "trie: the root does not depend on insertion order");
}

/* Empty values are legal and must still create the key. */
void test_empty_value() {
    tiger::Trie with_empty;
    insert(with_empty, "a", "");
    insert(with_empty, "b", "x");

    tiger::Trie without;
    insert(without, "b", "x");

    check(root_hex(with_empty) != root_hex(without),
          "trie: an empty value still contributes to the root");
    tiger::Bytes value;
    check(with_empty.get(reinterpret_cast<const uint8_t*>("a"), 1, value) && value.empty(),
          "trie: an empty value reads back as empty");
}

/* Keys sharing a long common prefix force an extension node, and the keys must
 * still be found through it. */
void test_long_shared_prefix() {
    tiger::Trie trie;
    insert(trie, "aaaaaaaaaaaaaaaaaaaaaaaaaaaa", "1");
    insert(trie, "aaaaaaaaaaaaaaaaaaaaaaaaaaab", "2");

    tiger::Bytes value;
    check(trie.get(reinterpret_cast<const uint8_t*>("aaaaaaaaaaaaaaaaaaaaaaaaaaaa"), 28, value) &&
              value == bytes("1"),
          "trie: a key behind an extension is found");
    check(trie.get(reinterpret_cast<const uint8_t*>("aaaaaaaaaaaaaaaaaaaaaaaaaaab"), 28, value) &&
              value == bytes("2"),
          "trie: the sibling behind the extension is found");
}

/* Deterministic pseudo-random keyset: the same input must always give the same
 * root, and a one-byte change must change it. */
void test_deterministic_large_keyset() {
    auto build = [](uint32_t seed) {
        tiger::Trie trie;
        uint32_t state = seed;
        for (int i = 0; i < 1000; ++i) {
            uint8_t key[32];
            uint8_t value[32];
            for (std::size_t b = 0; b < 32; ++b) {
                state = state * 1664525u + 1013904223u;
                key[b] = static_cast<uint8_t>(state >> 24);
                value[b] = static_cast<uint8_t>(state >> 16);
            }
            trie.insert(key, 32, value, 32);
        }
        return trie;
    };

    const std::string first = root_hex(build(1));
    const std::string second = root_hex(build(1));
    const std::string different = root_hex(build(2));

    check(first == second, "trie: the same keyset gives the same root");
    check(first != different, "trie: a different keyset gives a different root");
}

/* A lookup on a key that diverges from every stored key must not be confused
 * with a key that shares a prefix with them. */
void test_lookup_near_misses() {
    tiger::Trie trie;
    for (int i = 0; i < 100; ++i) {
        char key[16];
        std::snprintf(key, sizeof(key), "key%05d", i);
        insert(trie, key, "v");
    }

    tiger::Bytes value;
    check(trie.get(reinterpret_cast<const uint8_t*>("key00042"), 8, value),
          "trie: an existing key in a dense set is found");
    check(!trie.get(reinterpret_cast<const uint8_t*>("key00100"), 8, value),
          "trie: a key just past the end of a dense set is absent");
    check(!trie.get(reinterpret_cast<const uint8_t*>("key"), 3, value),
          "trie: a bare prefix of every key is absent");
}

} /* namespace */

int main() {
    test_rlp_wiki_vectors();
    test_hex_prefix();
    test_empty_trie();
    test_wiki_example();
    test_large_keyset();
    test_insert_get_delete_semantics();
    test_overwrite_changes_the_root();
    test_branch_value_slot();
    test_order_independence();
    test_empty_value();
    test_long_shared_prefix();
    test_deterministic_large_keyset();
    test_lookup_near_misses();

    std::printf("\n%d/%d checks passed\n", checks - failures, checks);
    return failures == 0 ? 0 : 1;
}
