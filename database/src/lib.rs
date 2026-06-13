//! Complete Database Schema for TigerScan
//! 
//! This module provides the full database schema for TigerScan blockchain explorer,
//! including all tables, migrations, and indexes for production use.
//! 
//! ## Tables
//! 
//! - blocks - Block data
//! - transactions - Transaction data
//! - receipts - Transaction receipts
//! - logs - Event logs
//! - traces - Internal transaction traces
//! - state_diffs - State changes
//! - contracts - Contract information
//! - verified_sources - Verified contract sources
//! - tokens - Token information
//! - token_transfers - Token transfers
//! - token_holders - Token holders
//! - nft_collections - NFT collections
//! - nft_tokens - NFT tokens
//! - nft_transfers - NFT transfers
//! - addresses - Address information
//! - blocks_rewards - Validator rewards
//! - uncles - Uncle blocks
//! - analytics - Analytics data
//! - api_keys - API keys
//! - webhooks - Webhook configurations
//! - alerts - Security alerts
//! 
//! ## Usage
//! 
//! ```ignore
//! let pool = Database::new("postgres://...").await?;
//! pool.migrate().await?;
//! ```

pub mod schema;
pub mod models;
pub mod migrations;
pub mod queries;

pub use schema::*;
pub use models::*;
pub use migrations::*;
pub use queries::*;