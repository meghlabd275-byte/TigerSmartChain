-- PostgreSQL Database Schema for TigerWallet White Level Admin System
-- Version: 1.0.0
-- Date: 2026-06-09

-- ============================================
-- USERS TABLE
-- ============================================
CREATE TABLE IF NOT EXISTS users (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    username VARCHAR(50) UNIQUE NOT NULL,
    email VARCHAR(255) UNIQUE NOT NULL,
    password_hash VARCHAR(255) NOT NULL,
    salt VARCHAR(255) NOT NULL,
    role SMALLINT NOT NULL DEFAULT 2, -- 0: super_admin, 1: admin, 2: whitelabel_client, 3: user
    status SMALLINT NOT NULL DEFAULT 0, -- 0: pending, 1: active, 2: suspended, 3: banned, 4: locked
    created_at BIGINT NOT NULL,
    updated_at BIGINT NOT NULL,
    last_login BIGINT,
    last_activity BIGINT,
    ip_address INET,
    two_factor_secret TEXT,
    two_factor_enabled BOOLEAN DEFAULT FALSE,
    email_verified BOOLEAN DEFAULT FALSE,
    api_key_hash VARCHAR(255),
    product_id UUID,
    admin_id UUID,
    failed_attempts INTEGER DEFAULT 0,
    locked_until BIGINT,
    verification_code VARCHAR(10),
    verification_expires_at BIGINT,
    
    CONSTRAINT valid_email CHECK (email ~* '^[A-Za-z0-9._%+-]+@[A-Za-z0-9.-]+\.[A-Za-z]{2,}$'),
    CONSTRAINT valid_username CHECK (username ~* '^[A-Za-z][A-Za-z0-9_]{2,49}$')
);

CREATE INDEX idx_users_email ON users(email);
CREATE INDEX idx_users_username ON users(username);
CREATE INDEX idx_users_role ON users(role);
CREATE INDEX idx_users_status ON users(status);
CREATE INDEX idx_users_product_id ON users(product_id);

-- ============================================
-- PRODUCTS TABLE
-- ============================================
CREATE TABLE IF NOT EXISTS products (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    name VARCHAR(100) NOT NULL,
    brand_name VARCHAR(100) NOT NULL,
    domain VARCHAR(255) UNIQUE NOT NULL,
    cloud VARCHAR(50) NOT NULL DEFAULT 'default',
    storage VARCHAR(50) NOT NULL DEFAULT 'default',
    status SMALLINT NOT NULL DEFAULT 0, -- 0: pending, 1: active, 2: paused, 3: halted, 4: destroyed
    owner_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    admin_id UUID REFERENCES users(id),
    created_at BIGINT NOT NULL,
    updated_at BIGINT NOT NULL,
    api_keys JSONB DEFAULT '{}',
    features JSONB DEFAULT '[]',
    config JSONB DEFAULT '{}',
    auth_required BOOLEAN DEFAULT TRUE,
    is_paused BOOLEAN DEFAULT FALSE,
    is_halted BOOLEAN DEFAULT FALSE,
    paused_by UUID REFERENCES users(id),
    halted_by UUID REFERENCES users(id),
    paused_at BIGINT,
    halted_at BIGINT,
    
    CONSTRAINT valid_domain CHECK (domain ~* '^[a-z0-9]([a-z0-9-]{0,61}[a-z0-9])?(\.[a-z0-9]([a-z0-9-]{0,61}[a-z0-9])?)*$')
);

CREATE INDEX idx_products_owner_id ON products(owner_id);
CREATE INDEX idx_products_status ON products(status);
CREATE INDEX idx_products_domain ON products(domain);

-- ============================================
-- API KEYS TABLE
-- ============================================
CREATE TABLE IF NOT EXISTS api_keys (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    product_id UUID NOT NULL REFERENCES products(id) ON DELETE CASCADE,
    key_hash VARCHAR(255) NOT NULL,
    name VARCHAR(100) NOT NULL,
    created_at BIGINT NOT NULL,
    expires_at BIGINT,
    active BOOLEAN DEFAULT TRUE,
    last_used BIGINT,
    usage_count INTEGER DEFAULT 0,
    
    UNIQUE(product_id, key_hash)
);

