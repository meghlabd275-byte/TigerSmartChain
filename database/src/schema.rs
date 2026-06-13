//! Database Schema for TigerScan
//! 
//! Complete SQL schema for all tables with proper indexes and constraints.

use serde::{Deserialize, Serialize};

// =============================================================================
// BLOCKS TABLE
// =============================================================================

/// Blocks table schema
pub const CREATE_BLOCKS_TABLE: &str = r#"
CREATE TABLE IF NOT EXISTS blocks (
    number BIGINT PRIMARY KEY,
    hash VARCHAR(66) NOT NULL UNIQUE,
    parent_hash VARCHAR(66) NOT NULL,
    nonce VARCHAR(66),
    sha3_uncles VARCHAR(66),
    logs_bloom TEXT,
    transactions_root VARCHAR(66),
    state_root VARCHAR(66),
    receipts_root VARCHAR(66),
    miner VARCHAR(42),
    difficulty VARCHAR(32),
    total_difficulty VARCHAR(32),
    size BIGINT,
    gas_limit BIGINT NOT NULL,
    gas_used BIGINT NOT NULL,
    timestamp BIGINT NOT NULL,
    extra_data TEXT,
    mix_hash VARCHAR(66),
    base_fee_per_gas BIGINT,
    blob_gas_used BIGINT,
    excess_blob_gas BIGINT,
    created_at TIMESTAMP DEFAULT NOW(),
    updated_at TIMESTAMP DEFAULT NOW()
);
CREATE INDEX IF NOT EXISTS idx_blocks_timestamp ON blocks(timestamp);
CREATE INDEX IF NOT EXISTS idx_blocks_miner ON blocks(miner);
CREATE INDEX IF NOT EXISTS idx_blocks_hash ON blocks(hash);
"#;

// =============================================================================
// TRANSACTIONS TABLE
// =============================================================================

/// Transactions table schema
pub const CREATE_TRANSACTIONS_TABLE: &str = r#"
CREATE TABLE IF NOT EXISTS transactions (
    hash VARCHAR(66) PRIMARY KEY,
    block_number BIGINT,
    block_hash VARCHAR(66),
    transaction_index BIGINT NOT NULL,
    from_address VARCHAR(42) NOT NULL,
    to_address VARCHAR(42),
    value NUMERIC(78, 0) NOT NULL,
    gas_price BIGINT,
    gas BIGINT,
    input TEXT,
    nonce BIGINT NOT NULL,
    tx_type SMALLINT,
    status VARCHAR(20),
    created_at TIMESTAMP DEFAULT NOW(),
    FOREIGN KEY (block_number) REFERENCES blocks(number) ON DELETE CASCADE);
CREATE INDEX IF NOT EXISTS idx_transactions_block_number ON transactions(block_number);
CREATE INDEX IF NOT EXISTS idx_transactions_from ON transactions(from_address);
CREATE INDEX IF NOT EXISTS idx_transactions_to ON transactions(to_address);
CREATE INDEX IF NOT EXISTS idx_transactions_nonce ON transactions(from_address, nonce);
CREATE INDEX IF NOT EXISTS idx_transactions_hash ON transactions(hash);
"#;

// =============================================================================
// RECEIPIPS TABLE
// =============================================================================

/// Transaction receipts table schema
pub const CREATE_RECEIPTS_TABLE: &str = r#"
CREATE TABLE IF NOT EXISTS receipts (
    transaction_hash VARCHAR(66) PRIMARY KEY,
    block_number BIGINT NOT NULL,
    block_hash VARCHAR(66) NOT NULL,
    contract_address VARCHAR(42),
    cumulative_gas_used BIGINT NOT NULL,
    gas_used BIGINT NOT NULL,
    logs_bloom TEXT NOT NULL,
    status VARCHAR(20),
    logs JSONB,
    created_at TIMESTAMP DEFAULT NOW(),
    FOREIGN KEY (transaction_hash) REFERENCES transactions(hash) ON DELETE CASCADE);
CREATE INDEX IF NOT EXISTS idx_receipts_block ON receipts(block_number);
CREATE INDEX IF NOT EXISTS idx_receipts_contract ON receipts(contract_address);
"#;

// =============================================================================
// LOGS TABLE
// =============================================================================

