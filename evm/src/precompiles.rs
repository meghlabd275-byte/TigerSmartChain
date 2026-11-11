//! Precompiled contracts, addresses `0x01` through `0x09`.
//!
//! A call to one of these addresses does not execute bytecode; it runs a
//! built-in function and returns its output. Without this dispatch a contract
//! calling, say, `ecrecover` at `0x01` would find no code there and the call
//! would silently return success with empty output, which is how a contract
//! that verifies signatures would accept everything.
//!
//! Gas follows the Ethereum schedule. `ecrecover`, `ecadd`, `ecmul` and
//! `ecpairing` are charged their flat cost. `sha256`, `ripemd160`, `identity`
//! and `blake2f` are charged per word or per round. `modexp` is charged by the
//! EIP-198 formula, which depends on the input.

use crate::{Address, EvmWord};

/// The largest precompile address this chain implements.
pub const LAST_PRECOMPILE: u8 = 0x09;

/// Whether `address` is a precompiled contract.
pub fn is_precompile(address: &Address) -> bool {
    address.0[..19].iter().all(|&b| b == 0) && address.0[19] >= 0x01 && address.0[19] <= LAST_PRECOMPILE
}

/// The result of running a precompile: its output and the gas it consumed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PrecompileOutput {
    pub output: Vec<u8>,
    pub gas_used: u64,
}

/// A precompile rejected its input.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PrecompileError {
    pub reason: String,
    pub gas_used: u64,
}

impl PrecompileError {
    fn new(reason: impl Into<String>, gas_used: u64) -> Self {
        Self {
            reason: reason.into(),
            gas_used,
        }
    }
}

/// Number of 32-byte words needed to hold `len` bytes.
fn word_count(len: usize) -> u64 {
    (len as u64).div_ceil(32)
}

/// Run the precompile at `address` with `input`, given `gas` available.
///
/// Returns the output and the gas consumed. A precompile that is not
/// implemented at that address is an error, not an empty success.
pub fn run(address: &Address, input: &[u8], gas: u64) -> Result<PrecompileOutput, PrecompileError> {
    let id = address.0[19];
    match id {
        0x01 => run_ecrecover(input, gas),
        0x02 => run_sha256(input, gas),
        0x03 => run_ripemd160(input, gas),
        0x04 => run_identity(input, gas),
        0x05 => run_modexp(input, gas),
        0x06 => run_ecadd(input, gas),
        0x07 => run_ecmul(input, gas),
        0x08 => run_ecpairing(input, gas),
        0x09 => run_blake2f(input, gas),
        other => Err(PrecompileError::new(
            format!("no precompile at address {other:#04x}"),
            0,
        )),
    }
}

/// Charge `cost` gas, failing if there is not enough.
fn charge(gas: u64, cost: u64) -> Result<(), PrecompileError> {
    if gas < cost {
        return Err(PrecompileError::new("out of gas", gas));
    }
    Ok(())
}

/// Address `0x01`: recover the address that signed a 32-byte hash.
///
/// The input is `hash || v || r || s` padded to 128 bytes. A signature that
/// does not recover yields an empty output with no error, which is what the
/// spec requires; the caller sees a zero result rather than a failure.
fn run_ecrecover(input: &[u8], gas: u64) -> Result<PrecompileOutput, PrecompileError> {
    const COST: u64 = 3_000;
    charge(gas, COST)?;

    // The input is right-padded with zeroes to the fixed 128-byte size.
    let mut buf = [0u8; 128];
    let take = input.len().min(128);
    buf[..take].copy_from_slice(&input[..take]);

    match tiger_precompile::ecrecover::ecrecover(&buf, COST) {
        Ok(output) => Ok(PrecompileOutput {
            output,
            gas_used: COST,
        }),
        // A signature that does not recover is not an exceptional halt; the
        // precompile returns empty output.
        Err(_) => Ok(PrecompileOutput {
            output: Vec::new(),
            gas_used: COST,
        }),
    }
}

/// Address `0x02`: SHA-256, charged 12 gas per word plus 60.
fn run_sha256(input: &[u8], gas: u64) -> Result<PrecompileOutput, PrecompileError> {
    let cost = 60 + 12 * word_count(input.len());
    charge(gas, cost)?;
    let output = tiger_precompile::sha256::sha256(input, cost)
        .map_err(|e| PrecompileError::new(e, cost))?;
    Ok(PrecompileOutput {
        output,
        gas_used: cost,
    })
}

