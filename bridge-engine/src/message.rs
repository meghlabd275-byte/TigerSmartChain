//! Cross-chain message passing primitives.

use crate::Chain;
use serde::{Deserialize, Serialize};

/// A message routed across chains by the bridge relayer set.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    pub id: String,
    pub source_chain: Chain,
    pub destination_chain: Chain,
    pub sender: String,
    pub recipient: String,
    pub data: Vec<u8>,
    pub nonce: u64,
    pub timestamp: i64,
}

impl Message {
    /// Build a new message with a deterministic id derived from its fields.
    pub fn new(
        source_chain: Chain,
        destination_chain: Chain,
        sender: String,
        recipient: String,
        data: Vec<u8>,
        nonce: u64,
    ) -> Self {
        use sha3::{Digest, Keccak256};
        let mut hasher = Keccak256::new();
        hasher.update(format!("{:?}", source_chain).as_bytes());
        hasher.update(format!("{:?}", destination_chain).as_bytes());
        hasher.update(sender.as_bytes());
        hasher.update(recipient.as_bytes());
        hasher.update(&data);
        hasher.update(nonce.to_le_bytes());
        let id = format!("0x{}", hex::encode(hasher.finalize()));
        Self {
            id,
            source_chain,
            destination_chain,
            sender,
            recipient,
            data,
            nonce,
            timestamp: chrono::Utc::now().timestamp(),
        }
    }
}
