/**
 * @file kzg_vectors.cpp
 * @brief Tests for the blob processor's crypto, RLP, gas and precompile paths
 *
 * Every expected value is either a published constant (NIST digests, the BLS
 * modulus, the EIP-4844 fee schedule) or produced here and cross-checked
 * against an independent implementation. A test that only asserted "no crash"
 * would not catch the failure this suite exists for: a verifier that accepts
 * anything.
 */

#include <cstdio>
#include <cstring>
#include <string>
#include <vector>

#include "blob_processor.hpp"
#include "crypto_utils.hpp"
#include "field_element.hpp"
#include "kzg.hpp"
#include "uint256.hpp"

using namespace tigerchain;
using namespace tigerchain::blob;

namespace {

int failures = 0;
int checks = 0;

void check(bool condition, const std::string& what) {
    ++checks;
    if (!condition) {
        ++failures;
        std::printf("  FAIL: %s\n", what.c_str());
    }
}

void check_hex(const std::vector<uint8_t>& value, const std::string& expected,
               const std::string& what) {
    check(crypto::bytes_to_hex(value) == expected, what + " (got " +
          crypto::bytes_to_hex(value) + ", want " + expected + ")");
}

void check_hex(const crypto::Hash256& value, const std::string& expected,
               const std::string& what) {
    check_hex(std::vector<uint8_t>(value.begin(), value.end()), expected, what);
}

// -----------------------------------------------------------------------------
// Hash functions
// -----------------------------------------------------------------------------

void test_hashes() {
    std::printf("hashes\n");
    const std::vector<uint8_t> empty;
    const std::vector<uint8_t> abc{'a', 'b', 'c'};

    // NIST / published vectors.
    check_hex(crypto::sha256(empty),
              "0xe3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855",
              "sha256(\"\")");
    check_hex(crypto::sha256(abc),
              "0xba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad",
              "sha256(\"abc\")");
    check_hex(crypto::sha3_256(empty),
              "0xa7ffc6f8bf1ed76651c14756a061d662f580ff4de43b49fa82d80a4b80f8434a",
              "sha3_256(\"\")");
    check_hex(crypto::sha3_256(abc),
              "0x3a985da74fe225b2045c172d6bd390bd855f086e3e9d525b46bfe24511431532",
              "sha3_256(\"abc\")");
    // Keccak-256 is not SHA3-256: same permutation, different padding byte.
    check_hex(crypto::keccak256(empty),
              "0xc5d2460186f7233c927e7db2dcc703c0e500b653ca82273b7bfad8045d85a470",
              "keccak256(\"\")");
    check_hex(crypto::keccak256(abc),
              "0x4e03657aea45a94fc7d47ba826c8d667c0d1e6e33a64a036ec44f58fa12d6c45",
              "keccak256(\"abc\")");
}

// -----------------------------------------------------------------------------
// Field elements
// -----------------------------------------------------------------------------

void test_field_elements() {
    std::printf("field elements\n");

    const auto one = FieldElement::from_hex("0x01");
    const auto two = FieldElement::from_hex("0x02");
    check(one.has_value() && two.has_value(), "parse small values");
    check(one->is_one() && !one->is_zero(), "one is one");
    check(FieldElement{}.is_zero(), "default is zero");
    check(one->is_canonical(), "one is canonical");

    // The modulus itself is not a valid scalar; one less than it is.
    const auto modulus =
        FieldElement::from_hex("0x73eda753299d7d483339d80809a1d80553bda402fffe5bfeffffffff00000001");
    check(modulus.has_value(), "parse the modulus");
    check(!modulus->is_canonical(), "the modulus is not canonical");
    const auto modulus_minus_one =
        FieldElement::from_hex("0x73eda753299d7d483339d80809a1d80553bda402fffe5bfeffffffff00000000");
    check(modulus_minus_one.has_value() && modulus_minus_one->is_canonical(),
          "modulus - 1 is canonical");

    // Real field arithmetic, checked against the defining property r == 0.
    check((*one + *two).to_hex() == "0x0000000000000000000000000000000000000000000000000000000000000003",
          "1 + 2 == 3");
    check((*one - *one).is_zero(), "1 - 1 == 0");
    check((*two * *two).to_hex() ==
              "0x0000000000000000000000000000000000000000000000000000000000000004",
          "2 * 2 == 4");
    check((*modulus_minus_one + *one).is_zero(), "(r-1) + 1 == 0");
    check((*modulus_minus_one + *one).is_zero(), "(r-1) + 1 wraps to zero");

    const auto inverse = FieldElement::inverse(*two);
    check(inverse.has_value(), "2 has an inverse");
    check((*inverse * *two).is_one(), "2 * 2^-1 == 1");
    check(!FieldElement::inverse(FieldElement{}).has_value(), "zero has no inverse");

    // Non-canonical encodings are rejected by the blob check, so a blob holding
    // r must not be accepted.
    check(!FieldElement::is_valid_encoding(modulus->bytes()),
          "the modulus is rejected as a blob element");
}

// -----------------------------------------------------------------------------
// Gas pricing
// -----------------------------------------------------------------------------

void test_gas_pricing() {
    std::printf("blob gas pricing\n");

    // fake_exponential(1, n, 3338477), computed with exact big-integer
    // arithmetic in an independent implementation.
    check(fake_exponential(1, 0, BLOB_GASPRICE_UPDATE_FRACTION).low64() == 1,
          "price at excess 0 is the minimum, 1");
    check(fake_exponential(1, 1, BLOB_GASPRICE_UPDATE_FRACTION).low64() == 1,
          "price at excess 1 is 1");
    check(fake_exponential(1, 131072, BLOB_GASPRICE_UPDATE_FRACTION).low64() == 1,
          "price at excess 131072 is 1");
    check(fake_exponential(1, 3 * 131072, BLOB_GASPRICE_UPDATE_FRACTION).low64() == 1,
          "price at excess 393216 is 1");
    check(fake_exponential(1, 6 * 131072, BLOB_GASPRICE_UPDATE_FRACTION).low64() == 1,
          "price at excess 786432 is 1");
    check(fake_exponential(1, 3338477, BLOB_GASPRICE_UPDATE_FRACTION).low64() == 2,
          "price at excess 3338477 is 2");
    check(fake_exponential(1, 6676954, BLOB_GASPRICE_UPDATE_FRACTION).low64() == 7,
          "price at excess 6676954 is 7");
    check(fake_exponential(1, 10000000, BLOB_GASPRICE_UPDATE_FRACTION).low64() == 19,
          "price at excess 10000000 is 19");

    // The curve must be monotonic, which a broken accumulator would not be.
    uint64_t previous = 0;
    bool monotonic = true;
    for (uint64_t excess = 0; excess < 40'000'000; excess += 250'000) {
        const uint64_t price = fake_exponential(1, excess, BLOB_GASPRICE_UPDATE_FRACTION).low64();
        if (price < previous) {
            monotonic = false;
            break;
        }
        previous = price;
    }
    check(monotonic, "the price curve never decreases");

    check(compute_excess_blob_gas(0, 0) == 0, "excess stays 0 below target");
    check(compute_excess_blob_gas(0, 3 * BLOB_GAS_PER_BLOB) == 0,
          "excess stays 0 at the target");
    check(compute_excess_blob_gas(0, 4 * BLOB_GAS_PER_BLOB) == BLOB_GAS_PER_BLOB,
          "one blob over target adds one blob of excess");
    check(compute_excess_blob_gas(1000, 6 * BLOB_GAS_PER_BLOB) ==
              1000 + 3 * BLOB_GAS_PER_BLOB,
          "excess accumulates on top of the parent");

    const uint64_t gas = 3 * BLOB_GAS_PER_BLOB;
    check(gas == 393216, "three blobs cost 393216 blob gas");
}

// -----------------------------------------------------------------------------
// RLP
// -----------------------------------------------------------------------------

void test_rlp() {
    std::printf("RLP\n");

    // Canonical encodings from the RLP specification.
    check_hex(rlp_encode_bytes({}), "0x80", "encode empty string");
    check_hex(rlp_encode_bytes({0x00}), "0x00", "encode a single low byte");
    check_hex(rlp_encode_bytes({0x0f}), "0x0f", "encode 0x0f");
    check_hex(rlp_encode_bytes({0x7f}), "0x7f", "encode 0x7f");
    check_hex(rlp_encode_bytes({0x80}), "0x8180", "encode 0x80");
    check_hex(rlp_encode_bytes({'d', 'o', 'g'}), "0x83646f67", "encode \"dog\"");
    check_hex(rlp_encode_list({rlp_encode_bytes({'c', 'a', 't'}),
                               rlp_encode_bytes({'d', 'o', 'g'})}),
              "0xc88363617483646f67", "encode the list [cat, dog]");
    check_hex(rlp_encode_bytes(std::vector<uint8_t>(55, 0xaa)),
              "0xb7" + std::string(55 * 2, 'a') + "",
              "encode a 55-byte string in the short form");
    check_hex(rlp_encode_bytes(std::vector<uint8_t>(56, 0xaa)),
              "0xb838" + std::string(56 * 2, 'a'),
              "encode a 56-byte string in the long form");

    // Round trips.
    for (const std::vector<uint8_t>& value :
         std::vector<std::vector<uint8_t>>{{}, {0x00}, {0x7f}, {0x80}, {0xff, 0x00, 0x01},
                                           std::vector<uint8_t>(200, 0x5a)}) {
        const auto encoded = rlp_encode_bytes(value);
        std::vector<uint8_t> decoded;
        size_t consumed = 0;
        check(rlp_decode_bytes(encoded.data(), encoded.size(), decoded, consumed) &&
                  consumed == encoded.size() && decoded == value,
              "round-trip a " + std::to_string(value.size()) + "-byte string");
    }

    // Non-canonical encodings must be rejected, otherwise one transaction
    // would have two valid encodings and therefore two hashes.
    std::vector<uint8_t> out;
    size_t consumed = 0;
    check(!rlp_decode_bytes(reinterpret_cast<const uint8_t*>("\x81\x01"), 2, out, consumed),
          "reject a length-prefixed single low byte");
    check(!rlp_decode_bytes(reinterpret_cast<const uint8_t*>("\xb8\x01\xaa"), 3, out, consumed),
          "reject a long form used for a 1-byte payload");
    {
        // 0xb9 announces a two-byte length whose first byte is zero, which is
        // not a canonical encoding of any length.
        std::vector<uint8_t> bad{0xb9, 0x00, 0x38};
        bad.insert(bad.end(), 56, 'a');
        check(!rlp_decode_bytes(bad.data(), bad.size(), out, consumed),
              "reject a length with a leading zero byte");
    }
    check(!rlp_decode_bytes(reinterpret_cast<const uint8_t*>("\x83\x61\x62"), 3, out, consumed),
          "reject a payload shorter than its length claims");

    // A list whose items are nested lists must round-trip.
    const auto inner = rlp_encode_list({rlp_encode_bytes({0x01}), rlp_encode_bytes({0x02})});
    const auto outer = rlp_encode_list({rlp_encode_bytes({0x09}), inner});
    std::vector<std::vector<uint8_t>> items;
    check(rlp_decode_list(outer, items) && items.size() == 2,
          "decode a list containing a nested list");
}

// -----------------------------------------------------------------------------
// KZG
// -----------------------------------------------------------------------------

void test_kzg() {
    std::printf("KZG\n");

    const auto& context = kzg_context();
    check(context.is_ready(), std::string("trusted setup loads: ") + context.error());
    if (!context.is_ready()) {
        return;
    }

    // A blob of zeros commits to the point at infinity, whose compressed
    // encoding is 0xc0 followed by zeros.
    BlobData zero_blob{};
    check(blob_is_canonical(zero_blob), "the zero blob is canonical");

    const auto zero_commitment = context.blob_to_commitment(zero_blob);
    check(zero_commitment.has_value(), "commitment to the zero blob");
    if (zero_commitment.has_value()) {
        Bytes48 expected{};
        expected[0] = 0xc0;
        check(*zero_commitment == expected,
              "the zero blob commits to the point at infinity");
    }

    // The versioned hash is sha256(commitment) with the version byte in front.
    if (zero_commitment.has_value()) {
        const auto versioned = compute_versioned_hash(*zero_commitment);
        check_hex(std::vector<uint8_t>(versioned.begin(), versioned.end()),
                  "0x010657f37554c781402a22917dee2f75def7ab966d7b770905398eba3c444014",
                  "versioned hash of the zero blob commitment");
        check(is_valid_versioned_hash(versioned), "the versioned hash has version 1");
    }

    // A blob with a non-zero element exercises the real polynomial path.
    BlobData blob{};
    blob[BLOB_SIZE - 1] = 0x01;  // the last element is 1
    const auto commitment = context.blob_to_commitment(blob);
    check(commitment.has_value(), "commitment to a non-zero blob");
    if (!commitment.has_value()) {
        return;
    }

    // Prove the evaluation at z = 1 and check it.
    Bytes32 z{};
    z[31] = 0x01;
    const auto proof_and_value = context.compute_proof(blob, z);
    check(proof_and_value.has_value(), "compute a point-evaluation proof");
    if (!proof_and_value.has_value()) {
        return;
    }
    const auto& [proof, y] = *proof_and_value;

    check(context.verify_proof(*commitment, z, y, proof),
          "a valid point-evaluation proof verifies");

    // A blob proof is a different object from a point-evaluation proof, so it
    // has to be produced by compute_blob_proof.
    const auto blob_proof = context.compute_blob_proof(blob, *commitment);
    check(blob_proof.has_value(), "compute a blob proof");
    if (!blob_proof.has_value()) {
        return;
    }
    check(context.verify_blob_proof(blob, *commitment, *blob_proof),
          "a valid blob proof verifies");
    check(!context.verify_blob_proof(blob, *commitment, proof),
          "a point-evaluation proof is not accepted as a blob proof");

    // The whole point of the component is rejecting bad proofs, so each
    // tampering below must fail.
    Bytes48 bad_proof = proof;
    bad_proof[0] ^= 0x01;
    check(!context.verify_proof(*commitment, z, y, bad_proof),
          "a tampered proof is rejected");

    Bytes32 bad_y = y;
    bad_y[31] ^= 0x01;
    check(!context.verify_proof(*commitment, z, bad_y, proof),
          "a wrong claimed value is rejected");

    Bytes32 bad_z = z;
    bad_z[31] = 0x02;
    check(!context.verify_proof(*commitment, bad_z, y, proof),
          "a wrong evaluation point is rejected");

    Bytes48 bad_commitment = *commitment;
    bad_commitment[47] ^= 0x01;
    check(!context.verify_proof(bad_commitment, z, y, proof),
          "a wrong commitment is rejected");

    BlobData other_blob{};
    other_blob[0] = 0x07;
    check(!context.verify_blob_proof(other_blob, *commitment, *blob_proof),
          "a proof for one blob does not verify against another");

    // A blob containing the modulus is not a valid blob, so no commitment can
    // be computed for it. Build it from the modulus' big-endian bytes.
    const auto modulus_element =
        FieldElement::from_hex("0x73eda753299d7d483339d80809a1d80553bda402fffe5bfeffffffff00000001");
    check(modulus_element.has_value(), "parse the modulus for the blob test");
    if (modulus_element.has_value()) {
        BlobData invalid{};
        std::copy(modulus_element->bytes().begin(), modulus_element->bytes().end(),
                  invalid.begin());
        check(!blob_is_canonical(invalid), "a blob containing the modulus is rejected");
        check(!context.blob_to_commitment(invalid).has_value(),
              "no commitment is produced for a non-canonical blob");
    }

}

// -----------------------------------------------------------------------------
// Batch verification
// -----------------------------------------------------------------------------

void test_batch_verification() {
    std::printf("batch verification\n");

    const auto& context = kzg_context();
    if (!context.is_ready()) {
        return;
    }

    std::vector<BlobData> blobs(2);
    blobs[0][0] = 0x01;
    blobs[1][BLOB_SIZE - 1] = 0x02;

    std::vector<Bytes48> commitments;
    std::vector<Bytes48> proofs;
    for (const auto& blob : blobs) {
        const auto commitment = context.blob_to_commitment(blob);
        if (!commitment.has_value()) {
            check(false, "batch: commitment");
            return;
        }
        const auto proof = context.compute_blob_proof(blob, *commitment);
        if (!proof.has_value()) {
            check(false, "batch: proof");
            return;
        }
        commitments.push_back(*commitment);
        proofs.push_back(*proof);
    }

    check(context.verify_blob_proof_batch(blobs, commitments, proofs),
          "two valid blobs verify in one batch");

    // Swapping the proofs must fail, since each proof belongs to one blob.
    std::vector<Bytes48> swapped = proofs;
    std::swap(swapped[0], swapped[1]);
    check(!context.verify_blob_proof_batch(blobs, commitments, swapped),
          "a batch with mismatched proofs is rejected");

    // A mismatched array length is a caller error and must not be accepted.
    std::vector<Bytes48> short_proofs{proofs[0]};
    check(!context.verify_blob_proof_batch(blobs, commitments, short_proofs),
          "a batch with the wrong proof count is rejected");
}

// -----------------------------------------------------------------------------
// Point-evaluation precompile
// -----------------------------------------------------------------------------

void test_precompile() {
    std::printf("point evaluation precompile\n");

    const auto& context = kzg_context();
    if (!context.is_ready()) {
        return;
    }

    BlobData blob{};
    blob[BLOB_SIZE - 1] = 0x03;
    const auto commitment = context.blob_to_commitment(blob);
    if (!commitment.has_value()) {
        check(false, "precompile: commitment");
        return;
    }
    Bytes32 z{};
    z[31] = 0x05;
    const auto proof_and_value = context.compute_proof(blob, z);
    if (!proof_and_value.has_value()) {
        check(false, "precompile: proof");
        return;
    }
    const auto& [proof, y] = *proof_and_value;

    PointEvaluationInput input;
    input.versioned_hash = compute_versioned_hash(*commitment);
    input.z = z;
    input.y = y;
    input.commitment = *commitment;
    input.proof = proof;

    // The precompile's success output is FIELD_ELEMENTS_PER_BLOB followed by
    // the BLS modulus, both 32 bytes.
    const auto output = evaluate_point(input);
    check(output.has_value(), "a valid point evaluation succeeds");
    if (output.has_value()) {
        check(output->size() == 64, "the output is 64 bytes");
        check_hex(*output,
                  "0x0000000000000000000000000000000000000000000000000000000000001000"
                  "73eda753299d7d483339d80809a1d80553bda402fffe5bfeffffffff00000001",
                  "the precompile output is the blob size and the modulus");
    }

    // Round-trip the 192-byte input form the EVM actually passes.
    const auto encoded = input.encode();
    check(encoded.size() == 192, "the encoded input is 192 bytes");
    const auto decoded = PointEvaluationInput::decode(encoded);
    check(decoded.has_value() && decoded->commitment == input.commitment,
          "the input round-trips");

    // Wrong-length input is not a precompile success.
    check(!PointEvaluationInput::decode(std::vector<uint8_t>(191)).has_value(),
          "a 191-byte input is rejected");
    check(!PointEvaluationInput::decode(std::vector<uint8_t>(193)).has_value(),
          "a 193-byte input is rejected");

    // A versioned hash that does not belong to the commitment must fail.
    PointEvaluationInput wrong_hash = input;
    wrong_hash.versioned_hash[31] ^= 0x01;
    check(!evaluate_point(wrong_hash).has_value(),
          "a mismatched versioned hash is rejected");

    // A wrong claimed value must fail.
    PointEvaluationInput wrong_y = input;
    wrong_y.y[31] ^= 0x01;
    check(!evaluate_point(wrong_y).has_value(), "a wrong y is rejected");

    // A tampered proof must fail.
    PointEvaluationInput wrong_proof = input;
    wrong_proof.proof[10] ^= 0x01;
    check(!evaluate_point(wrong_proof).has_value(), "a tampered proof is rejected");

    // Through the processor, which is the path the RPC layer uses.
    BlobProcessor processor;
    check(processor.is_ready(), "the processor reports a loaded setup");
    check(processor.evaluate_point(encoded).has_value(),
          "the processor runs the precompile");
    check(!processor.evaluate_point(std::vector<uint8_t>(10)).has_value(),
          "the processor rejects a short input");
}

// -----------------------------------------------------------------------------
// Merkle proofs
// -----------------------------------------------------------------------------

void test_merkle() {
    std::printf("Merkle proofs\n");

    std::vector<std::vector<uint8_t>> leaves;
    for (int i = 0; i < 7; ++i) {
        leaves.push_back({static_cast<uint8_t>(i), static_cast<uint8_t>(i * 2)});
    }
    const auto root = crypto::compute_merkle_root(leaves);

    for (size_t i = 0; i < leaves.size(); ++i) {
        const auto proof = crypto::compute_merkle_proof(leaves, i);
        check(crypto::verify_merkle_proof(leaves[i], proof, root),
              "leaf " + std::to_string(i) + " verifies against the root");
    }

    // A tampered leaf must not verify, which is the property the tree exists
    // for.
    const auto proof = crypto::compute_merkle_proof(leaves, 3);
    std::vector<uint8_t> tampered = leaves[3];
    tampered[0] ^= 0x01;
    check(!crypto::verify_merkle_proof(tampered, proof, root),
          "a tampered leaf is rejected");

    // Swapping a sibling's side must not verify.
    auto flipped = proof;
    if (!flipped.empty()) {
        flipped[0].sibling_is_right = !flipped[0].sibling_is_right;
        check(!crypto::verify_merkle_proof(leaves[3], flipped, root),
              "a proof with a flipped sibling side is rejected");
    }

    check(!crypto::verify_merkle_proof(leaves[3], {}, root),
          "an empty proof does not verify a non-trivial root");
    check(crypto::compute_merkle_root({}) == crypto::Hash256{},
          "an empty tree has the zero root");
}

// -----------------------------------------------------------------------------
// Transaction encoding
// -----------------------------------------------------------------------------

void test_transaction_encoding() {
    std::printf("blob transaction encoding\n");

    BlobTransaction tx;
    tx.chain_id = 6666;
    tx.nonce = 7;
    tx.gas_limit = 21000;
    tx.max_fee_per_gas = 30;
    tx.max_priority_fee_per_gas = 2;
    tx.max_fee_per_blob_gas = uint256_t(1000);
    tx.value = uint256_t(12345);
    tx.y_parity = 1;
    tx.r = uint256_t(0x1111);
    tx.s = uint256_t(0x2222);
    Address to{};
    to[19] = 0xab;
    tx.to = to;

    VersionedHash hash{};
    hash[0] = 0x01;
    hash[31] = 0x42;
    tx.blob_versioned_hashes.push_back(hash);

    const auto encoded = tx.encode();
    check(!encoded.empty() && encoded[0] == 0x03,
          "a type-3 transaction starts with its type byte");

    const auto decoded = BlobTransaction::decode(encoded);
    check(decoded.has_value(), "a transaction round-trips");
    if (!decoded.has_value()) {
        return;
    }
    check(decoded->chain_id == 6666, "chain id survives the round trip");
    check(decoded->nonce == 7, "nonce survives the round trip");
    check(decoded->gas_limit == 21000, "gas limit survives the round trip");
    check(decoded->max_fee_per_gas == 30, "max fee per gas survives");
    check(decoded->max_fee_per_blob_gas == uint256_t(1000),
          "max fee per blob gas survives");
    check(decoded->value == uint256_t(12345), "value survives");
    check(decoded->y_parity == 1, "y parity survives");
    check(decoded->r == uint256_t(0x1111) && decoded->s == uint256_t(0x2222),
          "the signature survives");
    check(decoded->to.has_value() && (*decoded->to)[19] == 0xab,
          "the recipient survives");
    check(decoded->blob_versioned_hashes.size() == 1 &&
              decoded->blob_versioned_hashes[0] == hash,
          "the versioned hash survives");

    // Re-encoding a decoded transaction must reproduce the same bytes, which is
    // what makes the hash stable.
    check(decoded->encode() == encoded, "re-encoding is byte-identical");

    check(BlobTransaction::decode({0x03}) == std::nullopt,
          "a truncated transaction is rejected");
    check(BlobTransaction::decode({0x02, 0xc0}) == std::nullopt,
          "a type-2 transaction is not accepted as type 3");
    check(BlobTransaction::decode(encoded).has_value(), "a well-formed one is accepted");

    // Gas accounting.
    check(tx.blob_gas() == BLOB_GAS_PER_BLOB, "one blob costs one blob of gas");
    check(tx.total_gas() == 21000 + BLOB_GAS_PER_BLOB,
          "total gas is execution plus blob");
    check(tx.blob_fee() == uint256_t(1000 * BLOB_GAS_PER_BLOB),
          "the blob fee is gas times price");
}

// -----------------------------------------------------------------------------
// Processor end to end
// -----------------------------------------------------------------------------

void test_processor_flow() {
    std::printf("processor end to end\n");

    const auto& context = kzg_context();
    if (!context.is_ready()) {
        return;
    }

    BlobProcessor processor;

    Blob blob;
    blob.data[BLOB_SIZE - 1] = 0x09;
    const auto commitment = context.blob_to_commitment(blob.data);
    if (!commitment.has_value()) {
        check(false, "processor: commitment");
        return;
    }
    blob.commitment = *commitment;
    const auto proof = context.compute_blob_proof(blob.data, *commitment);
    if (!proof.has_value()) {
        check(false, "processor: proof");
        return;
    }
    blob.proof = *proof;

    BlobTransaction tx;
    tx.chain_id = 6666;
    tx.blob_versioned_hashes.push_back(compute_versioned_hash(*commitment));
    tx.blobs.push_back(blob);
    tx.commitments.push_back(*commitment);
    tx.proofs.push_back(*proof);

    const auto verdict = processor.verify_transaction(tx);
    check(verdict.valid, "a well-formed blob transaction verifies: " + verdict.reason);

    // A transaction whose versioned hash does not match its commitment must be
    // rejected, since that is the binding the execution layer relies on.
    BlobTransaction bad_hash = tx;
    bad_hash.blob_versioned_hashes[0][31] ^= 0x01;
    check(!processor.verify_transaction(bad_hash).valid,
          "a mismatched versioned hash is rejected");

    // A transaction with a bad proof must be rejected.
    BlobTransaction bad_proof = tx;
    bad_proof.proofs[0][0] ^= 0x01;
    check(!processor.verify_transaction(bad_proof).valid,
          "a tampered proof is rejected");

    // Count mismatches are rejected before any pairing work.
    BlobTransaction missing_blob = tx;
    missing_blob.blobs.clear();
    check(!processor.verify_transaction(missing_blob).valid,
          "a transaction missing its blob data is rejected");

    BlobTransaction no_blobs = tx;
    no_blobs.blob_versioned_hashes.clear();
    no_blobs.blobs.clear();
    no_blobs.commitments.clear();
    no_blobs.proofs.clear();
    check(!processor.verify_transaction(no_blobs).valid,
          "a transaction with no blobs is rejected");

    // Per-blob verification.
    check(processor.verify_blob(blob, *commitment).valid, "verify a single blob");

    Blob tampered = blob;
    tampered.data[0] ^= 0x01;
    check(!processor.verify_blob(tampered, *commitment).valid,
          "a modified blob does not match its commitment");

    check(processor.calculate_blob_gas(tx) == BLOB_GAS_PER_BLOB,
          "the processor reports the blob gas");

    const auto info = processor.calculate_blob_gas_price(3338477);
    check(info.blob_gas_price.low64() == 2, "the processor prices excess correctly");
    check(info.fee_for(1) == uint256_t(2 * BLOB_GAS_PER_BLOB),
          "the fee for one blob at that price");

    // A transaction hash is a 32-byte hex string.
    const auto hash = tx.compute_hash();
    check(hash.size() == 66 && hash.rfind("0x", 0) == 0,
          "the transaction hash is 32 bytes of hex");
    check(tx.compute_hash() == hash, "the transaction hash is deterministic");
}

// -----------------------------------------------------------------------------
// uint256
// -----------------------------------------------------------------------------

void test_uint256() {
    std::printf("uint256\n");

    check(uint256_t(0).is_zero(), "zero is zero");
    check(uint256_t(1).is_one(), "one is one");
    check(uint256_t(255).low64() == 255, "255 round-trips through low64");
    check(uint256_t(0xffffffffffffffffULL).low64() == 0xffffffffffffffffULL,
          "the maximum 64-bit value round-trips");
    check(uint256_t(0xffffffffffffffffULL).fits_u64(), "it fits in 64 bits");

    const auto parsed = uint256_t::from_hex("0x100");
    check(parsed.has_value() && parsed->low64() == 256, "parse 0x100");
    check(parsed->to_hex() == "0x100", "0x100 renders without leading zeros");
    check(uint256_t(0).to_hex() == "0x0", "zero renders as 0x0");
    check(uint256_t(1).to_hex() == "0x1", "one renders as 0x1");
    check(uint256_t(0x1234).to_hex_fixed() ==
              "0000000000000000000000000000000000000000000000000000000000001234",
          "the fixed form is 64 digits");

    check(!uint256_t::from_hex("0xzz").has_value(), "reject non-hex digits");
    check(!uint256_t::from_hex("").has_value(), "reject an empty string");
    check(!uint256_t::from_hex("0x" + std::string(66, 'a')).has_value(),
          "reject more than 32 bytes");

    // Round-trip through bytes.
    const auto value = uint256_t::from_hex("0xdeadbeef");
    check(value.has_value() && uint256_t(value->bytes()).low64() == 0xdeadbeef,
          "round-trip through bytes");

    check(uint256_t(1) < uint256_t(2), "ordering follows the value");
    check(!(uint256_t(2) < uint256_t(2)), "equal values are not less");
}

} // namespace

int main() {
    std::printf("TigerSmartChain blob processor tests\n\n");
    test_hashes();
    test_field_elements();
    test_gas_pricing();
    test_rlp();
    test_kzg();
    test_batch_verification();
    test_precompile();
    test_merkle();
    test_uint256();
    test_transaction_encoding();
    test_processor_flow();

    std::printf("\n%d checks, %d failures\n", checks, failures);
    return failures == 0 ? 0 : 1;
}
