//! Authenticated encryption for TigerScan.
//!
//! Three constructions, all AEAD so that tampering is detected rather than
//! producing corrupt plaintext:
//!
//! * AES-256-GCM, for hardware-accelerated bulk encryption.
//! * ChaCha20-Poly1305, for platforms without AES acceleration.
//! * ECIES over secp256k1, for encrypting to a public key.
//!
//! There is deliberately no unauthenticated "simple XOR" cipher: it cannot be
//! made safe by documentation, so it is not offered.

use aes_gcm::aead::{Aead, KeyInit, OsRng, Payload};
use aes_gcm::{Aes256Gcm, Nonce};
use chacha20poly1305::{ChaCha20Poly1305, Nonce as ChaChaNonce};
use hkdf::Hkdf;
use hmac::{Hmac, Mac};
use k256::ecdh::diffie_hellman;
use k256::elliptic_curve::sec1::ToEncodedPoint;
use k256::{PublicKey, SecretKey};
use rand::RngCore;
use sha2::{Digest, Sha256, Sha384};
use thiserror::Error;
use zeroize::Zeroizing;

pub mod types;

pub use types::{EncryptedData, KeyPair};

// =============================================================================
// ERRORS
// =============================================================================

#[derive(Error, Debug)]
pub enum EncryptionError {
    #[error("encryption failed: {0}")]
    EncryptionFailed(String),

    #[error("decryption failed")]
    DecryptionFailed,

    #[error("invalid key length: {0} bytes")]
    InvalidKeyLength(usize),

    #[error("invalid ciphertext")]
    InvalidCiphertext,

    #[error("key derivation failed: {0}")]
    KeyDerivationFailed(String),

    #[error("signature error: {0}")]
    SignatureError(String),
}

type Aes256GcmCipher = Aes256Gcm;
type ChaChaCipher = ChaCha20Poly1305;
type HmacSha256 = Hmac<Sha256>;

/// Uncompressed secp256k1 point: 1 prefix byte + 32-byte X + 32-byte Y.
const SEC1_UNCOMPRESSED_LEN: usize = 65;
const GCM_NONCE_LEN: usize = 12;
const GCM_TAG_LEN: usize = 16;

// =============================================================================
// SYMMETRIC CIPHER
// =============================================================================

/// AES-256-GCM and ChaCha20-Poly1305 under independent subkeys.
pub struct AdvancedCipher {
    aes_key: Zeroizing<[u8; 32]>,
    chacha_key: Zeroizing<[u8; 32]>,
}

impl AdvancedCipher {
    /// Derive both subkeys from a master secret.
    ///
    /// Uses HKDF-SHA256 with distinct `info` strings, which is the construction
    /// HKDF is defined for. The previous implementation built a key by
    /// repeatedly HMAC-ing the growing key material, which is not HKDF and had
    /// no domain separation between the two subkeys.
    pub fn new(master_key: &[u8]) -> Result<Self, EncryptionError> {
        if master_key.len() < 32 {
            return Err(EncryptionError::InvalidKeyLength(master_key.len()));
        }

        let hkdf = Hkdf::<Sha256>::new(None, master_key);

        let mut aes_key = [0u8; 32];
        hkdf.expand(b"TigerScan/AES-256-GCM/v1", &mut aes_key)
            .map_err(|e| EncryptionError::KeyDerivationFailed(e.to_string()))?;

        let mut chacha_key = [0u8; 32];
        hkdf.expand(b"TigerScan/ChaCha20-Poly1305/v1", &mut chacha_key)
            .map_err(|e| EncryptionError::KeyDerivationFailed(e.to_string()))?;

        Ok(Self {
            aes_key: Zeroizing::new(aes_key),
            chacha_key: Zeroizing::new(chacha_key),
        })
    }

