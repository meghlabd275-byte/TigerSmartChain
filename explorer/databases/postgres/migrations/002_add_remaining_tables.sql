-- TigerScan Database Migrations - Additional Tables
-- Version: 002_add_remaining_tables.sql
-- Created: 2026-06-12

BEGIN;

-- ============================================
-- PENDING TRANSACTIONS
-- ============================================

CREATE TABLE IF NOT EXISTS pending_transactions (
    id SERIAL PRIMARY KEY,
    hash VARCHAR(66) NOT NULL,
    from_address VARCHAR(42) NOT NULL,
    to_address VARCHAR(42),
    value NUMERIC(78, 0),
    gas_price BIGINT,
    gas_limit BIGINT,
    nonce BIGINT,
    input_data TEXT,
    first_seen TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    mined_at TIMESTAMPTZ,
    gas_used BIGINT,
    status VARCHAR(20) NOT NULL DEFAULT 'pending',
    CONSTRAINT uk_pending_tx UNIQUE(hash)
);

CREATE INDEX IF NOT EXISTS idx_pending_from ON pending_transactions(from_address);
CREATE INDEX IF NOT EXISTS idx_pending_status ON pending_transactions(status);

-- ============================================
-- TOKEN APPROVALS & ALLOWANCES
-- ============================================

CREATE TABLE IF NOT EXISTS token_approvals (
    id SERIAL PRIMARY KEY,
    hash VARCHAR(66) NOT NULL,
    block_number BIGINT NOT NULL,
    transaction_hash VARCHAR(66) NOT NULL,
    token_address VARCHAR(42) NOT NULL,
    owner_address VARCHAR(42) NOT NULL,
    spender_address VARCHAR(42) NOT NULL,
    value NUMERIC(78, 0) NOT NULL,
    is_increase BOOLEAN NOT NULL DEFAULT TRUE,
    timestamp TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    CONSTRAINT uk_token_approval UNIQUE(hash)
);

CREATE TABLE IF NOT EXISTS token_allowances (
    id SERIAL PRIMARY KEY,
    token_address VARCHAR(42) NOT NULL,
    owner_address VARCHAR(42) NOT NULL,
    spender_address VARCHAR(42) NOT NULL,
    value NUMERIC(78, 0) NOT NULL,
    last_update TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    CONSTRAINT uk_token_allowance UNIQUE(token_address, owner_address, spender_address)
);

CREATE INDEX IF NOT EXISTS idx_approvals_token ON token_approvals(token_address);
CREATE INDEX IF NOT EXISTS idx_approvals_owner ON token_approvals(owner_address);
CREATE INDEX IF NOT EXISTS idx_allowances_spender ON token_allowances(spender_address);

-- ============================================
-- NFT ROYALTIES (EIP-2981)
-- ============================================

CREATE TABLE IF NOT EXISTS nft_collections (
    id SERIAL PRIMARY KEY,
    address VARCHAR(42) NOT NULL,
    name VARCHAR(200) NOT NULL,
    symbol VARCHAR(50),
    description TEXT,
    image_url TEXT,
    banner_url TEXT,
    external_url TEXT,
    contract_type VARCHAR(20) NOT NULL, -- 'ERC721', 'ERC1155'
    total_supply BIGINT DEFAULT 0,
    holders_count BIGINT DEFAULT 0,
    items_count BIGINT DEFAULT 0,
    is_verified BOOLEAN NOT NULL DEFAULT FALSE,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    CONSTRAINT uk_nft_collection UNIQUE(address)
);

CREATE TABLE IF NOT EXISTS nft_owners (
    id SERIAL PRIMARY KEY,
    collection_address VARCHAR(42) NOT NULL,
    token_id VARCHAR(78) NOT NULL,
    owner_address VARCHAR(42) NOT NULL,
    balance INT DEFAULT 1,
    last_update TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    CONSTRAINT uk_nft_owner UNIQUE(collection_address, token_id, owner_address)
);

CREATE INDEX IF NOT EXISTS idx_nft_owners_address ON nft_owners(owner_address);

