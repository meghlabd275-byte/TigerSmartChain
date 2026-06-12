// Package postgres provides database migrations for TigerScan Explorer.
package postgres

import (
	"context"
	"fmt"
	"time"

	"github.com/jackc/pgx/v5/pgxpool"
)

// =============================================================================
// MIGRATION SYSTEM
// =============================================================================

// Migration represents a database migration
type Migration struct {
	Version     int64
	Name        string
	UpSQL       string
	DownSQL     string
	AppliedAt   *time.Time
}

// =============================================================================
// MIGRATIONS
// =============================================================================

// migrations contains all database migrations
var migrations = []*Migration{
	// Initial schema - v1
	{
		Version: 1,
		Name:    "initial_schema",
		UpSQL: `
			-- Create accounts table
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
			
			-- Create blocks table
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
			
			-- Create transactions table
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
			
			-- Create tokens table
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
			
			-- Create token holders table
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
			
			-- Create token transfers table
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
			
			-- Create collections table
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
			
			-- Create nfts table
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
			
			-- Create nft holders table
			CREATE TABLE IF NOT EXISTS nft_holders (
				id BIGSERIAL PRIMARY KEY,
				collection_address VARCHAR(42) NOT NULL,
				address VARCHAR(42) NOT NULL,
				balance BIGINT DEFAULT 0,
				created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
				updated_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
				UNIQUE(collection_address, address)
			);
			
			-- Create nft transfers table
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
			
			-- Create internal transfers table
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
			
			-- Create contracts table
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
			
			-- Create logs table
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
			
			-- Create gas prices table
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
			
			-- Create validators table
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
			
			-- Create proposals table
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
			
			-- Create votes table
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
			
			-- Create daily stats table
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
			
			-- Create api keys table
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
			
			-- Create audit logs table
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
			
			-- Create stakings table
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
			
			-- Create bridge transfers table
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
			
			-- Create contract events table
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
			
			-- Create token prices table
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
			
			-- Create txn fees table
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
			
			-- Create indexes
			CREATE INDEX IF NOT EXISTS idx_blocks_number ON blocks(number DESC);
			CREATE INDEX IF NOT EXISTS idx_blocks_hash ON blocks(hash);
			CREATE INDEX IF NOT EXISTS idx_blocks_miner ON blocks(miner);
			CREATE INDEX IF NOT EXISTS idx_blocks_timestamp ON blocks(timestamp DESC);
			
			CREATE INDEX IF NOT EXISTS idx_transactions_hash ON transactions(hash);
			CREATE INDEX IF NOT EXISTS idx_transactions_block ON transactions(block_number DESC);
			CREATE INDEX IF NOT EXISTS idx_transactions_from ON transactions(from_address);
			CREATE INDEX IF NOT EXISTS idx_transactions_to ON transactions(to_address);
			CREATE INDEX IF NOT EXISTS idx_transactions_timestamp ON transactions(timestamp DESC);
			
			CREATE INDEX IF NOT EXISTS idx_tokens_address ON tokens(address);
			CREATE INDEX IF NOT EXISTS idx_tokens_symbol ON tokens(symbol);
			CREATE INDEX IF NOT EXISTS idx_tokens_name ON tokens(name);
			
			CREATE INDEX IF NOT EXISTS idx_token_holders_token ON token_holders(token_address);
			CREATE INDEX IF NOT EXISTS idx_token_holders_address ON token_holders(address);
			
			CREATE INDEX IF NOT EXISTS idx_token_transfers_token ON token_transfers(token_address);
			CREATE INDEX IF NOT EXISTS idx_token_transfers_hash ON token_transfers(hash);
			CREATE INDEX IF NOT EXISTS idx_token_transfers_block ON token_transfers(block_number DESC);
			CREATE INDEX IF NOT EXISTS idx_token_transfers_from ON token_transfers(from_address);
			CREATE INDEX IF NOT EXISTS idx_token_transfers_to ON token_transfers(to_address);
			
			CREATE INDEX IF NOT EXISTS idx_collections_address ON collections(address);
			CREATE INDEX IF NOT EXISTS idx_collections_name ON collections(name);
			
			CREATE INDEX IF NOT EXISTS idx_nfts_address ON nfts(address);
			CREATE INDEX IF NOT EXISTS idx_nfts_token_id ON nfts(token_id);
			CREATE INDEX IF NOT EXISTS idx_nfts_owner ON nfts(owner);
			CREATE INDEX IF NOT EXISTS idx_nfts_collection ON nfts(collection_address);
			
			CREATE INDEX IF NOT EXISTS idx_contracts_address ON contracts(address);
			CREATE INDEX IF NOT EXISTS idx_contracts_name ON contracts(name);
			CREATE INDEX IF NOT EXISTS idx_contracts_compiler ON contracts(compiler);
			CREATE INDEX IF NOT EXISTS idx_contracts_is_verified ON contracts(is_verified);
			
			CREATE INDEX IF NOT EXISTS idx_logs_tx ON logs(transaction_hash);
			CREATE INDEX IF NOT EXISTS idx_logs_block ON logs(block_number DESC);
			CREATE INDEX IF NOT EXISTS idx_logs_address ON logs(address);
			CREATE INDEX IF NOT EXISTS idx_logs_timestamp ON logs(timestamp DESC);
			
			CREATE INDEX IF NOT EXISTS idx_gas_prices_block ON gas_prices(block_number);
			CREATE INDEX IF NOT EXISTS idx_gas_prices_timestamp ON gas_prices(timestamp DESC);
			
			CREATE INDEX IF NOT EXISTS idx_validators_address ON validators(address);
			
			CREATE INDEX IF NOT EXISTS idx_api_keys_hash ON api_keys(key_hash);
			CREATE INDEX IF NOT EXISTS idx_api_keys_user ON api_keys(user_id);
			CREATE INDEX IF NOT EXISTS idx_api_keys_is_active ON api_keys(is_active);
			
			CREATE INDEX IF NOT EXISTS idx_audit_logs_user ON audit_logs(user_id);
			CREATE INDEX IF NOT EXISTS idx_audit_logs_action ON audit_logs(action);
			CREATE INDEX IF NOT EXISTS idx_audit_logs_created ON audit_logs(created_at DESC);
			
			-- Create migration tracking table
			CREATE TABLE IF NOT EXISTS schema_migrations (
				version BIGINT PRIMARY KEY,
				name VARCHAR(255) NOT NULL,
				applied_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP
			);
		`,
		DownSQL: `
			DROP TABLE IF EXISTS schema_migrations CASCADE;
		`,
	},
	// v2 - Add views
	{
		Version: 2,
		Name:    "add_views",
		UpSQL: `
			-- Create views
			CREATE OR REPLACE VIEW v_latest_blocks AS
			SELECT * FROM blocks ORDER BY number DESC LIMIT 100;
			
			CREATE OR REPLACE VIEW v_latest_transactions AS
			SELECT * FROM transactions ORDER BY timestamp DESC LIMIT 100;
			
			CREATE OR REPLACE VIEW v_pending_transactions AS
			SELECT hash, from_address, to_address, value, gas_price, nonce, created_at
			FROM transactions WHERE status = 0 ORDER BY gas_price DESC, created_at ASC;
			
			CREATE OR REPLACE VIEW v_top_tokens AS
			SELECT * FROM tokens ORDER BY transfers_count DESC LIMIT 50;
			
			CREATE OR REPLACE VIEW v_top_collections AS
			SELECT * FROM collections ORDER BY volume_total DESC LIMIT 50;
			
			CREATE OR REPLACE VIEW v_verified_contracts AS
			SELECT address, name, compiler, version, is_verified, is_proxy, proxy_implementation, hits_count
			FROM contracts WHERE is_verified = TRUE ORDER BY hits_count DESC;
			
			CREATE OR REPLACE VIEW v_network_stats AS
			SELECT 
				(SELECT COUNT(*) FROM blocks) as total_blocks,
				(SELECT COUNT(*) FROM transactions WHERE status = 1) as total_transactions,
				(SELECT COUNT(DISTINCT from_address) FROM transactions) as unique_senders,
				(SELECT COUNT(*) FROM tokens) as total_tokens,
				(SELECT COUNT(*) FROM collections) as total_collections,
				(SELECT COUNT(*) FROM nfts) as total_nfts,
				(SELECT SUM(gas_used) FROM blocks) as total_gas_used,
				(SELECT AVG(avg_gas_price) FROM gas_prices ORDER BY timestamp DESC LIMIT 1) as current_gas_price;
		`,
		DownSQL: `
			DROP VIEW IF EXISTS v_latest_blocks;
			DROP VIEW IF EXISTS v_latest_transactions;
			DROP VIEW IF EXISTS v_pending_transactions;
			DROP VIEW IF EXISTS v_top_tokens;
			DROP VIEW IF EXISTS v_top_collections;
			DROP VIEW IF EXISTS v_verified_contracts;
			DROP VIEW IF EXISTS v_network_stats;
		`,
	},
	// v3 - Add functions
	{
		Version: 3,
		Name:    "add_functions",
		UpSQL: `
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
			
			-- Trigger for token holder updates
			DROP TRIGGER IF EXISTS trigger_token_holder_count ON token_holders;
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
			
			-- Trigger for NFT owner count
			DROP TRIGGER IF EXISTS trigger_nft_owner_count ON nft_holders;
			CREATE TRIGGER trigger_nft_owner_count
			AFTER INSERT OR UPDATE ON nft_holders
			FOR EACH ROW EXECUTE FUNCTION update_collection_owner_count();
			
			-- Function to calculate block rewards
			CREATE OR REPLACE FUNCTION calculate_block_reward(block_num BIGINT)
			RETURNS VARCHAR AS $$
			DECLARE
				base_reward VARCHAR := '2000000000000000000';
				decade INTEGER;
			BEGIN
				decade := block_num / 63072000;
				IF decade > 0 THEN
					RETURN (2000000000000000000 * POWER(0.95, decade))::VARCHAR;
				END IF;
				RETURN base_reward;
			END;
			$$ LANGUAGE plpgsql;
			
			-- Search function
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
		`,
		DownSQL: `
			DROP FUNCTION IF EXISTS update_token_holder_count();
			DROP FUNCTION IF EXISTS update_collection_owner_count();
			DROP FUNCTION IF EXISTS calculate_block_reward(BIGINT);
			DROP FUNCTION IF EXISTS search_explorer(TEXT);
		`,
	},
}

