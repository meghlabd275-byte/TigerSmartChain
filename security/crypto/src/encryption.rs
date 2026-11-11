//! Authenticated encryption for TigerSmartChain.
//!
//! [`encrypt`] is AES-256-GCM. Every call draws a fresh 96-bit nonce from the
//! operating system CSPRNG, and the 128-bit GCM tag is verified before any
//! plaintext is returned, so a tampered ciphertext fails to decrypt rather
//! than yielding corrupt data.
//!
//! Keys come from [`generate_key`], which also uses the OS CSPRNG. A key
//! derived from a clock reading is guessable and is not accepted here.

use aes_gcm::aead::{Aead, KeyInit, Payload};
use aes_gcm::{Aes256Gcm, Nonce as GcmNonce};
use rand_core::{OsRng, RngCore};
use zeroize::{Zeroize, ZeroizeOnDrop};

use crate::{Error, Result};

/// The nonce length AES-GCM uses here. 96 bits is the size the construction
/// was designed around; other lengths are hashed internally, which adds cost
/// without adding security.
const NONCE_LEN: usize = 12;

/// The GCM authentication tag length.
const TAG_LEN: usize = 16;

/// A 256-bit AES key.
///
/// The bytes are zeroed on drop and are not printed, so a key cannot leak
/// through a log line or a debug dump.
#[derive(Clone, Zeroize, ZeroizeOnDrop)]
pub struct EncryptionKey {
    bytes: [u8; 32],
}

impl std::fmt::Debug for EncryptionKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("EncryptionKey(<redacted>)")
    }
}

/// A 96-bit nonce. Must never repeat under the same key.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Nonce {
    bytes: [u8; NONCE_LEN],
}

/// A ciphertext with the nonce and tag needed to open it.
#[derive(Clone, Debug)]
pub struct EncryptedData {
    ciphertext: Vec<u8>,
    nonce: Nonce,
    tag: [u8; TAG_LEN],
}

/// Generate a new key from the operating system CSPRNG.
pub fn generate_key() -> EncryptionKey {
    let mut bytes = [0u8; 32];
    OsRng.fill_bytes(&mut bytes);
    EncryptionKey { bytes }
}

/// Build a key from raw bytes.
///
/// This is the way to load a key from a secret store. It does not stretch the
/// input: a low-entropy byte string stays low-entropy.
pub fn key_from_bytes(bytes: [u8; 32]) -> EncryptionKey {
    EncryptionKey { bytes }
}

/// Generate a random nonce from the operating system CSPRNG.
pub fn generate_nonce() -> Nonce {
    let mut bytes = [0u8; NONCE_LEN];
    OsRng.fill_bytes(&mut bytes);
    Nonce { bytes }
}

/// Encrypt `plaintext` with AES-256-GCM under `key`.
pub fn encrypt(key: &EncryptionKey, plaintext: &[u8]) -> Result<EncryptedData> {
    encrypt_aad(key, plaintext, &[])
}

/// Encrypt `plaintext` with AES-256-GCM, authenticating `aad` alongside it.
///
/// The additional data is not encrypted, but it is covered by the tag: if it
/// differs at decryption time, the tag check fails.
pub fn encrypt_aad(key: &EncryptionKey, plaintext: &[u8], aad: &[u8]) -> Result<EncryptedData> {
    let cipher = Aes256Gcm::new_from_slice(&key.bytes)
        .map_err(|e| Error::EncryptionFailed(e.to_string()))?;

    let nonce = generate_nonce();

    // aes-gcm appends the 16-byte tag to the ciphertext, so split it back out
    // to keep the tag a separate field.
    let mut sealed = cipher
        .encrypt(
            GcmNonce::from_slice(&nonce.bytes),
            Payload {
                msg: plaintext,
                aad,
            },
        )
        .map_err(|e| Error::EncryptionFailed(e.to_string()))?;

    if sealed.len() < TAG_LEN {
        return Err(Error::EncryptionFailed(
            "ciphertext shorter than the authentication tag".to_string(),
        ));
    }

    let tag_start = sealed.len() - TAG_LEN;
    let mut tag = [0u8; TAG_LEN];
    tag.copy_from_slice(&sealed[tag_start..]);
    sealed.truncate(tag_start);

    Ok(EncryptedData {
        ciphertext: sealed,
        nonce,
        tag,
    })
}

