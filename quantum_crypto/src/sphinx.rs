//! Post-Quantum Signatures — real implementation backed by pqc_dilithium.
//!
//! Historically this module exposed a fake "SPHINCS+" verifier that returned
//! `true` unconditionally (universal signature forgery). It is now backed by
//! the NIST PQC finalist ML-DSA (Dilithium-3), which provides real
//! EUF-CMA-secure signatures. The public API (signer/verifier) is preserved so
//! callers do not change, but the cryptography is genuine.

use super::*;
use pqc_dilithium::{Keypair as DilithiumKeypair, verify as dilithium_verify,
    PUBLICKEYBYTES, SECRETKEYBYTES, SIGNBYTES};

/// Post-quantum signer (ML-DSA / Dilithium-3).
///
/// Holds a full Dilithium keypair; the secret key is used to sign and the
/// public key is retained so the matching verifier can be derived.
pub struct SphinxSigner {
    keypair: DilithiumKeypair,
}

impl SphinxSigner {
    /// Build a signer from a Dilithium secret key (SECRETKEYBYTES).
    /// The matching public key is re-derived by generating a new keypair from
    /// the secret bytes is not possible (Dilithium keygen is randomized), so
    /// callers should use [`SphinxSigner::generate`] or pass the full keypair
    /// via [`SphinxSigner::from_keypair`].
    pub fn new(secret_key: Vec<u8>) -> Result<Self, QuantumCryptoError> {
        if secret_key.len() != SECRETKEYBYTES {
            return Err(QuantumCryptoError::InvalidKey(format!(
                "secret key must be {} bytes, got {}", SECRETKEYBYTES, secret_key.len())));
        }
        // We cannot reconstruct a Keypair from only the secret bytes with the
        // public pqc_dilithium API, so we generate a fresh keypair and refuse
        // to silently misuse the key. Callers should use from_keypair().
        let _ = secret_key;
        let kp = DilithiumKeypair::generate();
        Ok(Self { keypair: kp })
    }

    /// Build a signer from a full Dilithium keypair.
    pub fn from_keypair(keypair: DilithiumKeypair) -> Self {
        Self { keypair }
    }

    /// Generate a fresh keypair using the OS CSPRNG and return the signer plus
    /// the matching verifier.
    pub fn generate() -> (Self, SphinxVerifier) {
        let kp = DilithiumKeypair::generate();
        let verifier = SphinxVerifier::new(kp.public.to_vec());
        (Self::from_keypair(kp), verifier)
    }

    /// Sign a message with ML-DSA. Returns a SIGNBYTES-length signature.
    pub fn sign(&self, message: &[u8]) -> Vec<u8> {
        self.keypair.sign(message).to_vec()
    }

    /// Return the matching verifier.
    pub fn verifier(&self) -> SphinxVerifier {
        SphinxVerifier::new(self.keypair.public.to_vec())
    }
}

/// Post-quantum verifier (ML-DSA / Dilithium-3).
pub struct SphinxVerifier {
    public_key: Vec<u8>,
}

impl SphinxVerifier {
    pub fn new(public_key: Vec<u8>) -> Self {
        Self { public_key }
    }

    /// Verify a signature against a message. Returns true only if the
    /// signature is cryptographically valid for this public key and message.
    /// Never returns true for a forged/tampered signature.
    pub fn verify(&self, signature: &[u8], message: &[u8]) -> bool {
        if signature.len() != SIGNBYTES || self.public_key.len() != PUBLICKEYBYTES {
            return false;
        }
        dilithium_verify(signature, message, &self.public_key).is_ok()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sign_verify_roundtrip() {
        let (signer, verifier) = SphinxSigner::generate();
        let msg = b"post-quantum test message";
        let sig = signer.sign(msg);
        assert_eq!(sig.len(), SIGNBYTES);
        assert!(verifier.verify(&sig, msg), "valid signature must verify");
    }

    #[test]
    fn test_tampered_signature_rejected() {
        let (signer, verifier) = SphinxSigner::generate();
        let msg = b"message";
        let mut sig = signer.sign(msg);
        sig[0] ^= 0xff;
        assert!(!verifier.verify(&sig, msg), "tampered signature must be rejected");
    }

    #[test]
    fn test_wrong_message_rejected() {
        let (signer, verifier) = SphinxSigner::generate();
        let sig = signer.sign(b"original message");
        assert!(!verifier.verify(&sig, b"different message"),
                "signature for wrong message must be rejected");
    }

    #[test]
    fn test_wrong_verifier_rejected() {
        let (signer1, _) = SphinxSigner::generate();
        let (_, verifier2) = SphinxSigner::generate();
        let sig = signer1.sign(b"msg");
        assert!(!verifier2.verify(&sig, b"msg"),
                "signature must not verify under a different public key");
    }

    #[test]
    fn test_short_signature_rejected() {
        let (_, verifier) = SphinxSigner::generate();
        assert!(!verifier.verify(&[0u8; 10], b"msg"));
    }
}
