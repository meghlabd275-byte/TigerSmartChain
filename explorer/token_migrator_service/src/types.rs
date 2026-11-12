//! Migrator types.

use serde::{Deserialize, Serialize};

/// A single ERC-20 `Transfer` event, already decoded.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TransferLog {
    pub token: String,
    pub from: String,
    pub to: String,
    /// Raw base-unit amount, as a decimal string.
    pub value: String,
    pub block_number: u64,
    pub tx_hash: String,
    pub log_index: u64,
}

/// A token migration observed on chain.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenMigration {
    pub old_token: String,
    pub new_token: String,
    pub migration_type: MigrationType,
    /// New tokens emitted per whole old token, as a decimal string.
    pub swap_rate: String,
    /// Total old tokens consumed, in base units.
    pub amount_migrated: String,
    pub tx_hash: String,
    pub block_number: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MigrationType {
    /// Old tokens left a holder and new tokens arrived in the same transaction.
    Swap,
    /// A bridge contract was an intermediary in the transfer.
    Bridge,
    /// Old tokens were burned and new tokens minted in the same transaction.
    Migrate,
    /// New tokens were minted to every holder without burning the old ones.
    Upgrade,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Airdrop {
    pub token: String,
    pub recipients: Vec<AirdropRecipient>,
    /// Total airdropped, in base units.
    pub amount: String,
    pub block_number: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AirdropRecipient {
    pub address: String,
    pub amount: String,
    pub claimed: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenBurn {
    pub token: String,
    pub amount: String,
    pub total_burned: String,
    pub burners_count: u64,
    pub block_number: u64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Config {
    pub rpc_url: String,
    /// First block to scan when the caller does not name one.
    pub from_block: u64,
    /// Blocks requested per `eth_getLogs` call.
    pub log_chunk_size: u64,
    /// Most recent block to scan when the caller does not name one.
    pub max_block_range: u64,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            rpc_url: std::env::var("RPC_HTTP_URL")
                .unwrap_or_else(|_| "http://127.0.0.1:8545".to_string()),
            from_block: std::env::var("MIGRATOR_FROM_BLOCK")
                .ok()
                .and_then(|value| value.parse().ok())
                .unwrap_or(0),
            log_chunk_size: 2_000,
            max_block_range: 50_000,
        }
    }
}