/// Address `0x03`: RIPEMD-160, charged 120 gas per word plus 600.
fn run_ripemd160(input: &[u8], gas: u64) -> Result<PrecompileOutput, PrecompileError> {
    let cost = 600 + 120 * word_count(input.len());
    charge(gas, cost)?;
    let output = tiger_precompile::ripemd160::ripemd160(input, cost)
        .map_err(|e| PrecompileError::new(e, cost))?;
    Ok(PrecompileOutput {
        output,
        gas_used: cost,
    })
}

/// Address `0x04`: the identity function, charged 3 gas per word plus 15.
fn run_identity(input: &[u8], gas: u64) -> Result<PrecompileOutput, PrecompileError> {
    let cost = 15 + 3 * word_count(input.len());
    charge(gas, cost)?;
    let output = tiger_precompile::identity::identity(input, cost)
        .map_err(|e| PrecompileError::new(e, cost))?;
    Ok(PrecompileOutput {
        output,
        gas_used: cost,
    })
}

/// Address `0x05`: modular exponentiation, charged by the EIP-198 formula.
fn run_modexp(input: &[u8], gas: u64) -> Result<PrecompileOutput, PrecompileError> {
    let cost = modexp_gas(input);
    charge(gas, cost)?;
    let output = tiger_precompile::modexp::modexp(input, cost)
        .map_err(|e| PrecompileError::new(e, cost))?;
    Ok(PrecompileOutput {
        output,
        gas_used: cost,
    })
}

/// EIP-198 gas for `modexp`: `max(adjusted_exponent_length, 1) * max(modulus_length, 1)^2 / 200`,
/// with a floor of 200, plus a 50-gas-per-word charge for each of the three
/// operand length words (the EIP-2565 repricing).
fn modexp_gas(input: &[u8]) -> u64 {
    let mut buf = [0u8; 96];
    let take = input.len().min(96);
    buf[..take].copy_from_slice(&input[..take]);
    let base_len = read_u256_word(&buf[0..32]);
    let exp_len = read_u256_word(&buf[32..64]);
    let mod_len = read_u256_word(&buf[64..96]);

    // Lengths beyond this are unrepresentable and would be rejected anyway.
    const MAX_LEN: u128 = 1 << 32;
    if base_len > MAX_LEN || exp_len > MAX_LEN || mod_len > MAX_LEN {
        return u64::MAX;
    }

    // The adjusted exponent length is the exponent's own byte length, plus 8
    // bits for every byte the base exceeds the modulus by.
    let exp_head_len = if exp_len > 32 { 32 } else { exp_len as usize };
    let exp_head_offset = 96 + base_len as usize;
    let exp_head = if exp_head_len > 0 && input.len() > exp_head_offset {
        let end = (exp_head_offset + exp_head_len).min(input.len());
        read_u256_word_padded(&input[exp_head_offset..end])
    } else {
        0
    };
    let bit_length = if exp_head == 0 { 0 } else { 256 - exp_head.leading_zeros() as u128 };
    let extra = if mod_len > 0 && base_len > mod_len {
        8 * (base_len - mod_len)
    } else {
        0
    };
    let adjusted_exp_len = if exp_len <= 32 {
        bit_length
    } else {
        bit_length + 8 * (exp_len - 32)
    } + extra;

    let max_len = mod_len.max(1);
    let complexity = adjusted_exp_len.max(1).saturating_mul(max_len).saturating_mul(max_len);
    let iteration_cost = complexity / 200;

    // The length words themselves cost 50 gas per word, rounded up.
    let length_cost = 50
        * (word_count(base_len as usize)
            + word_count(exp_len as usize)
            + word_count(mod_len as usize));

    let cost = iteration_cost.max(200).saturating_add(length_cost as u128);
    cost.min(u64::MAX as u128) as u64
}

fn read_u256_word(bytes: &[u8]) -> u128 {
    // The length words are 32-byte big-endian values; anything above 2^32 is
    // already out of range for a real input.
    let mut value: u128 = 0;
    for &b in bytes.iter().take(32) {
        value = (value << 8) | b as u128;
        if value > (1u128 << 32) {
            return value;
        }
    }
    value
}

