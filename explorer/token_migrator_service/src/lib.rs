//! Token migrator and airdrop finder.
//!
//! Everything here is derived from chain data. The service indexes ERC-20
//! `Transfer` logs, groups them into migrations, burns and airdrops, and
//! answers questions about them. There is no in-memory fixture data: if the RPC
//! endpoint cannot answer, the call fails rather than inventing a result.

#![forbid(unsafe_code)]

pub mod airdrop;
pub mod chain;
pub mod errors;
pub mod migration;
pub mod types;

pub use airdrop::AirdropDetector;
pub use chain::{ChainClient, TRANSFER_TOPIC};
pub use errors::{Error, Result};
pub use migration::{check_range, group_by_sender, MigrationService, ZERO_ADDRESS};
pub use types::{
    Airdrop, AirdropRecipient, Config, MigrationType, TokenBurn, TokenMigration, TransferLog,
};
