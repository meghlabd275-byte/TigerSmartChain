//! Token Migrator and Airdrop Finder Service

use std::collections::HashMap;
use std::sync::Arc;
use chrono::{DateTime, Utc};
use ethers::providers::Http;
use ethers::types::{Address, Filter};
use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Error, Debug)]
pub enum MigratorError { #[error("RPC error: {0}")] Rpc(String), #[error("Not found: {0}")] NotFound(String) }

#[derive(Debug, Clone, Deserialize)]
pub struct Config { pub rpc_url: String }
impl Default for Config { fn default() -> Self { Self { rpc_url: std::env::var("RPC_URL").unwrap_or_else(|_| "http://localhost:8545".to_string()) } } }

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenMigration { pub old_token: String, pub new_token: String, pub migration_type: MigrationType, pub swap_rate: String, pub deadline: i64, pub tx_hash: String }

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MigrationType { Swap, Bridge, Migrate, Upgrade }

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Airdrop { pub token: String, pub recipients: Vec<AirdropRecipient>, pub amount: String, pub claim_deadline: i64 }

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AirdropRecipient { pub address: String, pub amount: String, pub claimed: bool }

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenBurn { pub token: String, pub amount: String, pub total_burned: String, pub burners_count: u64 }

pub struct MigratorService { rpc: ethers::providers::Provider<Http>, state: Arc<RwLock<MigratorState>> }
#[derive(Debug)]
pub struct MigratorState { pub migrations: HashMap<String, Vec<TokenMigration>>, pub airdrops: HashMap<String, Airdrop>, pub burns: HashMap<String, TokenBurn> }

impl MigratorService {
    pub async fn new(config: Config) -> Result<Self, anyhow::Error> {
        let rpc = ethers::providers::Provider::<Http>::try_from(config.rpc_url)?;
        Ok(Self { rpc, state: Arc::new(RwLock::new(MigratorState { migrations: HashMap::new(), airdrops: HashMap::new(), burns: HashMap::new() })) })
    }
    pub fn add_migration(&self, migration: TokenMigration) { self.state.write().migrations.entry(migration.old_token.clone()).or_insert_with(Vec::new).push(migration); }
    pub fn get_migrations(&self, token: &str) -> Vec<TokenMigration> { self.state.read().migrations.get(token).cloned().unwrap_or_default() }
    pub fn add_airdrop(&self, airdrop: Airdrop) { self.state.write().airdrops.insert(airdrop.token.clone(), airdrop); }
    pub fn get_airdrop(&self, token: &str) -> Option<Airdrop> { self.state.read().airdrops.get(token).cloned() }
    pub fn add_burn(&self, burn: TokenBurn) { self.state.write().burns.insert(burn.token.clone(), burn); }
    pub fn get_burns(&self, token: &str) -> Option<TokenBurn> { self.state.read().burns.get(token).cloned() }
}