-- TigerScan Database Migrations - Missing Tables
-- This migration adds all missing tables identified in DETAILED_MISSING_GAPS.md
-- Version: 001_add_missing_tables.sql
-- Created: 2026-06-12

BEGIN;

-- ============================================
-- PART 1: TOKEN PRICE TABLES
-- ============================================

-- Token prices historical data
CREATE TABLE IF NOT EXISTS token_prices (
    id SERIAL PRIMARY KEY,
    token_address VARCHAR(42) NOT NULL,
    price_usd NUMERIC(78, 18) NOT NULL,
    price_eth NUMERIC(78, 18),
    market_cap NUMERIC(78, 18),
    volume_24h NUMERIC(78, 18),
    liquidity NUMERIC(78, 18),
    price_change_1h NUMERIC(20, 8),
    price_change_24h NUMERIC(20, 8),
    price_change_7d NUMERIC(20, 8),
    market_cap_rank INT,
    timestamp TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_token_prices_token_time 
    ON token_prices(token_address, timestamp DESC);
CREATE INDEX IF NOT EXISTS idx_token_prices_price_change_24h 
    ON token_prices(price_change_24h) WHERE price_change_24h IS NOT NULL;

-- Price feed configurations
CREATE TABLE IF NOT EXISTS price_feeds (
    id SERIAL PRIMARY KEY,
    name VARCHAR(100) NOT NULL,
    token_address VARCHAR(42) NOT NULL,
    source VARCHAR(50) NOT NULL,
    source_id VARCHAR(100),
    refresh_interval INT NOT NULL DEFAULT 60,
    is_active BOOLEAN NOT NULL DEFAULT TRUE,
    last_price NUMERIC(78, 18),
    last_update TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    CONSTRAINT uk_price_feeds_token UNIQUE(token_address)
);

-- ============================================
-- PART 2: NFT TABLES
-- ============================================

-- NFT floor price history
CREATE TABLE IF NOT EXISTS nft_floor_prices (
    id SERIAL PRIMARY KEY,
    collection_address VARCHAR(42) NOT NULL,
    token_id VARCHAR(78),
    floor_price NUMERIC(78, 18) NOT NULL,
    floor_price_usd NUMERIC(78, 18),
    volume_24h NUMERIC(78, 18),
    sales_count INT DEFAULT 0,
    avg_price NUMERIC(78, 18),
    timestamp TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_nft_floor_prices_collection 
    ON nft_floor_prices(collection_address, timestamp DESC);

-- NFT metadata cache
CREATE TABLE IF NOT EXISTS nft_metadata_cache (
    id SERIAL PRIMARY KEY,
    collection_address VARCHAR(42) NOT NULL,
    token_id VARCHAR(78) NOT NULL,
    name VARCHAR(500),
    description TEXT,
    image_url TEXT,
    animation_url TEXT,
    external_url TEXT,
    attributes JSONB,
    metadata_url TEXT,
    metadata_hash VARCHAR(66),
    last_synced TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    CONSTRAINT uk_nft_token UNIQUE(collection_address, token_id)
);

CREATE INDEX IF NOT EXISTS idx_nft_metadata_attributes 
    ON nft_metadata_cache USING GIN(attributes);
CREATE INDEX IF NOT EXISTS idx_nft_metadata_url 
    ON nft_metadata_cache(metadata_url);

-- NFT owner history
CREATE TABLE IF NOT EXISTS nft_owner_history (
    id SERIAL PRIMARY KEY,
    collection_address VARCHAR(42) NOT NULL,
    token_id VARCHAR(78) NOT NULL,
    from_address VARCHAR(42),
    to_address VARCHAR(42) NOT NULL,
    transaction_hash VARCHAR(66) NOT NULL,
    block_number BIGINT NOT NULL,
    timestamp TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_nft_owner_history_collection 
    ON nft_owner_history(collection_address, token_id, block_number DESC);
CREATE INDEX IF NOT EXISTS idx_nft_owner_history_to 
    ON nft_owner_history(to_address);

-- NFT royalties (EIP-2981)
CREATE TABLE IF NOT EXISTS nft_royalties (
    id SERIAL PRIMARY KEY,
    collection_address VARCHAR(42) NOT NULL,
    recipient_address VARCHAR(42) NOT NULL,
    royalty_percentage NUMERIC(5, 2) NOT NULL,
    royalty_bps INT NOT NULL,
    is_verified BOOLEAN NOT NULL DEFAULT FALSE,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    CONSTRAINT uk_nft_royalties_collection UNIQUE(collection_address)
);

-- ============================================
-- PART 3: ADDRESS LABELING
-- ============================================

-- Address labels
CREATE TABLE IF NOT EXISTS address_labels (
    id SERIAL PRIMARY KEY,
    address VARCHAR(42) NOT NULL,
    label VARCHAR(200) NOT NULL,
    label_type VARCHAR(50) NOT NULL,
    label_subtype VARCHAR(100),
    label_source VARCHAR(50),
    is_scam BOOLEAN NOT NULL DEFAULT FALSE,
    is_verified BOOLEAN NOT NULL DEFAULT FALSE,
    verification_proof TEXT,
    notes TEXT,
    created_by VARCHAR(42),
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    CONSTRAINT uk_address_labels_address UNIQUE(address)
);

CREATE INDEX IF NOT EXISTS idx_address_labels_label_type 
    ON address_labels(label_type);
CREATE INDEX IF NOT EXISTS idx_address_labels_is_scam 
    ON address_labels(is_scam) WHERE is_scam = TRUE;

-- Address taggings
CREATE TABLE IF NOT EXISTS address_taggings (
    id SERIAL PRIMARY KEY,
    address VARCHAR(42) NOT NULL,
    tag VARCHAR(100) NOT NULL,
    tag_category VARCHAR(50),
    created_by VARCHAR(42) NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    CONSTRAINT uk_address_tagging UNIQUE(address, tag)
);

CREATE INDEX IF NOT EXISTS idx_address_taggings_tag 
    ON address_taggings(tag);

-- ============================================
-- PART 4: SECURITY TABLES
-- ============================================

-- Phishing reports
CREATE TABLE IF NOT EXISTS phishing_reports (
    id SERIAL PRIMARY KEY,
    reported_address VARCHAR(42) NOT NULL,
    reporter_address VARCHAR(42),
    report_type VARCHAR(50) NOT NULL,
    description TEXT NOT NULL,
    transaction_hash VARCHAR(66),
    evidence_urls TEXT[],
    status VARCHAR(20) NOT NULL DEFAULT 'pending',
    resolved_at TIMESTAMPTZ,
    resolved_by VARCHAR(42),
    resolution_notes TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_phishing_reports_status 
    ON phishing_reports(status);
CREATE INDEX IF NOT EXISTS idx_phishing_reports_address 
    ON phishing_reports(reported_address);

-- API usage tracking
CREATE TABLE IF NOT EXISTS api_usage (
    id SERIAL PRIMARY KEY,
    api_key VARCHAR(64) NOT NULL,
    endpoint VARCHAR(200) NOT NULL,
    method VARCHAR(10) NOT NULL,
    status_code INT NOT NULL,
    response_time_ms INT,
    bytes_sent BIGINT DEFAULT 0,
    ip_address VARCHAR(45),
    user_agent TEXT,
    timestamp TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_api_usage_key_time 
    ON api_usage(api_key, timestamp DESC);
CREATE INDEX IF NOT EXISTS idx_api_usage_endpoint_time 
    ON api_usage(endpoint, timestamp DESC);

-- API keys management
CREATE TABLE IF NOT EXISTS api_keys (
    id SERIAL PRIMARY KEY,
    key_hash VARCHAR(64) NOT NULL,
    key_prefix VARCHAR(8) NOT NULL,
    user_id VARCHAR(42),
    label VARCHAR(100),
    rate_limit INT NOT NULL DEFAULT 1000,
    rate_limit_window INT NOT NULL DEFAULT 86400,
    is_active BOOLEAN NOT NULL DEFAULT TRUE,
    is_pro BOOLEAN NOT NULL DEFAULT FALSE,
    daily_usage BIGINT DEFAULT 0,
    last_used TIMESTAMPTZ,
    expires_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    CONSTRAINT uk_api_keys_hash UNIQUE(key_hash)
);

-- IP blocking
CREATE TABLE IF NOT EXISTS ip_blocklist (
    id SERIAL PRIMARY KEY,
    ip_address VARCHAR(45) NOT NULL,
    ip_range CIDR,
    reason TEXT,
    blocked_until TIMESTAMPTZ,
    is_permanent BOOLEAN NOT NULL DEFAULT FALSE,
    block_count INT DEFAULT 0,
    last_blocked TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_ip_blocklist_address 
    ON ip_blocklist(ip_address);
CREATE INDEX IF NOT EXISTS idx_ip_blocklist_until 
    ON ip_blocklist(blocked_until) WHERE blocked_until IS NOT NULL;

-- ============================================
-- PART 5: INTERNAL TRANSACTIONS
-- ============================================

-- Internal transactions
CREATE TABLE IF NOT EXISTS internal_transactions (
    id SERIAL PRIMARY KEY,
    transaction_hash VARCHAR(66) NOT NULL,
    block_number BIGINT NOT NULL,
    trace_address VARCHAR(200) NOT NULL,
    call_type VARCHAR(20) NOT NULL,
    from_address VARCHAR(42) NOT NULL,
    to_address VARCHAR(42),
    value NUMERIC(78, 0),
    input_data TEXT,
    output_data TEXT,
    gas INT,
    gas_used INT,
    revert_reason TEXT,
    depth INT NOT NULL DEFAULT 0,
    timestamp TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_internal_tx_hash 
    ON internal_transactions(transaction_hash);
CREATE INDEX IF NOT EXISTS idx_internal_tx_block 
    ON internal_transactions(block_number);

-- Uncle blocks
CREATE TABLE IF NOT EXISTS uncle_blocks (
    id SERIAL PRIMARY KEY,
    hash VARCHAR(66) NOT NULL,
    number INT NOT NULL,
    parent_hash VARCHAR(66) NOT NULL,
    miner VARCHAR(42) NOT NULL,
    gas_used BIGINT,
    gas_limit BIGINT,
    difficulty BIGINT,
    total_difficulty BIGINT,
    size INT,
    timestamp TIMESTAMPTZ NOT NULL,
    block_reward NUMERIC(78, 0),
    uncle_reward NUMERIC(78, 0),
    nonce VARCHAR(66),
    extra_data TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    CONSTRAINT uk_uncle_hash UNIQUE(hash),
    CONSTRAINT uk_uncle_number UNIQUE(number)
);

-- ============================================
-- PART 6: NOTIFICATIONS & WEBHOOKS
-- ============================================

-- Webhook events
CREATE TABLE IF NOT EXISTS webhook_events (
    id SERIAL PRIMARY KEY,
    url TEXT NOT NULL,
    event_type VARCHAR(50) NOT NULL,
    filters JSONB,
    is_active BOOLEAN NOT NULL DEFAULT TRUE,
    secret_hash VARCHAR(64),
    failure_count INT DEFAULT 0,
    last_triggered TIMESTAMPTZ,
    created_by VARCHAR(42) NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- User notifications
CREATE TABLE IF NOT EXISTS notifications (
    id SERIAL PRIMARY KEY,
    user_id VARCHAR(42) NOT NULL,
    notification_type VARCHAR(50) NOT NULL,
    title VARCHAR(200) NOT NULL,
    message TEXT NOT NULL,
    link VARCHAR(500),
    is_read BOOLEAN NOT NULL DEFAULT FALSE,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_notifications_user_read 
    ON notifications(user_id, is_read, created_at DESC);

-- User preferences
CREATE TABLE IF NOT EXISTS user_preferences (
    id SERIAL PRIMARY KEY,
    user_id VARCHAR(42) NOT NULL,
    preference_key VARCHAR(100) NOT NULL,
    preference_value TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    CONSTRAINT uk_user_preference UNIQUE(user_id, preference_key)
);

-- ============================================
-- PART 7: VERIFICATION & CONTRACTS
-- ============================================

-- Contract sources
CREATE TABLE IF NOT EXISTS contract_sources (
    id SERIAL PRIMARY KEY,
    address VARCHAR(42) NOT NULL,
    name VARCHAR(200) NOT NULL,
    compiler_version VARCHAR(50) NOT NULL,
    optimization_enabled BOOLEAN NOT NULL DEFAULT TRUE,
    optimization_runs INT DEFAULT 200,
    source_code TEXT NOT NULL,
    abi JSONB,
    constructor_args TEXT,
    evm_version VARCHAR(20),
    library_links JSONB,
    is_proxy BOOLEAN NOT NULL DEFAULT FALSE,
    implementation_address VARCHAR(42),
    proxy_admin VARCHAR(42),
    verified_at TIMESTAMPTZ,
    verified_by VARCHAR(42),
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    CONSTRAINT uk_contract_source UNIQUE(address)
);

CREATE INDEX IF NOT EXISTS idx_contract_sources_proxy 
    ON contract_sources(is_proxy) WHERE is_proxy = TRUE;

-- ============================================
-- PART 8: STAKING & DELEGATION
-- ============================================

-- Staking delegations
CREATE TABLE IF NOT EXISTS staking_delegations (
    id SERIAL PRIMARY KEY,
    delegator_address VARCHAR(42) NOT NULL,
    validator_address VARCHAR(42) NOT NULL,
    amount NUMERIC(78, 0) NOT NULL,
    reward NUMERIC(78, 0),
    is_compound BOOLEAN NOT NULL DEFAULT FALSE,
    undelegation_request_height BIGINT,
    undelegation_complete_height BIGINT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_staking_delegations_delegator 
    ON staking_delegations(delegator_address);
CREATE INDEX IF NOT EXISTS idx_staking_delegations_validator 
    ON staking_delegations(validator_address);

-- Governance proposals
CREATE TABLE IF NOT EXISTS governance_proposals (
    id SERIAL PRIMARY KEY,
    proposal_id INT NOT NULL,
    title VARCHAR(500) NOT NULL,
    description TEXT NOT NULL,
    proposer VARCHAR(42) NOT NULL,
    status VARCHAR(20) NOT NULL DEFAULT 'pending',
    for_votes NUMERIC(78, 0) DEFAULT 0,
    against_votes NUMERIC(78, 0) DEFAULT 0,
    abstain_votes NUMERIC(78, 0) DEFAULT 0,
    start_block BIGINT,
    end_block BIGINT,
    execution_target TEXT,
    execution_data TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_governance_proposals_status 
    ON governance_proposals(status);

-- Governance votes
CREATE TABLE IF NOT EXISTS governance_votes (
    id SERIAL PRIMARY KEY,
    proposal_id INT NOT NULL,
    voter_address VARCHAR(42) NOT NULL,
    support BOOLEAN NOT NULL,
    votes NUMERIC(78, 0) NOT NULL,
    timestamp TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    CONSTRAINT uk_proposal_voter UNIQUE(proposal_id, voter_address)
);

CREATE INDEX IF NOT EXISTS idx_governance_votes_proposal 
    ON governance_votes(proposal_id);

-- ============================================
-- PART 9: DEX & TRADING
-- ============================================

-- DEX pairs
CREATE TABLE IF NOT EXISTS dex_pairs (
    id SERIAL PRIMARY KEY,
    pair_address VARCHAR(42) NOT NULL,
    dex_name VARCHAR(50) NOT NULL,
    token0_address VARCHAR(42) NOT NULL,
    token1_address VARCHAR(42) NOT NULL,
    reserve0 NUMERIC(78, 0),
    reserve1 NUMERIC(78, 0),
    total_supply NUMERIC(78, 0),
    liquidity_usd NUMERIC(78, 18),
    volume_24h NUMERIC(78, 18),
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    CONSTRAINT uk_dex_pair UNIQUE(pair_address)
);

-- DEX pools
CREATE TABLE IF NOT EXISTS dex_pools (
    id SERIAL PRIMARY KEY,
    pool_address VARCHAR(42) NOT NULL,
    dex_name VARCHAR(50) NOT NULL,
    lp_token_address VARCHAR(42) NOT NULL,
    tokens JSONB NOT NULL,
    weights JSONB,
    pool_type VARCHAR(50),
    total_value_locked NUMERIC(78, 18),
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    CONSTRAINT uk_dex_pool UNIQUE(pool_address)
);

-- ============================================
-- PART 10: GAS & NETWORK ANALYTICS
-- ============================================

-- Gas price history
CREATE TABLE IF NOT EXISTS gas_price_history (
    id SERIAL PRIMARY KEY,
    low_gas_price BIGINT NOT NULL,
    medium_gas_price BIGINT NOT NULL,
    high_gas_price BIGINT NOT NULL,
    base_fee_per_gas BIGINT,
    priority_fee_avg BIGINT,
    network_utilization NUMERIC(5, 2),
    pending_tx_count BIGINT,
    timestamp TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_gas_price_history_timestamp 
    ON gas_price_history(timestamp DESC);

-- Network stats snapshots
CREATE TABLE IF NOT EXISTS network_stats (
    id SERIAL PRIMARY KEY,
    block_number BIGINT NOT NULL,
    total_transactions BIGINT NOT NULL,
    total_blocks BIGINT NOT NULL,
    total_accounts BIGINT NOT NULL,
    avg_gas_price BIGINT,
    avg_gas_used BIGINT,
    tps NUMERIC(10, 2),
    finality_seconds INT,
    active_validators INT,
    total_staked NUMERIC(78, 0),
    timestamp TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_network_stats_timestamp 
    ON network_stats(timestamp DESC);

COMMIT;