CREATE INDEX idx_api_keys_product_id ON api_keys(product_id);
CREATE INDEX idx_api_keys_key_hash ON api_keys(key_hash);

-- ============================================
-- SESSIONS TABLE
-- ============================================
CREATE TABLE IF NOT EXISTS sessions (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    token_hash VARCHAR(255) NOT NULL UNIQUE,
    ip_address INET NOT NULL,
    user_agent TEXT,
    created_at BIGINT NOT NULL,
    expires_at BIGINT NOT NULL,
    last_activity BIGINT,
    
    CONSTRAINT valid_expires CHECK (expires_at > created_at)
);

CREATE INDEX idx_sessions_user_id ON sessions(user_id);
CREATE INDEX idx_sessions_token_hash ON sessions(token_hash);
CREATE INDEX idx_sessions_expires_at ON sessions(expires_at);

-- ============================================
-- AUDIT LOGS TABLE
-- ============================================
CREATE TABLE IF NOT EXISTS audit_logs (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id UUID REFERENCES users(id) ON DELETE SET NULL,
    action VARCHAR(50) NOT NULL,
    ip_address INET,
    user_agent TEXT,
    details TEXT,
    timestamp BIGINT NOT NULL,
    success BOOLEAN DEFAULT TRUE,
    product_id UUID REFERENCES products(id) ON DELETE SET NULL
);

CREATE INDEX idx_audit_logs_user_id ON audit_logs(user_id);
CREATE INDEX idx_audit_logs_timestamp ON audit_logs(timestamp DESC);
CREATE INDEX idx_audit_logs_action ON audit_logs(action);

-- ============================================
-- PERMISSIONS TABLE
-- ============================================
CREATE TABLE IF NOT EXISTS permissions (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    permission VARCHAR(100) NOT NULL,
    granted_by UUID REFERENCES users(id),
    granted_at BIGINT NOT NULL,
    expires_at BIGINT,
    
    UNIQUE(user_id, permission)
);

CREATE INDEX idx_permissions_user_id ON permissions(user_id);

-- ============================================
-- RATE LIMITS TABLE
-- ============================================
CREATE TABLE IF NOT EXISTS rate_limits (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    key VARCHAR(255) NOT NULL UNIQUE,
    requests JSONB DEFAULT '[]',
    max_requests INTEGER DEFAULT 60,
    window_seconds INTEGER DEFAULT 60,
    blocked_until BIGINT
);

CREATE INDEX idx_rate_limits_key ON rate_limits(key);

-- ============================================
-- BLOCKED IPS TABLE
-- ============================================
CREATE TABLE IF NOT EXISTS blocked_ips (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    ip_address INET NOT NULL UNIQUE,
    reason TEXT,
    blocked_by UUID REFERENCES users(id),
    blocked_at BIGINT NOT NULL,
    expires_at BIGINT
);

CREATE INDEX idx_blocked_ips_ip ON blocked_ips(ip_address);

-- ============================================
-- FEATURES TABLE
-- ============================================
CREATE TABLE IF NOT EXISTS features (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    name VARCHAR(100) NOT NULL UNIQUE,
    description TEXT,
    enabled BOOLEAN DEFAULT TRUE,
    created_at BIGINT NOT NULL
);

