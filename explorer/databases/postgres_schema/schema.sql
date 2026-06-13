-- TigerSmartChain Complete Database Schema
-- Production-grade PostgreSQL schema for TigerScan Explorer
-- Complete schema with all tables for blocks, transactions, tokens, NFTs, traces, contracts

-- ============================================================================
-- ENUMS
-- ============================================================================

CREATE TYPE tx_status AS ENUM ('pending', 'success', 'failed');
CREATE TYPE log_type AS ENUM ('token_transfer', 'token_approval', 'nft_transfer', 'nft_approval', 'swap', 'mint', 'burn', 'stake', 'unstake', 'vote', 'delegate', 'unknown');
CREATE TYPE contract_type AS ENUM ('contract', 'token', 'nft', 'multisig', 'proxy', 'factory');
CREATE TYPE verification_status AS ENUM ('unverified', 'pending', 'verified', 'failed', 'queued');

-- ============================================================================
-- BLOCKS
-- ============================================================================

CREATE TABLE IF NOT EXISTS blocks (
    id BIGSERIAL PRIMARY KEY,
    number BIGINT NOT NULL UNIQUE,
    hash VARCHAR(66) NOT NULL,
    parent_hash VARCHAR(66) NOT NULL,
    nonce VARCHAR(66),
    sha3_uncles VARCHAR(66),
    logs_bloom TEXT,
    transactions_root VARCHAR(66),
    state_root VARCHAR(66),
    receipts_root VARCHAR(66),
    miner VARCHAR(42) NOT NULL,
    difficulty VARCHAR(32),
    total_difficulty VARCHAR(32),
    gas_limit BIGINT NOT NULL,
    gas_used BIGINT NOT NULL,
    timestamp BIGINT NOT NULL,
    size BIGINT NOT NULL,
    extra_data TEXT,
    base_fee_per_gas BIGINT,
    blob_gas_used BIGINT,
    excess_blob_gas BIGINT,
    parent_beacon_block_root VARCHAR(66),
    tx_count INTEGER NOT NULL DEFAULT 0,
    uncle_count INTEGER NOT NULL DEFAULT 0,
    reward VARCHAR(66),
    is_uncle BOOLEAN DEFAULT FALSE,
    created_at TIMESTAMP WITH TIME ZONE DEFAULT NOW(),
    updated_at TIMESTAMP WITH TIME ZONE DEFAULT NOW()
);

CREATE INDEX idx_blocks_number ON blocks(number DESC);
CREATE INDEX idx_blocks_hash ON blocks(hash);
CREATE INDEX idx_blocks_timestamp ON blocks(timestamp DESC);
CREATE INDEX idx_blocks_miner ON blocks(miner);
CREATE INDEX idx_blocks_timestamp_number ON blocks(timestamp DESC, number DESC);

-- ============================================================================
-- UNCLES/OMMER BLOCKS
-- ============================================================================

CREATE TABLE IF NOT EXISTS uncles (
    id BIGSERIAL PRIMARY KEY,
    number BIGINT NOT NULL,
    hash VARCHAR(66) NOT NULL,
    parent_hash VARCHAR(66) NOT NULL,
    block_number BIGINT NOT NULL,
    miner VARCHAR(42) NOT NULL,
    gas_limit BIGINT NOT NULL,
    gas_used BIGINT NOT NULL,
    timestamp BIGINT NOT NULL,
    difficulty VARCHAR(32),
    reward VARCHAR(66),
    created_at TIMESTAMP WITH TIME ZONE DEFAULT NOW()
);

CREATE INDEX idx_uncles_hash ON uncles(hash);
CREATE INDEX idx_uncles_block_number ON uncles(block_number);
CREATE INDEX idx_uncles_miner ON uncles(miner);

-- ============================================================================
-- TRANSACTIONS
-- ============================================================================

CREATE TABLE IF NOT EXISTS transactions (
    id BIGSERIAL PRIMARY KEY,
    hash VARCHAR(66) NOT NULL UNIQUE,
    nonce BIGINT NOT NULL,
    block_hash VARCHAR(66),
    block_number BIGINT,
    transaction_index INTEGER,
    from_address VARCHAR(42) NOT NULL,
    to_address VARCHAR(42),
    value NUMERIC(78, 0) NOT NULL DEFAULT 0,
    gas_price BIGINT NOT NULL,
    gas_limit BIGINT NOT NULL,
    gas_used BIGINT,
    max_fee_per_gas BIGINT,
    max_priority_fee_per_gas BIGINT,
    input TEXT,
    v BIGINT NOT NULL,
    r VARCHAR(66),
    s VARCHAR(66),
    chain_id BIGINT,
    transaction_type VARCHAR(32),
    status tx_status NOT NULL DEFAULT 'pending',
    cumulative_gas_used BIGINT,
    effective_gas_price BIGINT,
    logs JSONB,
    contract_address VARCHAR(42),
    created_at TIMESTAMP WITH TIME ZONE DEFAULT NOW(),
    block_timestamp TIMESTAMP WITH TIME ZONE,
    FOREIGN KEY (block_number) REFERENCES blocks(number) ON DELETE SET NULL
);

CREATE INDEX idx_transactions_hash ON transactions(hash);
CREATE INDEX idx_transactions_block_number ON transactions(block_number DESC);
CREATE INDEX idx_transactions_from ON transactions(from_address);
CREATE INDEX idx_transactions_to ON transactions(to_address);
CREATE INDEX idx_transactions_timestamp ON transactions(block_timestamp DESC);
CREATE INDEX idx_transactions_status ON transactions(status);
CREATE INDEX idx_transactions_value ON transactions(value DESC);
CREATE INDEX idx_transactions_from_to ON transactions(from_address, to_address);
CREATE INDEX idx_transactions_hash_status ON transactions(hash, status);

-- ============================================================================
-- INTERNAL TRANSACTIONS
-- ============================================================================

CREATE TABLE IF NOT EXISTS internal_transactions (
    id BIGSERIAL PRIMARY KEY,
    transaction_hash VARCHAR(66) NOT NULL,
    block_number BIGINT NOT NULL,
    transaction_index INTEGER NOT NULL,
    depth INTEGER NOT NULL,
    call_type VARCHAR(32) NOT NULL,
    from_address VARCHAR(42) NOT NULL,
    to_address VARCHAR(42) NOT NULL,
    value NUMERIC(78, 0) NOT NULL DEFAULT 0,
    gas BIGINT NOT NULL,
    input TEXT,
    output TEXT,
    revert BOOLEAN DEFAULT FALSE,
    created_at TIMESTAMP WITH TIME ZONE DEFAULT NOW()
);

CREATE INDEX idx_internal_tx_hash ON internal_transactions(transaction_hash);
CREATE INDEX idx_internal_tx_block ON internal_transactions(block_number);
CREATE INDEX idx_internal_tx_from ON internal_transactions(from_address);
CREATE INDEX idx_internal_tx_to ON internal_transactions(to_address);
CREATE INDEX idx_internal_tx_depth ON internal_transactions(depth);

-- ============================================================================
-- TOKEN TRANSFERS
-- ============================================================================

CREATE TABLE IF NOT EXISTS token_transfers (
    id BIGSERIAL PRIMARY KEY,
    token_address VARCHAR(42) NOT NULL,
    from_address VARCHAR(42) NOT NULL,
    to_address VARCHAR(42) NOT NULL,
    value NUMERIC(78, 0) NOT NULL,
    transaction_hash VARCHAR(66) NOT NULL,
    block_number BIGINT NOT NULL,
    log_index INTEGER NOT NULL,
    created_at TIMESTAMP WITH TIME ZONE DEFAULT NOW()
);

