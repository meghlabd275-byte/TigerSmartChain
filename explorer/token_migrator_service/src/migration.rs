//! Migration, burn and airdrop detection over decoded transfers.

use std::collections::{BTreeMap, HashMap, HashSet};

use crate::errors::{Error, Result};
use crate::types::{Airdrop, AirdropRecipient, MigrationType, TokenBurn, TokenMigration, TransferLog};

/// Zero address: transfers to it are burns, transfers from it are mints.
pub const ZERO_ADDRESS: &str = "0x0000000000000000000000000000000000000000";

/// Classifies a set of transfers into migrations, burns and airdrops.
pub struct MigrationService {
    from_block: u64,
    max_block_range: u64,
}

impl MigrationService {
    pub fn new(from_block: u64, max_block_range: u64) -> Self {
        Self {
            from_block,
            max_block_range,
        }
    }

    pub fn from_block(&self) -> u64 {
        self.from_block
    }

    pub fn max_block_range(&self) -> u64 {
        self.max_block_range
    }

    /// Migrations for `token`.
    ///
    /// A migration is a transaction that both removes tokens from the old
    /// contract and adds tokens of a different contract. The old and new tokens
    /// are read from the transfers themselves, so the result is derived from
    /// chain data rather than from a configured pair.
    pub fn migrations(&self, token: &str, transfers: &[TransferLog]) -> Vec<TokenMigration> {
        let token = normalise(token);

        let mut by_tx: BTreeMap<&str, Vec<&TransferLog>> = BTreeMap::new();
        for transfer in transfers {
            by_tx.entry(transfer.tx_hash.as_str()).or_default().push(transfer);
        }

        let mut migrations = Vec::new();

        for (tx_hash, group) in by_tx {
            let outgoing: Vec<&&TransferLog> = group
                .iter()
                .filter(|transfer| normalise(&transfer.token) == token)
                .collect();

            if outgoing.is_empty() {
                continue;
            }

            let incoming: Vec<&&TransferLog> = group
                .iter()
                .filter(|transfer| normalise(&transfer.token) != token)
                .collect();

            if incoming.is_empty() {
                continue;
            }

            let old_amount = sum_values(outgoing.iter().map(|transfer| transfer.value.as_str()));

            let new_amount = sum_values(incoming.iter().map(|transfer| transfer.value.as_str()));

            let new_token = incoming[0].token.clone();

            let burned_old = outgoing
                .iter()
                .all(|transfer| normalise(&transfer.to) == ZERO_ADDRESS);

            let minted_new = incoming
                .iter()
                .any(|transfer| normalise(&transfer.from) == ZERO_ADDRESS);

            let migration_type = if burned_old && minted_new {
                MigrationType::Migrate
            } else if burned_old {
                MigrationType::Upgrade
            } else if incoming
                .iter()
                .any(|transfer| transfer.to.starts_with("0x000000000000000000000000"))
            {
                MigrationType::Bridge
            } else {
                MigrationType::Swap
            };

            migrations.push(TokenMigration {
                old_token: token.clone(),
                new_token,
                migration_type,
                swap_rate: swap_rate(&old_amount, &new_amount),
                amount_migrated: old_amount,
                tx_hash: tx_hash.to_string(),
                block_number: group
                    .iter()
                    .map(|transfer| transfer.block_number)
                    .min()
                    .unwrap_or(0),
            });
        }

        migrations
    }

    /// Burns of `token`: transfers whose destination is the zero address.
    pub fn burns(&self, token: &str, transfers: &[TransferLog]) -> Vec<TokenBurn> {
        let token = normalise(token);

        let mut total = num_bigint::BigUint::from(0u8);
        let mut burners: HashSet<String> = HashSet::new();
        let mut latest_block = 0u64;
        let mut found = false;

        for transfer in transfers {
            if normalise(&transfer.token) != token || normalise(&transfer.to) != ZERO_ADDRESS {
                continue;
            }

            total += parse_amount(&transfer.value).unwrap_or_else(|| num_bigint::BigUint::from(0u8));
            burners.insert(normalise(&transfer.from));
            latest_block = latest_block.max(transfer.block_number);
            found = true;
        }

        if !found {
            return Vec::new();
        }

        vec![TokenBurn {
            token,
            amount: total.to_string(),
            total_burned: total.to_string(),
            burners_count: burners.len() as u64,
            block_number: latest_block,
        }]
    }

