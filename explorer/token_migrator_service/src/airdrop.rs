//! Airdrop detection over decoded transfers.

use std::collections::HashSet;

use crate::migration::ZERO_ADDRESS;
use crate::types::{Airdrop, AirdropRecipient, TransferLog};

/// Groups mint transfers into airdrops.
pub struct AirdropDetector;

impl AirdropDetector {
    /// Build an airdrop from the recipients of `transfers`.
    ///
    /// Only transfers that mint (`from` is the zero address) are considered.
    /// A recipient that appears more than once is counted once, at its first
    /// amount, so a replayed log cannot inflate the recipient count.
    pub fn detect(token: &str, transfers: &[TransferLog]) -> Option<Airdrop> {
        let token = token.trim().to_ascii_lowercase();

        let mut seen: HashSet<String> = HashSet::new();
        let mut recipients = Vec::new();
        let mut total = num_bigint::BigUint::from(0u8);
        let mut block_number = u64::MAX;

        for transfer in transfers {
            if transfer.token.trim().to_ascii_lowercase() != token {
                continue;
            }

            if transfer.from.trim().to_ascii_lowercase() != ZERO_ADDRESS {
                continue;
            }

            let address = transfer.to.trim().to_ascii_lowercase();

            if !seen.insert(address.clone()) {
                continue;
            }

            let amount = transfer
                .value
                .parse::<num_bigint::BigUint>()
                .unwrap_or_else(|_| num_bigint::BigUint::from(0u8));

            total += &amount;
            block_number = block_number.min(transfer.block_number);

            recipients.push(AirdropRecipient {
                address,
                amount: amount.to_string(),
                claimed: true,
            });
        }

        if recipients.is_empty() {
            return None;
        }

        Some(Airdrop {
            token,
            recipients,
            amount: total.to_string(),
            block_number: if block_number == u64::MAX {
                0
            } else {
                block_number
            },
        })
    }

    /// Whether the recipient already received the airdrop.
    ///
    /// `claimed` is set from the chain when the airdrop is built, so this is a
    /// read of observed state, not a guess.
    pub fn has_claimed(recipient: &AirdropRecipient) -> bool {
        recipient.claimed
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const TOKEN: &str = "0x2222222222222222222222222222222222222222";
    const ALICE: &str = "0xaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
    const BOB: &str = "0xbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";

    fn mint(to: &str, value: &str) -> TransferLog {
        TransferLog {
            token: TOKEN.to_string(),
            from: ZERO_ADDRESS.to_string(),
            to: to.to_string(),
            value: value.to_string(),
            block_number: 42,
            tx_hash: "0xtx".to_string(),
            log_index: 0,
        }
    }

    #[test]
    fn mints_become_recipients() {
        let airdrop = AirdropDetector::detect(TOKEN, &[mint(ALICE, "100"), mint(BOB, "250")]).unwrap();

        assert_eq!(airdrop.recipients.len(), 2);
        assert_eq!(airdrop.amount, "350");
        assert_eq!(airdrop.block_number, 42);
    }

    #[test]
    fn a_transfer_from_a_holder_is_not_an_airdrop() {
        let mut transfer = mint(ALICE, "100");
        transfer.from = BOB.to_string();

        assert!(AirdropDetector::detect(TOKEN, &[transfer]).is_none());
    }

    #[test]
    fn a_repeated_recipient_is_counted_once() {
        let airdrop =
            AirdropDetector::detect(TOKEN, &[mint(ALICE, "100"), mint(ALICE, "100")]).unwrap();

        assert_eq!(airdrop.recipients.len(), 1);
        assert_eq!(airdrop.amount, "100");
    }

    #[test]
    fn a_recipient_is_reported_as_claimed_once_seen() {
        let airdrop = AirdropDetector::detect(TOKEN, &[mint(ALICE, "1")]).unwrap();

        assert!(AirdropDetector::has_claimed(&airdrop.recipients[0]));
    }

    #[test]
    fn transfers_of_another_token_are_ignored() {
        let mut transfer = mint(ALICE, "100");
        transfer.token = "0x3333333333333333333333333333333333333333".to_string();

        assert!(AirdropDetector::detect(TOKEN, &[transfer]).is_none());
    }
}