/// Event logs table schema
pub const CREATE_LOGS_TABLE: &str = r#"
CREATE TABLE IF NOT EXISTS logs (
    id BIGSERIAL PRIMARY KEY,
    address VARCHAR(42) NOT NULL,
    topics JSONB NOT NULL,
    data TEXT NOT NULL,
    block_number BIGINT NOT NULL,
    transaction_hash VARCHAR(66) NOT NULL,
    log_index BIGINT NOT NULL,
    created_at TIMESTAMP DEFAULT NOW(),
    FOREIGN KEY (block_number) REFERENCES blocks(number) ON DELETE CASCADE,
    FOREIGN KEY (transaction_hash) REFERENCES transactions(hash) ON DELETE CASCADE);
CREATE INDEX IF NOT EXISTS idx_logs_address ON logs(address);
CREATE INDEX IF NOT EXISTS idx_logs_block ON logs(block_number);
CREATE INDEX IF NOT EXISTS idx_logs_tx_hash ON logs(transaction_hash);
CREATE INDEX IF NOT EXISTS idx_logs_topics ON logs(topics);
"#;

// =============================================================================
// TRACES TABLE
// =============================================================================

/// Internal transaction traces table schema
pub const CREATE_TRACES_TABLE: &str = r#"
CREATE TABLE IF NOT EXISTS traces (
    id BIGSERIAL PRIMARY KEY,
    transaction_hash VARCHAR(66) NOT NULL,
    block_number BIGINT NOT NULL,
    subtrace_index BIGINT NOT NULL,
    call_type VARCHAR(30) NOT NULL,
    from_address VARCHAR(42) NOT NULL,
    to_address VARCHAR(42) NOT NULL,
    value NUMERIC(78, 0),
    gas BIGINT,
    gas_used BIGINT,
    input TEXT,
    output TEXT,
    error TEXT,
    depth BIGINT NOT NULL,
    parent_index BIGINT,
    trace_type VARCHAR(30) NOT NULL,
    created_at TIMESTAMP DEFAULT NOW(),
    FOREIGN KEY (block_number) REFERENCES blocks(number) ON DELETE CASCADE,
    FOREIGN KEY (transaction_hash) REFERENCES transactions(hash) ON DELETE CASCADE);
CREATE INDEX IF NOT EXISTS idx_traces_tx ON traces(transaction_hash);
CREATE INDEX IF NOT EXISTS idx_traces_block ON traces(block_number);
CREATE INDEX IF NOT EXISTS idx_traces_from ON traces(from_address);
CREATE INDEX IF NOT EXISTS idx_traces_to ON traces(to_address);
CREATE INDEX IF NOT EXISTS idx_traces_depth ON traces(depth);
"#;

// =============================================================================
// STATE DIFFS TABLE
// =============================================================================

/// State diffs table schema
pub const CREATE_STATE_DIFFS_TABLE: &str = r#"
CREATE TABLE IF NOT EXISTS state_diffs (
    id BIGSERIAL PRIMARY KEY,
    transaction_hash VARCHAR(66),
    block_number BIGINT NOT NULL,
    address VARCHAR(42) NOT NULL,
    storage_key VARCHAR(66),
    previous_value TEXT NOT NULL,
    current_value TEXT NOT NULL,
    diff_type VARCHAR(20) NOT NULL,
    created_at TIMESTAMP DEFAULT NOW(),
    FOREIGN KEY (block_number) REFERENCES blocks(number) ON DELETE CASCADE);
CREATE INDEX IF NOT EXISTS idx_state_diffs_address ON state_diffs(address);
CREATE INDEX IF NOT EXISTS idx_state_diffs_block ON state_diffs(block_number);
CREATE INDEX IF NOT EXISTS idx_state_diffs_tx ON state_diffs(transaction_hash);
"#;

// =============================================================================
// CONTRACTS TABLE
// =============================================================================

/// Contracts table schema
pub const CREATE_CONTRACTS_TABLE: &str = r#"
CREATE TABLE IF NOT EXISTS contracts (
    address VARCHAR(42) PRIMARY KEY,
    bytecode TEXT,
    bytecode_hash VARCHAR(66),
    is_verified BOOLEAN DEFAULT FALSE,
    is_verified_24h BOOLEAN DEFAULT FALSE,
    verification_date TIMESTAMP,
    is_contract BOOLEAN DEFAULT TRUE,
    contract_type VARCHAR(30),
    created_at TIMESTAMP DEFAULT NOW(),
    updated_at TIMESTAMP DEFAULT NOW());