CREATE INDEX idx_token_transfers_token ON token_transfers(token_address);
CREATE INDEX idx_token_transfers_from ON token_transfers(from_address);
CREATE INDEX idx_token_transfers_to ON token_transfers(to_address);
CREATE INDEX idx_token_transfers_tx ON token_transfers(transaction_hash);
CREATE INDEX idx_token_transfers_block ON token_transfers(block_number DESC);
CREATE INDEX idx_token_transfers_composite ON token_transfers(token_address, from_address, to_address);

-- ============================================================================
-- TOKEN APPROVALS
-- ============================================================================

CREATE TABLE IF NOT EXISTS token_approvals (
    id BIGSERIAL PRIMARY KEY,
    token_address VARCHAR(42) NOT NULL,
    owner VARCHAR(42) NOT NULL,
    spender VARCHAR(42) NOT NULL,
    value NUMERIC(78, 0) NOT NULL,
    transaction_hash VARCHAR(66) NOT NULL,
    block_number BIGINT NOT NULL,
    log_index INTEGER NOT NULL,
    created_at TIMESTAMP WITH TIME ZONE DEFAULT NOW()
);

CREATE INDEX idx_token_approvals_token ON token_approvals(token_address);
CREATE INDEX idx_token_approvals_owner ON token_approvals(owner);
CREATE INDEX idx_token_approvals_spender ON token_approvals(spender);

-- ============================================================================
-- NFT TRANSFERS
-- ============================================================================

CREATE TABLE IF NOT EXISTS nft_transfers (
    id BIGSERIAL PRIMARY KEY,
    token_address VARCHAR(42) NOT NULL,
    from_address VARCHAR(42) NOT NULL,
    to_address VARCHAR(42) NOT NULL,
    token_id NUMERIC(78, 0) NOT NULL,
    value NUMERIC(78, 0) DEFAULT 1,
    transaction_hash VARCHAR(66) NOT NULL,
    block_number BIGINT NOT NULL,
    log_index INTEGER NOT NULL,
    created_at TIMESTAMP WITH TIME ZONE DEFAULT NOW()
);

CREATE INDEX idx_nft_transfers_token ON nft_transfers(token_address);
CREATE INDEX idx_nft_transfers_token_id ON nft_transfers(token_address, token_id);
CREATE INDEX idx_nft_transfers_from ON nft_transfers(from_address);
CREATE INDEX idx_nft_transfers_to ON nft_transfers(to_address);
CREATE INDEX idx_nft_transfers_tx ON nft_transfers(transaction_hash);
CREATE INDEX idx_nft_transfers_block ON nft_transfers(block_number DESC);

-- ============================================================================
-- NFT APPROVALS
-- ============================================================================

CREATE TABLE IF NOT EXISTS nft_approvals (
    id BIGSERIAL PRIMARY KEY,
    token_address VARCHAR(42) NOT NULL,
    owner VARCHAR(42) NOT NULL,
    approved VARCHAR(42),
    token_id NUMERIC(78, 0),
    transaction_hash VARCHAR(66) NOT NULL,
    block_number BIGINT NOT NULL,
    log_index INTEGER NOT NULL,
    created_at TIMESTAMP WITH TIME ZONE DEFAULT NOW()
);

CREATE INDEX idx_nft_approvals_token ON nft_approvals(token_address);
CREATE INDEX idx_nft_approvals_owner ON nft_approvals(owner);

-- ============================================================================
-- CONTRACTS
-- ============================================================================

CREATE TABLE IF NOT EXISTS contracts (
    id BIGSERIAL PRIMARY KEY,
    address VARCHAR(42) NOT NULL UNIQUE,
    contract_name VARCHAR(255),
    compiler VARCHAR(64),
    compiler_version VARCHAR(64),
    optimization_enabled BOOLEAN DEFAULT TRUE,
    optimization_runs INTEGER DEFAULT 200,
    evm_version VARCHAR(32),
    license_type VARCHAR(128),
    source_code TEXT NOT NULL,
    abi JSONB,
    bytecode TEXT,
    runtime_bytecode TEXT,
    constructor_args TEXT,
    contract_type contract_type DEFAULT 'contract',
    is_verified BOOLEAN DEFAULT FALSE,
    verification_status verification_status DEFAULT 'unverified',
    verified_at TIMESTAMP WITH TIME ZONE,
    created_at TIMESTAMP WITH TIME ZONE DEFAULT NOW(),
    updated_at TIMESTAMP WITH TIME ZONE DEFAULT NOW()
);

CREATE INDEX idx_contracts_address ON contracts(address);
CREATE INDEX idx_contracts_name ON contracts(contract_name);
CREATE INDEX idx_contracts_verified ON contracts(is_verified);
CREATE INDEX idx_contracts_type ON contracts(contract_type);
CREATE INDEX idx_contracts_compiler ON contracts(compiler);

-- ============================================================================
-- CONTRACT METADATA
-- ============================================================================

CREATE TABLE IF NOT EXISTS contract_metadata (
    id BIGSERIAL PRIMARY KEY,
    contract_address VARCHAR(42) NOT NULL UNIQUE,
    title VARCHAR(255),
    description TEXT,
    author VARCHAR(255),
    license VARCHAR(128),
    settings JSONB,
    metadata_hash VARCHAR(66),
    created_at TIMESTAMP WITH TIME ZONE DEFAULT NOW(),
    updated_at TIMESTAMP WITH TIME ZONE DEFAULT NOW(),
    FOREIGN KEY (contract_address) REFERENCES contracts(address) ON DELETE CASCADE
);

CREATE INDEX idx_contract_metadata_address ON contract_metadata(contract_address);

-- ============================================================================
-- VERIFIED SOURCE FILES (Multi-file)
// ============================================================================

CREATE TABLE IF NOT EXISTS verified_sources (
    id BIGSERIAL PRIMARY KEY,
    contract_address VARCHAR(42) NOT NULL,
    file_name VARCHAR(512) NOT NULL,
    source_code TEXT NOT NULL,
    compiler_version VARCHAR(64),
    language VARCHAR(32),
    created_at TIMESTAMP WITH TIME ZONE DEFAULT NOW(),
    FOREIGN KEY (contract_address) REFERENCES contracts(address) ON DELETE CASCADE
);

CREATE INDEX idx_verified_sources_address ON verified_sources(contract_address);
CREATE INDEX idx_verified_sources_file ON verified_sources(file_name);

-- ============================================================================
-- CONTRACT CREATIONS
// ============================================================================

CREATE TABLE IF NOT EXISTS contract_creations (
    id BIGSERIAL PRIMARY KEY,
    address VARCHAR(42) NOT NULL UNIQUE,
    creator VARCHAR(42) NOT NULL,
    transaction_hash VARCHAR(66) NOT NULL,
    block_number BIGINT NOT NULL,
    gas_limit BIGINT,
    gas_used BIGINT,
    init_code TEXT,
    created_at TIMESTAMP WITH TIME ZONE DEFAULT NOW()
);

CREATE INDEX idx_contract_creations_address ON contract_creations(address);
CREATE INDEX idx_contract_creations_creator ON contract_creations(creator);
CREATE INDEX idx_contract_creations_tx ON contract_creations(transaction_hash);

-- ============================================================================
-- SOURCIFY METADATA
// ============================================================================