// =============================================================================
// MIGRATION RUNNER
// =============================================================================

// Migrate runs all pending migrations
func Migrate(ctx context.Context, pool *pgxpool.Pool) error {
	for _, m := range migrations {
		// Check if already applied
		var count int
		err := pool.QueryRow(ctx, "SELECT COUNT(*) FROM schema_migrations WHERE version = $1", m.Version).Scan(&count)
		if err != nil {
			return fmt.Errorf("failed to check migration %d: %w", m.Version, err)
		}

		if count > 0 {
			fmt.Printf("Migration %d (%s) already applied, skipping\n", m.Version, m.Name)
			continue
		}

		fmt.Printf("Applying migration %d (%s)...\n", m.Version, m.Name)

		// Apply migration
		_, err = pool.Exec(ctx, m.UpSQL)
		if err != nil {
			return fmt.Errorf("failed to apply migration %d: %w", m.Version, err)
		}

		// Record migration
		_, err = pool.Exec(ctx, "INSERT INTO schema_migrations (version, name) VALUES ($1, $2)", m.Version, m.Name)
		if err != nil {
			return fmt.Errorf("failed to record migration %d: %w", m.Version, err)
		}

		fmt.Printf("Migration %d (%s) applied successfully\n", m.Version, m.Name)
	}

	return nil
}

