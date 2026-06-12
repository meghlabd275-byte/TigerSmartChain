-- PostgreSQL Schema for TigerScan Explorer
-- Version: 1.0.0

-- ============================================
-- ACCOUNTS TABLE
-- ============================================
CREATE TABLE IF NOT EXISTS accounts (
    id BIGSERIAL PRIMARY KEY,
    address VARCHAR(42) NOT NULL UNIQUE,
    balance VARCHAR(66) DEFAULT '0',
    nonce INTEGER DEFAULT 0,
    code_hash VARCHAR(66),
    code_length INTEGER DEFAULT 0,
    is_contract BOOLEAN DEFAULT FALSE,
    is_verified BOOLEAN DEFAULT FALSE,
    is_self_destructed BOOLEAN DEFAULT FALSE,
    first_block_number BIGINT,
    last_block_number BIGINT,
    created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP
);

CREATE INDEX idx_accounts_address ON accounts(address);
CREATE INDEX idx_accounts_is_contract ON accounts(is_contract);
CREATE INDEX idx_accounts_is_verified ON accounts(is_verified);
CREATE INDEX idx_accounts_balance ON accounts(balance DESC);

-- ============================================
-- BLOCKS TABLE
-- ============================================
CREATE TABLE IF NOT EXISTS blocks (
    id BIGSERIAL PRIMARY KEY,
    number BIGINT NOT NULL UNIQUE,
    hash VARCHAR(66) NOT NULL UNIQUE,
    parent_hash VARCHAR(66) NOT NULL,
    nonce VARCHAR(66),
    sha3_uncles VARCHAR(66),
    logs_bloom VARCHAR(1024),
    transactions_root VARCHAR(66),
    state_root VARCHAR(66),
    receipts_root VARCHAR(66),
    miner VARCHAR(42) NOT NULL,
    difficulty VARCHAR(32),
    total_difficulty VARCHAR(32),
    size BIGINT,
    gas_limit BIGINT,
    gas_used BIGINT,
    timestamp BIGINT NOT NULL,
    extra_data TEXT,
    base_fee_per_gas BIGINT,
    created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP
);

CREATE INDEX idx_blocks_number ON blocks(number DESC);
CREATE INDEX idx_blocks_hash ON blocks(hash);
CREATE INDEX idx_blocks_miner ON blocks(miner);
CREATE INDEX idx_blocks_timestamp ON blocks(timestamp DESC);

-- ============================================
-- TRANSACTIONS TABLE
-- ============================================
CREATE TABLE IF NOT EXISTS transactions (
    id BIGSERIAL PRIMARY KEY,
    hash VARCHAR(66) NOT NULL UNIQUE,
    block_number BIGINT,
    block_hash VARCHAR(66),
    transaction_index INTEGER,
    from_address VARCHAR(42) NOT NULL,
    to_address VARCHAR(42),
    value VARCHAR(66),
    gas_price BIGINT,
    gas_limit BIGINT,
    gas_used BIGINT,
    nonce BIGINT,
    input_data TEXT,
    v INTEGER,
    r VARCHAR(66),
    s VARCHAR(66),
    status INTEGER DEFAULT 0,
    timestamp BIGINT,
    created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
    FOREIGN KEY (block_number) REFERENCES blocks(number) ON DELETE SET NULL
);

CREATE INDEX idx_transactions_hash ON transactions(hash);
CREATE INDEX idx_transactions_block ON transactions(block_number DESC);
CREATE INDEX idx_transactions_from ON transactions(from_address);
CREATE INDEX idx_transactions_to ON transactions(to_address);
CREATE INDEX idx_transactions_timestamp ON transactions(timestamp DESC);

-- ============================================
-- TOKENS TABLE
-- ============================================
CREATE TABLE IF NOT EXISTS tokens (
    id BIGSERIAL PRIMARY KEY,
    address VARCHAR(42) NOT NULL UNIQUE,
    name VARCHAR(255) NOT NULL,
    symbol VARCHAR(50) NOT NULL,
    decimals INTEGER NOT NULL,
    total_supply VARCHAR(66) NOT NULL,
    holders_count INTEGER DEFAULT 0,
    transfers_count INTEGER DEFAULT 0,
    creator VARCHAR(42),
    contract_type VARCHAR(20) DEFAULT 'BEP20',
    is_verified BOOLEAN DEFAULT FALSE,
    is_active BOOLEAN DEFAULT TRUE,
    price VARCHAR(66),
    market_cap VARCHAR(66),
    volume_24h VARCHAR(66),
    price_change_24h VARCHAR(20),
    tx_hash VARCHAR(66),
    block_number BIGINT,
    created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP
);

