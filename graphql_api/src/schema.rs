//! GraphQL Schema Definition for TigerScan

use async_graphql::Object;
use async_graphql::EmptySubscription;
use async_graphql::InputObject;
use async_graphql::SimpleObject;
use async_graphql::Enum;
use async_graphql::ID;

// =============================================================================
// BLOCKS
// =============================================================================

/// Block type
#[derive(SimpleObject, Clone)]
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
    pub base_fee_per_gas: Option<i64>,
    pub transactions: Option<Vec<Transaction>>,
    pub uncles: Option<Vec<Uncle>>,
}

/// Block filter
#[derive(InputObject)]
pub struct BlockFilter {
    pub from_block: Option<i64>,
    pub to_block: Option<i64>,
    pub miner: Option<String>,
    pub hash: Option<String>,
}

// =============================================================================
// TRANSACTIONS
// =============================================================================

/// Transaction type
#[derive(SimpleObject, Clone)]
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
    pub tx_type: Option<i32>,
    pub status: Option<String>,
    pub receipt: Option<Receipt>,
    pub traces: Option<Vec<Trace>>,
}

/// Transaction filter
#[derive(InputObject)]
pub struct TransactionFilter {
    pub from: Option<String>,
    pub to: Option<String>,
    pub block: Option<i64>,
    pub from_block: Option<i64>,
    pub to_block: Option<i64>,
}

// =============================================================================
// RECEIPT
// =============================================================================

/// Transaction receipt
#[derive(SimpleObject, Clone)]
pub struct Receipt {
    pub transaction_hash: String,
    pub block_number: i64,
    pub block_hash: String,
    pub contract_address: Option<String>,
    pub cumulative_gas_used: i64,
    pub gas_used: i64,
    pub logs_bloom: String,
    pub status: Option<String>,
    pub logs: Vec<Log>,
}

// =============================================================================
// LOGS
// =============================================================================

/// Log type
#[derive(SimpleObject, Clone)]
pub struct Log {
    pub id: i64,
    pub address: String,
    pub topics: Vec<String>,
    pub data: String,
    pub block_number: i64,
    pub transaction_hash: String,
    pub log_index: i64,
}

// =============================================================================
// TRACES
// =============================================================================

/// Trace type
#[derive(SimpleObject, Clone)]
pub struct Trace {
    pub id: i64,
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
    pub trace_type: String,
}

// =============================================================================
// CONTRACTS
// =============================================================================

/// Contract type
#[derive(SimpleObject, Clone)]
pub struct Contract {
    pub address: String,
    pub bytecode: Option<String>,
    pub bytecode_hash: Option<String>,
    pub is_verified: bool,
    pub is_contract: bool,
    pub contract_type: Option<String>,
    pub source_code: Option<String>,
    pub compiler_version: Option<String>,
    pub evm_version: Option<String>,
    pub license: Option<String>,
}

// =============================================================================
// TOKENS
// =============================================================================

/// Token type
#[derive(SimpleObject, Clone)]
pub struct Token {
    pub address: String,
    pub name: String,
    pub symbol: String,
    pub decimals: i32,
    pub total_supply: Option<String>,
    pub token_type: TokenType,
    pub price: Option<f64>,
    pub price_24h_ago: Option<f64>,
    pub market_cap: Option<f64>,
    pub volume_24h: Option<f64>,
    pub holders_count: Option<i64>,
    pub transfers_count: Option<i64>,
    pub is_verified: bool,
    pub is_spam: bool,
    pub holder_distribution: Option<Vec<Holder>>,
}

/// Token type enum
#[derive(Enum, Clone, Copy, Eq, PartialEq)]
pub enum TokenType {
    ERC20,
    ERC721,
    ERC1155,
    Unknown,
}

/// Holder
#[derive(SimpleObject, Clone)]
pub struct Holder {
    pub address: String,
    pub balance: String,
    pub percentage: f64,
}

// =============================================================================
// TOKEN TRANSFERS
// =============================================================================

/// Token transfer
#[derive(SimpleObject, Clone)]
pub struct TokenTransfer {
    pub id: i64,
    pub token_address: String,
    pub from_address: String,
    pub to_address: String,
    pub value: String,
    pub transaction_hash: String,
    pub block_number: i64,
    pub timestamp: i64,
}

// =============================================================================
// NFT COLLECTIONS
// =============================================================================

