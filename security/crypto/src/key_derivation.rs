//! BIP-39 mnemonics and BIP-32 key derivation.
//!
//! The word list is the real 2048-word English list, supplied by the `bip39`
//! crate. The previous hand-written list was truncated to about 32 words, so
//! a mnemonic generated from it carried almost no entropy and every index
//! above 31 panicked.
//!
//! Derivation follows BIP-32: `m / 44' / 60' / 0' / 0 / index` is the usual
//! Ethereum path, and the hardened child step is implemented rather than
//! hashing the seed in a loop.

use bip39::{Language, Mnemonic as Bip39Mnemonic};
use hmac::{Hmac, Mac};
use sha2::Sha512;
use zeroize::{Zeroize, ZeroizeOnDrop};

use crate::{Error, Result};

type HmacSha512 = Hmac<Sha512>;

/// A BIP-39 mnemonic phrase.
#[derive(Clone, Zeroize, ZeroizeOnDrop)]
pub struct Mnemonic(String);

impl std::fmt::Debug for Mnemonic {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Mnemonic(<redacted>)")
    }
}

/// A 64-byte BIP-39 seed.
#[derive(Clone, Zeroize, ZeroizeOnDrop)]
pub struct Seed([u8; 64]);

/// A BIP-32 extended private key.
#[derive(Clone, Zeroize, ZeroizeOnDrop)]
pub struct ExtendedPrivateKey {
    key: [u8; 32],
    chain_code: [u8; 32],
}

impl std::fmt::Debug for ExtendedPrivateKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("ExtendedPrivateKey(<redacted>)")
    }
}

/// Entropy size for a mnemonic.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EntropyStrength {
    /// 128 bits, 12 words.
    Bits128,
    /// 160 bits, 15 words.
    Bits160,
    /// 192 bits, 18 words.
    Bits192,
    /// 224 bits, 21 words.
    Bits224,
    /// 256 bits, 24 words.
    Bits256,
}

impl EntropyStrength {
    /// The entropy size in bytes.
    pub fn bytes(self) -> usize {
        match self {
            EntropyStrength::Bits128 => 16,
            EntropyStrength::Bits160 => 20,
            EntropyStrength::Bits192 => 24,
            EntropyStrength::Bits224 => 28,
            EntropyStrength::Bits256 => 32,
        }
    }

    /// The resulting word count.
    pub fn word_count(self) -> usize {
        // Entropy plus one checksum bit per 32 entropy bits, in 11-bit words.
        (self.bytes() * 8 + self.bytes() / 4) / 11
    }
}

/// Build a mnemonic from raw entropy.
///
/// The entropy length must be one of the five BIP-39 sizes; a 17-byte input
/// cannot be split into 11-bit words with a whole-byte checksum and is
/// rejected rather than silently padded.
pub fn mnemonic_from_entropy(entropy: &[u8]) -> Result<Mnemonic> {
    let mnemonic = Bip39Mnemonic::from_entropy_in(Language::English, entropy)
        .map_err(|e| Error::InvalidData(e.to_string()))?;
    Ok(Mnemonic(mnemonic.to_string()))
}

/// Generate a mnemonic from the operating system CSPRNG.
pub fn generate_mnemonic(strength: EntropyStrength) -> Result<Mnemonic> {
    let mut entropy = vec![0u8; strength.bytes()];
    use rand_core::{OsRng, RngCore};
    OsRng.fill_bytes(&mut entropy);

    let result = mnemonic_from_entropy(&entropy);
    entropy.zeroize();
    result
}

/// Derive the 64-byte seed for a mnemonic.
///
/// The salt is `"mnemonic" || passphrase`, and PBKDF2-HMAC-SHA512 with 2048
/// iterations is applied. An empty passphrase is allowed and is what most
/// wallets use.
pub fn seed_from_mnemonic(mnemonic: &Mnemonic, passphrase: &str) -> Result<Seed> {
    let parsed = Bip39Mnemonic::parse_in_normalized(Language::English, &mnemonic.0)
        .map_err(|e| Error::InvalidData(e.to_string()))?;

    let seed = parsed.to_seed(passphrase);
    Ok(Seed(seed))
}

/// Validate a mnemonic phrase, including its checksum.
///
/// A word-count check is not enough: a phrase with the right number of words
/// but a wrong checksum is not a valid wallet and must be rejected before it
/// is used to derive anything.
pub fn validate_mnemonic(mnemonic: &str) -> bool {
    Bip39Mnemonic::parse_in_normalized(Language::English, mnemonic).is_ok()
}