CREATE INDEX idx_tokens_address ON tokens(address);
CREATE INDEX idx_tokens_symbol ON tokens(symbol);
CREATE INDEX idx_tokens_name ON tokens(name);

-- ============================================
-- TOKEN HOLDERS TABLE
-- ============================================
CREATE TABLE IF NOT EXISTS token_holders (
    id BIGSERIAL PRIMARY KEY,
    token_address VARCHAR(42) NOT NULL,
    address VARCHAR(42) NOT NULL,
    balance VARCHAR(66) NOT NULL,
    percent DECIMAL(10,4) DEFAULT 0,
    created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
    UNIQUE(token_address, address)
);

CREATE INDEX idx_token_holders_token ON token_holders(token_address);
CREATE INDEX idx_token_holders_address ON token_holders(address);

-- ============================================
-- TOKEN TRANSFERS TABLE
-- ============================================
CREATE TABLE IF NOT EXISTS token_transfers (
    id BIGSERIAL PRIMARY KEY,
    token_address VARCHAR(42) NOT NULL,
    hash VARCHAR(66) NOT NULL UNIQUE,
    block_number BIGINT NOT NULL,
    transaction_hash VARCHAR(66) NOT NULL,
    from_address VARCHAR(42) NOT NULL,
    to_address VARCHAR(42) NOT NULL,
    value VARCHAR(66) NOT NULL,
    log_index INTEGER,
    timestamp BIGINT NOT NULL,
    created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
    FOREIGN KEY (block_number) REFERENCES blocks(number) ON DELETE CASCADE
);

CREATE INDEX idx_token_transfers_token ON token_transfers(token_address);
CREATE INDEX idx_token_transfers_hash ON token_transfers(hash);
CREATE INDEX idx_token_transfers_block ON token_transfers(block_number DESC);
CREATE INDEX idx_token_transfers_from ON token_transfers(from_address);
CREATE INDEX idx_token_transfers_to ON token_transfers(to_address);

-- ============================================
-- NFT COLLECTIONS TABLE
-- ============================================
CREATE TABLE IF NOT EXISTS collections (
    id BIGSERIAL PRIMARY KEY,
    address VARCHAR(42) NOT NULL UNIQUE,
    name VARCHAR(255) NOT NULL,
    symbol VARCHAR(50),
    description TEXT,
    image_url TEXT,
    external_url TEXT,
    contract_type VARCHAR(20) DEFAULT 'BEP721',
    total_supply BIGINT DEFAULT 0,
    owners_count BIGINT DEFAULT 0,
    nfts_count BIGINT DEFAULT 0,
    floor_price VARCHAR(66),
    volume_24h VARCHAR(66),
    volume_total VARCHAR(66),
    creator VARCHAR(42),
    is_verified BOOLEAN DEFAULT FALSE,
    is_active BOOLEAN DEFAULT TRUE,
    tx_hash VARCHAR(66),
    block_number BIGINT,
    created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP
);

CREATE INDEX idx_collections_address ON collections(address);
CREATE INDEX idx_collections_name ON collections(name);

-- ============================================
-- NFTs TABLE
-- ============================================
CREATE TABLE IF NOT EXISTS nfts (
    id BIGSERIAL PRIMARY KEY,
    address VARCHAR(42) NOT NULL,
    token_id VARCHAR(78) NOT NULL,
    owner VARCHAR(42) NOT NULL,
    creator VARCHAR(42),
    name VARCHAR(255),
    description TEXT,
    image_url TEXT,
    animation_url TEXT,
    external_url TEXT,
    attributes JSONB,
    contract_type VARCHAR(20) DEFAULT 'BEP721',
    token_uri TEXT,
    collection_address VARCHAR(42),
    block_number BIGINT,
    block_hash VARCHAR(66),
    transaction_hash VARCHAR(66),
    timestamp BIGINT,
    created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
    UNIQUE(address, token_id)
);

CREATE INDEX idx_nfts_address ON nfts(address);
CREATE INDEX idx_nfts_token_id ON nfts(token_id);
CREATE INDEX idx_nfts_owner ON nfts(owner);
CREATE INDEX idx_nfts_collection ON nfts(collection_address);

-- ============================================
-- NFT HOLDERS TABLE
-- ============================================
CREATE TABLE IF NOT EXISTS nft_holders (
    id BIGSERIAL PRIMARY KEY,
    collection_address VARCHAR(42) NOT NULL,
    address VARCHAR(42) NOT NULL,
    balance BIGINT DEFAULT 0,
    created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
    UNIQUE(collection_address, address)
);

CREATE INDEX idx_nft_holders_collection ON nft_holders(collection_address);
CREATE INDEX idx_nft_holders_address ON nft_holders(address);