CREATE TABLE IF NOT EXISTS sourcify_metadata (
    id BIGSERIAL PRIMARY KEY,
    contract_address VARCHAR(42) NOT NULL UNIQUE,
    chain_id BIGINT NOT NULL,
    status VARCHAR(32) NOT NULL,
    metadata JSONB,
    sources JSONB,
    created_at TIMESTAMP WITH TIME ZONE DEFAULT NOW(),
    updated_at TIMESTAMP WITH TIME ZONE DEFAULT NOW()
);

CREATE INDEX idx_sourcify_address ON sourcify_metadata(contract_address);
CREATE INDEX idx_sourcify_chain ON sourcify_metadata(chain_id);

-- ============================================================================
-- TOKENS (TEP20)
// ============================================================================

CREATE TABLE IF NOT EXISTS tokens (
    id BIGSERIAL PRIMARY KEY,
    address VARCHAR(42) NOT NULL UNIQUE,
    name VARCHAR(255) NOT NULL,
    symbol VARCHAR(64) NOT NULL,
    decimals INTEGER NOT NULL DEFAULT 18,
    total_supply NUMERIC(78, 0),
    holders_count BIGINT DEFAULT 0,
    transfers_count BIGINT DEFAULT 0,
    circulating_supply NUMERIC(78, 0),
    price_usd NUMERIC(78, 18),
    price_change_24h NUMERIC(18, 8),
    market_cap NUMERIC(78, 0),
    volume_24h NUMERIC(78, 0),
    is_verified BOOLEAN DEFAULT FALSE,
    is_spam BOOLEAN DEFAULT FALSE,
    spam_score INTEGER DEFAULT 0,
    contract_address VARCHAR(42),
    logo_url TEXT,
    description TEXT,
    website TEXT,
    social JSONB,
    created_at TIMESTAMP WITH TIME ZONE DEFAULT NOW(),
    updated_at TIMESTAMP WITH TIME ZONE DEFAULT NOW()
);

CREATE INDEX idx_tokens_address ON tokens(address);
CREATE INDEX idx_tokens_symbol ON tokens(symbol);
CREATE INDEX idx_tokens_name ON tokens(name);
CREATE INDEX idx_tokens_price ON tokens(price_usd DESC);
CREATE INDEX idx_tokens_market_cap ON tokens(market_cap DESC);
CREATE INDEX idx_tokens_verified ON tokens(is_verified);

-- ============================================================================
-- TOKEN HOLDERS
// ============================================================================

CREATE TABLE IF NOT EXISTS token_holders (
    id BIGSERIAL PRIMARY KEY,
    token_address VARCHAR(42) NOT NULL,
    address VARCHAR(42) NOT NULL,
    balance NUMERIC(78, 0) NOT NULL DEFAULT 0,
    balance_usd NUMERIC(78, 18),
    percent_holdings NUMERIC(18, 8),
    is_contract BOOLEAN DEFAULT FALSE,
    updated_block BIGINT,
    created_at TIMESTAMP WITH TIME ZONE DEFAULT NOW(),
    updated_at TIMESTAMP WITH TIME ZONE DEFAULT NOW(),
    UNIQUE(token_address, address)
);

CREATE INDEX idx_token_holders_token ON token_holders(token_address);
CREATE INDEX idx_token_holders_address ON token_holders(address);
CREATE INDEX idx_token_holders_balance ON token_holders(balance DESC);
CREATE INDEX idx_token_holders_composite ON token_holders(token_address, balance DESC);

-- ============================================================================
-- NFT COLLECTIONS
// ============================================================================

CREATE TABLE IF NOT EXISTS nft_collections (
    id BIGSERIAL PRIMARY KEY,
    address VARCHAR(42) NOT NULL UNIQUE,
    name VARCHAR(255) NOT NULL,
    symbol VARCHAR(64),
    contract_type VARCHAR(32) NOT NULL,
    total_supply BIGINT,
    holders_count BIGINT DEFAULT 0,
    transfers_count BIGINT DEFAULT 0,
    floor_price NUMERIC(78, 18),
    floor_price_change_24h NUMERIC(18, 8),
    volume_24h NUMERIC(78, 0),
    volume_change_24h NUMERIC(18, 8),
    average_price_24h NUMERIC(78, 18),
    market_cap NUMERIC(78, 0),
    description TEXT,
    image_url TEXT,
    external_url TEXT,
    banner_url TEXT,
    social JSONB,
    is_verified BOOLEAN DEFAULT FALSE,
    is_spam BOOLEAN DEFAULT FALSE,
    spam_score INTEGER DEFAULT 0,
    created_at TIMESTAMP WITH TIME ZONE DEFAULT NOW(),
    updated_at TIMESTAMP WITH TIME ZONE DEFAULT NOW()
);

CREATE INDEX idx_nft_collections_address ON nft_collections(address);
CREATE INDEX idx_nft_collections_name ON nft_collections(name);
CREATE INDEX idx_nft_collections_floor ON nft_collections(floor_price DESC);
CREATE INDEX idx_nft_collections_volume ON nft_collections(volume_24h DESC);

-- ============================================================================
-- NFTS
// ============================================================================

CREATE TABLE IF NOT EXISTS nfts (
    id BIGSERIAL PRIMARY KEY,
    token_address VARCHAR(42) NOT NULL,
    token_id NUMERIC(78, 0) NOT NULL,
    owner VARCHAR(42) NOT NULL,
    uri TEXT,
    metadata JSONB,
    name VARCHAR(512),
    description TEXT,
    image_url TEXT,
    external_url TEXT,
    attributes JSONB,
    is_burned BOOLEAN DEFAULT FALSE,
    last_transfer_block BIGINT,
    last_transfer_timestamp TIMESTAMP WITH TIME ZONE,
    created_at TIMESTAMP WITH TIME ZONE DEFAULT NOW(),
    updated_at TIMESTAMP WITH TIME ZONE DEFAULT NOW(),
    UNIQUE(token_address, token_id)
);

CREATE INDEX idx_nfts_address ON nfts(token_address);
CREATE INDEX idx_nfts_token_id ON nfts(token_address, token_id);
CREATE INDEX idx_nfts_owner ON nfts(owner);
CREATE INDEX idx_nfts_attributes ON nfts USING GIN(attributes jsonb_path_ops);
CREATE INDEX idx_nfts_last_transfer ON nfts(last_transfer_block DESC);

-- ============================================================================
-- NFT OWNERS (Current)
// ============================================================================

CREATE TABLE IF NOT EXISTS nft_owners (
    id BIGSERIAL PRIMARY KEY,
    token_address VARCHAR(42) NOT NULL,
    token_id NUMERIC(78, 0) NOT NULL,
    owner VARCHAR(42) NOT NULL,
    updated_block BIGINT NOT NULL,
    updated_at TIMESTAMP WITH TIME ZONE DEFAULT NOW(),
    UNIQUE(token_address, token_id)
);

CREATE INDEX idx_nft_owners_token ON nft_owners(token_address);
CREATE INDEX idx_nft_owners_token_id ON nft_owners(token_address, token_id);
CREATE INDEX idx_nft_owners_owner ON nft_owners(owner);

-- ============================================================================
-- VALIDATORS
// ============================================================================

CREATE TABLE IF NOT EXISTS validators (
    id BIGSERIAL PRIMARY KEY,
    address VARCHAR(42) NOT NULL UNIQUE,
    name VARCHAR(255),
    moniker VARCHAR(255),
    identity VARCHAR(255),
    website VARCHAR(512),
    contact VARCHAR(512),
    description TEXT,
    commission_rate NUMERIC(5, 2),
    max_rate NUMERIC(5, 2),
    max_change_rate NUMERIC(5, 2),
    min_self_delegation NUMERIC(78, 0),
    self_delegation NUMERIC(78, 0),
    delegation NUMERIC(78, 0),
    total_stake NUMERIC(78, 0),
    tokens NUMERIC(78, 0),
    uptime NUMERIC(5, 2),
    blocks_count BIGINT DEFAULT 0,
    misses_count BIGINT DEFAULT 0,
    is_jailed BOOLEAN DEFAULT FALSE,
    is_active BOOLEAN DEFAULT TRUE,
    status VARCHAR(32),
    created_at TIMESTAMP WITH TIME ZONE DEFAULT NOW(),
    updated_at TIMESTAMP WITH TIME ZONE DEFAULT NOW()
);