/// Derive a child key along a BIP-32 path from a seed.
///
/// `path` holds the indices, most significant group first. An index at or
/// above [`HARDENED_OFFSET`] is a hardened step.
pub fn derive_child_key(seed: &Seed, path: &[u32]) -> Result<ExtendedPrivateKey> {
    let mut mac = HmacSha512::new_from_slice(b"Bitcoin seed")
        .map_err(|e| Error::DerivationFailed(e.to_string()))?;
    mac.update(&seed.0);
    let master = mac.finalize().into_bytes();

    let mut key = [0u8; 32];
    key.copy_from_slice(&master[..32]);
    let mut chain_code = [0u8; 32];
    chain_code.copy_from_slice(&master[32..]);

    let mut extended = ExtendedPrivateKey { key, chain_code };

    for &index in path {
        extended = extended.derive_child(index)?;
    }

    key.zeroize();
    chain_code.zeroize();

    Ok(extended)
}

/// Derive the standard Ethereum key for an account index:
/// `m / 44' / 60' / 0' / 0 / index`.
pub fn derive_ethereum_key(seed: &Seed, account: u32) -> Result<ExtendedPrivateKey> {
    derive_child_key(
        seed,
        &[
            44 | HARDENED_OFFSET,
            60 | HARDENED_OFFSET,
            0 | HARDENED_OFFSET,
            0,
            account,
        ],
    )
}

/// The BIP-32 hardened index flag.
pub const HARDENED_OFFSET: u32 = 0x8000_0000;

impl ExtendedPrivateKey {
    /// Derive one child.
    ///
    /// A hardened child hashes the private key; a normal child hashes the
    /// public key. Using the private key for a normal child would let anyone
    /// who learns one child key and the chain code derive the parent.
    pub fn derive_child(&self, index: u32) -> Result<Self> {
        let mut mac = HmacSha512::new_from_slice(&self.chain_code)
            .map_err(|e| Error::DerivationFailed(e.to_string()))?;

        if index >= HARDENED_OFFSET {
            mac.update(&[0u8]);
            mac.update(&self.key);
        } else {
            mac.update(&self.public_key_bytes());
        }
        mac.update(&index.to_be_bytes());

        let output = mac.finalize().into_bytes();
        let mut tweak = [0u8; 32];
        tweak.copy_from_slice(&output[..32]);

        // The child key is (parent + tweak) mod n. Deriving with the secp256k1
        // scalar arithmetic rather than plain addition keeps the result on the
        // curve.
        let child_key = self
            .scalar_add(&tweak)
            .ok_or_else(|| Error::DerivationFailed("child key is not a valid scalar".to_string()))?;

        let mut chain_code = [0u8; 32];
        chain_code.copy_from_slice(&output[32..]);

        tweak.zeroize();

        Ok(ExtendedPrivateKey {
            key: child_key,
            chain_code,
        })
    }

    fn scalar_add(&self, tweak: &[u8; 32]) -> Option<[u8; 32]> {
        use k256::elliptic_curve::ops::Reduce;
        use k256::{NonZeroScalar, Scalar, U256};

        let parent = NonZeroScalar::try_from(&self.key[..]).ok()?;
        let tweak_scalar = <Scalar as Reduce<U256>>::reduce_bytes(&(*tweak).into());

        // Adding through the scalar and re-wrapping keeps the sum on the
        // curve. The wrap fails when the sum is zero, which is a derivation
        // failure rather than a panic.
        let sum = *parent.as_ref() + tweak_scalar;
        let sum = NonZeroScalar::new(sum).into_option()?;

        Some(sum.to_bytes().into())
    }

    /// The 33-byte compressed public key for this key.
    fn public_key_bytes(&self) -> [u8; 33] {
        let signing_key = k256::ecdsa::SigningKey::from_slice(&self.key)
            .expect("extended private key holds a valid scalar");
        let point = signing_key.verifying_key().to_encoded_point(true);
        let mut bytes = [0u8; 33];
        bytes.copy_from_slice(point.as_bytes());
        bytes
    }

    /// The private key bytes.
    pub fn private_key_bytes(&self) -> [u8; 32] {
        self.key
    }

    /// The chain code.
    pub fn chain_code(&self) -> [u8; 32] {
        self.chain_code
    }

    /// Turn this into a signing key.
    pub fn to_private_key(&self) -> Result<crate::signing::PrivateKey> {
        crate::signing::PrivateKey::from_bytes(self.key)
    }
}

impl Mnemonic {
    /// The phrase.
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// How many words the phrase has.
    pub fn word_count(&self) -> usize {
        self.0.split_whitespace().count()
    }
}