-- ============================================
-- NFT TRANSFERS TABLE
-- ============================================
CREATE TABLE IF NOT EXISTS nft_transfers (
    id BIGSERIAL PRIMARY KEY,
    nft_address VARCHAR(42) NOT NULL,
    token_id VARCHAR(78) NOT NULL,
    hash VARCHAR(66) NOT NULL UNIQUE,
    block_number BIGINT NOT NULL,
    transaction_hash VARCHAR(66) NOT NULL,
    from_address VARCHAR(42) NOT NULL,
    to_address VARCHAR(42) NOT NULL,
    log_index INTEGER,
    timestamp BIGINT NOT NULL,
    created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
    FOREIGN KEY (block_number) REFERENCES blocks(number) ON DELETE CASCADE
);

CREATE INDEX idx_nft_transfers_nft ON nft_transfers(nft_address, token_id);
CREATE INDEX idx_nft_transfers_hash ON nft_transfers(hash);
CREATE INDEX idx_nft_transfers_block ON nft_transfers(block_number DESC);
CREATE INDEX idx_nft_transfers_from ON nft_transfers(from_address);
CREATE INDEX idx_nft_transfers_to ON nft_transfers(to_address);

-- ============================================
-- CONTRACTS TABLE
-- ============================================
CREATE TABLE IF NOT EXISTS contracts (
    id BIGSERIAL PRIMARY KEY,
    address VARCHAR(42) NOT NULL UNIQUE,
    name VARCHAR(255),
    compiler_version VARCHAR(50),
    optimization BOOLEAN DEFAULT TRUE,
    optimization_runs INTEGER DEFAULT 200,
    source_code LONGTEXT,
    abi JSONB,
    bytecode LONGTEXT,
    constructor_args TEXT,
    is_verified BOOLEAN DEFAULT FALSE,
    is_active BOOLEAN DEFAULT TRUE,
    creator VARCHAR(42),
    tx_hash VARCHAR(66),
    block_number BIGINT,
    created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP
);

CREATE INDEX idx_contracts_address ON contracts(address);

-- ============================================
-- INTERNAL TRANSFERS TABLE
-- ============================================
CREATE TABLE IF NOT EXISTS internal_transfers (
    id BIGSERIAL PRIMARY KEY,
    transaction_hash VARCHAR(66) NOT NULL,
    block_number BIGINT NOT NULL,
    from_address VARCHAR(42),
    to_address VARCHAR(42),
    value VARCHAR(66),
    call_type VARCHAR(20),
    depth INTEGER,
    created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
    FOREIGN KEY (block_number) REFERENCES blocks(number) ON DELETE CASCADE
);

CREATE INDEX idx_internal_transfers_tx ON internal_transfers(transaction_hash);
CREATE INDEX idx_internal_transfers_block ON internal_transfers(block_number DESC);

-- ============================================
-- TOKENS TABLE (ADDITIONAL)
-- ============================================
CREATE TABLE IF NOT EXISTS token_prices (
    id BIGSERIAL PRIMARY KEY,
    token_address VARCHAR(42) NOT NULL,
    price VARCHAR(66) NOT NULL,
    market_cap VARCHAR(66),
    volume_24h VARCHAR(66),
    price_change_24h VARCHAR(20),
    price_change_7d VARCHAR(20),
    timestamp BIGINT NOT NULL,
    created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
    UNIQUE(token_address, timestamp)
);

CREATE INDEX idx_token_prices_token ON token_prices(token_address);
CREATE INDEX idx_token_prices_timestamp ON token_prices(timestamp DESC);

-- ============================================
-- VALIDATORS TABLE
-- ============================================
CREATE TABLE IF NOT EXISTS validators (
    id BIGSERIAL PRIMARY KEY,
    address VARCHAR(42) NOT NULL UNIQUE,
    name VARCHAR(255),
    website VARCHAR(255),
    email VARCHAR(255),
    description TEXT,
    logo_url TEXT,
    commission_rate DECIMAL(5,2) DEFAULT 10.00,
    total_stake VARCHAR(66),
    self_stake VARCHAR(66),
    delegators_count INTEGER DEFAULT 0,
    blocks_proposed INTEGER DEFAULT 0,
    blocks_missed INTEGER DEFAULT 0,
    uptime DECIMAL(5,2) DEFAULT 100.00,
    is_active BOOLEAN DEFAULT TRUE,
    is_jailed BOOLEAN DEFAULT FALSE,
    jail_reason TEXT,
    jailed_until BIGINT,
    created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP
);

CREATE INDEX idx_validators_address ON validators(address);