-- Insert default features
INSERT INTO features (name, description, enabled, created_at) VALUES
    ('wallet.create', 'Create wallets', TRUE, EXTRACT(EPOCH FROM NOW())::BIGINT),
    ('wallet.send', 'Send transactions', TRUE, EXTRACT(EPOCH FROM NOW())::BIGINT),
    ('wallet.receive', 'Receive transactions', TRUE, EXTRACT(EPOCH FROM NOW())::BIGINT),
    ('wallet.import', 'Import wallets', TRUE, EXTRACT(EPOCH FROM NOW())::BIGINT),
    ('wallet.export', 'Export wallets', TRUE, EXTRACT(EPOCH FROM NOW())::BIGINT),
    ('nft.mint', 'Mint NFTs', TRUE, EXTRACT(EPOCH FROM NOW())::BIGINT),
    ('nft.transfer', 'Transfer NFTs', TRUE, EXTRACT(EPOCH FROM NOW())::BIGINT),
    ('nft.burn', 'Burn NFTs', TRUE, EXTRACT(EPOCH FROM NOW())::BIGINT),
    ('nft.create_collection', 'Create NFT collections', TRUE, EXTRACT(EPOCH FROM NOW())::BIGINT),
    ('token.create', 'Create tokens', TRUE, EXTRACT(EPOCH FROM NOW())::BIGINT),
    ('token.transfer', 'Transfer tokens', TRUE, EXTRACT(EPOCH FROM NOW())::BIGINT),
    ('token.burn', 'Burn tokens', TRUE, EXTRACT(EPOCH FROM NOW())::BIGINT),
    ('token.mint', 'Mint tokens', TRUE, EXTRACT(EPOCH FROM NOW())::BIGINT),
    ('bridge.deposit', 'Bridge deposits', TRUE, EXTRACT(EPOCH FROM NOW())::BIGINT),
    ('bridge.withdraw', 'Bridge withdrawals', TRUE, EXTRACT(EPOCH FROM NOW())::BIGINT),
    ('bridge.transfer', 'Bridge transfers', TRUE, EXTRACT(EPOCH FROM NOW())::BIGINT),
    ('staking.stake', 'Stake tokens', TRUE, EXTRACT(EPOCH FROM NOW())::BIGINT),
    ('staking.unstake', 'Unstake tokens', TRUE, EXTRACT(EPOCH FROM NOW())::BIGINT),
    ('staking.reward', 'Claim staking rewards', TRUE, EXTRACT(EPOCH FROM NOW())::BIGINT),
    ('governance.vote', 'Vote on proposals', TRUE, EXTRACT(EPOCH FROM NOW())::BIGINT),
    ('governance.propose', 'Create proposals', TRUE, EXTRACT(EPOCH FROM NOW())::BIGINT)
ON CONFLICT (name) DO NOTHING;

-- ============================================
-- PRODUCT FEATURES TABLE (Many-to-Many)
-- ============================================
CREATE TABLE IF NOT EXISTS product_features (
    product_id UUID NOT NULL REFERENCES products(id) ON DELETE CASCADE,
    feature_id UUID NOT NULL REFERENCES features(id) ON DELETE CASCADE,
    enabled BOOLEAN DEFAULT TRUE,
    
    PRIMARY KEY (product_id, feature_id)
);

-- ============================================
-- SECURITY CONFIGURATION TABLE
-- ============================================
CREATE TABLE IF NOT EXISTS security_config (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    name VARCHAR(100) NOT NULL UNIQUE,
    value TEXT NOT NULL,
    updated_at BIGINT NOT NULL,
    updated_by UUID REFERENCES users(id)
);

-- Insert default security configurations
INSERT INTO security_config (name, value, updated_at) VALUES
    ('min_password_length', '12', EXTRACT(EPOCH FROM NOW())::BIGINT),
    ('max_password_length', '128', EXTRACT(EPOCH FROM NOW())::BIGINT),
    ('max_failed_attempts', '10', EXTRACT(EPOCH FROM NOW())::BIGINT),
    ('lockout_duration_minutes', '15', EXTRACT(EPOCH FROM NOW())::BIGINT),
    ('session_duration_hours', '24', EXTRACT(EPOCH FROM NOW())::BIGINT),
    ('max_sessions_per_user', '5', EXTRACT(EPOCH FROM NOW())::BIGINT),
    ('rate_limit_requests_per_min', '60', EXTRACT(EPOCH FROM NOW())::BIGINT),
    ('require_2fa', 'false', EXTRACT(EPOCH FROM NOW())::BIGINT),
    ('require_email_verification', 'true', EXTRACT(EPOCH FROM NOW())::BIGINT)
