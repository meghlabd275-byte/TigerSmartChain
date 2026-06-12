//! NFT Sync Types

use serde::{Deserialize, Serialize};

// =============================================================================
// NFT SYNC
// =============================================================================

/// NFT Transfer
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NftTransfer {
    pub collection: String,
    pub token_id: u64,
    pub from: String,
    pub to: String,
    pub block: u64,
    pub tx_hash: String,
}

/// NFT Sync
pub struct Sync {
    transfers: Vec<NftTransfer>,
}

impl Sync {
    pub fn new() -> Self {
        Self {
            transfers: vec![],
        }
    }

    /// Add transfer
    pub fn add_transfer(&mut self, transfer: NftTransfer) {
        self.transfers.push(transfer);
    }

    /// Get transfers
    pub fn get_transfers(&self) -> &[NftTransfer] {
        &self.transfers
    }
}

impl Default for Sync {
    fn default() -> Self {
        Self::new()
    }
}