-- ============================================
-- PROPOSALS TABLE (GOVERNANCE)
-- ============================================
CREATE TABLE IF NOT EXISTS proposals (
    id BIGSERIAL PRIMARY KEY,
    proposal_id BIGINT NOT NULL UNIQUE,
    proposer VARCHAR(42) NOT NULL,
    title VARCHAR(255) NOT NULL,
    description TEXT,
    target_contract VARCHAR(42),
    call_data TEXT,
    value VARCHAR(66),
    status VARCHAR(20) DEFAULT 'pending',
    for_votes VARCHAR(66) DEFAULT '0',
    against_votes VARCHAR(66) DEFAULT '0',
    abstain_votes VARCHAR(66) DEFAULT '0',
    start_block BIGINT,
    end_block BIGINT,
    eta BIGINT,
    created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP
);

CREATE INDEX idx_proposals_id ON proposals(proposal_id);
CREATE INDEX idx_proposals_status ON proposals(status);

-- ============================================
-- VOTES TABLE
-- ============================================
CREATE TABLE IF NOT EXISTS votes (
    id BIGSERIAL PRIMARY KEY,
    proposal_id BIGINT NOT NULL,
    voter VARCHAR(42) NOT NULL,
    support BOOLEAN NOT NULL,
    votes VARCHAR(66) NOT NULL,
    reason TEXT,
    timestamp BIGINT NOT NULL,
    created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
    UNIQUE(proposal_id, voter)
);

CREATE INDEX idx_votes_proposal ON votes(proposal_id);
CREATE INDEX idx_votes_voter ON votes(voter);

-- ============================================
-- STATS TABLES
-- ============================================
CREATE TABLE IF NOT EXISTS daily_stats (
    id BIGSERIAL PRIMARY KEY,
    date DATE NOT NULL UNIQUE,
    blocks_count INTEGER DEFAULT 0,
    transactions_count INTEGER DEFAULT 0,
    new_addresses_count INTEGER DEFAULT 0,
    gas_used BIGINT DEFAULT 0,
    avg_gas_price BIGINT DEFAULT 0,
    total_value_locked VARCHAR(66),
    created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP
);

CREATE INDEX idx_daily_stats_date ON daily_stats(date DESC);

-- ============================================
-- TXN FEES TABLE
-- ============================================
CREATE TABLE IF NOT EXISTS txn_fees (
    id BIGSERIAL PRIMARY KEY,
    block_number BIGINT NOT NULL,
    miner VARCHAR(42) NOT NULL,
    gas_used BIGINT NOT NULL,
    gas_price BIGINT NOT NULL,
    fees VARCHAR(66) NOT NULL,
    timestamp BIGINT NOT NULL,
    created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
    FOREIGN KEY (block_number) REFERENCES blocks(number) ON DELETE CASCADE
);

CREATE INDEX idx_txn_fees_block ON txn_fees(block_number DESC);
CREATE INDEX idx_txn_fees_miner ON txn_fees(miner);
CREATE INDEX idx_txn_fees_timestamp ON txn_fees(timestamp DESC);

-- ============================================
-- VIEWS
-- ============================================
CREATE OR REPLACE VIEW v_latest_blocks AS
SELECT * FROM blocks ORDER BY number DESC LIMIT 100;

CREATE OR REPLACE VIEW v_latest_transactions AS
SELECT * FROM transactions ORDER BY timestamp DESC LIMIT 100;

CREATE OR REPLACE VIEW v_top_tokens AS
SELECT * FROM tokens ORDER BY transfers_count DESC LIMIT 50;

CREATE OR REPLACE VIEW v_top_collections AS
SELECT * FROM collections ORDER BY volume_total DESC LIMIT 50;

-- ============================================
-- FUNCTIONS
-- ============================================

-- Function to update token holder count
CREATE OR REPLACE FUNCTION update_token_holder_count()
RETURNS TRIGGER AS $$
BEGIN
    IF TG_OP = 'INSERT' OR TG_OP = 'UPDATE' THEN
        UPDATE tokens 
        SET holders_count = (SELECT COUNT(DISTINCT address) FROM token_holders WHERE token_address = NEW.token_address),
            updated_at = CURRENT_TIMESTAMP
        WHERE address = NEW.token_address;
    ELSIF TG_OP = 'DELETE' THEN
        UPDATE tokens 
        SET holders_count = (SELECT COUNT(DISTINCT address) FROM token_holders WHERE token_address = OLD.token_address),
            updated_at = CURRENT_TIMESTAMP
        WHERE address = OLD.token_address;
    END IF;
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

