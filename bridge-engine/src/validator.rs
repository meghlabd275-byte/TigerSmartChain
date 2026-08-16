//! Validator set: verifies validator signatures over bridge payloads.

use ethers_core::types::{RecoveryMessage, Signature, U256};

/// A bridge validator identified by its Ethereum address.
#[derive(Debug, Clone)]
pub struct Validator {
    pub address: String,
}

impl Validator {
    pub fn new(address: String) -> Self {
        Self { address }
    }

    /// Verify that `signature_hex` (0x-prefixed 65-byte r||s||v) was produced
    /// by this validator over `message`. The digest uses the Ethereum
    /// signed-message envelope over the message bytes (applied by recover()).
    pub fn verify(&self, message: &[u8], signature_hex: &str) -> bool {
        match recover_signer(message, signature_hex) {
            Ok(recovered) => recovered.eq_ignore_ascii_case(&self.address),
            Err(_) => false,
        }
    }
}

/// Recover the signer address from an Ethereum-style signature over `message`.
pub fn recover_signer(message: &[u8], signature_hex: &str) -> Result<String, String> {
    let sig_bytes = hex::decode(signature_hex.trim_start_matches("0x"))
        .map_err(|e| format!("hex decode: {}", e))?;
    if sig_bytes.len() != 65 {
        return Err(format!(
            "invalid signature length: expected 65 bytes, got {}",
            sig_bytes.len()
        ));
    }
    let mut r = [0u8; 32];
    let mut s = [0u8; 32];
    r.copy_from_slice(&sig_bytes[0..32]);
    s.copy_from_slice(&sig_bytes[32..64]);
    let v = sig_bytes[64];

    let signature = Signature {
        r: U256::from_big_endian(&r),
        s: U256::from_big_endian(&s),
        v: v as u64,
    };

    let address = signature
        .recover(RecoveryMessage::Data(message.to_vec()))
        .map_err(|e| e.to_string())?;
    Ok(format!("{:?}", address))
}

/// Threshold validator set. A payload is considered valid once at least
/// `threshold` validators have signed it.
#[derive(Debug, Clone)]
pub struct ValidatorSet {
    pub validators: Vec<Validator>,
    pub threshold: usize,
}

impl ValidatorSet {
    pub fn new(addresses: Vec<String>, threshold: usize) -> Self {
        let validators = addresses.into_iter().map(Validator::new).collect();
        Self {
            validators,
            threshold,
        }
    }

    /// Count how many of the provided signatures belong to validators in this
    /// set and return whether the count meets the threshold.
    pub fn verify_threshold(&self, message: &[u8], signatures: &[String]) -> bool {
        let valid = signatures
            .iter()
            .filter(|sig| {
                self.validators
                    .iter()
                    .any(|v| v.verify(message, sig))
            })
            .count();
        valid >= self.threshold
    }
}