impl Seed {
    /// The seed bytes.
    pub fn as_bytes(&self) -> &[u8; 64] {
        &self.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The BIP-39 test vector for the passphrase "TREZOR".
    const VECTOR_ENTROPY: [u8; 16] = [
        0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
        0x00,
    ];
    const VECTOR_MNEMONIC: &str = "abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon about";
    const VECTOR_SEED: &str = "c55257c360c07c72029aebc1b53c05ed0362ada38ead3e3e9efa3708e53495531f09a6987599d18264c1e1c92f2cf141630c7a3c4ab7c81b2f001698e7463b04";

    #[test]
    fn mnemonic_matches_the_test_vector() {
        let mnemonic = mnemonic_from_entropy(&VECTOR_ENTROPY).unwrap();
        assert_eq!(mnemonic.as_str(), VECTOR_MNEMONIC);
    }

    #[test]
    fn seed_matches_the_test_vector() {
        let mnemonic = mnemonic_from_entropy(&VECTOR_ENTROPY).unwrap();
        let seed = seed_from_mnemonic(&mnemonic, "TREZOR").unwrap();

        assert_eq!(hex::encode(seed.as_bytes()), VECTOR_SEED);
    }

    #[test]
    fn all_entropy_sizes_are_accepted() {
        for strength in [
            EntropyStrength::Bits128,
            EntropyStrength::Bits160,
            EntropyStrength::Bits192,
            EntropyStrength::Bits224,
            EntropyStrength::Bits256,
        ] {
            let mnemonic = generate_mnemonic(strength).unwrap();
            assert_eq!(mnemonic.word_count(), strength.word_count());
            assert!(validate_mnemonic(mnemonic.as_str()));
        }
    }

    #[test]
    fn rejects_an_unsupported_entropy_length() {
        // 17 bytes is not one of the five BIP-39 sizes.
        assert!(mnemonic_from_entropy(&[0u8; 17]).is_err());
    }

    #[test]
    fn rejects_a_bad_checksum() {
        // Last word changed, so the checksum no longer matches.
        let bad = "abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon";
        assert!(!validate_mnemonic(bad));
    }

    #[test]
    fn rejects_a_wrong_word_count() {
        assert!(!validate_mnemonic("abandon abandon abandon"));
    }

    #[test]
    fn rejects_an_unknown_word() {
        let bad = "abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon tiger";
        assert!(!validate_mnemonic(bad));
    }

    #[test]
    fn generation_is_random() {
        let first = generate_mnemonic(EntropyStrength::Bits256).unwrap();
        let second = generate_mnemonic(EntropyStrength::Bits256).unwrap();

        assert_ne!(first.as_str(), second.as_str());
    }

    #[test]
    fn hardened_and_normal_children_differ() {
        let seed = Seed([7u8; 64]);
        let key = derive_child_key(&seed, &[]).unwrap();

        let normal = key.derive_child(0).unwrap();
        let hardened = key.derive_child(0 | HARDENED_OFFSET).unwrap();

        assert_ne!(normal.private_key_bytes(), hardened.private_key_bytes());
    }

    #[test]
    fn derivation_is_deterministic() {
        let seed = Seed([9u8; 64]);

        let first = derive_ethereum_key(&seed, 0).unwrap();
        let second = derive_ethereum_key(&seed, 0).unwrap();

        assert_eq!(first.private_key_bytes(), second.private_key_bytes());
        assert_eq!(first.chain_code(), second.chain_code());
    }

    #[test]
    fn different_accounts_give_different_keys() {
        let seed = Seed([3u8; 64]);

        let account_zero = derive_ethereum_key(&seed, 0).unwrap();
        let account_one = derive_ethereum_key(&seed, 1).unwrap();

        assert_ne!(
            account_zero.private_key_bytes(),
            account_one.private_key_bytes()
        );
    }

    #[test]
    fn derived_key_is_usable_for_signing() {
        let seed = Seed([5u8; 64]);
        let extended = derive_ethereum_key(&seed, 0).unwrap();
        let private_key = extended.to_private_key().unwrap();

        let digest = crate::hashing::keccak256(b"transaction");
        let signature = crate::signing::sign(&private_key, &digest).unwrap();

        assert!(crate::signing::verify(&private_key.public_key(), &digest, &signature).is_ok());
    }

    #[test]
    fn mnemonic_does_not_appear_in_debug_output() {
        let mnemonic = generate_mnemonic(EntropyStrength::Bits128).unwrap();
        let rendered = format!("{:?}", mnemonic);
        assert!(rendered.contains("redacted"));
    }
}