-- Trigger to update holder count
CREATE TRIGGER trigger_token_holder_count
AFTER INSERT OR UPDATE OR DELETE ON token_holders
FOR EACH ROW EXECUTE FUNCTION update_token_holder_count();

-- Function to update NFT owner count
CREATE OR REPLACE FUNCTION update_collection_owner_count()
RETURNS TRIGGER AS $$
BEGIN
    IF TG_OP = 'INSERT' OR TG_OP = 'UPDATE' THEN
        UPDATE collections 
        SET owners_count = (SELECT COUNT(DISTINCT address) FROM nft_holders WHERE collection_address = NEW.collection_address),
            updated_at = CURRENT_TIMESTAMP
        WHERE address = NEW.collection_address;
    END IF;
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

-- Trigger to update NFT owner count
CREATE TRIGGER trigger_nft_owner_count
AFTER INSERT OR UPDATE ON nft_holders
FOR EACH ROW EXECUTE FUNCTION update_collection_owner_count();

-- Function to calculate block rewards
CREATE OR REPLACE FUNCTION calculate_block_reward(block_num BIGINT)
RETURNS VARCHAR AS $$
DECLARE
    base_reward VARCHAR := '2000000000000000000'; -- 2 TSC
    decade INTEGER;
BEGIN
    decade := block_num / 63072000; -- ~2 years in seconds (with 3s block time)
    
    IF decade > 0 THEN
        -- Reduce reward by 5% each decade
        RETURN (2000000000000000000 * POWER(0.95, decade))::VARCHAR;
    END IF;
    
    RETURN base_reward;
END;
$$ LANGUAGE plpgsql;

-- ============================================
-- COMMENTS
-- ============================================
COMMENT ON TABLE blocks IS 'Blockchain blocks table';
COMMENT ON TABLE transactions IS 'Blockchain transactions table';
COMMENT ON TABLE tokens IS 'BEP20 tokens table';
COMMENT ON TABLE collections IS 'NFT collections table';
COMMENT ON TABLE nfts IS 'NFTs table';
COMMENT ON TABLE contracts IS 'Verified smart contracts table';
COMMENT ON TABLE validators IS 'Validators table';
COMMENT ON TABLE proposals IS 'Governance proposals table';
-- ============================================
-- CONTRACTS TABLE (Verified Smart Contracts)
-- ============================================
CREATE TABLE IF NOT EXISTS contracts (
    id BIGSERIAL PRIMARY KEY,
    address VARCHAR(42) NOT NULL UNIQUE,
    name VARCHAR(255) NOT NULL,
    compiler VARCHAR(50) NOT NULL,
    version VARCHAR(50) NOT NULL,
    optimization_enabled BOOLEAN DEFAULT TRUE,
    optimization_runs INTEGER DEFAULT 200,
    source_code TEXT NOT NULL,
    abi JSONB,
    bytecode TEXT,
    constructor_args TEXT,
    evm_version VARCHAR(50),
    library_refs JSONB,
    is_verified BOOLEAN DEFAULT TRUE,
    verification_date TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
    verified_by VARCHAR(255),
    is_proxy BOOLEAN DEFAULT FALSE,
    proxy_implementation VARCHAR(42),
    is_upgradable BOOLEAN DEFAULT FALSE,
    license VARCHAR(100),
    external_libs JSONB,
    hits_count INTEGER DEFAULT 0,
    created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP
);

CREATE INDEX idx_contracts_address ON contracts(address);
CREATE INDEX idx_contracts_name ON contracts(name);
CREATE INDEX idx_contracts_compiler ON contracts(compiler);
CREATE INDEX idx_contracts_is_verified ON contracts(is_verified);
CREATE INDEX idx_contracts_is_proxy ON contracts(is_proxy);

-- ============================================
-- LOGS TABLE (EVM Event Logs)
-- ============================================
CREATE TABLE IF NOT EXISTS logs (
    id BIGSERIAL PRIMARY KEY,
    transaction_hash VARCHAR(66) NOT NULL,
    block_number BIGINT NOT NULL,
    address VARCHAR(42) NOT NULL,
    topics JSONB NOT NULL,
    data TEXT NOT NULL,
    log_index INTEGER NOT NULL,
    timestamp BIGINT NOT NULL,
    created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
    FOREIGN KEY (block_number) REFERENCES blocks(number) ON DELETE CASCADE,
    UNIQUE(transaction_hash, log_index)
);

CREATE INDEX idx_logs_tx ON logs(transaction_hash);
CREATE INDEX idx_logs_block ON logs(block_number DESC);
CREATE INDEX idx_logs_address ON logs(address);
CREATE INDEX idx_logs_timestamp ON logs(timestamp DESC);

