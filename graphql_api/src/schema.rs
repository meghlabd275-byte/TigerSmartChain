//! GraphQL Schema Definition for TigerScan

use async_graphql::Object;
use async_graphql::InputObject;
use async_graphql::SimpleObject;
use async_graphql::Enum;
use crate::db::Database;

// =============================================================================
// BLOCKS
// =============================================================================

/// Block type
#[derive(Clone)]
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
#[derive(Clone)]
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
#[derive(Clone)]
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
#[derive(Clone)]
pub struct NFTToken {
    pub id: i64,
    pub collection_address: String,
    pub token_id: String,
    pub owner: Option<String>,
    pub uri: Option<String>,
    pub image_url: Option<String>,
    pub animation_url: Option<String>,
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
// OBJECT, INTERFACE AND UNION IMPLEMENTATIONS
// =============================================================================

/// Every field of a block, plus the two relations that need a query of their
/// own. The scalar fields are resolved from the row the block was loaded from;
/// `transactions` and `uncles` read the database lazily, so listing blocks
/// does not pull every transaction of every block.
#[Object]
impl Block {
    async fn number(&self) -> i64 { self.number }
    async fn hash(&self) -> &str { &self.hash }
    async fn parent_hash(&self) -> &str { &self.parent_hash }
    async fn nonce(&self) -> Option<&str> { self.nonce.as_deref() }
    async fn sha3_uncles(&self) -> Option<&str> { self.sha3_uncles.as_deref() }
    async fn logs_bloom(&self) -> Option<&str> { self.logs_bloom.as_deref() }
    async fn transactions_root(&self) -> Option<&str> { self.transactions_root.as_deref() }
    async fn state_root(&self) -> Option<&str> { self.state_root.as_deref() }
    async fn receipts_root(&self) -> Option<&str> { self.receipts_root.as_deref() }
    async fn miner(&self) -> Option<&str> { self.miner.as_deref() }
    async fn difficulty(&self) -> Option<&str> { self.difficulty.as_deref() }
    async fn total_difficulty(&self) -> Option<&str> { self.total_difficulty.as_deref() }
    async fn size(&self) -> Option<i64> { self.size }
    async fn gas_limit(&self) -> i64 { self.gas_limit }
    async fn gas_used(&self) -> i64 { self.gas_used }
    async fn timestamp(&self) -> i64 { self.timestamp }
    async fn extra_data(&self) -> Option<&str> { self.extra_data.as_deref() }
    async fn base_fee_per_gas(&self) -> Option<i64> { self.base_fee_per_gas }

    /// The transactions included in this block, in the order they were mined.
    async fn transactions(&self, ctx: &async_graphql::Context<'_>) -> Vec<Transaction> {
        match ctx.data_opt::<Database>() {
            Some(db) => crate::queries::load_block_transactions(db, self.number).await,
            None => Vec::new(),
        }
    }

    /// The uncles referenced by this block.
    async fn uncles(&self, ctx: &async_graphql::Context<'_>) -> Vec<Uncle> {
        match ctx.data_opt::<Database>() {
            Some(db) => crate::queries::load_block_uncles(db, self.number).await,
            None => Vec::new(),
        }
    }
}

/// Every field of a transaction, plus its receipt and traces.
#[Object]
impl Transaction {
    async fn hash(&self) -> &str { &self.hash }
    async fn block_number(&self) -> Option<i64> { self.block_number }
    async fn block_hash(&self) -> Option<&str> { self.block_hash.as_deref() }
    async fn transaction_index(&self) -> i64 { self.transaction_index }
    async fn from_address(&self) -> &str { &self.from_address }
    async fn to_address(&self) -> Option<&str> { self.to_address.as_deref() }
    async fn value(&self) -> &str { &self.value }
    async fn gas_price(&self) -> Option<i64> { self.gas_price }
    async fn gas(&self) -> Option<i64> { self.gas }
    async fn input(&self) -> Option<&str> { self.input.as_deref() }
    async fn nonce(&self) -> i64 { self.nonce }
    async fn tx_type(&self) -> Option<i32> { self.tx_type }
    async fn status(&self) -> Option<&str> { self.status.as_deref() }

    /// The receipt, which is the transaction row plus its logs.
    async fn receipt(&self, ctx: &async_graphql::Context<'_>) -> Option<Receipt> {
        let db = ctx.data_opt::<Database>()?;
        crate::queries::load_receipt(db, &self.hash).await
    }

    /// The execution traces for this transaction.
    async fn traces(&self, ctx: &async_graphql::Context<'_>) -> Vec<Trace> {
        match ctx.data_opt::<Database>() {
            Some(db) => crate::queries::load_traces(db, &self.hash).await,
            None => Vec::new(),
        }
    }
}

/// Every field of a token, plus its holder distribution.
#[Object]
impl Token {
    async fn address(&self) -> &str { &self.address }
    async fn name(&self) -> &str { &self.name }
    async fn symbol(&self) -> &str { &self.symbol }
    async fn decimals(&self) -> i32 { self.decimals }
    async fn total_supply(&self) -> Option<&str> { self.total_supply.as_deref() }
    async fn token_type(&self) -> TokenType { self.token_type }
    async fn price(&self) -> Option<f64> { self.price }
    async fn price_24h_ago(&self) -> Option<f64> { self.price_24h_ago }
    async fn market_cap(&self) -> Option<f64> { self.market_cap }
    async fn volume_24h(&self) -> Option<f64> { self.volume_24h }
    async fn holders_count(&self) -> Option<i64> { self.holders_count }
    async fn transfers_count(&self) -> Option<i64> { self.transfers_count }
    async fn is_verified(&self) -> bool { self.is_verified }
    async fn is_spam(&self) -> bool { self.is_spam }