fn read_u256_word_padded(bytes: &[u8]) -> u128 {
    let mut value: u128 = 0;
    for &b in bytes {
        value = (value << 8) | b as u128;
        if value > (1u128 << 64) {
            return value;
        }
    }
    value
}

/// Address `0x06`: add two points on the BN254 curve. Flat 150 gas.
fn run_ecadd(input: &[u8], gas: u64) -> Result<PrecompileOutput, PrecompileError> {
    const COST: u64 = 150;
    charge(gas, COST)?;
    let mut buf = vec![0u8; 128];
    let take = input.len().min(128);
    buf[..take].copy_from_slice(&input[..take]);
    let output = tiger_precompile::bn128::ecadd(&buf, COST)
        .map_err(|e| PrecompileError::new(e, COST))?;
    Ok(PrecompileOutput {
        output,
        gas_used: COST,
    })
}

/// Address `0x07`: multiply a BN254 point by a scalar. Flat 6,000 gas.
fn run_ecmul(input: &[u8], gas: u64) -> Result<PrecompileOutput, PrecompileError> {
    const COST: u64 = 6_000;
    charge(gas, COST)?;
    let mut buf = vec![0u8; 96];
    let take = input.len().min(96);
    buf[..take].copy_from_slice(&input[..take]);
    let output = tiger_precompile::bn128::ecmul(&buf, COST)
        .map_err(|e| PrecompileError::new(e, COST))?;
    Ok(PrecompileOutput {
        output,
        gas_used: COST,
    })
}

/// Address `0x08`: the BN254 pairing check, 45,000 gas plus 34,000 per pair.
fn run_ecpairing(input: &[u8], gas: u64) -> Result<PrecompileOutput, PrecompileError> {
    const BASE: u64 = 45_000;
    const PER_PAIR: u64 = 34_000;
    // A partial pair is padded to a full one, as the spec requires.
    let pairs = input.len().div_ceil(192) as u64;
    let cost = BASE + PER_PAIR * pairs;
    charge(gas, cost)?;

    // Pad the input up to a whole number of pairs before running.
    let mut buf = input.to_vec();
    let padded = (pairs as usize) * 192;
    if buf.len() < padded {
        buf.resize(padded, 0);
    }
    let output = tiger_precompile::bn128::ecpairing(&buf, cost)
        .map_err(|e| PrecompileError::new(e, cost))?;
    Ok(PrecompileOutput {
        output,
        gas_used: cost,
    })
}

/// Address `0x09`: the BLAKE2b F compression function. Gas equals the rounds.
fn run_blake2f(input: &[u8], gas: u64) -> Result<PrecompileOutput, PrecompileError> {
    if input.len() != 213 {
        return Err(PrecompileError::new(
            "blake2f input must be exactly 213 bytes",
            0,
        ));
    }
    let rounds = u32::from_be_bytes([input[0], input[1], input[2], input[3]]) as u64;
    charge(gas, rounds)?;
    let output = tiger_precompile::blake2f::blake2f(input, rounds)
        .map_err(|e| PrecompileError::new(e, rounds))?;
    Ok(PrecompileOutput {
        output,
        gas_used: rounds,
    })
}

/// Build the 20-byte address of precompile `id`.
pub fn precompile_address(id: u8) -> Address {
    let mut bytes = [0u8; 20];
    bytes[19] = id;
    Address(bytes)
}