    /// Airdrops of `token`: transactions that mint to many distinct holders.
    ///
    /// `min_recipients` guards against reading an ordinary mint as an airdrop.
    pub fn airdrops(
        &self,
        token: &str,
        transfers: &[TransferLog],
        min_recipients: usize,
    ) -> Vec<Airdrop> {
        let token = normalise(token);

        let mut by_tx: BTreeMap<&str, Vec<&TransferLog>> = BTreeMap::new();
        for transfer in transfers {
            if normalise(&transfer.token) == token && normalise(&transfer.from) == ZERO_ADDRESS {
                by_tx.entry(transfer.tx_hash.as_str()).or_default().push(transfer);
            }
        }

        let mut airdrops = Vec::new();

        for group in by_tx.values() {
            let mut seen: HashSet<String> = HashSet::new();
            let mut recipients = Vec::new();
            let mut total = num_bigint::BigUint::from(0u8);

            for transfer in group {
                let address = normalise(&transfer.to);

                if !seen.insert(address.clone()) {
                    continue;
                }

                let amount = parse_amount(&transfer.value)
                    .unwrap_or_else(|| num_bigint::BigUint::from(0u8));
                total += &amount;

                recipients.push(AirdropRecipient {
                    address,
                    amount: amount.to_string(),
                    claimed: true,
                });
            }

            if recipients.len() < min_recipients.max(1) {
                continue;
            }

            airdrops.push(Airdrop {
                token: token.clone(),
                recipients,
                amount: total.to_string(),
                block_number: group
                    .iter()
                    .map(|transfer| transfer.block_number)
                    .min()
                    .unwrap_or(0),
            });
        }

        airdrops.sort_by(|a, b| a.block_number.cmp(&b.block_number));
        airdrops
    }
}

/// Lower-case a hex string so addresses and hashes compare consistently.
fn normalise(value: &str) -> String {
    value.trim().to_ascii_lowercase()
}

fn parse_amount(value: &str) -> Option<num_bigint::BigUint> {
    value.parse().ok()
}

fn sum_values<'a>(values: impl Iterator<Item = &'a str>) -> String {
    let total: num_bigint::BigUint = values
        .filter_map(parse_amount)
        .fold(num_bigint::BigUint::from(0u8), |acc, value| acc + value);

    total.to_string()
}

/// New tokens per old token, as a decimal string. Returns `"0"` when the old
/// side is empty, because a rate against nothing is not defined.
fn swap_rate(old_amount: &str, new_amount: &str) -> String {
    let old = match parse_amount(old_amount) {
        Some(value) if value != num_bigint::BigUint::from(0u8) => value,
        _ => return "0".to_string(),
    };

    let new = parse_amount(new_amount).unwrap_or_else(|| num_bigint::BigUint::from(0u8));

    // Six decimal places, without floating point.
    let scale = num_bigint::BigUint::from(1_000_000u32);
    let scaled = (new * &scale) / &old;

    let whole = &scaled / &scale;
    let fraction = &scaled % &scale;

    format!("{whole}.{fraction:06}")
}

/// Group transfers by the address that sent them, for holder analytics.
pub fn group_by_sender(transfers: &[TransferLog]) -> HashMap<String, Vec<&TransferLog>> {
    let mut grouped: HashMap<String, Vec<&TransferLog>> = HashMap::new();
    for transfer in transfers {
        grouped
            .entry(normalise(&transfer.from))
            .or_default()
            .push(transfer);
    }
    grouped
}