CREATE INDEX idx_validators_address ON validators(address);
CREATE INDEX idx_validators_name ON validators(name);
CREATE INDEX idx_validators_stake ON validators(total_stake DESC);
CREATE INDEX idx_validators_uptime ON validators(uptime DESC);
CREATE INDEX idx_validators_active ON validators(is_active);

-- ============================================================================
-- BLOCK REWARDS
// ============================================================================

CREATE TABLE IF NOT EXISTS block_rewards (
    id BIGSERIAL PRIMARY KEY,
    block_number BIGINT NOT NULL UNIQUE,
    block_hash VARCHAR(66) NOT NULL,
    validator VARCHAR(42) NOT NULL,
    amount NUMERIC(78, 0) NOT NULL,
    transaction_hash VARCHAR(66),
    created_at TIMESTAMP WITH TIME ZONE DEFAULT NOW()
);

CREATE INDEX idx_block_rewards_block ON block_rewards(block_number DESC);
CREATE INDEX idx_block_rewards_validator ON block_rewards(validator);
CREATE INDEX idx_block_rewards_hash ON block_rewards(block_hash);

-- ============================================================================
-- TRACES
// ============================================================================

CREATE TABLE IF NOT EXISTS traces (
    id BIGSERIAL PRIMARY KEY,
    transaction_hash VARCHAR(66) NOT NULL,
    block_number BIGINT NOT NULL,
    transaction_index INTEGER NOT NULL,
    from_address VARCHAR(42) NOT NULL,
    to_address VARCHAR(42),
    call_type VARCHAR(32) NOT NULL,
    value NUMERIC(78, 0),
    gas BIGINT,
    input TEXT,
    output TEXT,
    revert BOOLEAN DEFAULT FALSE,
    error TEXT,
    depth INTEGER NOT NULL,
    created_at TIMESTAMP WITH TIME ZONE DEFAULT NOW()
);

CREATE INDEX idx_traces_tx ON traces(transaction_hash);
CREATE INDEX idx_traces_block ON traces(block_number);
CREATE INDEX idx_traces_from ON traces(from_address);
CREATE INDEX idx_traces_to ON traces(to_address);
CREATE INDEX idx_traces_depth ON traces(depth);
CREATE INDEX idx_traces_composite ON traces(transaction_hash, depth);

-- ============================================================================
-- STATE ACCOUNTS (Historical)
// ============================================================================

CREATE TABLE IF NOT EXISTS state_accounts (
    id BIGSERIAL PRIMARY KEY,
    address VARCHAR(42) NOT NULL,
    block_number BIGINT NOT NULL,
    balance NUMERIC(78, 0),
    nonce BIGINT,
    code_hash VARCHAR(66),
    storage_root VARCHAR(66),
    is_contract BOOLEAN DEFAULT FALSE,
    created_at TIMESTAMP WITH TIME ZONE DEFAULT NOW(),
    UNIQUE(address, block_number)
);

CREATE INDEX idx_state_accounts_address ON state_accounts(address);
CREATE INDEX idx_state_accounts_block ON state_accounts(block_number DESC);
CREATE INDEX idx_state_accounts_composite ON state_accounts(address, block_number DESC);

-- ============================================================================
-- ACCOUNTS
// ============================================================================

CREATE TABLE IF NOT EXISTS accounts (
    id BIGSERIAL PRIMARY KEY,
    address VARCHAR(42) NOT NULL UNIQUE,
    balance NUMERIC(78, 0) DEFAULT 0,
    nonce BIGINT DEFAULT 0,
    code_hash VARCHAR(66),
    is_contract BOOLEAN DEFAULT FALSE,
    is_verified BOOLEAN DEFAULT FALSE,
    token_balance_count BIGINT DEFAULT 0,
    nft_balance_count BIGINT DEFAULT 0,
    created_at TIMESTAMP WITH TIME ZONE DEFAULT NOW(),
    updated_at TIMESTAMP WITH TIME ZONE DEFAULT NOW()
);

CREATE INDEX idx_accounts_address ON accounts(address);
CREATE INDEX idx_accounts_balance ON accounts(balance DESC);
CREATE INDEX idx_accounts_contract ON accounts(is_contract);

-- ============================================================================
-- DEX PAIRS
// ============================================================================

CREATE TABLE IF NOT EXISTS dex_pairs (
    id BIGSERIAL PRIMARY KEY,
    pair_address VARCHAR(42) NOT NULL UNIQUE,
    token0_address VARCHAR(42) NOT NULL,
    token1_address VARCHAR(42) NOT NULL,
    token0_symbol VARCHAR(64),
    token1_symbol VARCHAR(64),
    reserve0 NUMERIC(78, 0),
    reserve1 NUMERIC(78, 0),
    total_supply NUMERIC(78, 0),
    liquidity_usd NUMERIC(78, 0),
    volume_24h NUMERIC(78, 0),
    volume_change_24h NUMERIC(18, 8),
    fee_24h NUMERIC(78, 0),
    factory_address VARCHAR(42),
    pair_type VARCHAR(32),
    created_at TIMESTAMP WITH TIME ZONE DEFAULT NOW(),
    updated_at TIMESTAMP WITH TIME ZONE DEFAULT NOW()
);

CREATE INDEX idx_dex_pairs_address ON dex_pairs(pair_address);
CREATE INDEX idx_dex_pairs_token0 ON dex_pairs(token0_address);
CREATE INDEX idx_dex_pairs_token1 ON dex_pairs(token1_address);
CREATE INDEX idx_dex_pairs_liquidity ON dex_pairs(liquidity_usd DESC);
CREATE INDEX idx_dex_pairs_volume ON dex_pairs(volume_24h DESC);

-- ============================================================================
-- TOKEN PRICES (Historical)
// ============================================================================

CREATE TABLE IF NOT EXISTS token_prices (
    id BIGSERIAL PRIMARY KEY,
    token_address VARCHAR(42) NOT NULL,
    price_usd NUMERIC(78, 18) NOT NULL,
    timestamp BIGINT NOT NULL,
    source VARCHAR(64),
    created_at TIMESTAMP WITH TIME ZONE DEFAULT NOW(),
    UNIQUE(token_address, timestamp)
);

CREATE INDEX idx_token_prices_address ON token_prices(token_address);
CREATE INDEX idx_token_prices_time ON token_prices(timestamp DESC);
CREATE INDEX idx_token_prices_composite ON token_prices(token_address, timestamp DESC);

-- ============================================================================
-- GAS PRICES
// ============================================================================

CREATE TABLE IF NOT EXISTS gas_prices (
    id BIGSERIAL PRIMARY KEY,
    gas_price BIGINT NOT NULL,
    gas_used BIGINT,
    gas_limit BIGINT,
    timestamp BIGINT NOT NULL,
    base_fee BIGINT,
    priority_fee BIGINT,
    created_at TIMESTAMP WITH TIME ZONE DEFAULT NOW()
);

CREATE INDEX idx_gas_prices_time ON gas_prices(timestamp DESC);
CREATE INDEX idx_gas_prices_gas_price ON gas_prices(gas_price);