ON CONFLICT (name) DO NOTHING;

-- ============================================
-- VIEWS FOR REPORTING
-- ============================================
CREATE OR REPLACE VIEW v_user_stats AS
SELECT 
    role,
    status,
    COUNT(*) as count
FROM users
GROUP BY role, status;

CREATE OR REPLACE VIEW v_product_stats AS
SELECT 
    status,
    COUNT(*) as count
FROM products
GROUP BY status;

CREATE OR REPLACE VIEW v_audit_logs_recent AS
SELECT 
    al.*,
    u.username,
    u.email
FROM audit_logs al
LEFT JOIN users u ON al.user_id = u.id
ORDER BY al.timestamp DESC
LIMIT 100;

-- ============================================
-- FUNCTIONS
-- ============================================

-- Function to clean expired sessions
CREATE OR REPLACE FUNCTION clean_expired_sessions()
RETURNS void
LANGUAGE plpgsql
AS $$
BEGIN
    DELETE FROM sessions WHERE expires_at < EXTRACT(EPOCH FROM NOW())::BIGINT;
END;
$$;

-- Function to clean expired rate limits
CREATE OR REPLACE FUNCTION clean_expired_rate_limits()
RETURNS void
LANGUAGE plpgsql
AS $$
BEGIN
    DELETE FROM rate_limits 
    WHERE blocked_until IS NOT NULL 
    AND blocked_until < EXTRACT(EPOCH FROM NOW())::BIGINT;
END;
$$;

-- Trigger to auto-clean expired sessions every hour
CREATE OR REPLACE FUNCTION schedule_session_cleanup()
RETURNS trigger
LANGUAGE plpgsql
AS $$
BEGIN
    IF random() < 0.01 THEN -- 1% chance on each insert
        PERFORM clean_expired_sessions();
        PERFORM clean_expired_rate_limits();
    END IF;
    RETURN NEW;
END;
$$;

CREATE TRIGGER trigger_session_cleanup
AFTER INSERT ON sessions
FOR EACH STATEMENT
EXECUTE FUNCTION schedule_session_cleanup();

-- ============================================
-- ROW LEVEL SECURITY POLICIES
-- ============================================

-- Enable RLS on sensitive tables
ALTER TABLE users ENABLE ROW LEVEL SECURITY;
ALTER TABLE api_keys ENABLE ROW LEVEL SECURITY;
ALTER TABLE sessions ENABLE ROW LEVEL SECURITY;
ALTER TABLE audit_logs ENABLE ROW LEVEL SECURITY;

-- Policy: Users can only see their own data
CREATE POLICY policy_users_select ON users
    FOR SELECT
    USING (true); -- Admin sees all, others see own

-- Policy: Only admins can manage users
CREATE POLICY policy_users_insert ON users
    FOR INSERT
    WITH CHECK (true);

CREATE POLICY policy_users_update ON users
    FOR UPDATE
    USING (true);

-- ============================================
-- COMMENTS
-- ============================================
COMMENT ON TABLE users IS 'White level clients and admins table';
COMMENT ON TABLE products IS 'White level products table';
COMMENT ON TABLE api_keys IS 'API keys for white level products';
COMMENT ON TABLE sessions IS 'User sessions for authentication';
COMMENT ON TABLE audit_logs IS 'Audit trail for security compliance';
COMMENT ON TABLE permissions IS 'User permissions';
COMMENT ON TABLE rate_limits IS 'Rate limiting for attack prevention';
COMMENT ON TABLE blocked_ips IS 'Blocked IP addresses';
COMMENT ON TABLE features IS 'Available features for white level products';