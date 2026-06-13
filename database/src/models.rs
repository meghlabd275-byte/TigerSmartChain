//! Database Models for TigerScan
//! 
//! Rust structs representing database entities with sqlx derives.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

// =============================================================================
// BLOCK MODEL
// =============================================================================

/// Block model
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Block {
    pub number: i64,
    pub hash: String,
    pub parent_hash: String,
    pub nonce: Option<String>,
    pub sha3_uncles: Option<String>,
    pub logs_bloom: Option<String>,
    pub transactions_root: Option<String>,
    pub state_root: Option<String>,
    pub receipts_root: Option<String>,
    pub miner: Option<String>,
    pub difficulty: Option<String>,
    pub total_difficulty: Option<String>,
    pub size: Option<i64>,
    pub gas_limit: i64,
    pub gas_used: i64,
    pub timestamp: i64,
    pub extra_data: Option<String>,
    pub mix_hash: Option<String>,
    pub base_fee_per_gas: Option<i64>,
    pub created_at: Option<DateTime<Utc>>,
    pub updated_at: Option<DateTime<Utc>>,
}

// =============================================================================
// TRANSACTION MODEL
// =============================================================================

/// Transaction model
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Transaction {
    pub hash: String,
    pub block_number: Option<i64>,
    pub block_hash: Option<String>,
    pub transaction_index: i64,
    pub from_address: String,
    pub to_address: Option<String>,
    pub value: String,
    pub gas_price: Option<i64>,
    pub gas: Option<i64>,
    pub input: Option<String>,
    pub nonce: i64,
    pub tx_type: Option<i16>,
    pub status: Option<String>,
    pub created_at: Option<DateTime<Utc>>,
}

// =============================================================================
// RECEIPT MODEL
// =============================================================================

/// Receipt model
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Receipt {
    pub transaction_hash: String,
    pub block_number: i64,
    pub block_hash: String,
    pub contract_address: Option<String>,
    pub cumulative_gas_used: i64,
    pub gas_used: i64,
    pub logs_bloom: String,
    pub status: Option<String>,
    pub logs: Option<String>,
    pub created_at: Option<DateTime<Utc>>,
}

// =============================================================================
// LOG MODEL
// =============================================================================

/// Log model
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Log {
    pub id: Option<i64>,
    pub address: String,
    pub topics: String,
    pub data: String,
    pub block_number: i64,
    pub transaction_hash: String,
    pub log_index: i64,
    pub created_at: Option<DateTime<Utc>>,
}

// =============================================================================
// TRACE MODEL
// =============================================================================

/// Trace model
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Trace {
    pub id: Option<i64>,
    pub transaction_hash: String,
    pub block_number: i64,
    pub subtrace_index: i32,
    pub call_type: String,
    pub from_address: String,
    pub to_address: String,
    pub value: Option<String>,
    pub gas: Option<i64>,
    pub gas_used: Option<i64>,
    pub input: Option<String>,
    pub output: Option<String>,
    pub error: Option<String>,
    pub depth: i32,
    pub parent_index: Option<i32>,
    pub trace_type: String,
    pub created_at: Option<DateTime<Utc>>,
}

// =============================================================================
// CONTRACT MODEL
// =============================================================================

/// Contract model
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Contract {
    pub address: String,
    pub bytecode: Option<String>,
    pub bytecode_hash: Option<String>,
    pub is_verified: bool,
    pub is_verified_24h: bool,
    pub verification_date: Option<DateTime<Utc>>,
    pub is_contract: bool,
    pub contract_type: Option<String>,
    pub created_at: Option<DateTime<Utc>>,
    pub updated_at: Option<DateTime<Utc>>,
}

// =============================================================================
// VERIFIED SOURCE MODEL
// =============================================================================

/// Verified source model
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VerifiedSource {
    pub id: Option<i64>,
    pub contract_address: String,
    pub source_code: String,
    pub file_name: String,
    pub compiler_version: String,
    pub evm_version: Option<String>,
    pub license: Option<String>,
    pub optimization_enabled: Option<bool>,
    pub optimization_runs: Option<i32>,
    pub constructor_args: Option<String>,
    pub libraries: Option<String>,
    pub is_proxy: bool,
    pub proxy_master_copy: Option<String>,
    pub is_upgradeable: bool,
    pub admin_address: Option<String>,
    pub implementation_address: Option<String>,
    pub verified_at: Option<DateTime<Utc>>,
    pub updated_at: Option<DateTime<Utc>>,
}

