//! Hashing primitives.
//!
//! Keccak-256 is the Ethereum hash and is what addresses, transaction ids and
//! state roots are built from. It is not the same function as SHA3-256, which
//! uses different padding.

use blake3::Hasher as Blake3Hasher;
use ripemd::Ripemd160;
use sha2::{Digest, Sha256, Sha512};
use sha3::{Keccak256, Keccak512 as Sha3Keccak512};

/// SHA-256.
pub fn sha256(data: &[u8]) -> [u8; 32] {
    let mut hash = [0u8; 32];
    hash.copy_from_slice(&Sha256::digest(data));
    hash
}

/// SHA-512.
pub fn sha512(data: &[u8]) -> [u8; 64] {
    let mut hash = [0u8; 64];
    hash.copy_from_slice(&Sha512::digest(data));
    hash
}

/// Keccak-256, the Ethereum hash.
pub fn keccak256(data: &[u8]) -> [u8; 32] {
    let mut hash = [0u8; 32];
    hash.copy_from_slice(&Keccak256::digest(data));
    hash
}

/// Keccak-512.
pub fn keccak512(data: &[u8]) -> [u8; 64] {
    let mut hash = [0u8; 64];
    hash.copy_from_slice(&Sha3Keccak512::digest(data));
    hash
}

/// BLAKE3.
pub fn blake3(data: &[u8]) -> [u8; 32] {
    *Blake3Hasher::new().update(data).finalize().as_bytes()
}

/// Keyed BLAKE3.
///
/// The key must be exactly 32 bytes. A shorter key is rejected rather than
/// padded, since padding would let two different keys produce the same MAC.
pub fn blake3_with_key(data: &[u8], key: &[u8]) -> [u8; 32] {
    let key: &[u8; 32] = key
        .try_into()
        .expect("BLAKE3 keyed mode requires a 32-byte key");
    *Blake3Hasher::new_keyed(key).update(data).finalize().as_bytes()
}

/// RIPEMD-160, used in Bitcoin-style address derivation.
pub fn ripemd160(data: &[u8]) -> [u8; 20] {
    let mut hash = [0u8; 20];
    hash.copy_from_slice(&Ripemd160::digest(data));
    hash
}

/// RIPEMD-160 of the SHA-256 of `data`.
pub fn hash160(data: &[u8]) -> [u8; 20] {
    ripemd160(&sha256(data))
}

/// Derive an Ethereum address from a public key.
///
/// Accepts either the 64-byte `x || y` form or the 65-byte uncompressed form
/// with its `0x04` prefix. The prefix is part of the key but not part of the
/// address, so including it by mistake would give the wrong address; the
/// length is checked rather than guessed.
pub fn public_key_to_address(public_key: &[u8]) -> [u8; 20] {
    let body = match public_key.len() {
        65 => &public_key[1..],
        64 => public_key,
        other => panic!("public key must be 64 or 65 bytes, got {other}"),
    };

    let hash = keccak256(body);
    let mut address = [0u8; 20];
    address.copy_from_slice(&hash[12..]);
    address
}

/// EIP-55 mixed-case checksum encoding of a 20-byte address.
pub fn checksum_address(address: &[u8; 20]) -> String {
    let lower = hex::encode(address);
    let hash = keccak256(lower.as_bytes());

    let mut out = String::with_capacity(42);
    out.push_str("0x");
    for (i, ch) in lower.chars().enumerate() {
        // Uppercase a hex letter when the matching nibble of the hash is >= 8.
        let nibble = if i % 2 == 0 {
            hash[i / 2] >> 4
        } else {
            hash[i / 2] & 0x0F
        };
        if ch.is_ascii_alphabetic() && nibble >= 8 {
            out.push(ch.to_ascii_uppercase());
        } else {
            out.push(ch);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sha256_known_vectors() {
        assert_eq!(
            hex::encode(sha256(b"abc")),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
        assert_eq!(
            hex::encode(sha256(b"")),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
    }

    #[test]
    fn keccak256_known_vectors() {
        // Keccak-256("") differs from SHA3-256(""): the padding differs.
        assert_eq!(
            hex::encode(keccak256(b"")),
            "c5d2460186f7233c927e7db2dcc703c0e500b653ca82273b7bfad8045d85a470"
        );
        assert_eq!(
            hex::encode(keccak256(b"abc")),
            "4e03657aea45a94fc7d47ba826c8d667c0d1e6e33a64a036ec44f58fa12d6c45"
        );
    }

    #[test]
    fn keccak512_produces_a_full_width_hash() {
        // The previous implementation called Keccak256 and then copied 64
        // bytes out of a 32-byte result, which could not work.
        let hash = keccak512(b"abc");
        assert_eq!(hash.len(), 64);
        assert_eq!(
            hex::encode(hash),
            concat!(
                "18587dc2ea106b9a1563e32b3312421ca164c7f1f07bc922a9c83d77cea3a1e5",
                "d0c69910739025372dc14ac9642629379540c17e2a65b19d77aa511a9d00bb96"
            )
        );
    }

    #[test]
    fn blake3_known_vector() {
        assert_eq!(
            hex::encode(blake3(b"")),
            "af1349b9f5f9a1a6a0404dea36dcc9499bcb25c9adc112b7cc9a93cae41f3262"
        );
    }

    #[test]
    fn keyed_blake3_depends_on_the_key() {
        let first = blake3_with_key(b"message", &[1u8; 32]);
        let second = blake3_with_key(b"message", &[2u8; 32]);

        assert_ne!(first, second);
    }

    #[test]
    fn hash160_is_ripemd160_of_sha256() {
        assert_eq!(hash160(b"abc"), ripemd160(&sha256(b"abc")));
    }

    #[test]
    fn address_derivation_matches_the_known_vector() {
        // Public key for private key 0x01, without the 0x04 prefix.
        let public_key = hex::decode(concat!(
            "79be667ef9dcbbac55a06295ce870b07029bfcdb2dce28d959f2815b16f81798",
            "483ada7726a3c4655da4fbfc0e1108a8fd17b448a68554199c47d08ffb10d4b8"
        ))
        .unwrap();

        assert_eq!(
            hex::encode(public_key_to_address(&public_key)),
            "7e5f4552091a69125d5dfcb7b8c2659029395bdf"
        );
    }

    #[test]
    fn address_derivation_accepts_the_prefixed_key() {
        let prefixed = hex::decode(concat!(
            "0479be667ef9dcbbac55a06295ce870b07029bfcdb2dce28d959f2815b16f81798",
            "483ada7726a3c4655da4fbfc0e1108a8fd17b448a68554199c47d08ffb10d4b8"
        ))
        .unwrap();

        assert_eq!(
            hex::encode(public_key_to_address(&prefixed)),
            "7e5f4552091a69125d5dfcb7b8c2659029395bdf"
        );
    }

    #[test]
    fn checksum_address_matches_the_eip55_vector() {
        let address: [u8; 20] = hex::decode("5aaeb6053f3e94c9b9a09f33669435e7ef1beaed")
            .unwrap()
            .try_into()
            .unwrap();

        assert_eq!(
            checksum_address(&address),
            "0x5aAeb6053F3E94C9b9A09f33669435E7Ef1BeAed"
        );
    }

    #[test]
    fn checksum_address_handles_an_all_numeric_address() {
        let address: [u8; 20] = [0u8; 20];
        assert_eq!(
            checksum_address(&address),
            "0x0000000000000000000000000000000000000000"
        );
    }
}
