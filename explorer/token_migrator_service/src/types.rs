//! Migrator Types

use serde::{Deserialize, Serialize};

/// Token Migration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenMigration {
    pub old_token: String,
    pub new_token: String,
    pub migration_type: MigrationType,
    pub swap_rate: String,
    pub deadline: i64,
    pub tx_hash: String,
}

/// Migration Type
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MigrationType {
    Swap,
    Bridge,
    Migrate,
    Upgrade,
}

/// Airdrop
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Airdrop {
    pub token: String,
    pub recipients: Vec<AirdropRecipient>,
    pub amount: String,
    pub claim_deadline: i64,
}

/// Airdrop Recipient
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AirdropRecipient {
    pub address: String,
    pub amount: String,
    pub claimed: bool,
}

/// Token Burn
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenBurn {
    pub token: String,
    pub amount: String,
    pub total_burned: String,
    pub burners_count: u64,
}

/// Config
#[derive(Debug, Clone, Deserialize)]
pub struct Config {
    pub rpc_url: String,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            rpc_url: "http://localhost:8545".to_string(),
        }
    }
}