/// Decrypt AES-256-GCM data.
///
/// Fails with [`Error::DecryptionFailed`] if the key is wrong, the ciphertext
/// was altered, or the nonce or tag was changed.
pub fn decrypt(key: &EncryptionKey, encrypted: &EncryptedData) -> Result<Vec<u8>> {
    decrypt_aad(key, encrypted, &[])
}

/// Decrypt AES-256-GCM data that was sealed with additional authenticated
/// data. The same `aad` must be supplied, or the tag check fails.
pub fn decrypt_aad(
    key: &EncryptionKey,
    encrypted: &EncryptedData,
    aad: &[u8],
) -> Result<Vec<u8>> {
    let cipher = Aes256Gcm::new_from_slice(&key.bytes)
        .map_err(|e| Error::DecryptionFailed(e.to_string()))?;

    let mut sealed = Vec::with_capacity(encrypted.ciphertext.len() + TAG_LEN);
    sealed.extend_from_slice(&encrypted.ciphertext);
    sealed.extend_from_slice(&encrypted.tag);

    cipher
        .decrypt(
            GcmNonce::from_slice(&encrypted.nonce.bytes),
            Payload {
                msg: &sealed,
                aad,
            },
        )
        .map_err(|_| Error::DecryptionFailed("authentication tag mismatch".to_string()))
}

impl EncryptionKey {
    /// Borrow the raw key bytes.
    pub fn as_bytes(&self) -> &[u8; 32] {
        &self.bytes
    }

    /// Hex-encode the key.
    ///
    /// Prefer to keep keys out of logs and out of version control.
    pub fn to_hex(&self) -> String {
        hex::encode(self.bytes)
    }
}

impl Nonce {
    /// Borrow the nonce bytes.
    pub fn as_bytes(&self) -> &[u8; NONCE_LEN] {
        &self.bytes
    }
}

impl EncryptedData {
    /// The ciphertext, without the nonce or tag.
    pub fn ciphertext(&self) -> &[u8] {
        &self.ciphertext
    }

    /// The nonce used for this message.
    pub fn nonce(&self) -> &Nonce {
        &self.nonce
    }

    /// The authentication tag.
    pub fn tag(&self) -> &[u8; TAG_LEN] {
        &self.tag
    }

