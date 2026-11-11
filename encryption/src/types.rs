//! Shared encryption types.

use serde::{Deserialize, Serialize};

/// The output of an AEAD encryption.
///
/// Ciphertext, nonce and tag are kept separate so a caller cannot accidentally
/// drop the tag, which is what authenticates the ciphertext.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EncryptedData {
    pub ciphertext: Vec<u8>,
    pub nonce: Vec<u8>,
    pub tag: Vec<u8>,
}

/// A secp256k1 key pair, hex-encoded.
///
/// `public_key` is the 65-byte uncompressed SEC1 point, suitable for
/// `ecies_encrypt`. `private_key` is the 32-byte scalar.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct KeyPair {
    pub public_key: String,
    pub private_key: String,
}