-- ============================================================================
-- PENDING TRANSACTIONS (MEMPOOL)
// ============================================================================

CREATE TABLE IF NOT EXISTS pending_transactions (
    id BIGSERIAL PRIMARY KEY,
    hash VARCHAR(66) NOT NULL UNIQUE,
    nonce BIGINT NOT NULL,
    from_address VARCHAR(42) NOT NULL,
    to_address VARCHAR(42),
    value NUMERIC(78, 0) NOT NULL DEFAULT 0,
    gas_price BIGINT NOT NULL,
    gas_limit BIGINT NOT NULL,
    input TEXT,
    created_at TIMESTAMP WITH TIME ZONE DEFAULT NOW(),
    expires_at TIMESTAMP WITH TIME ZONE
);

CREATE INDEX idx_pending_tx_hash ON pending_transactions(hash);
CREATE INDEX idx_pending_tx_from ON pending_transactions(from_address);
CREATE INDEX idx_pending_tx_nonce ON pending_transactions(nonce);
CREATE INDEX idx_pending_tx_created ON pending_transactions(created_at DESC);

-- ============================================================================
-- SEARCH INDEX (Full-text)
// ============================================================================

CREATE TABLE IF NOT EXISTS search_index (
    id BIGSERIAL PRIMARY KEY,
    search_type VARCHAR(32) NOT NULL,
    address VARCHAR(42),
    hash VARCHAR(66),
    number BIGINT,
    name VARCHAR(512),
    description TEXT,
    content_tsv tsvector,
    created_at TIMESTAMP WITH TIME ZONE DEFAULT NOW(),
    updated_at TIMESTAMP WITH TIME ZONE DEFAULT NOW()
);

CREATE INDEX idx_search_address ON search_index(address);
CREATE INDEX idx_search_hash ON search_index(hash);
CREATE INDEX idx_search_number ON search_index(number DESC);
CREATE INDEX idx_search_tsv ON search_index USING GIN(content_tsv);

-- ============================================================================
-- API KEYS
// ============================================================================

CREATE TABLE IF NOT EXISTS api_keys (
    id BIGSERIAL PRIMARY KEY,
    key_hash VARCHAR(64) NOT NULL UNIQUE,
    key_name VARCHAR(255) NOT NULL,
    user_id BIGINT,
    rate_limit INTEGER DEFAULT 1000,
    daily_limit BIGINT,
    requests_count BIGINT DEFAULT 0,
    requests_today BIGINT DEFAULT 0,
    last_request_date DATE,
    is_active BOOLEAN DEFAULT TRUE,
    expires_at TIMESTAMP WITH TIME ZONE,
    created_at TIMESTAMP WITH TIME ZONE DEFAULT NOW(),
    updated_at TIMESTAMP WITH TIME ZONE DEFAULT NOW()
);

CREATE INDEX idx_api_keys_hash ON api_keys(key_hash);
CREATE INDEX idx_api_keys_user ON api_keys(user_id);
CREATE INDEX idx_api_keys_active ON api_keys(is_active);

-- ============================================================================
-- WEBHOOKS
// ============================================================================

CREATE TABLE IF NOT EXISTS webhooks (
    id BIGSERIAL PRIMARY KEY,
    url TEXT NOT NULL,
    event_type VARCHAR(64) NOT NULL,
    secret_hash VARCHAR(64),
    is_active BOOLEAN DEFAULT TRUE,
    retry_count INTEGER DEFAULT 3,
    timeout_seconds INTEGER DEFAULT 30,
    created_at TIMESTAMP WITH TIME ZONE DEFAULT NOW(),
    updated_at TIMESTAMP WITH TIME ZONE DEFAULT NOW()
);

CREATE INDEX idx_webhooks_event ON webhooks(event_type);
CREATE INDEX idx_webhooks_active ON webhooks(is_active);

-- ============================================================================
-- WEBHOOK EVENTS
// ============================================================================

CREATE TABLE IF NOT EXISTS webhook_events (
    id BIGSERIAL PRIMARY KEY,
    webhook_id BIGINT NOT NULL,
    event_type VARCHAR(64) NOT NULL,
    payload JSONB NOT NULL,
    status VARCHAR(32) NOT NULL DEFAULT 'pending',
    attempts INTEGER DEFAULT 0,
    last_attempt TIMESTAMP WITH TIME ZONE,
    response_code INTEGER,
    response_body TEXT,
    error TEXT,
    created_at TIMESTAMP WITH TIME ZONE DEFAULT NOW(),
    FOREIGN KEY (webhook_id) REFERENCES webhooks(id) ON DELETE CASCADE
);

CREATE INDEX idx_webhook_events_webhook ON webhook_events(webhook_id);
CREATE INDEX idx_webhook_events_status ON webhook_events(status);
CREATE INDEX idx_webhook_events_created ON webhook_events(created_at DESC);

-- ============================================================================
-- MALICIOUS CONTRACTS
// ============================================================================

CREATE TABLE IF NOT EXISTS malicious_contracts (
    id BIGSERIAL PRIMARY KEY,
    address VARCHAR(42) NOT NULL UNIQUE,
    threat_type VARCHAR(64) NOT NULL,
    confidence INTEGER NOT NULL,
    description TEXT,
    source VARCHAR(255),
    first_seen BIGINT NOT NULL,
    last_seen BIGINT NOT NULL,
    is_active BOOLEAN DEFAULT TRUE,
    created_at TIMESTAMP WITH TIME ZONE DEFAULT NOW()
);

CREATE INDEX idx_malicious_address ON malicious_contracts(address);
CREATE INDEX idx_malicious_type ON malicious_contracts(threat_type);
CREATE INDEX idx_malicious_confidence ON malicious_contracts(confidence DESC);
CREATE INDEX idx_malicious_active ON malicious_contracts(is_active);

-- ============================================================================
-- PHISHING URLs
// ============================================================================

CREATE TABLE IF NOT EXISTS phishing_urls (
    id BIGSERIAL PRIMARY KEY,
    url TEXT NOT NULL UNIQUE,
    domain VARCHAR(255) NOT NULL,
    target_addresses TEXT[],
    description TEXT,
    source VARCHAR(255),
    first_seen BIGINT NOT NULL,
    last_seen BIGINT NOT NULL,
    is_active BOOLEAN DEFAULT TRUE,
    created_at TIMESTAMP WITH TIME ZONE DEFAULT NOW()
);

CREATE INDEX idx_phishing_url ON phishing_urls(url);
CREATE INDEX idx_phishing_domain ON phishing_urls(domain);
CREATE INDEX idx_phishing_active ON phishing_urls(is_active);

-- ============================================================================
-- SCAM TOKENS
// ============================================================================

CREATE TABLE IF NOT EXISTS scam_tokens (
    id BIGSERIAL PRIMARY KEY,
    address VARCHAR(42) NOT NULL UNIQUE,
    name VARCHAR(255),
    symbol VARCHAR(64),
    threat_type VARCHAR(64) NOT NULL,
    confidence INTEGER NOT NULL,
    description TEXT,
    source VARCHAR(255),
    first_seen BIGINT NOT NULL,
    is_active BOOLEAN DEFAULT TRUE,
    created_at TIMESTAMP WITH TIME ZONE DEFAULT NOW()
);

CREATE INDEX idx_scam_tokens_address ON scam_tokens(address);
CREATE INDEX idx_scam_tokens_type ON scam_tokens(threat_type);
CREATE INDEX idx_scam_tokens_active ON scam_tokens(is_active);

-- ============================================================================
-- WHALE TRANSACTIONS
// ============================================================================