CREATE INDEX IF NOT EXISTS idx_contracts_bytecode_hash ON contracts(bytecode_hash);
CREATE INDEX IF NOT EXISTS idx_contracts_verified ON contracts(is_verified);
"#;

// =============================================================================
// VERIFIED SOURCES TABLE
// =============================================================================

/// Verified contract sources table schema
pub const CREATE_VERIFIED_SOURCES_TABLE: &str = r#"
CREATE TABLE IF NOT EXISTS verified_sources (
    id BIGSERIAL PRIMARY KEY,
    contract_address VARCHAR(42) NOT NULL,
    source_code TEXT NOT NULL,
    file_name VARCHAR(255) NOT NULL,
    compiler_version VARCHAR(50) NOT NULL,
    evm_version VARCHAR(30),
    license VARCHAR(50),
    optimization_enabled BOOLEAN,
    optimization_runs INTEGER,
    constructor_args TEXT,
    libraries JSONB,
    is_proxy BOOLEAN DEFAULT FALSE,
    proxy_master_copy VARCHAR(42),
    is_upgradeable BOOLEAN DEFAULT FALSE,
    admin_address VARCHAR(42),
    implementation_address VARCHAR(42),
    verified_at TIMESTAMP DEFAULT NOW(),
    updated_at TIMESTAMP DEFAULT NOW(),
    FOREIGN KEY (contract_address) REFERENCES contracts(address) ON DELETE CASCADE);
CREATE INDEX IF NOT EXISTS idx_verified_sources_address ON verified_sources(contract_address);
CREATE INDEX IF NOT EXISTS idx_verified_sources_file ON verified_sources(file_name);
"#;

// =============================================================================
// TOKENS TABLE
// =============================================================================

/// Tokens table schema
pub const CREATE_TOKENS_TABLE: &str = r#"
CREATE TABLE IF NOT EXISTS tokens (
    address VARCHAR(42) PRIMARY KEY,
    name VARCHAR(255) NOT NULL,
    symbol VARCHAR(50) NOT NULL,
    decimals SMALLINT NOT NULL,
    total_supply NUMERIC(78, 0),
    type VARCHAR(20) NOT NULL,
    price USD,
    price_24h_ago USD,
    market_cap USD,
    volume_24h USD,
    holders_count BIGINT DEFAULT 0,
    transfers_count BIGINT DEFAULT 0,
    is_verified BOOLEAN DEFAULT FALSE,
    is_spam BOOLEAN DEFAULT FALSE,
    price_source VARCHAR(50),
    last_updated TIMESTAMP,
    created_at TIMESTAMP DEFAULT NOW(),
    updated_at TIMESTAMP DEFAULT NOW());
CREATE INDEX IF NOT EXISTS idx_tokens_symbol ON tokens(symbol);
CREATE INDEX IF NOT EXISTS idx_tokens_type ON tokens(type);
CREATE INDEX IF NOT EXISTS idx_tokens_price ON tokens(price);
CREATE INDEX IF NOT EXISTS idx_tokens_holders ON tokens(holders_count);
"#;

// =============================================================================
// TOKEN TRANSFERS TABLE
// =============================================================================

/// Token transfers table schema
pub const CREATE_TOKEN_TRANSFERS_TABLE: &str = r#"
CREATE TABLE IF NOT EXISTS token_transfers (
    id BIGSERIAL PRIMARY KEY,
    token_address VARCHAR(42) NOT NULL,
    from_address VARCHAR(42) NOT NULL,
    to_address VARCHAR(42) NOT NULL,
    value NUMERIC(78, 0) NOT NULL,
    transaction_hash VARCHAR(66) NOT NULL,
    block_number BIGINT NOT NULL,
    log_index BIGINT NOT NULL,
    created_at TIMESTAMP DEFAULT NOW(),
    FOREIGN KEY (token_address) REFERENCES tokens(address) ON DELETE CASCADE,
    FOREIGN KEY (block_number) REFERENCES blocks(number) ON DELETE CASCADE);