    /// AES-256-GCM encrypt. Returns ciphertext, nonce and tag separately.
    pub fn encrypt_aead(
        &self,
        plaintext: &[u8],
        aad: &[u8],
    ) -> Result<EncryptedData, EncryptionError> {
        let cipher = Aes256GcmCipher::new(self.aes_key.as_ref().into());
        let nonce = Self::random_nonce();

        let mut sealed = cipher
            .encrypt(Nonce::from_slice(&nonce), Payload { msg: plaintext, aad })
            .map_err(|_| EncryptionError::EncryptionFailed("AES-256-GCM".into()))?;

        let tag = sealed.split_off(sealed.len() - GCM_TAG_LEN);

        Ok(EncryptedData {
            ciphertext: sealed,
            nonce: nonce.to_vec(),
            tag,
        })
    }

    /// AES-256-GCM decrypt. Fails if the ciphertext, nonce, tag or AAD differ
    /// from what was authenticated.
    pub fn decrypt_aead(
        &self,
        data: &EncryptedData,
        aad: &[u8],
    ) -> Result<Vec<u8>, EncryptionError> {
        Self::check_shape(data)?;

        let cipher = Aes256GcmCipher::new(self.aes_key.as_ref().into());

        let mut sealed = data.ciphertext.clone();
        sealed.extend_from_slice(&data.tag);

        cipher
            .decrypt(
                Nonce::from_slice(&data.nonce),
                Payload { msg: &sealed, aad },
            )
            .map_err(|_| EncryptionError::DecryptionFailed)
    }

    /// ChaCha20-Poly1305 encrypt.
    pub fn encrypt_chacha(
        &self,
        plaintext: &[u8],
        aad: &[u8],
    ) -> Result<EncryptedData, EncryptionError> {
        let cipher = ChaChaCipher::new(self.chacha_key.as_ref().into());
        let nonce = Self::random_nonce();

        let mut sealed = cipher
            .encrypt(
                ChaChaNonce::from_slice(&nonce),
                Payload { msg: plaintext, aad },
            )
            .map_err(|_| EncryptionError::EncryptionFailed("ChaCha20-Poly1305".into()))?;

        let tag = sealed.split_off(sealed.len() - GCM_TAG_LEN);

        Ok(EncryptedData {
            ciphertext: sealed,
            nonce: nonce.to_vec(),
            tag,
        })
    }

    /// ChaCha20-Poly1305 decrypt.
    pub fn decrypt_chacha(
        &self,
        data: &EncryptedData,
        aad: &[u8],
    ) -> Result<Vec<u8>, EncryptionError> {
        Self::check_shape(data)?;

        let cipher = ChaChaCipher::new(self.chacha_key.as_ref().into());

        let mut sealed = data.ciphertext.clone();
        sealed.extend_from_slice(&data.tag);

        cipher
            .decrypt(
                ChaChaNonce::from_slice(&data.nonce),
                Payload { msg: &sealed, aad },
            )
            .map_err(|_| EncryptionError::DecryptionFailed)
    }

    fn check_shape(data: &EncryptedData) -> Result<(), EncryptionError> {
        if data.nonce.len() != GCM_NONCE_LEN || data.tag.len() != GCM_TAG_LEN {
            return Err(EncryptionError::InvalidCiphertext);
        }
        Ok(())
    }

    fn random_nonce() -> [u8; GCM_NONCE_LEN] {
        let mut nonce = [0u8; GCM_NONCE_LEN];
        OsRng.fill_bytes(&mut nonce);
        nonce
    }
}

// =============================================================================
// ECIES
// =============================================================================

/// An ephemeral-key ECIES message.
///
/// Wire format: `ephemeral_pubkey (65) || nonce (12) || ciphertext || tag (16)`.
/// The ephemeral key is needed to rederive the shared secret at decryption time.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EciesMessage {
    pub bytes: Vec<u8>,
}