CREATE TABLE IF NOT EXISTS whale_transactions (
    id BIGSERIAL PRIMARY KEY,
    transaction_hash VARCHAR(66) NOT NULL UNIQUE,
    from_address VARCHAR(42) NOT NULL,
    to_address VARCHAR(42),
    value NUMERIC(78, 0) NOT NULL,
    value_usd NUMERIC(78, 0),
    token_address VARCHAR(42),
    block_number BIGINT NOT NULL,
    whale_type VARCHAR(32),
    created_at TIMESTAMP WITH TIME ZONE DEFAULT NOW()
);

CREATE INDEX idx_whale_tx_hash ON whale_transactions(transaction_hash);
CREATE INDEX idx_whale_from ON whale_transactions(from_address);
CREATE INDEX idx_whale_value ON whale_transactions(value_usd DESC);
CREATE INDEX idx_whale_block ON whale_transactions(block_number DESC);

-- ============================================================================
-- DEX SWAPS
// ============================================================================

CREATE TABLE IF NOT EXISTS dex_swaps (
    id BIGSERIAL PRIMARY KEY,
    transaction_hash VARCHAR(66) NOT NULL,
    pair_address VARCHAR(42) NOT NULL,
    from_address VARCHAR(42) NOT NULL,
    to_address VARCHAR(42),
    from_token VARCHAR(42) NOT NULL,
    to_token VARCHAR(42) NOT NULL,
    from_amount NUMERIC(78, 0) NOT NULL,
    to_amount NUMERIC(78, 0) NOT NULL,
    path VARCHAR(42)[],
    block_number BIGINT NOT NULL,
    log_index INTEGER NOT NULL,
    created_at TIMESTAMP WITH TIME ZONE DEFAULT NOW()
);

CREATE INDEX idx_dex_swaps_tx ON dex_swaps(transaction_hash);
CREATE INDEX idx_dex_swaps_pair ON dex_swaps(pair_address);
CREATE INDEX idx_dex_swaps_from ON dex_swaps(from_address);
CREATE INDEX idx_dex_swaps_block ON dex_swaps(block_number DESC);

-- ============================================================================
-- CROSS-CHAIN TRANSFERS
// ============================================================================

CREATE TABLE IF NOT EXISTS cross_chain_transfers (
    id BIGSERIAL PRIMARY KEY,
    transaction_hash VARCHAR(66) NOT NULL UNIQUE,
    source_chain_id BIGINT NOT NULL,
    dest_chain_id BIGINT NOT NULL,
    from_address VARCHAR(42) NOT NULL,
    to_address VARCHAR(42) NOT NULL,
    token_address VARCHAR(42) NOT NULL,
    amount NUMERIC(78, 0) NOT NULL,
    fee NUMERIC(78, 0),
    status VARCHAR(32) NOT NULL DEFAULT 'pending',
    message_hash VARCHAR(66),
    block_number BIGINT NOT NULL,
    created_at TIMESTAMP WITH TIME ZONE DEFAULT NOW(),
    confirmed_at TIMESTAMP WITH TIME ZONE
);

CREATE INDEX idx_cross_chain_tx ON cross_chain_transfers(transaction_hash);
CREATE INDEX idx_cross_chain_from ON cross_chain_transfers(from_address);
CREATE INDEX idx_cross_chain_status ON cross_chain_transfers(status);
CREATE INDEX idx_cross_chain_block ON cross_chain_transfers(block_number DESC);

-- ============================================================================
-- ANALYTICS
// ============================================================================

CREATE TABLE IF NOT EXISTS analytics_daily (
    id BIGSERIAL PRIMARY KEY,
    date DATE NOT NULL UNIQUE,
    total_blocks BIGINT DEFAULT 0,
    total_transactions BIGINT DEFAULT 0,
    total_gas_used BIGINT DEFAULT 0,
    total_gas_fees NUMERIC(78, 0) DEFAULT 0,
    total_volume NUMERIC(78, 0) DEFAULT 0,
    avg_gas_price BIGINT DEFAULT 0,
    avg_block_time FLOAT,
    new_contracts BIGINT DEFAULT 0,
    new_tokens BIGINT DEFAULT 0,
    new_nfts BIGINT DEFAULT 0,
    created_at TIMESTAMP WITH TIME ZONE DEFAULT NOW()
);

CREATE INDEX idx_analytics_date ON analytics_daily(date DESC);

-- ============================================================================
-- FUNCTIONS AND PROCEDURES
// ============================================================================

-- Function to update token holder counts
CREATE OR REPLACE FUNCTION update_token_holders_count()
RETURNS TRIGGER AS $$
BEGIN
    UPDATE tokens 
    SET holders_count = (
        SELECT COUNT(DISTINCT address) 
        FROM token_holders 
        WHERE token_address = NEW.token_address 
        AND balance > 0
    )
    WHERE address = NEW.token_address;
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

-- Trigger to update token holders count
CREATE TRIGGER trigger_update_token_holders
AFTER INSERT ON token_holders
FOR EACH ROW
EXECUTE FUNCTION update_token_holders_count();

-- Function to update account nonce
CREATE OR REPLACE FUNCTION update_account_nonce()
RETURNS TRIGGER AS $$
BEGIN
    UPDATE accounts 
    SET nonce = NEW.nonce + 1,
        updated_at = NOW()
    WHERE address = NEW.from_address;
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

-- Trigger to update account nonce
CREATE TRIGGER trigger_update_nonce
AFTER INSERT ON transactions
FOR EACH ROW
WHEN (NEW.status = 'success')
EXECUTE FUNCTION update_account_nonce();

-- Function to get internal transactions
CREATE OR REPLACE FUNCTION get_internal_transactions(p_tx_hash VARCHAR)
RETURNS TABLE(
    depth INTEGER,
    call_type VARCHAR,
    from_address VARCHAR,
    to_address VARCHAR,
    value NUMERIC,
    input TEXT
) AS $$
BEGIN
    RETURN QUERY
    SELECT 
        t.depth,
        t.call_type::VARCHAR,
        t.from_address,
        t.to_address,
        t.value,
        t.input
    FROM internal_transactions t
    WHERE t.transaction_hash = p_tx_hash
    ORDER BY t.depth, t.transaction_index;
END;
$$ LANGUAGE plpgsql;

-- Function to get token transfers
CREATE OR REPLACE FUNCTION get_token_transfers(p_token_address VARCHAR, p_from_block BIGINT, p_to_block BIGINT)
RETURNS TABLE(
    hash VARCHAR,
    from_address VARCHAR,
    to_address VARCHAR,
    value NUMERIC,
    block_number BIGINT,
    timestamp BIGINT
) AS $$
BEGIN
    RETURN QUERY
    SELECT 
        t.hash,
        t.from_address,
        t.to_address,
        t.value,
        t.block_number,
        b.timestamp
    FROM token_transfers t
    JOIN blocks b ON b.number = t.block_number
    WHERE t.token_address = p_token_address
    AND t.block_number BETWEEN p_from_block AND p_to_block
    ORDER BY t.block_number DESC, t.log_index;
END;
$$ LANGUAGE plpgsql;

-- Function to get transaction with traces
CREATE OR REPLACE FUNCTION get_transaction_traces(p_tx_hash VARCHAR)
RETURNS TABLE(
    action JSONB,
    error TEXT
) AS $$
BEGIN
    RETURN QUERY
    SELECT 
        jsonb_build_object(
            'from', t.from_address,
            'to', t.to_address,
            'input', t.input,
            'output', t.output,
            'value', t.value,
            'gas', t.gas,
            'type', t.call_type
        )::JSONB,
        t.error
    FROM traces t
    WHERE t.transaction_hash = p_tx_hash
    ORDER BY t.depth;