-- ============================================
-- GAS PRICES HISTORY TABLE
-- ============================================
CREATE TABLE IF NOT EXISTS gas_prices (
    id BIGSERIAL PRIMARY KEY,
    block_number BIGINT NOT NULL,
    slow_gas_price BIGINT NOT NULL,
    avg_gas_price BIGINT NOT NULL,
    fast_gas_price BIGINT NOT NULL,
    base_fee_per_gas BIGINT,
    timestamp BIGINT NOT NULL,
    created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
    FOREIGN KEY (block_number) REFERENCES blocks(number) ON DELETE CASCADE
);

CREATE INDEX idx_gas_prices_block ON gas_prices(block_number);
CREATE INDEX idx_gas_prices_timestamp ON gas_prices(timestamp DESC);

-- ============================================
-- API KEYS TABLE (Rate Limiting)
-- ============================================
CREATE TABLE IF NOT EXISTS api_keys (
    id BIGSERIAL PRIMARY KEY,
    key_hash VARCHAR(255) NOT NULL UNIQUE,
    key_prefix VARCHAR(20) NOT NULL,
    user_id VARCHAR(255) NOT NULL,
    label VARCHAR(255),
    rate_limit INTEGER DEFAULT 1000,
    rate_limit_window INTEGER DEFAULT 60,
    requests_count INTEGER DEFAULT 0,
    is_active BOOLEAN DEFAULT TRUE,
    expires_at TIMESTAMP,
    last_used_at TIMESTAMP,
    created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP
);

CREATE INDEX idx_api_keys_hash ON api_keys(key_hash);
CREATE INDEX idx_api_keys_user ON api_keys(user_id);
CREATE INDEX idx_api_keys_is_active ON api_keys(is_active);

-- ============================================
-- AUDIT LOGS TABLE (Security)
-- ============================================
CREATE TABLE IF NOT EXISTS audit_logs (
    id BIGSERIAL PRIMARY KEY,
    user_id VARCHAR(255),
    action VARCHAR(100) NOT NULL,
    resource_type VARCHAR(100),
    resource_id VARCHAR(255),
    ip_address INET,
    user_agent TEXT,
    metadata JSONB,
    success BOOLEAN DEFAULT TRUE,
    error_message TEXT,
    created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP
);

CREATE INDEX idx_audit_logs_user ON audit_logs(user_id);
CREATE INDEX idx_audit_logs_action ON audit_logs(action);
CREATE INDEX idx_audit_logs_created ON audit_logs(created_at DESC);
CREATE INDEX idx_audit_logs_resource ON audit_logs(resource_type, resource_id);

-- ============================================
-- STAKING TABLE
-- ============================================
CREATE TABLE IF NOT EXISTS stakings (
    id BIGSERIAL PRIMARY KEY,
    delegator VARCHAR(42) NOT NULL,
    validator VARCHAR(42) NOT NULL,
    amount VARCHAR(66) NOT NULL,
    rewards VARCHAR(66) DEFAULT '0',
    lock_start_time BIGINT,
    lock_end_time BIGINT,
    is_compound BOOLEAN DEFAULT FALSE,
    is_withdrawn BOOLEAN DEFAULT FALSE,
    block_number BIGINT NOT NULL,
    created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
    FOREIGN KEY (validator) REFERENCES validators(address) ON DELETE CASCADE,
    FOREIGN KEY (block_number) REFERENCES blocks(number) ON DELETE CASCADE
);

CREATE INDEX idx_stakings_delegator ON stakings(delegator);
CREATE INDEX idx_stakings_validator ON stakings(validator);
CREATE INDEX idx_staking_is_withdrawn ON stakings(is_withdrawn);

-- ============================================
-- BRIDGE TRANSFERS TABLE
-- ============================================
CREATE TABLE IF NOT EXISTS bridge_transfers (
    id BIGSERIAL PRIMARY KEY,
    transfer_id VARCHAR(100) NOT NULL UNIQUE,
    source_chain_id BIGINT NOT NULL,
    dest_chain_id BIGINT NOT NULL,
    sender VARCHAR(42) NOT NULL,
    recipient VARCHAR(42) NOT NULL,
    token_address VARCHAR(42),
    amount VARCHAR(66),
    amount_usd VARCHAR(66),
    status VARCHAR(50) DEFAULT 'pending',
    direction VARCHAR(10) NOT NULL,
    fee VARCHAR(66),
    nonce BIGINT,
    message_hash VARCHAR(66),
    confirmation_time BIGINT,
    completion_time BIGINT,
    block_number BIGINT NOT NULL,
    created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
    FOREIGN KEY (block_number) REFERENCES blocks(number) ON DELETE CASCADE
);