/// Encrypt `plaintext` to `recipient_pubkey`, an uncompressed secp256k1 point.
pub fn ecies_encrypt(
    plaintext: &[u8],
    recipient_pubkey: &[u8],
    aad: &[u8],
) -> Result<EciesMessage, EncryptionError> {
    if recipient_pubkey.len() != SEC1_UNCOMPRESSED_LEN {
        return Err(EncryptionError::InvalidKeyLength(recipient_pubkey.len()));
    }

    let recipient = PublicKey::from_sec1_bytes(recipient_pubkey)
        .map_err(|e| EncryptionError::EncryptionFailed(e.to_string()))?;

    let ephemeral = SecretKey::random(&mut OsRng);
    let ephemeral_pub = ephemeral.public_key();

    let shared = diffie_hellman(ephemeral.to_nonzero_scalar(), recipient.as_affine());
    let key = ecies_kdf(shared.raw_secret_bytes().as_slice(), &ephemeral_pub, &recipient)?;
    let cipher = AdvancedCipher::new(&key)?;

    // The ephemeral public key is bound into the AAD so it cannot be swapped.
    let bound_aad = ecies_aad(&ephemeral_pub, recipient_pubkey, aad);
    let sealed = cipher.encrypt_aead(plaintext, &bound_aad)?;

    let mut bytes = Vec::with_capacity(SEC1_UNCOMPRESSED_LEN + GCM_NONCE_LEN + sealed.ciphertext.len() + GCM_TAG_LEN);
    bytes.extend_from_slice(ephemeral_pub.to_encoded_point(false).as_bytes());
    bytes.extend_from_slice(&sealed.nonce);
    bytes.extend_from_slice(&sealed.ciphertext);
    bytes.extend_from_slice(&sealed.tag);

    Ok(EciesMessage { bytes })
}

/// Decrypt an ECIES message with the recipient's 32-byte secret key.
pub fn ecies_decrypt(
    message: &EciesMessage,
    private_key: &[u8],
    aad: &[u8],
) -> Result<Vec<u8>, EncryptionError> {
    if private_key.len() != 32 {
        return Err(EncryptionError::InvalidKeyLength(private_key.len()));
    }

    let bytes = &message.bytes;
    if bytes.len() < SEC1_UNCOMPRESSED_LEN + GCM_NONCE_LEN + GCM_TAG_LEN {
        return Err(EncryptionError::InvalidCiphertext);
    }

    let mut sk_bytes = Zeroizing::new([0u8; 32]);
    sk_bytes.copy_from_slice(private_key);
    let secret = SecretKey::from_slice(sk_bytes.as_slice())
        .map_err(|e| EncryptionError::SignatureError(e.to_string()))?;

    let (ephemeral_bytes, rest) = bytes.split_at(SEC1_UNCOMPRESSED_LEN);
    let ephemeral = PublicKey::from_sec1_bytes(ephemeral_bytes)
        .map_err(|_| EncryptionError::InvalidCiphertext)?;

    let (nonce, rest) = rest.split_at(GCM_NONCE_LEN);
    let (ciphertext, tag) = rest.split_at(rest.len() - GCM_TAG_LEN);

    let shared = diffie_hellman(secret.to_nonzero_scalar(), ephemeral.as_affine());
    let recipient_pub = secret.public_key().to_encoded_point(false);
    let key = ecies_kdf(
        shared.raw_secret_bytes().as_slice(),
        &ephemeral,
        &secret.public_key(),
    )?;
    let cipher = AdvancedCipher::new(&key)?;

    let bound_aad = ecies_aad(&ephemeral, recipient_pub.as_bytes(), aad);

    cipher.decrypt_aead(
        &EncryptedData {
            ciphertext: ciphertext.to_vec(),
            nonce: nonce.to_vec(),
            tag: tag.to_vec(),
        },
        &bound_aad,
    )
}

/// Derive the message key from the ECDH secret and both public keys.
///
/// Mixing in both public keys keeps the derived key unique to this key pair.
fn ecies_kdf(
    shared_secret: &[u8],
    ephemeral: &PublicKey,
    recipient: &PublicKey,
) -> Result<[u8; 32], EncryptionError> {
    let hkdf = Hkdf::<Sha256>::new(None, shared_secret);
    let mut info = Vec::new();
    info.extend_from_slice(b"TigerScan/ECIES/v1");
    info.extend_from_slice(ephemeral.to_encoded_point(false).as_bytes());
    info.extend_from_slice(recipient.to_encoded_point(false).as_bytes());

    let mut key = [0u8; 32];
    hkdf.expand(&info, &mut key)
        .map_err(|e| EncryptionError::KeyDerivationFailed(e.to_string()))?;
    Ok(key)
}