CREATE INDEX IF NOT EXISTS idx_token_transfers_token ON token_transfers(token_address);
CREATE INDEX IF NOT EXISTS idx_token_transfers_from ON token_transfers(from_address);
CREATE INDEX IF NOT EXISTS idx_token_transfers_to ON token_transfers(to_address);
CREATE INDEX IF NOT EXISTS idx_token_transfers_block ON token_transfers(block_number);
CREATE INDEX IF NOT EXISTS idx_token_transfers_tx ON token_transfers(transaction_hash);
"#;

// =============================================================================
// TOKEN HOLDERS TABLE
// =============================================================================

/// Token holders table schema
pub const CREATE_TOKEN_HOLDERS_TABLE: &str = r#"
CREATE TABLE IF NOT EXISTS token_holders (
    id BIGSERIAL PRIMARY KEY,
    token_address VARCHAR(42) NOT NULL,
    address VARCHAR(42) NOT NULL,
    balance NUMERIC(78, 0) NOT NULL,
    block_number BIGINT NOT NULL,
    updated_at TIMESTAMP DEFAULT NOW(),
    FOREIGN KEY (token_address) REFERENCES tokens(address) ON DELETE CASCADE,
    FOREIGN KEY (block_number) REFERENCES blocks(number) ON DELETE CASCADE,
    UNIQUE(token_address, address));
CREATE INDEX IF NOT EXISTS idx_token_holders_address ON token_holders(address);
CREATE INDEX IF NOT EXISTS idx_token_holders_token ON token_holders(token_address, balance DESC);
"#;

// =============================================================================
// NFT COLLECTIONS TABLE
// =============================================================================

/// NFT collections table schema
pub const CREATE_NFT_COLLECTIONS_TABLE: &str = r#"
CREATE TABLE IF NOT EXISTS nft_collections (
    address VARCHAR(42) PRIMARY KEY,
    name VARCHAR(255) NOT NULL,
    symbol VARCHAR(50),
    contract_type VARCHAR(20) NOT NULL,
    total_supply BIGINT,
    minted_count BIGINT DEFAULT 0,
    owner_count BIGINT DEFAULT 0,
    floor_price NUMERIC(78, 0),
    average_price NUMERIC(78, 0),
    volume_24h NUMERIC(78, 0),
    volume_7d NUMERIC(78, 0),
    volume_30d NUMERIC(78, 0),
    image_url TEXT,
    banner_url TEXT,
    description TEXT,
    external_url TEXT,
    twitter VARCHAR(100),
    discord VARCHAR(100),
    is_verified BOOLEAN DEFAULT FALSE,
    is_spam BOOLEAN DEFAULT FALSE,
    last_updated TIMESTAMP,
    created_at TIMESTAMP DEFAULT NOW(),
    updated_at TIMESTAMP DEFAULT NOW());
CREATE INDEX IF NOT EXISTS idx_nft_collections_type ON nft_collections(contract_type);
CREATE INDEX IF NOT EXISTS idx_nft_collections_floor ON nft_collections(floor_price);
"#;

// =============================================================================
// NFT TOKENS TABLE
// =============================================================================

/// NFT tokens table schema
pub const CREATE_NFT_TOKENS_TABLE: &str = r#"
CREATE TABLE IF NOT EXISTS nft_tokens (
    id BIGSERIAL PRIMARY KEY,
    collection_address VARCHAR(42) NOT NULL,
    token_id VARCHAR(78) NOT NULL,
    owner VARCHAR(42),
    uri TEXT,
    metadata JSONB,
    image_url TEXT,
    animation_url TEXT,
    external_url TEXT,
    metadata_fetched_at TIMESTAMP,
    created_at TIMESTAMP DEFAULT NOW(),
    updated_at TIMESTAMP DEFAULT NOW(),
    FOREIGN KEY (collection_address) REFERENCES nft_collections(address) ON DELETE CASCADE,
    UNIQUE(collection_address, token_id));
CREATE INDEX IF NOT EXISTS idx_nft_tokens_owner ON nft_tokens(owner);
CREATE INDEX IF NOT EXISTS idx_nft_tokens_token_id ON nft_tokens(collection_address, token_id);
"#;

// =============================================================================
// NFT TRANSFERS TABLE
// =============================================================================