    /// Pack as `nonce || tag || ciphertext`.
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(NONCE_LEN + TAG_LEN + self.ciphertext.len());
        out.extend_from_slice(&self.nonce.bytes);
        out.extend_from_slice(&self.tag);
        out.extend_from_slice(&self.ciphertext);
        out
    }

    /// Unpack from `nonce || tag || ciphertext`.
    pub fn from_bytes(bytes: &[u8]) -> Result<Self> {
        if bytes.len() < NONCE_LEN + TAG_LEN {
            return Err(Error::InvalidData(
                "encrypted payload is shorter than nonce plus tag".to_string(),
            ));
        }

        let mut nonce = [0u8; NONCE_LEN];
        nonce.copy_from_slice(&bytes[..NONCE_LEN]);

        let mut tag = [0u8; TAG_LEN];
        tag.copy_from_slice(&bytes[NONCE_LEN..NONCE_LEN + TAG_LEN]);

        Ok(EncryptedData {
            ciphertext: bytes[NONCE_LEN + TAG_LEN..].to_vec(),
            nonce: Nonce { bytes: nonce },
            tag,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip() {
        let key = generate_key();
        let plaintext = b"Secret message";

        let encrypted = encrypt(&key, plaintext).unwrap();
        assert_eq!(decrypt(&key, &encrypted).unwrap(), plaintext);
    }

    #[test]
    fn empty_plaintext_round_trips() {
        let key = generate_key();
        let encrypted = encrypt(&key, b"").unwrap();
        assert_eq!(decrypt(&key, &encrypted).unwrap(), b"");
    }

    #[test]
    fn nonce_is_not_reused() {
        let key = generate_key();
        let first = encrypt(&key, b"same plaintext").unwrap();
        let second = encrypt(&key, b"same plaintext").unwrap();

        assert_ne!(first.nonce(), second.nonce());
        assert_ne!(first.ciphertext(), second.ciphertext());
    }

    #[test]
    fn wrong_key_fails() {
        let encrypted = encrypt(&generate_key(), b"Secret").unwrap();
        assert!(decrypt(&generate_key(), &encrypted).is_err());
    }

    #[test]
    fn tampered_ciphertext_is_rejected() {
        let key = generate_key();
        let mut encrypted = encrypt(&key, b"transfer 1 TGR").unwrap();
        encrypted.ciphertext[0] ^= 0x01;

        assert!(decrypt(&key, &encrypted).is_err());
    }

    #[test]
    fn tampered_tag_is_rejected() {
        let key = generate_key();
        let mut encrypted = encrypt(&key, b"data").unwrap();
        encrypted.tag[0] ^= 0x01;

        assert!(decrypt(&key, &encrypted).is_err());
    }

    #[test]
    fn tampered_nonce_is_rejected() {
        let key = generate_key();
        let mut encrypted = encrypt(&key, b"data").unwrap();
        encrypted.nonce.bytes[0] ^= 0x01;

        assert!(decrypt(&key, &encrypted).is_err());
    }

    #[test]
    fn aad_round_trip() {
        let key = generate_key();
        let encrypted = encrypt_aad(&key, b"Secret data", b"context").unwrap();
        assert_eq!(
            decrypt_aad(&key, &encrypted, b"context").unwrap(),
            b"Secret data"
        );
    }

    #[test]
    fn mismatched_aad_is_rejected() {
        let key = generate_key();
        let encrypted = encrypt_aad(&key, b"Secret data", b"context").unwrap();

        assert!(decrypt_aad(&key, &encrypted, b"other context").is_err());
        assert!(decrypt(&key, &encrypted).is_err());
    }

    #[test]
    fn aad_protects_the_tag() {
        // A payload sealed with AAD must not open without it, and one sealed
        // without AAD must not open with it.
        let key = generate_key();

        let with_aad = encrypt_aad(&key, b"msg", b"binding").unwrap();
        assert!(decrypt(&key, &with_aad).is_err());

        let without_aad = encrypt(&key, b"msg").unwrap();
        assert!(decrypt_aad(&key, &without_aad, b"binding").is_err());
    }

    #[test]
    fn serialisation_round_trip() {
        let key = generate_key();
        let encrypted = encrypt(&key, b"payload").unwrap();

        let packed = encrypted.to_bytes();
        let unpacked = EncryptedData::from_bytes(&packed).unwrap();

        assert_eq!(unpacked.nonce(), encrypted.nonce());
        assert_eq!(unpacked.tag(), encrypted.tag());
        assert_eq!(unpacked.ciphertext(), encrypted.ciphertext());
        assert_eq!(decrypt(&key, &unpacked).unwrap(), b"payload");
    }

    #[test]
    fn truncated_payload_is_rejected() {
        assert!(EncryptedData::from_bytes(&[0u8; NONCE_LEN + TAG_LEN - 1]).is_err());
    }

    #[test]
    fn ciphertext_is_the_same_length_as_the_plaintext() {
        // GCM is a stream mode, so no padding is added. The tag is stored
        // separately rather than appended.
        let key = generate_key();
        let plaintext = vec![0xAB; 1000];
        let encrypted = encrypt(&key, &plaintext).unwrap();

        assert_eq!(encrypted.ciphertext().len(), plaintext.len());
    }

    #[test]
    fn key_does_not_appear_in_debug_output() {
        let key = generate_key();
        let rendered = format!("{:?}", key);
        assert!(!rendered.contains(&key.to_hex()));
        assert!(rendered.contains("redacted"));
    }

    #[test]
    fn known_answer() {
        // NIST AES-256-GCM test vector: all-zero key and nonce, empty message
        // and empty AAD. The expected tag is 530f8afbc74536b9a963b4f1c4cb738b.
        let key = EncryptionKey { bytes: [0u8; 32] };
        let nonce = Nonce { bytes: [0u8; NONCE_LEN] };

        let cipher = Aes256Gcm::new_from_slice(&key.bytes).unwrap();
        let sealed = cipher
            .encrypt(GcmNonce::from_slice(&nonce.bytes), Payload { msg: b"", aad: b"" })
            .unwrap();

        assert_eq!(sealed.len(), TAG_LEN);
        assert_eq!(hex::encode(&sealed), "530f8afbc74536b9a963b4f1c4cb738b");
    }
}