// =============================================================================
// TOKEN MODEL
// =============================================================================

/// Token model
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Token {
    pub address: String,
    pub name: String,
    pub symbol: String,
    pub decimals: i16,
    pub total_supply: Option<String>,
    pub token_type: String,
    pub price: Option<f64>,
    pub price_24h_ago: Option<f64>,
    pub market_cap: Option<f64>,
    pub volume_24h: Option<f64>,
    pub holders_count: Option<i64>,
    pub transfers_count: Option<i64>,
    pub is_verified: bool,
    pub is_spam: bool,
    pub price_source: Option<String>,
    pub last_updated: Option<DateTime<Utc>>,
    pub created_at: Option<DateTime<Utc>>,
    pub updated_at: Option<DateTime<Utc>>,
}

// =============================================================================
// TOKEN TRANSFER MODEL
// =============================================================================

/// Token transfer model
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenTransfer {
    pub id: Option<i64>,
    pub token_address: String,
    pub from_address: String,
    pub to_address: String,
    pub value: String,
    pub transaction_hash: String,
    pub block_number: i64,
    pub log_index: i32,
    pub created_at: Option<DateTime<Utc>>,
}

// =============================================================================
// TOKEN HOLDER MODEL
// =============================================================================

/// Token holder model
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenHolder {
    pub id: Option<i64>,
    pub token_address: String,
    pub address: String,
    pub balance: String,
    pub block_number: i64,
    pub updated_at: Option<DateTime<Utc>>,
}

// =============================================================================
// NFT COLLECTION MODEL
// =============================================================================

/// NFT collection model
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NFTCollection {
    pub address: String,
    pub name: String,
    pub symbol: Option<String>,
    pub contract_type: String,
    pub total_supply: Option<i64>,
    pub minted_count: Option<i64>,
    pub owner_count: Option<i64>,
    pub floor_price: Option<String>,
    pub average_price: Option<String>,
    pub volume_24h: Option<String>,
    pub volume_7d: Option<String>,
    pub volume_30d: Option<String>,
    pub image_url: Option<String>,
    pub banner_url: Option<String>,
    pub description: Option<String>,
    pub external_url: Option<String>,
    pub twitter: Option<String>,
    pub discord: Option<String>,
    pub is_verified: bool,
    pub is_spam: bool,
    pub last_updated: Option<DateTime<Utc>>,
    pub created_at: Option<DateTime<Utc>>,
    pub updated_at: Option<DateTime<Utc>>,
}

// =============================================================================
// NFT TOKEN MODEL
// =============================================================================

/// NFT token model
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NFTToken {
    pub id: Option<i64>,
    pub collection_address: String,
    pub token_id: String,
    pub owner: Option<String>,
    pub uri: Option<String>,
    pub metadata: Option<String>,
    pub image_url: Option<String>,
    pub animation_url: Option<String>,
    pub external_url: Option<String>,
    pub metadata_fetched_at: Option<DateTime<Utc>>,
    pub created_at: Option<DateTime<Utc>>,
    pub updated_at: Option<DateTime<Utc>>,
}

// =============================================================================
// NFT TRANSFER MODEL
// =============================================================================

/// NFT transfer model
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NFTTransfer {
    pub id: Option<i64>,
    pub collection_address: String,
    pub token_id: String,
    pub from_address: String,
    pub to_address: String,
    pub transaction_hash: String,
    pub block_number: i64,
    pub log_index: i32,
    pub created_at: Option<DateTime<Utc>>,
}

// =============================================================================
// ADDRESS MODEL
// =============================================================================

/// Address model
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Address {
    pub address: String,
    pub is_contract: bool,
    pub is_multisig: bool,
    pub multisig_owners: Option<String>,
    pub name: Option<String>,
    pub ens_name: Option<String>,
    pub is_scammer: bool,
    pub is_verified: bool,
    pub first_seen_block: Option<i64>,
    pub last_seen_block: Option<i64>,
    pub total_transactions: Option<i64>,
    pub total_received: Option<String>,
    pub total_sent: Option<String>,
    pub created_at: Option<DateTime<Utc>>,
    pub updated_at: Option<DateTime<Utc>>,
}

