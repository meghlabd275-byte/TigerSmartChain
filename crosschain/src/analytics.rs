//! Cross-Chain Analytics

use super::types::*;

// =============================================================================
// CROSS-CHAIN ANALYTICS
// =============================================================================

/// Analyzer
pub struct Analyzer {
    transfers: Vec<Transfer>,
}

impl Analyzer {
    pub fn new() -> Self {
        Self {
            transfers: vec![],
        }
    }

    /// Add transfer
    pub fn add_transfer(&mut self, transfer: Transfer) {
        self.transfers.push(transfer);
    }

    /// Get daily volume
    pub fn daily_volume(&self, chain: &str) -> u64 {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();
        let day_ago = now - 86400;
        
        self.transfers
            .iter()
            .filter(|t| t.source_chain == chain && t.timestamp > day_ago)
            .map(|t| t.amount)
            .sum()
    }

    /// Get total volume
    pub fn total_volume(&self, chain: &str) -> u64 {
        self.transfers
            .iter()
            .filter(|t| t.source_chain == chain)
            .map(|t| t.amount)
            .sum()
    }
}

impl Default for Analyzer {
    fn default() -> Self {
        Self::new()
    }
}