    /// The largest holders of this token.
    async fn holder_distribution(&self, ctx: &async_graphql::Context<'_>) -> Vec<Holder> {
        match ctx.data_opt::<Database>() {
            Some(db) => crate::queries::load_token_holders(db, &self.address, 100).await,
            None => Vec::new(),
        }
    }
}

/// Every field of an NFT token, plus its metadata document.
#[Object]
impl NFTToken {
    async fn id(&self) -> i64 { self.id }
    async fn collection_address(&self) -> &str { &self.collection_address }
    async fn token_id(&self) -> &str { &self.token_id }
    async fn owner(&self) -> Option<&str> { self.owner.as_deref() }
    async fn uri(&self) -> Option<&str> { self.uri.as_deref() }
    async fn image_url(&self) -> Option<&str> { self.image_url.as_deref() }
    async fn animation_url(&self) -> Option<&str> { self.animation_url.as_deref() }
    async fn attributes(&self) -> Option<&[NFTAttribute]> {
        self.attributes.as_deref()
    }

    /// The metadata document indexed for this token, when there is one.
    async fn metadata(&self, ctx: &async_graphql::Context<'_>) -> Option<NFTMetadata> {
        let db = ctx.data_opt::<Database>()?;
        crate::queries::load_nft_metadata(db, &self.collection_address, &self.token_id).await
    }
}

/// The result of a full-text search. The name field of every variant is
/// searchable, so the union is what the front end switches on to decide which
/// page to open.
#[derive(async_graphql::Union, Clone)]
pub enum SearchResult {
    Block(Block),
    Transaction(Transaction),
    Token(Token),
    Contract(Contract),
    NFTCollection(NFTCollection),
    /// Returned when the query matched nothing. It is a variant rather than a
    /// null so the field can stay non-null and the client still learns that
    /// the search was understood and simply found nothing.
    Empty(EmptyResult),
}

/// The result of a search that matched nothing.
#[derive(SimpleObject, Clone)]
pub struct EmptyResult {
    pub message: String,
}

impl Default for SearchResult {
    fn default() -> Self {
        SearchResult::Empty(EmptyResult {
            message: "no match".to_string(),
        })
    }
}

impl SearchResult {
    /// The kind of entity, as a stable string the client can branch on.
    pub fn kind(&self) -> &'static str {
        match self {
            SearchResult::Block(_) => "block",
            SearchResult::Transaction(_) => "transaction",
            SearchResult::Token(_) => "token",
            SearchResult::Contract(_) => "contract",
            SearchResult::NFTCollection(_) => "nft_collection",
            SearchResult::Empty(_) => "empty",
        }
    }
}


// =============================================================================
// PAGINATION
// =============================================================================

/// The page information for a connection.
///
/// The cursors are offsets rather than opaque keys: a client that follows
/// `end_cursor` resumes at the next row, and the number is stable as long as
/// the rows in front of it do not change. A chain index is append-only, so
/// that holds for every query here.
#[derive(SimpleObject, Clone)]
pub struct OffsetPageInfo {
    pub has_next_page: bool,
    pub has_previous_page: bool,
    pub start_cursor: Option<String>,
    pub end_cursor: Option<String>,
}

/// A page of results.
///
/// `nodes` carries the page and `page_info` the cursors that walk it. A
/// per-node cursor would require a wrapper type named after every node type,
/// which a generic type cannot provide without the registrations colliding;
/// `start_cursor` and `end_cursor` are what a client needs to page forwards,
/// and the offset between them is derivable, so nothing is lost.
#[derive(SimpleObject, Clone)]
#[graphql(concrete(name = "BlockConnection", params(Block)))]
#[graphql(concrete(name = "TransactionConnection", params(Transaction)))]
#[graphql(concrete(name = "TokenConnection", params(Token)))]
#[graphql(concrete(name = "TokenTransferConnection", params(TokenTransfer)))]
#[graphql(concrete(name = "NFTCollectionConnection", params(NFTCollection)))]
#[graphql(concrete(name = "NFTTokenConnection", params(NFTToken)))]
#[graphql(concrete(name = "NFTTransferConnection", params(NFTTransfer)))]
pub struct Connection<T: async_graphql::OutputType + Clone + Send + Sync> {
    pub nodes: Vec<T>,
    pub page_info: OffsetPageInfo,
}

impl<T: async_graphql::OutputType + Clone + Send + Sync> Connection<T> {
    /// Build a page that starts at `offset`.
    pub fn new(has_next: bool, nodes: Vec<T>, offset: i64) -> Self {
        let start_cursor = nodes.first().map(|_| format!("cursor:{offset}"));
        let end_cursor = nodes
            .last()
            .map(|_| format!("cursor:{}", offset + nodes.len() as i64 - 1));
        Self {
            nodes,
            page_info: OffsetPageInfo {
                has_next_page: has_next,
                has_previous_page: offset > 0,
                start_cursor,
                end_cursor,
            },
        }
    }
}

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
        .finish()
}