/// NFT transfers table schema
pub const CREATE_NFT_TRANSFERS_TABLE: &str = r#"
CREATE TABLE IF NOT EXISTS nft_transfers (
    id BIGSERIAL PRIMARY KEY,
    collection_address VARCHAR(42) NOT NULL,
    token_id VARCHAR(78) NOT NULL,
    from_address VARCHAR(42) NOT NULL,
    to_address VARCHAR(42) NOT NULL,
    transaction_hash VARCHAR(66) NOT NULL,
    block_number BIGINT NOT NULL,
    log_index BIGINT NOT NULL,
    created_at TIMESTAMP DEFAULT NOW(),
    FOREIGN KEY (collection_address) REFERENCES nft_collections(address) ON DELETE CASCADE,
    FOREIGN KEY (block_number) REFERENCES blocks(number) ON DELETE CASCADE);
CREATE INDEX IF NOT EXISTS idx_nft_transfers_collection ON nft_transfers(collection_address);
CREATE INDEX IF NOT EXISTS idx_nft_transfers_token ON nft_transfers(collection_address, token_id);
CREATE INDEX IF NOT EXISTS idx_nft_transfers_from ON nft_transfers(from_address);
CREATE INDEX IF NOT EXISTS idx_nft_transfers_to ON nft_transfers(to_address);
"#;

// =============================================================================
// ADDRESSES TABLE
// =============================================================================

/// Addresses table schema
pub const CREATE_ADDRESSES_TABLE: &str = r#"
CREATE TABLE IF NOT EXISTS addresses (
    address VARCHAR(42) PRIMARY KEY,
    is_contract BOOLEAN DEFAULT FALSE,
    is_multisig BOOLEAN DEFAULT FALSE,
    multisig_owners JSONB,
    name VARCHAR(255),
    ens_name VARCHAR(255),
    is_scammer BOOLEAN DEFAULT FALSE,
    is_verified BOOLEAN DEFAULT FALSE,
    first_seen_block BIGINT,
    last_seen_block BIGINT,
    total_transactions BIGINT DEFAULT 0,
    total_received NUMERIC(78, 0) DEFAULT 0,
    total_sent NUMERIC(78, 0) DEFAULT 0,
    created_at TIMESTAMP DEFAULT NOW(),
    updated_at TIMESTAMP DEFAULT NOW());
CREATE INDEX IF NOT EXISTS idx_addresses_is_contract ON addresses(is_contract);
CREATE INDEX IF NOT EXISTS idx_addresses_is_multisig ON addresses(is_multisig);
CREATE INDEX IF NOT EXISTS idx_addresses_ens ON addresses(ens_name);
"#;

// =============================================================================
// BLOCK REWARDS TABLE
// =============================================================================

/// Block rewards table schema
pub const CREATE_BLOCK_REWARDS_TABLE: &str = r#"
CREATE TABLE IF NOT EXISTS block_rewards (
    id BIGSERIAL PRIMARY KEY,
    block_number BIGINT NOT NULL UNIQUE,
    miner VARCHAR(42) NOT NULL,
    reward NUMERIC(78, 0) NOT NULL,
    uncle_rewards JSONB,
    gas_fees NUMERIC(78, 0),
    created_at TIMESTAMP DEFAULT NOW(),
    FOREIGN KEY (block_number) REFERENCES blocks(number) ON DELETE CASCADE);
CREATE INDEX IF NOT EXISTS idx_block_rewards_miner ON block_rewards(miner);
CREATE INDEX IF NOT EXISTS idx_block_rewards_block ON block_rewards(block_number);
"#;

// =============================================================================
// UNCLES TABLE
// =============================================================================

/// Uncle blocks table schema
pub const CREATE_UNCLES_TABLE: &str = r#"
CREATE TABLE IF NOT EXISTS uncles (
    hash VARCHAR(66) PRIMARY KEY,
    block_number BIGINT NOT NULL,
    miner VARCHAR(42) NOT NULL,
    parent_hash VARCHAR(66) NOT NULL,
    difficulty VARCHAR(32) NOT NULL,
    gas_limit BIGINT,
    gas_used BIGINT,
    timestamp BIGINT NOT NULL,
    created_at TIMESTAMP DEFAULT NOW(),
    FOREIGN KEY (block_number) REFERENCES blocks(number) ON DELETE CASCADE);