// =============================================================================
// BLOCK REWARD MODEL
// =============================================================================

/// Block reward model
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BlockReward {
    pub id: Option<i64>,
    pub block_number: i64,
    pub miner: String,
    pub reward: String,
    pub uncle_rewards: Option<String>,
    pub gas_fees: Option<String>,
    pub created_at: Option<DateTime<Utc>>,
}

// =============================================================================
// UNCLE MODEL
// =============================================================================

/// Uncle block model
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Uncle {
    pub hash: String,
    pub block_number: i64,
    pub miner: String,
    pub parent_hash: String,
    pub difficulty: String,
    pub gas_limit: Option<i64>,
    pub gas_used: Option<i64>,
    pub timestamp: i64,
    pub created_at: Option<DateTime<Utc>>,
}

// =============================================================================
// CONTRACT CREATION MODEL
// =============================================================================

/// Contract creation model
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContractCreation {
    pub id: Option<i64>,
    pub address: String,
    pub transaction_hash: String,
    pub creator: String,
    pub block_number: i64,
    pub init_code: Option<String>,
    pub created_at: Option<DateTime<Utc>>,
}

// =============================================================================
// TOKEN APPROVAL MODEL
// =============================================================================

/// Token approval model
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenApproval {
    pub id: Option<i64>,
    pub token_address: String,
    pub owner: String,
    pub spender: String,
    pub value: Option<String>,
    pub transaction_hash: String,
    pub block_number: i64,
    pub is_current: bool,
    pub created_at: Option<DateTime<Utc>>,
    pub updated_at: Option<DateTime<Utc>>,
}

// =============================================================================
// ANALYTICS MODEL
// =============================================================================

/// Analytics metric model
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Analytics {
    pub id: Option<i64>,
    pub metric_name: String,
    pub metric_value: String,
    pub block_number: Option<i64>,
    pub timestamp: i64,
    pub labels: Option<String>,
    pub created_at: Option<DateTime<Utc>>,
}

// =============================================================================
// API KEY MODEL
// =============================================================================

/// API key model
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct APIKey {
    pub id: Uuid,
    pub key_hash: String,
    pub name: String,
    pub user_id: Option<String>,
    pub rate_limit: Option<i32>,
    pub monthly_limit: Option<i64>,
    pub requests_used: Option<i64>,
    pub is_active: bool,
    pub expires_at: Option<DateTime<Utc>>,
    pub created_at: Option<DateTime<Utc>>,
    pub updated_at: Option<DateTime<Utc>>,
}

// =============================================================================
// ALERT MODEL
// =============================================================================

/// Security alert model
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Alert {
    pub id: Uuid,
    pub alert_type: String,
    pub severity: String,
    pub title: String,
    pub description: Option<String>,
    pub address: Option<String>,
    pub transaction_hash: Option<String>,
    pub payload: Option<String>,
    pub acknowledged: bool,
    pub acknowledged_at: Option<DateTime<Utc>>,
    pub created_at: Option<DateTime<Utc>>,
}

// =============================================================================
// DEX PAIR MODEL
// =============================================================================

/// DEX pair model
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DexPair {
    pub id: Option<i64>,
    pub pair_address: String,
    pub token0_address: String,
    pub token1_address: String,
    pub reserve0: Option<String>,
    pub reserve1: Option<String>,
    pub total_supply: Option<String>,
    pub volume_24h: Option<String>,
    pub volume_7d: Option<String>,
    pub liquidity_usd: Option<String>,
    pub dex_name: String,
    pub created_at: Option<DateTime<Utc>>,
    pub updated_at: Option<DateTime<Utc>>,
}

// =============================================================================
// PENDING TRANSACTION MODEL
// =============================================================================

/// Pending transaction (mempool) model
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PendingTransaction {
    pub hash: String,
    pub from_address: String,
    pub to_address: Option<String>,
    pub value: String,
    pub gas_price: Option<i64>,
    pub gas: Option<i64>,
    pub nonce: i64,
    pub input: Option<String>,
    pub received_at: Option<DateTime<Utc>>,
    pub expires_at: Option<DateTime<Utc>>,
}