fn ecies_aad(ephemeral: &PublicKey, recipient: &[u8], aad: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(SEC1_UNCOMPRESSED_LEN + recipient.len() + aad.len());
    out.extend_from_slice(ephemeral.to_encoded_point(false).as_bytes());
    out.extend_from_slice(recipient);
    out.extend_from_slice(aad);
    out
}

// =============================================================================
// KEY PAIRS AND SIGNATURES
// =============================================================================

impl AdvancedCipher {
    /// Generate a secp256k1 key pair, hex-encoded.
    ///
    /// `private_key` is the 32-byte scalar and `public_key` the uncompressed
    /// point, so `public_key` can be passed straight to `ecies_encrypt`.
    pub fn generate_key_pair() -> KeyPair {
        let secret = SecretKey::random(&mut OsRng);
        KeyPair {
            public_key: hex::encode(secret.public_key().to_encoded_point(false).as_bytes()),
            private_key: hex::encode(secret.to_bytes()),
        }
    }

    /// Sign `message` with ECDSA over secp256k1, returning a 64-byte `r || s`.
    pub fn sign(private_key_hex: &str, message: &[u8]) -> Result<Vec<u8>, EncryptionError> {
        use k256::ecdsa::signature::Signer;
        use k256::ecdsa::SigningKey;

        let raw = hex::decode(private_key_hex)
            .map_err(|e| EncryptionError::SignatureError(e.to_string()))?;
        let key = SigningKey::from_slice(&raw)
            .map_err(|e| EncryptionError::SignatureError(e.to_string()))?;

        let signature: k256::ecdsa::Signature = key.sign(message);
        Ok(signature.to_bytes().to_vec())
    }

    /// Verify a signature produced by [`AdvancedCipher::sign`].
    pub fn verify(public_key_hex: &str, message: &[u8], signature: &[u8]) -> bool {
        use k256::ecdsa::signature::Verifier;
        use k256::ecdsa::{Signature, VerifyingKey};

        let Ok(raw) = hex::decode(public_key_hex) else {
            return false;
        };
        let Ok(key) = VerifyingKey::from_sec1_bytes(&raw) else {
            return false;
        };
        let Ok(sig) = Signature::from_slice(signature) else {
            return false;
        };
        key.verify(message, &sig).is_ok()
    }
}

// =============================================================================
// HASHES AND ENCODING
// =============================================================================

pub fn hash_sha256(data: &[u8]) -> Vec<u8> {
    Sha256::digest(data).to_vec()
}

pub fn hash_sha384(data: &[u8]) -> Vec<u8> {
    Sha384::digest(data).to_vec()
}

/// HMAC-SHA256. `key` may be any length, per RFC 2104.
pub fn hmac_sha256(key: &[u8], data: &[u8]) -> Vec<u8> {
    let mut mac = <HmacSha256 as Mac>::new_from_slice(key)
        .expect("HMAC accepts keys of any length");
    mac.update(data);
    mac.finalize().into_bytes().to_vec()
}

/// Serialise ciphertext, nonce and tag as one base64 string.
pub fn encode_encrypted(data: &EncryptedData) -> String {
    use base64::Engine;
    let mut combined = data.ciphertext.clone();
    combined.extend_from_slice(&data.nonce);
    combined.extend_from_slice(&data.tag);
    base64::engine::general_purpose::STANDARD.encode(&combined)
}