CREATE INDEX IF NOT EXISTS idx_uncles_miner ON uncles(miner);
CREATE INDEX IF NOT EXISTS idx_uncles_block ON uncles(block_number);
"#;

// =============================================================================
// CONTRACT CREATIONS TABLE
// =============================================================================

/// Contract creations table schema
pub const CREATE_CONTRACT_CREATIONS_TABLE: &str = r#"
CREATE TABLE IF NOT EXISTS contract_creations (
    id BIGSERIAL PRIMARY KEY,
    address VARCHAR(42) NOT NULL UNIQUE,
    transaction_hash VARCHAR(66) NOT NULL,
    creator VARCHAR(42) NOT NULL,
    block_number BIGINT NOT NULL,
    init_code TEXT,
    created_at TIMESTAMP DEFAULT NOW(),
    FOREIGN KEY (block_number) REFERENCES blocks(number) ON DELETE CASCADE,
    FOREIGN KEY (creator) REFERENCES addresses(address));
CREATE INDEX IF NOT EXISTS idx_contract_creations_creator ON contract_creations(creator);
CREATE INDEX IF NOT EXISTS idx_contract_creations_block ON contract_creations(block_number);
"#;

// =============================================================================
// TOKEN APPROVALS TABLE
// =============================================================================

/// Token approvals table schema
pub const CREATE_TOKEN_APPROVALS_TABLE: &str = r#"
CREATE TABLE IF NOT EXISTS token_approvals (
    id BIGSERIAL PRIMARY KEY,
    token_address VARCHAR(42) NOT NULL,
    owner VARCHAR(42) NOT NULL,
    spender VARCHAR(42) NOT NULL,
    value NUMERIC(78, 0),
    transaction_hash VARCHAR(66) NOT NULL,
    block_number BIGINT NOT NULL,
    is_current BOOLEAN DEFAULT TRUE,
    created_at TIMESTAMP DEFAULT NOW(),
    updated_at TIMESTAMP DEFAULT NOW(),
    FOREIGN KEY (token_address) REFERENCES tokens(address) ON DELETE CASCADE,
    FOREIGN KEY (block_number) REFERENCES blocks(number) ON DELETE CASCADE,
    UNIQUE(token_address, owner, spender));
CREATE INDEX IF NOT EXISTS idx_token_approvals_token ON token_approvals(token_address);
CREATE INDEX IF NOT EXISTS idx_token_approvals_owner ON token_approvals(owner);
CREATE INDEX IF NOT EXISTS idx_token_approvals_spender ON token_approvals(spender);
"#;

// =============================================================================
// NFT APPROVALS TABLE
// =============================================================================

/// NFT approvals table schema
pub const CREATE_NFT_APPROVALS_TABLE: &str = r#"
CREATE TABLE IF NOT EXISTS nft_approvals (
    id BIGSERIAL PRIMARY KEY,
    collection_address VARCHAR(42) NOT NULL,
    owner VARCHAR(42) NOT NULL,
    operator VARCHAR(42) NOT NULL,
    approved_all BOOLEAN,
    token_ids JSONB,
    transaction_hash VARCHAR(66) NOT NULL,
    block_number BIGINT NOT NULL,
    is_current BOOLEAN DEFAULT TRUE,
    created_at TIMESTAMP DEFAULT NOW(),
    updated_at TIMESTAMP DEFAULT NOW(),
    FOREIGN KEY (collection_address) REFERENCES nft_collections(address) ON DELETE CASCADE,
    FOREIGN KEY (block_number) REFERENCES blocks(number) ON DELETE CASCADE,
    UNIQUE(collection_address, owner, operator));
CREATE INDEX IF NOT EXISTS idx_nft_approvals_collection ON nft_approvals(collection_address);
CREATE INDEX IF NOT EXISTS idx_nft_approvals_owner ON nft_approvals(owner);
CREATE INDEX IF NOT EXISTS idx_nft_approvals_operator ON nft_approvals(operator);
"#;

// =============================================================================
// ANALYTICS TABLE
// =============================================================================