END;
$$ LANGUAGE plpgsql;

-- Function to get account balance at block
CREATE OR REPLACE FUNCTION get_account_balance_at_block(p_address VARCHAR, p_block_number BIGINT)
RETURNS NUMERIC AS $$
DECLARE
    balance NUMERIC;
BEGIN
    SELECT sa.balance INTO balance
    FROM state_accounts sa
    WHERE sa.address = p_address
    AND sa.block_number <= p_block_number
    ORDER BY sa.block_number DESC
    LIMIT 1;
    
    RETURN COALESCE(balance, 0);
END;
$$ LANGUAGE plpgsql;

-- ============================================================================
-- VIEWS
-- ============================================================================

-- View for top token holders
CREATE OR REPLACE VIEW v_top_token_holders AS
SELECT 
    th.token_address,
    th.address,
    th.balance,
    th.percent_holdings,
    t.name AS token_name,
    t.symbol AS token_symbol,
    t.decimals AS token_decimals
FROM token_holders th
JOIN tokens t ON t.address = th.token_address
WHERE th.balance > 0
ORDER BY th.balance DESC;

-- View for recent transactions
CREATE OR REPLACE VIEW v_recent_transactions AS
SELECT 
    t.hash,
    t.from_address,
    t.to_address,
    t.value,
    t.gas_used,
    t.status,
    t.block_number,
    b.timestamp,
    t.input
FROM transactions t
JOIN blocks b ON b.number = t.block_number
WHERE t.status != 'pending'
ORDER BY b.timestamp DESC
LIMIT 100;

-- View for contract analytics
CREATE OR REPLACE VIEW v_contract_analytics AS
SELECT 
    c.address,
    c.contract_name,
    c.contract_type,
    c.is_verified,
    COUNT(t.hash) AS tx_count,
    MAX(b.timestamp) AS last_interaction
FROM contracts c
LEFT JOIN transactions t ON t.to_address = c.address
LEFT JOIN blocks b ON b.number = t.block_number
GROUP BY c.address, c.contract_name, c.contract_type, c.is_verified;

-- View for validator performance
CREATE OR REPLACE VIEW v_validator_performance AS
SELECT 
    v.address,
    v.moniker,
    v.self_delegation,
    v.delegation,
    v.total_stake,
    v.uptime,
    v.blocks_count,
    v.misses_count,
    CASE 
        WHEN v.blocks_count + v.misses_count > 0 
        THEN (v.blocks_count::FLOAT / (v.blocks_count + v.misses_count)::FLOAT) * 100
        ELSE 0
    END AS success_rate
FROM validators v
WHERE v.is_active = TRUE
ORDER BY v.total_stake DESC;

-- View for pending transactions with from address
CREATE OR REPLACE VIEW v_pending_transactions AS
SELECT 
    p.hash,
    p.nonce,
    p.from_address,
    p.to_address,
    p.value,
    p.gas_price,
    p.gas_limit,
    p.input,
    p.created_at
FROM pending_transactions p
ORDER BY p.gas_price DESC;

-- View for NFT holders
CREATE OR REPLACE VIEW v_nft_holders AS
SELECT 
    no.token_address,
    no.owner,
    COUNT(*) AS nft_count,
    nc.name AS collection_name,
    nc.symbol AS collection_symbol
FROM nft_owners no
JOIN nft_collections nc ON nc.address = no.token_address
GROUP BY no.token_address, no.owner, nc.name, nc.symbol;

-- ============================================================================
-- PARTITIONS (Optional for large datasets)
// ============================================================================

-- Partition transactions by month (example)
-- CREATE TABLE transactions_y2024 PARTITION OF transactions
--     FOR VALUES FROM (1704067200) TO (1735689600);

-- Partition token_prices by day (example)
-- CREATE TABLE token_prices_y2024 PARTITION OF token_prices
--     FOR VALUES FROM (20240101) TO (20250101);

-- ============================================================================
-- COMMENTS
// ============================================================================

COMMENT ON TABLE blocks IS 'All blocks on the blockchain';
COMMENT ON TABLE transactions IS 'All transactions on the blockchain';
COMMENT ON TABLE tokens IS 'TEP20 tokens';
COMMENT ON TABLE nft_collections IS 'NFT collections (TEP721/TEP1155)';
COMMENT ON TABLE nfts IS 'Individual NFTs';
COMMENT ON TABLE contracts IS 'Verified smart contracts';
COMMENT ON TABLE validators IS 'Network validators';
COMMENT ON TABLE traces IS 'Transaction traces for debugging';
COMMENT ON TABLE pending_transactions IS 'Pending transactions in mempool';
COMMENT ON TABLE malicious_contracts IS 'Known malicious contracts';
COMMENT ON TABLE phishing_urls IS 'Known phishing URLs';
COMMENT ON TABLE scam_tokens IS 'Known scam tokens';
COMMENT ON TABLE whale_transactions IS 'Whale transactions (> $10k)';

-- ============================================================================
-- ADDITIONAL TABLES
-- ============================================================================

-- DeFi TVL
CREATE TABLE IF NOT EXISTS defi_tvl (
    id BIGSERIAL PRIMARY KEY,
    protocol VARCHAR(128) NOT NULL,
    tvl NUMERIC(78, 2) NOT NULL DEFAULT 0,
    tvl_change_24h NUMERIC(10, 2),
    volume_24h NUMERIC(78, 2),
    updated_at TIMESTAMP WITH TIME ZONE DEFAULT NOW(),
    UNIQUE(protocol)
);

-- Lending Rates
CREATE TABLE IF NOT EXISTS lending_rates (
    id BIGSERIAL PRIMARY KEY,
    protocol VARCHAR(128) NOT NULL,
    token VARCHAR(42) NOT NULL,
    supply_rate NUMERIC(20, 8) NOT NULL DEFAULT 0,
    borrow_rate NUMERIC(20, 8) NOT NULL DEFAULT 0,
    utilization NUMERIC(5, 2) NOT NULL DEFAULT 0,
    total_supply NUMERIC(78, 0),
    total_borrow NUMERIC(78, 0),
    updated_at TIMESTAMP WITH TIME ZONE DEFAULT NOW()
);

-- Multi-chain configs
CREATE TABLE IF NOT EXISTS chain_configs (
    id BIGSERIAL PRIMARY KEY,
    chain_id BIGINT NOT NULL UNIQUE,
    chain_name VARCHAR(128) NOT NULL,
    symbol VARCHAR(32) NOT NULL,
    rpc_url TEXT,
    explorer_url TEXT,
    is_active BOOLEAN DEFAULT TRUE
);

-- API Usage
CREATE TABLE IF NOT EXISTS api_usage (
    id BIGSERIAL PRIMARY KEY,
    api_key_hash VARCHAR(64) NOT NULL,
    endpoint VARCHAR(128) NOT NULL,
    method VARCHAR(16) NOT NULL,
    status_code INTEGER NOT NULL,
    response_time_ms INTEGER,
    ip_address VARCHAR(45),
    user_agent TEXT,
    created_at TIMESTAMP WITH TIME ZONE DEFAULT NOW()
);

-- Audit Logs
CREATE TABLE IF NOT EXISTS audit_logs (
    id BIGSERIAL PRIMARY KEY,
    user_id BIGINT,
    action VARCHAR(128) NOT NULL,
    resource_type VARCHAR(64),
    resource_id VARCHAR(128),
    details JSONB,
    ip_address VARCHAR(45),
    created_at TIMESTAMP WITH TIME ZONE DEFAULT NOW()
);