-- ============================================
-- CONTRACT SOURCES & BYTECODE
-- ============================================

CREATE TABLE IF NOT EXISTS contract_compilations (
    id SERIAL PRIMARY KEY,
    address VARCHAR(42) NOT NULL,
    name VARCHAR(200) NOT NULL,
    compiler VARCHAR(50) NOT NULL,
    version VARCHAR(50) NOT NULL,
    optimization BOOLEAN NOT NULL DEFAULT TRUE,
    runs INT DEFAULT 200,
    source_code TEXT NOT NULL,
    abi JSONB,
    bytecode TEXT,
    runtime_bytecode TEXT,
    compilation_errors TEXT,
    verified_at TIMESTAMPTZ,
    verified_by VARCHAR(42),
    CONSTRAINT uk_compilation UNIQUE(address)
);

CREATE TABLE IF NOT EXISTS contract_checksums (
    id SERIAL PRIMARY KEY,
    address VARCHAR(42) NOT NULL,
    source_hash VARCHAR(66),
    creation_code_hash VARCHAR(66),
    runtime_code_hash VARCHAR(66),
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    CONSTRAINT uk_contract_checksum UNIQUE(address)
);

-- ============================================
-- BLOCK REWARDS
-- ============================================

CREATE TABLE IF NOT EXISTS block_rewards (
    id SERIAL PRIMARY KEY,
    block_number BIGINT NOT NULL,
    miner VARCHAR(42) NOT NULL,
    block_reward NUMERIC(78, 0) NOT NULL,
    uncle_reward NUMERIC(78, 0) DEFAULT 0,
    total_reward NUMERIC(78, 0) NOT NULL,
    timestamp TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    CONSTRAINT uk_block_reward UNIQUE(block_number)
);

CREATE INDEX IF NOT EXISTS idx_block_rewards_miner ON block_rewards(miner);

-- ============================================
-- STAKING & VALIDATORS
-- ============================================

CREATE TABLE IF NOT EXISTS staking_pools (
    id SERIAL PRIMARY KEY,
    validator_address VARCHAR(42) NOT NULL,
    name VARCHAR(200),
    commission_rate NUMERIC(5, 2),
    delegator_count BIGINT DEFAULT 0,
    total_staked NUMERIC(78, 0) DEFAULT 0,
    rewards_accumulated NUMERIC(78, 0) DEFAULT 0,
    is_active BOOLEAN NOT NULL DEFAULT TRUE,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    CONSTRAINT uk_staking_pool UNIQUE(validator_address)
);

CREATE INDEX IF NOT EXISTS idx_staking_pools_active ON staking_pools(is_active);

-- ============================================
-- GOVERNANCE
-- ============================================

CREATE TABLE IF NOT EXISTS governance_timelocks (
    id SERIAL PRIMARY KEY,
    address VARCHAR(42) NOT NULL,
    name VARCHAR(200) NOT NULL,
    delay BIGINT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    CONSTRAINT uk_timelock UNIQUE(address)
);

CREATE TABLE IF NOT EXISTS governance_voters (
    id SERIAL PRIMARY KEY,
    proposal_id INT NOT NULL,
    voter_address VARCHAR(42) NOT NULL,
    support BOOLEAN NOT NULL,
    votes NUMERIC(78, 0) NOT NULL,
    timestamp TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    CONSTRAINT uk_proposal_voter UNIQUE(proposal_id, voter_address)
);

-- ============================================
-- DEX DATA
-- ============================================

CREATE TABLE IF NOT EXISTS dex_token_pairs (
    id SERIAL PRIMARY KEY,
    pair_address VARCHAR(42) NOT NULL,
    dex_name VARCHAR(50) NOT NULL,
    token0_address VARCHAR(42) NOT NULL,
    token1_address VARCHAR(42) NOT NULL,
    reserve0 NUMERIC(78, 0),
    reserve1 NUMERIC(78, 0),
    liquidity_usd NUMERIC(78, 18),
    volume_24h NUMERIC(78, 18),
    volume_change_24h NUMERIC(20, 8),
    last_update TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    CONSTRAINT uk_dex_pair UNIQUE(pair_address)
);

