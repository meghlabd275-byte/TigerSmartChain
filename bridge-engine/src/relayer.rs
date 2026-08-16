//! Relayer: observes source-chain lock events and submits release txs on the
//! destination chain, attaching a relayer signature that the bridge engine
//! verifies before completing a transfer.

use crate::validator::recover_signer;

/// A relayer identified by its Ethereum address.
#[derive(Debug, Clone)]
pub struct Relayer {
    pub address: String,
}

impl Relayer {
    pub fn new(address: String) -> Self {
        Self { address }
    }

    /// Returns true when `signature_hex` over `transfer_id` recovers to this
    /// relayer's address.
    pub fn owns_signature(&self, transfer_id: &str, signature_hex: &str) -> bool {
        match recover_signer(transfer_id.as_bytes(), signature_hex) {
            Ok(recovered) => recovered.eq_ignore_ascii_case(&self.address),
            Err(_) => false,
        }
    }
}

/// The configured relayer set. The bridge engine only accepts
/// `complete_transfer` calls whose signature recovers to a member of this set.
#[derive(Debug, Clone)]
pub struct RelayerSet {
    pub relayers: Vec<Relayer>,
}

impl RelayerSet {
    pub fn new(addresses: Vec<String>) -> Self {
        let relayers = addresses.into_iter().map(Relayer::new).collect();
        Self { relayers }
    }

    pub fn is_authorized(&self, transfer_id: &str, signature_hex: &str) -> bool {
        self.relayers
            .iter()
            .any(|r| r.owns_signature(transfer_id, signature_hex))
    }

    pub fn authorized_address(&self, transfer_id: &str, signature_hex: &str) -> Option<String> {
        self.relayers
            .iter()
            .find(|r| r.owns_signature(transfer_id, signature_hex))
            .map(|r| r.address.clone())
    }
}