// Rollback rolls back the last migration
func Rollback(ctx context.Context, pool *pgxpool.Pool) error {
	// Get latest applied migration
	var m Migration
	err := pool.QueryRow(ctx, "SELECT version, name FROM schema_migrations ORDER BY version DESC LIMIT 1").Scan(&m.Version, &m.Name)
	if err != nil {
		return fmt.Errorf("no migrations to rollback: %w", err)
	}

	// Find migration
	var migration *Migration
	for _, mig := range migrations {
		if mig.Version == m.Version {
			migration = mig
			break
		}
	}

	if migration == nil {
		return fmt.Errorf("migration %d not found", m.Version)
	}

	fmt.Printf("Rolling back migration %d (%s)...\n", m.Version, m.Name)

	// Rollback
	_, err = pool.Exec(ctx, migration.DownSQL)
	if err != nil {
		return fmt.Errorf("failed to rollback migration %d: %w", m.Version, err)
	}

	// Remove migration record
	_, err = pool.Exec(ctx, "DELETE FROM schema_migrations WHERE version = $1", m.Version)
	if err != nil {
		return fmt.Errorf("failed to remove migration record %d: %w", m.Version, err)
	}

	fmt.Printf("Migration %d (%s) rolled back successfully\n", m.Version, m.Name)

	return nil
}

// GetMigrationStatus returns the status of all migrations
func GetMigrationStatus(ctx context.Context, pool *pgxpool.Pool) ([]*Migration, error) {
	rows, err := pool.Query(ctx, "SELECT version, name, applied_at FROM schema_migrations ORDER BY version")
	if err != nil {
		return nil, err
	}
	defer rows.Close()

	var applied []*Migration
	for rows.Next() {
		var m Migration
		if err := rows.Scan(&m.Version, &m.Name, &m.AppliedAt); err != nil {
			return nil, err
		}
		applied = append(applied, &m)
	}

	return applied, rows.Err()
}