/// Inverse of [`encode_encrypted`].
pub fn decode_encrypted(encoded: &str) -> Result<EncryptedData, EncryptionError> {
    use base64::Engine;
    let combined = base64::engine::general_purpose::STANDARD
        .decode(encoded)
        .map_err(|_| EncryptionError::InvalidCiphertext)?;

    if combined.len() < GCM_NONCE_LEN + GCM_TAG_LEN {
        return Err(EncryptionError::InvalidCiphertext);
    }

    let ct_len = combined.len() - GCM_NONCE_LEN - GCM_TAG_LEN;
    let (ciphertext, rest) = combined.split_at(ct_len);
    let (nonce, tag) = rest.split_at(GCM_NONCE_LEN);

    Ok(EncryptedData {
        ciphertext: ciphertext.to_vec(),
        nonce: nonce.to_vec(),
        tag: tag.to_vec(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const MASTER: &[u8] = b"0123456789abcdef0123456789abcdef";

    #[test]
    fn aes_gcm_round_trips() {
        let cipher = AdvancedCipher::new(MASTER).unwrap();
        let sealed = cipher.encrypt_aead(b"Hello, TigerScan!", b"aad").unwrap();
        assert_eq!(sealed.nonce.len(), GCM_NONCE_LEN);
        assert_eq!(sealed.tag.len(), GCM_TAG_LEN);
        assert_eq!(
            cipher.decrypt_aead(&sealed, b"aad").unwrap(),
            b"Hello, TigerScan!"
        );
    }

    #[test]
    fn chacha_round_trips() {
        let cipher = AdvancedCipher::new(MASTER).unwrap();
        let sealed = cipher.encrypt_chacha(b"Hello, ChaCha!", b"aad").unwrap();
        assert_eq!(
            cipher.decrypt_chacha(&sealed, b"aad").unwrap(),
            b"Hello, ChaCha!"
        );
    }

    #[test]
    fn wrong_aad_is_rejected() {
        let cipher = AdvancedCipher::new(MASTER).unwrap();
        let sealed = cipher.encrypt_aead(b"secret", b"right").unwrap();
        assert!(cipher.decrypt_aead(&sealed, b"wrong").is_err());
        assert!(cipher.decrypt_aead(&sealed, b"").is_err());
    }

    #[test]
    fn tampered_ciphertext_is_rejected() {
        let cipher = AdvancedCipher::new(MASTER).unwrap();
        let mut sealed = cipher.encrypt_aead(b"secret", b"").unwrap();
        sealed.ciphertext[0] ^= 1;
        assert!(cipher.decrypt_aead(&sealed, b"").is_err());

        let mut sealed = cipher.encrypt_chacha(b"secret", b"").unwrap();
        sealed.tag[0] ^= 1;
        assert!(cipher.decrypt_chacha(&sealed, b"").is_err());
    }

    #[test]
    fn short_master_keys_are_rejected() {
        assert!(AdvancedCipher::new(b"too short").is_err());
    }

    #[test]
    fn the_two_subkeys_differ() {
        // A single key reused for both ciphers would make this equal.
        let cipher = AdvancedCipher::new(MASTER).unwrap();
        assert_ne!(cipher.aes_key.as_ref(), cipher.chacha_key.as_ref());
    }

    #[test]
    fn a_different_master_key_cannot_decrypt() {
        let sealed = AdvancedCipher::new(MASTER)
            .unwrap()
            .encrypt_aead(b"secret", b"")
            .unwrap();
        let other = AdvancedCipher::new(b"fedcba9876543210fedcba9876543210").unwrap();
        assert!(other.decrypt_aead(&sealed, b"").is_err());
    }

    #[test]
    fn ecies_round_trips_between_two_parties() {
        let recipient = AdvancedCipher::generate_key_pair();
        let pubkey = hex::decode(&recipient.public_key).unwrap();
        let privkey = hex::decode(&recipient.private_key).unwrap();

        let message = ecies_encrypt(b"attack at dawn", &pubkey, b"ctx").unwrap();
        let plaintext = ecies_decrypt(&message, &privkey, b"ctx").unwrap();

        assert_eq!(plaintext, b"attack at dawn");
    }

    #[test]
    fn ecies_rejects_the_wrong_recipient() {
        let recipient = AdvancedCipher::generate_key_pair();
        let attacker = AdvancedCipher::generate_key_pair();
        let pubkey = hex::decode(&recipient.public_key).unwrap();

        let message = ecies_encrypt(b"for the recipient", &pubkey, b"").unwrap();
        let attacker_key = hex::decode(&attacker.private_key).unwrap();

        assert!(ecies_decrypt(&message, &attacker_key, b"").is_err());
    }

    #[test]
    fn ecies_rejects_a_tampered_message() {
        let recipient = AdvancedCipher::generate_key_pair();
        let pubkey = hex::decode(&recipient.public_key).unwrap();
        let privkey = hex::decode(&recipient.private_key).unwrap();

        let mut message = ecies_encrypt(b"payload", &pubkey, b"").unwrap();
        let last = message.bytes.len() - 1;
        message.bytes[last] ^= 1;

        assert!(ecies_decrypt(&message, &privkey, b"").is_err());
    }

    #[test]
    fn ecies_rejects_a_swapped_ephemeral_key() {
        // The ephemeral key is authenticated as AAD, so replacing it must fail
        // even when the replacement is a valid point.
        let recipient = AdvancedCipher::generate_key_pair();
        let pubkey = hex::decode(&recipient.public_key).unwrap();
        let privkey = hex::decode(&recipient.private_key).unwrap();

        let mut message = ecies_encrypt(b"payload", &pubkey, b"").unwrap();
        let other = AdvancedCipher::generate_key_pair();
        let other_pub = hex::decode(&other.public_key).unwrap();
        message.bytes[..SEC1_UNCOMPRESSED_LEN].copy_from_slice(&other_pub);

        assert!(ecies_decrypt(&message, &privkey, b"").is_err());
    }

    #[test]
    fn ecies_rejects_malformed_input() {
        let recipient = AdvancedCipher::generate_key_pair();
        let privkey = hex::decode(&recipient.private_key).unwrap();
        assert!(ecies_encrypt(b"x", &[0u8; 10], b"").is_err());
        assert!(ecies_decrypt(&EciesMessage { bytes: vec![0u8; 20] }, &privkey, b"").is_err());
        assert!(ecies_decrypt(
            &EciesMessage {
                bytes: vec![0u8; 100]
            },
            &[0u8; 16],
            b""
        )
        .is_err());
    }

    #[test]
    fn generated_key_pairs_sign_and_verify() {
        let pair = AdvancedCipher::generate_key_pair();
        let signature = AdvancedCipher::sign(&pair.private_key, b"message").unwrap();

        assert!(AdvancedCipher::verify(&pair.public_key, b"message", &signature));
        assert!(!AdvancedCipher::verify(&pair.public_key, b"other", &signature));

        let mut bad = signature.clone();
        bad[0] ^= 1;
        assert!(!AdvancedCipher::verify(&pair.public_key, b"message", &bad));
    }

    #[test]
    fn hashes_match_published_vectors() {
        assert_eq!(
            hex::encode(hash_sha256(b"abc")),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
        assert_eq!(
            hex::encode(hash_sha384(b"abc")),
            "cb00753f45a35e8bb5a03d699ac65007272c32ab0eded1631a8b605a43ff5bed\
             8086072ba1e7cc2358baeca134c825a7"
        );
    }

    #[test]
    fn hmac_sha256_matches_rfc4231_test_case_2() {
        // RFC 4231 case 2: key "Jefe", data "what do ya want for nothing?".
        assert_eq!(
            hex::encode(hmac_sha256(b"Jefe", b"what do ya want for nothing?")),
            "5bdcc146bf60754e6a042426089575c75a003f089d2739839dec58b964ec3843"
        );
    }

    #[test]
    fn base64_encoding_round_trips() {
        let cipher = AdvancedCipher::new(MASTER).unwrap();
        let sealed = cipher.encrypt_aead(b"payload", b"aad").unwrap();

        let encoded = encode_encrypted(&sealed);
        let decoded = decode_encrypted(&encoded).unwrap();

        assert_eq!(decoded, sealed);
        assert_eq!(cipher.decrypt_aead(&decoded, b"aad").unwrap(), b"payload");
    }

    #[test]
    fn base64_decoding_rejects_garbage() {
        assert!(decode_encrypted("not base64!!").is_err());
        assert!(decode_encrypted("").is_err());
    }
}