/// NFT Collection type
#[derive(SimpleObject, Clone)]
pub struct NFTCollection {
    pub address: String,
    pub name: String,
    pub symbol: Option<String>,
    pub contract_type: NFTContractType,
    pub total_supply: Option<i64>,
    pub minted_count: Option<i64>,
    pub owner_count: Option<i64>,
    pub floor_price: Option<String>,
    pub average_price: Option<String>,
    pub volume_24h: Option<String>,
    pub volume_7d: Option<String>,
    pub image_url: Option<String>,
    pub description: Option<String>,
    pub twitter: Option<String>,
    pub discord: Option<String>,
    pub is_verified: bool,
    pub is_spam: bool,
}

/// NFT contract type
#[derive(Enum, Clone, Copy, Eq, PartialEq)]
pub enum NFTContractType {
    ERC721,
    ERC1155,
}

// =============================================================================
// NFT TOKENS
// =============================================================================

/// NFT Token type
#[derive(SimpleObject, Clone)]
pub struct NFTToken {
    pub id: i64,
    pub collection_address: String,
    pub token_id: String,
    pub owner: Option<String>,
    pub uri: Option<String>,
    pub image_url: Option<String>,
    pub animation_url: Option<String>,
    pub metadata: Option<NFTMetadata>,
    pub attributes: Option<Vec<NFTAttribute>>,
}

/// NFT Metadata
#[derive(SimpleObject, Clone)]
pub struct NFTMetadata {
    pub name: Option<String>,
    pub description: Option<String>,
    pub image: Option<String>,
    pub external_url: Option<String>,
    pub background_color: Option<String>,
    pub attributes: Vec<NFTAttribute>,
}

/// NFT Attribute
#[derive(SimpleObject, Clone)]
pub struct NFTAttribute {
    pub trait_type: String,
    pub value: String,
    pub display_type: Option<String>,
}

// =============================================================================
// NFT TRANSFERS
// =============================================================================

/// NFT Transfer
#[derive(SimpleObject, Clone)]
pub struct NFTTransfer {
    pub id: i64,
    pub collection_address: String,
    pub token_id: String,
    pub from_address: String,
    pub to_address: String,
    pub transaction_hash: String,
    pub block_number: i64,
    pub timestamp: i64,
}

// =============================================================================
// ADDRESSES
// =============================================================================

/// Address info
#[derive(SimpleObject, Clone)]
pub struct Address {
    pub address: String,
    pub is_contract: bool,
    pub is_multisig: bool,
    pub multisig_owners: Option<Vec<String>>,
    pub name: Option<String>,
    pub ens_name: Option<String>,
    pub is_scammer: bool,
    pub balance: Option<String>,
    pub transaction_count: i64,
    pub first_seen_block: Option<i64>,
    pub last_seen_block: Option<i64>,
}

// =============================================================================
// ANALYTICS
// =============================================================================

/// Analytics metric
#[derive(SimpleObject, Clone)]
pub struct AnalyticsMetric {
    pub name: String,
    pub value: String,
    pub timestamp: i64,
    pub change_24h: Option<f64>,
}

/// Block stats
#[derive(SimpleObject, Clone)]
pub struct BlockStats {
    pub total_blocks: i64,
    pub total_transactions: i64,
    pub total_addresses: i64,
    pub total_contracts: i64,
    pub gas_price: GasPrice,
    pub tps: f64,
}

/// Gas price
#[derive(SimpleObject, Clone)]
pub struct GasPrice {
    pub slow: i64,
    pub standard: i64,
    pub fast: i64,
}

// =============================================================================
// UNCLES
// =============================================================================

/// Uncle block
#[derive(SimpleObject, Clone)]
pub struct Uncle {
    pub hash: String,
    pub block_number: i64,
    pub miner: String,
    pub parent_hash: String,
    pub difficulty: String,
    pub gas_limit: Option<i64>,
    pub timestamp: i64,
}

// =============================================================================
// TOKEN PRICES
// =============================================================================

/// Token price history point
#[derive(SimpleObject, Clone)]
pub struct PriceHistoryPoint {
    pub timestamp: i64,
    pub price: f64,
}

// =============================================================================
// API ERROR
// =============================================================================

/// API Error
#[derive(SimpleObject, Clone)]
pub struct APIError {
    pub message: String,
    pub code: i32,
}

// =============================================================================
// ROOT
// =============================================================================

/// Root query
pub struct QueryRoot;

/// Root mutation
pub struct MutationRoot;

/// Root subscription
pub struct SubscriptionRoot;

impl QueryRoot {}

impl MutationRoot {}

impl SubscriptionRoot {}

// =============================================================================
// SCHEMA BUILD
// =============================================================================

/// Build the complete schema
pub fn build_schema() -> async_graphql::Schema<
    QueryRoot,
    MutationRoot,
    SubscriptionRoot,
> {
    async_graphql::Schema::build(QueryRoot, MutationRoot, SubscriptionRoot)
}