/// The word value of a precompile's output, as a stack item.
pub fn output_word(output: &[u8]) -> EvmWord {
    EvmWord::from_be_bytes(output)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn addr(id: u8) -> Address {
        precompile_address(id)
    }

    #[test]
    fn only_addresses_1_through_9_are_precompiles() {
        for id in 1..=9u8 {
            assert!(is_precompile(&addr(id)), "{id} must be a precompile");
        }
        assert!(!is_precompile(&Address::zero()));
        assert!(!is_precompile(&addr(0x0a)));
        // An address that is not 19 zero bytes followed by the id is not a
        // precompile even if its last byte matches.
        let mut other = [0u8; 20];
        other[0] = 0x01;
        other[19] = 0x01;
        assert!(!is_precompile(&Address(other)));
    }

    #[test]
    fn identity_returns_its_input_and_charges_per_word() {
        let out = run(&addr(0x04), b"hello", 1000).unwrap();
        assert_eq!(out.output, b"hello");
        assert_eq!(out.gas_used, 15 + 3); // one word
    }

    #[test]
    fn sha256_matches_the_known_digest() {
        let out = run(&addr(0x02), b"abc", 1000).unwrap();
        assert_eq!(
            hex::encode(&out.output),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }

    #[test]
    fn ripemd160_is_left_padded_to_32_bytes() {
        let out = run(&addr(0x03), b"abc", 1000).unwrap();
        assert_eq!(
            hex::encode(&out.output),
            "0000000000000000000000008eb208f7e05d987a9b044a8e98c6b087f15a0bfc"
        );
    }

    #[test]
    fn ecrecover_recovers_a_real_signer() {
        // Sign a hash with a known key and check the precompile returns that
        // key's address. A stub returning zeros would fail this.
        use secp256k1::{Message, Secp256k1, SecretKey};
        let key_bytes =
            hex::decode("4646464646464646464646464646464646464646464646464646464646464646")
                .unwrap();
        let mut key = [0u8; 32];
        key.copy_from_slice(&key_bytes);
        let secret = SecretKey::from_slice(&key).unwrap();
        let secp = Secp256k1::signing_only();
        let hash = tiger_crypto::keccak256(b"tiger precompile test");
        let msg = Message::from_digest_slice(&hash).unwrap();
        let sig = secp.sign_ecdsa_recoverable(&msg, &secret);
        let (rec_id, compact) = sig.serialize_compact();

        let mut input = vec![0u8; 128];
        input[0..32].copy_from_slice(&hash);
        input[63] = rec_id.to_i32() as u8 + 27;
        input[64..96].copy_from_slice(&compact[..32]);
        input[96..128].copy_from_slice(&compact[32..]);

        let out = run(&addr(0x01), &input, 10_000).unwrap();
        assert_eq!(out.gas_used, 3_000);
        assert_eq!(out.output.len(), 32);

        let expected = tiger_crypto::private_key_to_address(&key).unwrap();
        assert_eq!(&out.output[12..32], &expected);

        // A different hash must not recover the same address.
        let mut other = input.clone();
        other[0] ^= 0xff;
        let out_other = run(&addr(0x01), &other, 10_000).unwrap();
        assert_ne!(&out_other.output[12..32], &expected);
    }

    #[test]
    fn out_of_gas_is_reported_rather_than_ignored() {
        let err = run(&addr(0x02), b"abc", 10).unwrap_err();
        assert_eq!(err.gas_used, 10);
        let err = run(&addr(0x01), &[0u8; 128], 100).unwrap_err();
        assert_eq!(err.gas_used, 100);
    }

    #[test]
    fn an_unimplemented_precompile_address_is_an_error() {
        let err = run(&addr(0x0a), b"", 1_000_000).unwrap_err();
        assert!(err.reason.contains("no precompile"));
    }

    #[test]
    fn ecadd_adds_the_identity_point() {
        // The identity (0, 0) plus the generator is the generator. The
        // generator in EIP-196 encoding is (1, 2).
        let mut input = vec![0u8; 128];
        input[31] = 1;
        input[63] = 2;
        let out = run(&addr(0x06), &input, 1_000).unwrap();
        assert_eq!(out.gas_used, 150);
        assert_eq!(out.output.len(), 64);
        assert_eq!(out.output[31], 1);
        assert_eq!(out.output[63], 2);
    }

    #[test]
    fn blake2f_charges_one_gas_per_round() {
        let mut input = vec![0u8; 213];
        input[0..4].copy_from_slice(&12u32.to_be_bytes());
        let out = run(&addr(0x09), &input, 100).unwrap();
        assert_eq!(out.gas_used, 12);
        assert_eq!(out.output.len(), 64);

        // A bad length is rejected before any gas is charged.
        let err = run(&addr(0x09), &[0u8; 212], 100).unwrap_err();
        assert_eq!(err.gas_used, 0);
    }

    #[test]
    fn modexp_gas_grows_with_the_operands() {
        // base_len=1, exp_len=1, mod_len=1, then 1 || 1 || 1.
        let mut input = vec![0u8; 99];
        input[31] = 1;
        input[63] = 1;
        input[95] = 1;
        input[96] = 2; // base
        input[97] = 5; // exp
        input[98] = 7; // mod
        let out = run(&addr(0x05), &input, 1_000_000).unwrap();
        // 2^5 mod 7 = 32 mod 7 = 4
        assert_eq!(out.output, vec![4]);
        assert!(out.gas_used >= 200);
    }
}