-- ============================================
-- NETWORK STATS
-- ============================================

CREATE TABLE IF NOT EXISTS network_stats_history (
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

CREATE INDEX IF NOT EXISTS idx_network_stats_timestamp ON network_stats_history(timestamp DESC);

-- ============================================
-- PHISHING & SECURITY
-- ============================================

CREATE TABLE IF NOT EXISTS verified_addresses (
    id SERIAL PRIMARY KEY,
    address VARCHAR(42) NOT NULL,
    name VARCHAR(200) NOT NULL,
    category VARCHAR(50) NOT NULL,
    website TEXT,
    is_verified BOOLEAN NOT NULL DEFAULT FALSE,
    verification_proof TEXT,
    first_seen TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    CONSTRAINT uk_verified_address UNIQUE(address)
);

CREATE TABLE IF NOT EXISTS security_alerts (
    id SERIAL PRIMARY KEY,
    alert_type VARCHAR(50) NOT NULL,
    severity VARCHAR(20) NOT NULL,
    address VARCHAR(42),
    description TEXT NOT NULL,
    resolution TEXT,
    is_resolved BOOLEAN NOT NULL DEFAULT FALSE,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    resolved_at TIMESTAMPTZ
);

CREATE INDEX IF NOT EXISTS idx_security_alerts_type ON security_alerts(alert_type);
CREATE INDEX IF NOT EXISTS idx_security_alerts_severity ON security_alerts(severity);

-- ============================================
-- EXPORT API
-- ============================================

CREATE TABLE IF NOT EXISTS export_jobs (
    id SERIAL PRIMARY KEY,
    user_id VARCHAR(42),
    export_type VARCHAR(50) NOT NULL, -- 'transactions', 'tokens', 'nfts'
    format VARCHAR(10) NOT NULL, -- 'csv', 'json'
    filters JSONB,
    status VARCHAR(20) NOT NULL DEFAULT 'pending',
    file_url TEXT,
    record_count BIGINT DEFAULT 0,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    completed_at TIMESTAMPTZ
);

CREATE INDEX IF NOT EXISTS idx_export_jobs_status ON export_jobs(status);
CREATE INDEX IF NOT EXISTS idx_export_jobs_user ON export_jobs(user_id);

-- ============================================
-- BATCH API
-- ============================================

CREATE TABLE IF NOT EXISTS batch_requests (
    id SERIAL PRIMARY KEY,
    request_id VARCHAR(36) NOT NULL,
    requests JSONB NOT NULL,
    responses JSONB,
    status VARCHAR(20) NOT NULL DEFAULT 'processing',
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    completed_at TIMESTAMPTZ,
    CONSTRAINT uk_batch_request UNIQUE(request_id)
);

-- ============================================
-- PRO API
-- ============================================

CREATE TABLE IF NOT EXISTS pro_api_keys (
    id SERIAL PRIMARY KEY,
    key_hash VARCHAR(64) NOT NULL,
    key_prefix VARCHAR(8) NOT NULL,
    user_id VARCHAR(42) NOT NULL,
    plan VARCHAR(20) NOT NULL DEFAULT 'free', -- 'free', 'pro', 'enterprise'
    rate_limit BIGINT NOT NULL DEFAULT 1000,
    rate_limit_period BIGINT NOT NULL DEFAULT 86400,
    is_active BOOLEAN NOT NULL DEFAULT TRUE,
    expires_at TIMESTAMPTZ,
    last_used TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    CONSTRAINT uk_pro_api_key UNIQUE(key_hash)
);

-- ============================================
-- MULTI-CHAIN
-- ============================================

CREATE TABLE IF NOT EXISTS chain_configs (
    id SERIAL PRIMARY KEY,
    chain_id INT NOT NULL,
    name VARCHAR(50) NOT NULL,
    symbol VARCHAR(10) NOT NULL,
    rpc_url TEXT,
    explorer_url TEXT,
    is_active BOOLEAN NOT NULL DEFAULT TRUE,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    CONSTRAINT uk_chain_config UNIQUE(chain_id)
);

COMMIT;