CREATE INDEX idx_bridge_transfers_id ON bridge_transfers(transfer_id);
CREATE INDEX idx_bridge_transfers_sender ON bridge_transfers(sender);
CREATE INDEX idx_bridge_transfers_recipient ON bridge_transfers(recipient);
CREATE INDEX idx_bridge_transfers_status ON bridge_transfers(status);

-- ============================================
-- CONTRACT EVENTS TABLE
-- ============================================
CREATE TABLE IF NOT EXISTS contract_events (
    id BIGSERIAL PRIMARY KEY,
    contract_address VARCHAR(42) NOT NULL,
    event_name VARCHAR(255) NOT NULL,
    event_signature VARCHAR(255) NOT NULL,
    topic0 VARCHAR(66),
    first_block BIGINT NOT NULL,
    created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
    FOREIGN KEY (contract_address) REFERENCES contracts(address) ON DELETE CASCADE,
    UNIQUE(contract_address, event_signature)
);

CREATE INDEX idx_contract_events_address ON contract_events(contract_address);

-- ============================================
-- TOKEN TRANSFERS (ADDITIONAL INDEXES)
-- ============================================
CREATE INDEX idx_token_transfers_timestamp ON token_transfers(timestamp DESC);
CREATE INDEX idx_token_transfers_value ON (value::NUMERIC(78,0));

-- ============================================
-- NFT TRANSFERS TABLE
-- ============================================
CREATE TABLE IF NOT EXISTS nft_transfers (
    id BIGSERIAL PRIMARY KEY,
    nft_address VARCHAR(42) NOT NULL,
    token_id VARCHAR(78) NOT NULL,
    hash VARCHAR(66) NOT NULL UNIQUE,
    block_number BIGINT NOT NULL,
    transaction_hash VARCHAR(66) NOT NULL,
    from_address VARCHAR(42) NOT NULL,
    to_address VARCHAR(42) NOT NULL,
    amount BIGINT DEFAULT 1,
    log_index INTEGER,
    timestamp BIGINT NOT NULL,
    is_mint BOOLEAN DEFAULT FALSE,
    is_burn BOOLEAN DEFAULT FALSE,
    created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
    FOREIGN KEY (block_number) REFERENCES blocks(number) ON DELETE CASCADE
);

CREATE INDEX idx_nft_transfers_address ON nft_transfers(nft_address);
CREATE INDEX idx_nft_transfers_token ON nft_transfers(nft_address, token_id);
CREATE INDEX idx_nft_transfers_from ON nft_transfers(from_address);
CREATE INDEX idx_nft_transfers_to ON nft_transfers(to_address);
CREATE INDEX idx_nft_transfers_block ON nft_transfers(block_number DESC);
CREATE INDEX idx_nft_transfers_timestamp ON nft_transfers(timestamp DESC);

-- ============================================
-- VIEW: VERIFIED CONTRACTS
-- ============================================
CREATE OR REPLACE VIEW v_verified_contracts AS
SELECT address, name, compiler, version, is_verified, is_proxy, proxy_implementation, hits_count
FROM contracts
WHERE is_verified = TRUE
ORDER BY hits_count DESC;

-- ============================================
-- VIEW: PENDING TRANSACTIONS
-- ============================================
CREATE OR REPLACE VIEW v_pending_transactions AS
SELECT hash, from_address, to_address, value, gas_price, nonce, created_at
FROM transactions
WHERE status = 0
ORDER BY gas_price DESC, created_at ASC;

-- ============================================
-- VIEW: LATEST TRANSACTIONS WITH BLOCK INFO
-- ============================================
CREATE OR REPLACE VIEW v_transactions_with_blocks AS
SELECT t.hash, t.from_address, t.to_address, t.value, t.gas_used, t.gas_price, 
       t.status, t.input_data, b.number as block_number, b.timestamp
FROM transactions t
LEFT JOIN blocks b ON t.block_number = b.number
WHERE t.status != 0
ORDER BY b.timestamp DESC, t.transaction_index DESC
LIMIT 100;

-- ============================================
-- VIEW: TOP TOKEN HOLDERS
-- ============================================
CREATE OR REPLACE VIEW v_top_token_holders AS
SELECT th.token_address, th.address, th.balance, th.percent
FROM token_holders th
JOIN tokens t ON t.address = th.token_address
WHERE th.balance::NUMERIC(78,0) > 0
ORDER BY th.balance::NUMERIC(78,0) DESC
LIMIT 100;