-- =============================================================================
-- MISSING TABLES FOR COMPLETE FUNCTIONALITY
-- =============================================================================

-- Internal Transactions (from trace data)
CREATE TABLE IF NOT EXISTS internal_transactions (
    id BIGSERIAL PRIMARY KEY,
    transaction_hash VARCHAR(66) NOT NULL,
    block_number BIGINT NOT NULL,
    trace_index INTEGER NOT NULL,
    subtrace_index INTEGER DEFAULT 0,
    call_type VARCHAR(32) NOT NULL,
    from_address VARCHAR(42) NOT NULL,
    to_address VARCHAR(42),
    value NUMERIC(78, 0) DEFAULT 0,
    gas VARCHAR(32),
    gas_used VARCHAR(32),
    input TEXT,
    output TEXT,
    error TEXT,
    depth INTEGER NOT NULL DEFAULT 1,
    parent_trace_index INTEGER,
    creates VARCHAR(42),
    success BOOLEAN DEFAULT TRUE,
    created_at TIMESTAMP WITH TIME ZONE DEFAULT NOW()
);

CREATE INDEX idx_internal_tx_hash ON internal_transactions(transaction_hash);
CREATE INDEX idx_internal_tx_block ON internal_transactions(block_number);
CREATE INDEX idx_internal_tx_from ON internal_transactions(from_address);
CREATE INDEX idx_internal_tx_to ON internal_transactions(to_address);

-- Token Holders
CREATE TABLE IF NOT EXISTS token_holders (
    id BIGSERIAL PRIMARY KEY,
    token_address VARCHAR(42) NOT NULL,
    holder_address VARCHAR(42) NOT NULL,
    balance NUMERIC(78, 0) NOT NULL DEFAULT 0,
    percent NUMERIC(10, 6) DEFAULT 0,
    rank INTEGER DEFAULT 0,
    first_block BIGINT NOT NULL,
    last_updated TIMESTAMP WITH TIME ZONE DEFAULT NOW(),
    UNIQUE(token_address, holder_address)
);

CREATE INDEX idx_token_holders_token ON token_holders(token_address);
CREATE INDEX idx_token_holders_holder ON token_holders(holder_address);

-- Token Holder History
CREATE TABLE IF NOT EXISTS token_holder_history (
    id BIGSERIAL PRIMARY KEY,
    token_address VARCHAR(42) NOT NULL,
    holder_address VARCHAR(42) NOT NULL,
    balance NUMERIC(78, 0) NOT NULL,
    block_number BIGINT NOT NULL,
    timestamp BIGINT NOT NULL,
    created_at TIMESTAMP WITH TIME ZONE DEFAULT NOW()
);

-- Traces (raw trace data)
CREATE TABLE IF NOT EXISTS traces (
    id BIGSERIAL PRIMARY KEY,
    transaction_hash VARCHAR(66) NOT NULL,
    block_number BIGINT NOT NULL,
    trace_address TEXT NOT NULL,
    trace_type VARCHAR(32) NOT NULL,
    call_type VARCHAR(32),
    from_address VARCHAR(42) NOT NULL,
    to_address VARCHAR(42),
    value NUMERIC(78, 0) DEFAULT 0,
    gas VARCHAR(32),
    gas_used VARCHAR(32),
    input TEXT,
    output TEXT,
    error TEXT,
    subtraces INTEGER DEFAULT 0,
    trace_id TEXT,
    created_at TIMESTAMP WITH TIME ZONE DEFAULT NOW()
);

CREATE INDEX idx_traces_hash ON traces(transaction_hash);
CREATE INDEX idx_traces_block ON traces(block_number);

-- State Diffs (balance/storage changes)
CREATE TABLE IF NOT EXISTS state_diffs (
    id BIGSERIAL PRIMARY KEY,
    transaction_hash VARCHAR(66),
    block_number BIGINT NOT NULL,
    address VARCHAR(42) NOT NULL,
    storage_key VARCHAR(66),
    storage_value VARCHAR(66),
    old_value TEXT,
    new_value TEXT,
    diff_type VARCHAR(32) NOT NULL,
    created_at TIMESTAMP WITH TIME ZONE DEFAULT NOW()
);

CREATE INDEX idx_state_diffs_block ON state_diffs(block_number);
CREATE INDEX idx_state_diffs_address ON state_diffs(address);

-- Governance Proposals
CREATE TABLE IF NOT EXISTS governance_proposals (
    id BIGSERIAL PRIMARY KEY,
    proposal_id VARCHAR(64) NOT NULL UNIQUE,
    contract_address VARCHAR(42) NOT NULL,
    proposer VARCHAR(42) NOT NULL,
    title TEXT NOT NULL,
    description TEXT,
    status VARCHAR(32) NOT NULL,
    for_votes NUMERIC(78, 0) DEFAULT 0,
    against_votes NUMERIC(78, 0) DEFAULT 0,
    abstain_votes NUMERIC(78, 0) DEFAULT 0,
    total_votes NUMERIC(78, 0) DEFAULT 0,
    start_block BIGINT NOT NULL,
    end_block BIGINT NOT NULL,
    created_at TIMESTAMP WITH TIME ZONE DEFAULT NOW(),
    updated_at TIMESTAMP WITH TIME ZONE DEFAULT NOW()
);

-- Governance Votes
CREATE TABLE IF NOT EXISTS governance_votes (
    id BIGSERIAL PRIMARY KEY,
    proposal_id VARCHAR(64) NOT NULL,
    voter VARCHAR(42) NOT NULL,
    vote_choice VARCHAR(16) NOT NULL,
    votes NUMERIC(78, 0) NOT NULL,
    block_number BIGINT NOT NULL,
    transaction_hash VARCHAR(66) NOT NULL,
    created_at TIMESTAMP WITH TIME ZONE DEFAULT NOW()
);

-- MEV Bundles
CREATE TABLE IF NOT EXISTS mev_bundles (
    id BIGSERIAL PRIMARY KEY,
    bundle_hash VARCHAR(66) NOT NULL UNIQUE,
    block_number BIGINT NOT NULL,
    sender VARCHAR(42) NOT NULL,
    mev_type VARCHAR(32) NOT NULL,
    tx_hashes TEXT NOT NULL,
    gas_used BIGINT,
    profit_eth NUMERIC(78, 0),
    profit_usd NUMERIC(20, 2),
    inserted_at TIMESTAMP WITH TIME ZONE DEFAULT NOW()
);

-- NFT Floor Prices
CREATE TABLE IF NOT EXISTS nft_floor_prices (
    id BIGSERIAL PRIMARY KEY,
    collection_address VARCHAR(42) NOT NULL,
    floor_price NUMERIC(78, 0),
    floor_price_usd NUMERIC(20, 2),
    volume_24h NUMERIC(78, 0),
    volume_24h_usd NUMERIC(20, 2),
    sales_24h INTEGER DEFAULT 0,
    holders INTEGER DEFAULT 0,
    updated_at TIMESTAMP WITH TIME ZONE DEFAULT NOW(),
    UNIQUE(collection_address)
);

-- NFT Rarity
CREATE TABLE IF NOT EXISTS nft_rarity (
    id BIGSERIAL PRIMARY KEY,
    collection_address VARCHAR(42) NOT NULL,
    token_id VARCHAR(78) NOT NULL,
    rarity_score NUMERIC(10, 4),
    rank INTEGER,
    traits JSONB,
    updated_at TIMESTAMP WITH TIME ZONE DEFAULT NOW(),
    UNIQUE(collection_address, token_id)
);