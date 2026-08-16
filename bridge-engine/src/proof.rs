//! Bridge validity proofs (validator attestations over a transfer/message).

use serde::{Deserialize, Serialize};

/// A validator attestation that a transfer/message is valid and may be
/// released on the destination chain. `signature` is a secp256k1 signature
/// over keccak256(payload) produced by an authorized validator.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Proof {
    pub transfer_id: String,
    pub validator: String,
    pub signature: String,
    pub block_number: u64,
}

impl Proof {
    pub fn new(
        transfer_id: String,
        validator: String,
        signature: String,
        block_number: u64,
    ) -> Self {
        Self {
            transfer_id,
            validator,
            signature,
            block_number,
        }
    }
}