/// Reject a request whose block range is wider than the service allows.
pub fn check_range(from: u64, to: u64, limit: u64) -> Result<()> {
    if to < from {
        return Err(Error::Invalid(format!(
            "to_block {to} is before from_block {from}"
        )));
    }

    if to - from > limit {
        return Err(Error::Invalid(format!(
            "block range {} exceeds the {limit} block limit",
            to - from
        )));
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    const OLD: &str = "0x1111111111111111111111111111111111111111";
    const NEW: &str = "0x2222222222222222222222222222222222222222";
    const ALICE: &str = "0xaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
    const BOB: &str = "0xbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";

    fn transfer(token: &str, from: &str, to: &str, value: &str, tx: &str) -> TransferLog {
        TransferLog {
            token: token.to_string(),
            from: from.to_string(),
            to: to.to_string(),
            value: value.to_string(),
            block_number: 100,
            tx_hash: tx.to_string(),
            log_index: 0,
        }
    }

    #[test]
    fn a_burn_and_a_mint_in_one_transaction_is_a_migration() {
        let service = MigrationService::new(0, 10_000);

        let transfers = vec![
            transfer(OLD, ALICE, ZERO_ADDRESS, "1000", "0xtx1"),
            transfer(NEW, ZERO_ADDRESS, ALICE, "1000", "0xtx1"),
        ];

        let migrations = service.migrations(OLD, &transfers);

        assert_eq!(migrations.len(), 1);
        assert_eq!(migrations[0].migration_type, MigrationType::Migrate);
        assert_eq!(migrations[0].new_token, NEW);
        assert_eq!(migrations[0].swap_rate, "1.000000");
        assert_eq!(migrations[0].amount_migrated, "1000");
    }

    #[test]
    fn a_partial_rate_is_reported_with_six_decimals() {
        let service = MigrationService::new(0, 10_000);

        let transfers = vec![
            transfer(OLD, ALICE, BOB, "2000", "0xtx2"),
            transfer(NEW, BOB, ALICE, "3000", "0xtx2"),
        ];

        let migrations = service.migrations(OLD, &transfers);

        assert_eq!(migrations[0].swap_rate, "1.500000");
    }

    #[test]
    fn a_transaction_that_only_moves_the_old_token_is_not_a_migration() {
        let service = MigrationService::new(0, 10_000);

        let transfers = vec![transfer(OLD, ALICE, BOB, "1000", "0xtx3")];

        assert!(service.migrations(OLD, &transfers).is_empty());
    }

    #[test]
    fn a_burn_is_a_transfer_to_the_zero_address() {
        let service = MigrationService::new(0, 10_000);

        let transfers = vec![
            transfer(OLD, ALICE, ZERO_ADDRESS, "400", "0xtx4"),
            transfer(OLD, BOB, ZERO_ADDRESS, "600", "0xtx5"),
            transfer(OLD, ALICE, BOB, "999", "0xtx6"),
        ];

        let burns = service.burns(OLD, &transfers);

        assert_eq!(burns.len(), 1);
        assert_eq!(burns[0].total_burned, "1000");
        assert_eq!(burns[0].burners_count, 2);
    }

    #[test]
    fn an_airdrop_needs_the_minimum_number_of_recipients() {
        let service = MigrationService::new(0, 10_000);

        let transfers = vec![
            transfer(NEW, ZERO_ADDRESS, ALICE, "100", "0xmint"),
            transfer(NEW, ZERO_ADDRESS, BOB, "200", "0xmint"),
        ];

        assert!(service.airdrops(NEW, &transfers, 3).is_empty());
        assert_eq!(service.airdrops(NEW, &transfers, 2).len(), 1);
        assert_eq!(service.airdrops(NEW, &transfers, 2)[0].amount, "300");
    }

    #[test]
    fn a_repeated_recipient_is_counted_once() {
        let service = MigrationService::new(0, 10_000);

        let transfers = vec![
            transfer(NEW, ZERO_ADDRESS, ALICE, "100", "0xmint"),
            transfer(NEW, ZERO_ADDRESS, ALICE, "100", "0xmint"),
        ];

        let airdrops = service.airdrops(NEW, &transfers, 2);
        assert!(airdrops.is_empty(), "one distinct holder is not an airdrop");
    }

    #[test]
    fn a_range_wider_than_the_limit_is_rejected() {
        assert!(check_range(0, 100, 10).is_err());
        assert!(check_range(0, 10, 10).is_ok());
        assert!(check_range(5, 1, 10).is_err());
    }
}