/// Analytics table schema (time-series data)
pub const CREATE_ANALYTICS_TABLE: &str = r#"
CREATE TABLE IF NOT EXISTS analytics (
    id BIGSERIAL PRIMARY KEY,
    metric_name VARCHAR(50) NOT NULL,
    metric_value NUMERIC(78, 0) NOT NULL,
    block_number BIGINT,
    timestamp BIGINT NOT NULL,
    labels JSONB,
    created_at TIMESTAMP DEFAULT NOW());
CREATE INDEX IF NOT EXISTS idx_analytics_metric ON analytics(metric_name, timestamp DESC);
CREATE INDEX IF NOT EXISTS idx_analytics_block ON analytics(block_number DESC);
CREATE INDEX IF NOT EXISTS idx_analytics_timestamp ON analytics(timestamp DESC);
"#;

// =============================================================================
// API KEYS TABLE
// =============================================================================

/// API keys table schema
pub const CREATE_API_KEYS_TABLE: &str = r#"
CREATE TABLE IF NOT EXISTS api_keys (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    key_hash VARCHAR(64) NOT NULL UNIQUE,
    name VARCHAR(255) NOT NULL,
    user_id VARCHAR(42),
    rate_limit INTEGER DEFAULT 1000,
    monthly_limit BIGINT DEFAULT 1000000,
    requests_used BIGINT DEFAULT 0,
    is_active BOOLEAN DEFAULT TRUE,
    expires_at TIMESTAMP,
    created_at TIMESTAMP DEFAULT NOW(),
    updated_at TIMESTAMP DEFAULT NOW());
CREATE INDEX IF NOT EXISTS idx_api_keys_hash ON api_keys(key_hash);
CREATE INDEX IF NOT EXISTS idx_api_keys_user ON api_keys(user_id);
"#;

// =============================================================================
// WEBHOOKS TABLE
// =============================================================================

/// Webhooks table schema
pub const CREATE_WEBHOOKS_TABLE: &str = r#"
CREATE TABLE IF NOT EXISTS webhooks (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    url TEXT NOT NULL,
    events JSONB NOT NULL,
    secret_hash VARCHAR(64),
    is_active BOOLEAN DEFAULT TRUE,
    failure_count INTEGER DEFAULT 0,
    last_failure_at TIMESTAMP,
    created_at TIMESTAMP DEFAULT NOW(),
    updated_at TIMESTAMP DEFAULT NOW());
CREATE INDEX IF NOT EXISTS idx_webhooks_active ON webhooks(is_active);
"#;

// =============================================================================
// ALERTS TABLE
// =============================================================================

/// Security alerts table schema
pub const CREATE_ALERTS_TABLE: &str = r#"
CREATE TABLE IF NOT EXISTS alerts (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    alert_type VARCHAR(50) NOT NULL,
    severity VARCHAR(20) NOT NULL,
    title VARCHAR(255) NOT NULL,
    description TEXT,
    address VARCHAR(42),
    transaction_hash VARCHAR(66),
    payload JSONB,
    acknowledged BOOLEAN DEFAULT FALSE,
    acknowledged_at TIMESTAMP,
    created_at TIMESTAMP DEFAULT NOW());
CREATE INDEX IF NOT EXISTS idx_alerts_type ON alerts(alert_type);
CREATE INDEX IF NOT EXISTS idx_alerts_severity ON alerts(severity);
CREATE INDEX IF NOT EXISTS idx_alerts_address ON alerts(address);
CREATE INDEX IF NOT EXISTS idx_alerts_created ON alerts(created_at DESC);
CREATE INDEX IF NOT EXISTS idx_alerts_acknowledged ON alerts(acknowledged);
"#;

// =============================================================================
// SOURCIFY METADATA TABLE
// =============================================================================

/// Sourcify metadata table schema
pub const CREATE_SOURCIFY_METADATA_TABLE: &str = r#"
CREATE TABLE IF NOT EXISTS sourcify_metadata (
    id BIGSERIAL PRIMARY KEY,
    contract_address VARCHAR(42) NOT NULL UNIQUE,
    chain_id BIGINT NOT NULL,
    metadata JSONB NOT NULL,
    compilation_artifacts JSONB,
    source_files JSONB,
    verified_at TIMESTAMP DEFAULT NOW(),
    updated_at TIMESTAMP DEFAULT NOW(),
    FOREIGN KEY (contract_address) REFERENCES contracts(address) ON DELETE CASCADE);
