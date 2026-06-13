//! Airdrop Detection

use crate::types::{Airdrop, AirdropRecipient};

pub struct AirdropDetector;

impl AirdropDetector {
    /// Detect airdrop from logs
    pub fn detect(token: &str, transfers: Vec<(String, String)>) -> Option<Airdrop> {
        if transfers.is_empty() {
            return None;
        }
        
        let recipients: Vec<AirdropRecipient> = transfers
            .into_iter()
            .map(|(addr, amount)| AirdropRecipient {
                address: addr,
                amount,
                claimed: false,
            })
            .collect();
        
        Some(Airdrop {
            token: token.to_string(),
            recipients,
            amount: "0".to_string(),
            claim_deadline: 0,
        })
    }
    
    /// Check if address claimed
    pub fn has_claimed(recipient: &AirdropRecipient) -> bool {
        recipient.claimed
    }
}