-- ============================================
-- VIEW: NETWORK STATS
-- ============================================
CREATE OR REPLACE VIEW v_network_stats AS
SELECT 
    (SELECT COUNT(*) FROM blocks) as total_blocks,
    (SELECT COUNT(*) FROM transactions WHERE status = 1) as total_transactions,
    (SELECT COUNT(DISTINCT from_address) FROM transactions) as unique_senders,
    (SELECT COUNT(*) FROM tokens) as total_tokens,
    (SELECT COUNT(*) FROM collections) as total_collections,
    (SELECT COUNT(*) FROM nfts) as total_nfts,
    (SELECT SUM(gas_used::BIGINT) FROM blocks) as total_gas_used,
    (SELECT AVG(avg_gas_price::BIGINT) FROM gas_prices ORDER BY timestamp DESC LIMIT 1) as current_gas_price;

-- ============================================
-- FUNCTION: SEARCH EXPLORER
-- ============================================
CREATE OR REPLACE FUNCTION search_explorer(query_text TEXT)
RETURNS TABLE (
    result_type VARCHAR(50),
    result_id VARCHAR(255),
    result_data JSONB,
    rank_score FLOAT
) AS $$
BEGIN
    RETURN QUERY
    SELECT 'block'::VARCHAR(50), 
           b.number::VARCHAR(255),
           jsonb_build_object('hash', b.hash, 'txs', (SELECT COUNT(*) FROM transactions WHERE block_number = b.number), 'gas', b.gas_used)::JSONB,
           1.0::FLOAT
    FROM blocks b
    WHERE b.number = query_text::BIGINT OR b.hash LIKE query_text || '%'
    UNION ALL
    SELECT 'transaction'::VARCHAR(50),
           t.hash::VARCHAR(255),
           jsonb_build_object('from', t.from_address, 'to', t.to_address, 'value', t.value, 'status', t.status)::JSONB,
           1.0::FLOAT
    FROM transactions t
    WHERE t.hash LIKE query_text || '%'
    UNION ALL
    SELECT 'token'::VARCHAR(50),
           tok.address::VARCHAR(255),
           jsonb_build_object('name', tok.name, 'symbol', tok.symbol, 'holders', tok.holders_count)::JSONB,
           1.0::FLOAT
    FROM tokens tok
    WHERE tok.symbol ILIKE query_text || '%' OR tok.name ILIKE '%' || query_text || '%'
    UNION ALL
    SELECT 'address'::VARCHAR(50),
           a.address::VARCHAR(255),
           jsonb_build_object('balance', a.balance)::JSONB,
           0.5::FLOAT
    FROM accounts a
    WHERE a.address LIKE query_text || '%'
    LIMIT 100;
END;
$$ LANGUAGE plpgsql;

-- ============================================
-- FUNCTION: GET ACCOUNT TOTAL BALANCE
-- ============================================
CREATE OR REPLACE FUNCTION get_account_total_balance(addr VARCHAR(42))
RETURNS NUMERIC(78, 0) AS $$
DECLARE
    native_balance NUMERIC(78, 0);
    token_balance NUMERIC(78, 0);
BEGIN
    SELECT COALESCE(a.balance::NUMERIC(78, 0), 0) INTO native_balance 
    FROM accounts a WHERE a.address = addr;
    
    SELECT COALESCE(SUM(th.balance::NUMERIC(78, 0) * COALESCE(t.price::NUMERIC(78, 18)::NUMERIC(78, 0), 0)), 0)
    INTO token_balance
    FROM token_holders th
    JOIN tokens t ON t.address = th.token_address
    WHERE th.address = addr AND th.balance::NUMERIC(78, 0) > 0;
    
    RETURN COALESCE(native_balance, 0) + token_balance;
END;
$$ LANGUAGE plpgsql;

-- ============================================
-- COMMENTS
-- ============================================
COMMENT ON TABLE blocks IS 'Blockchain blocks table';
COMMENT ON TABLE transactions IS 'Blockchain transactions table';
COMMENT ON TABLE tokens IS 'BEP20 tokens table';
COMMENT ON TABLE collections IS 'NFT collections table';
COMMENT ON TABLE nfts IS 'NFTs table';
COMMENT ON TABLE contracts IS 'Verified smart contracts table';
COMMENT ON TABLE validators IS 'Validators table';
COMMENT ON TABLE proposals IS 'Governance proposals table';
COMMENT ON TABLE daily_stats IS 'Daily network statistics';
COMMENT ON TABLE logs IS 'EVM event logs';
COMMENT ON TABLE gas_prices IS 'Historical gas prices';
COMMENT ON TABLE api_keys IS 'API keys for rate limiting';
COMMENT ON TABLE audit_logs IS 'Security audit trail';
COMMENT ON TABLE bridge_transfers IS 'Cross-chain bridge transfers';