CREATE INDEX IF NOT EXISTS idx_sourcify_address ON sourcify_metadata(contract_address);
CREATE INDEX IF NOT EXISTS idx_sourcify_chain ON sourcify_metadata(chain_id);
"#;

// =============================================================================
// DEX PAIRS TABLE
// =============================================================================

/// DEX pairs table schema
pub const CREATE_DEX_PAIRS_TABLE: &str = r#"
CREATE TABLE IF NOT EXISTS dex_pairs (
    id BIGSERIAL PRIMARY KEY,
    pair_address VARCHAR(42) NOT NULL UNIQUE,
    token0_address VARCHAR(42) NOT NULL,
    token1_address VARCHAR(42) NOT NULL,
    reserve0 NUMERIC(78, 0),
    reserve1 NUMERIC(78, 0),
    total_supply NUMERIC(78, 0),
    volume_24h NUMERIC(78, 0),
    volume_7d NUMERIC(78, 0),
    liquidity_usd NUMERIC(78, 0),
    dex_name VARCHAR(50) NOT NULL,
    created_at TIMESTAMP DEFAULT NOW(),
    updated_at TIMESTAMP DEFAULT NOW());
CREATE INDEX IF NOT EXISTS idx_dex_pairs_token0 ON dex_pairs(token0_address);
CREATE INDEX IF NOT EXISTS idx_dex_pairs_token1 ON dex_pairs(token1_address);
CREATE INDEX IF NOT EXISTS idx_dex_pairs_dex ON dex_pairs(dex_name);
"#;

// =============================================================================
// PENDING TRANSACTIONS TABLE
// =============================================================================

/// Pending transactions table (mempool)
pub const CREATE_PENDING_TX_TABLE: &str = r#"
CREATE TABLE IF NOT EXISTS pending_transactions (
    hash VARCHAR(66) PRIMARY KEY,
    from_address VARCHAR(42) NOT NULL,
    to_address VARCHAR(42),
    value NUMERIC(78, 0) NOT NULL,
    gas_price BIGINT,
    gas BIGINT,
    nonce BIGINT NOT NULL,
    input TEXT,
    received_at TIMESTAMP DEFAULT NOW(),
    expires_at TIMESTAMP);
CREATE INDEX IF NOT EXISTS idx_pending_from ON pending_transactions(from_address);
CREATE INDEX IF NOT EXISTS idx_pending_nonce ON pending_transactions(from_address, nonce);
CREATE INDEX IF NOT EXISTS idx_pending_received ON pending_transactions(received_at DESC);
"#;

// =============================================================================
// ALL TABLES CREATION
// =============================================================================

/// All schema creation statements
pub const ALL_TABLES: &[&str] = &[
    CREATE_BLOCKS_TABLE,
    CREATE_TRANSACTIONS_TABLE,
    CREATE_RECEIPTS_TABLE,
    CREATE_LOGS_TABLE,
    CREATE_TRACES_TABLE,
    CREATE_STATE_DIFFS_TABLE,
    CREATE_CONTRACTS_TABLE,
    CREATE_VERIFIED_SOURCES_TABLE,
    CREATE_TOKENS_TABLE,
    CREATE_TOKEN_TRANSFERS_TABLE,
    CREATE_TOKEN_HOLDERS_TABLE,
    CREATE_NFT_COLLECTIONS_TABLE,
    CREATE_NFT_TOKENS_TABLE,
    CREATE_NFT_TRANSFERS_TABLE,
    CREATE_ADDRESSES_TABLE,
    CREATE_BLOCK_REWARDS_TABLE,
    CREATE_UNCLES_TABLE,
    CREATE_CONTRACT_CREATIONS_TABLE,
    CREATE_TOKEN_APPROVALS_TABLE,
    CREATE_NFT_APPROVALS_TABLE,
    CREATE_ANALYTICS_TABLE,
    CREATE_API_KEYS_TABLE,
    CREATE_WEBHOOKS_TABLE,
    CREATE_ALERTS_TABLE,
    CREATE_SOURCIFY_METADATA_TABLE,
    CREATE_DEX_PAIRS_TABLE,
    CREATE_PENDING_TX_TABLE,
];

use serde::Serialize;

/// Numeric type for currency values
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct USD(f64);