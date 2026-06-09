// Package admin provides database migrations.
package admin

import (
	"database/sql"
	"fmt"
	"time"
)

// Migration represents a database migration.
type Migration struct {
	Version    string
	Name      string
	Migrate   func(*sql.DB) error
	Rollback  func(*sql.DB) error
}

// RunMigrations runs all pending migrations.
func RunMigrations(db *sql.DB) error {
	// Create migrations table
	if _, err := db.Exec(`
		CREATE TABLE IF NOT EXISTS migrations (
			version VARCHAR(50) PRIMARY KEY,
			name VARCHAR(255) NOT NULL,
			applied_at BIGINT NOT NULL
		)
	`); err != nil {
		return fmt.Errorf("failed to create migrations table: %w", err)
	}

	migrations := getMigrations()
	
	for _, m := range migrations {
		// Check if already applied
		var exists bool
		err := db.QueryRow("SELECT EXISTS(SELECT 1 FROM migrations WHERE version = $1)", m.Version).Scan(&exists)
		if err != nil {
			return fmt.Errorf("failed to check migration: %w", err)
		}
		
		if exists {
			fmt.Printf("Migration %s already applied\n", m.Version)
			continue
		}
		
		// Apply migration
		fmt.Printf("Applying migration %s: %s\n", m.Version, m.Name)
		if err := m.Migrate(db); err != nil {
			return fmt.Errorf("failed to apply migration %s: %w", m.Version, err)
		}
		
		// Record migration
		if _, err := db.Exec(
			"INSERT INTO migrations (version, name, applied_at) VALUES ($1, $2, $3)",
			m.Version, m.Name, time.Now().Unix(),
		); err != nil {
			return fmt.Errorf("failed to record migration: %w", err)
		}
		
		fmt.Printf("Migration %s applied successfully\n", m.Version)
	}
	
	return nil
}

// RollbackMigrations rolls back migrations to a specific version.
func RollbackMigrations(db *sql.DB, toVersion string) error {
	migrations := getMigrations()
	
	// Find target version
	targetIndex := -1
	for i, m := range migrations {
		if m.Version == toVersion {
			targetIndex = i
			break
		}
	}
	
	if targetIndex < 0 {
		return fmt.Errorf("version %s not found", toVersion)
	}
	
	// Rollback migrations after target
	for i := len(migrations) - 1; i > targetIndex; i-- {
		m := migrations[i]
		
		// Check if applied
		var exists bool
		err := db.QueryRow("SELECT EXISTS(SELECT 1 FROM migrations WHERE version = $1)", m.Version).Scan(&exists)
		if err != nil {
			return fmt.Errorf("failed to check migration: %w", err)
		}
		
		if !exists {
			continue
		}
		
		// Rollback migration
		fmt.Printf("Rolling back migration %s: %s\n", m.Version, m.Name)
		if m.Rollback != nil {
			if err := m.Rollback(db); err != nil {
				return fmt.Errorf("failed to rollback migration %s: %w", m.Version, err)
			}
		}
		
		// Remove migration record
		if _, err := db.Exec("DELETE FROM migrations WHERE version = $1", m.Version); err != nil {
			return fmt.Errorf("failed to remove migration record: %w", err)
		}
		
		fmt.Printf("Migration %s rolled back successfully\n", m.Version)
	}
	
	return nil
}

func getMigrations() []Migration {
	return []Migration{
		{
			Version: "2024060901",
			Name: "create_initial_tables",
			Migrate: func(db *sql.DB) error {
				_, err := db.Exec(`
					CREATE TABLE IF NOT EXISTS users (
						id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
						username VARCHAR(50) UNIQUE NOT NULL,
						email VARCHAR(255) UNIQUE NOT NULL,
						password_hash VARCHAR(255) NOT NULL,
						salt VARCHAR(255) NOT NULL,
						role SMALLINT NOT NULL DEFAULT 2,
						status SMALLINT NOT NULL DEFAULT 0,
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
						verification_expires_at BIGINT
					);
					
					CREATE TABLE IF NOT EXISTS products (
						id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
						name VARCHAR(100) NOT NULL,
						brand_name VARCHAR(100) NOT NULL,
						domain VARCHAR(255) UNIQUE NOT NULL,
						cloud VARCHAR(50) NOT NULL DEFAULT 'default',
						storage VARCHAR(50) NOT NULL DEFAULT 'default',
						status SMALLINT NOT NULL DEFAULT 0,
						owner_id UUID NOT NULL,
						admin_id UUID,
						created_at BIGINT NOT NULL,
						updated_at BIGINT NOT NULL,
						api_keys JSONB DEFAULT '{}',
						features JSONB DEFAULT '[]',
						config JSONB DEFAULT '{}',
						auth_required BOOLEAN DEFAULT TRUE,
						is_paused BOOLEAN DEFAULT FALSE,
						is_halted BOOLEAN DEFAULT FALSE,
						paused_by UUID,
						halted_by UUID,
						paused_at BIGINT,
						halted_at BIGINT
					);
					
					CREATE TABLE IF NOT EXISTS api_keys (
						id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
						product_id UUID NOT NULL,
						key_hash VARCHAR(255) NOT NULL,
						name VARCHAR(100) NOT NULL,
						created_at BIGINT NOT NULL,
						expires_at BIGINT,
						active BOOLEAN DEFAULT TRUE,
						last_used BIGINT,
						usage_count INTEGER DEFAULT 0
					);
					
					CREATE TABLE IF NOT EXISTS sessions (
						id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
						user_id UUID NOT NULL,
						token_hash VARCHAR(255) NOT NULL UNIQUE,
						ip_address INET NOT NULL,
						user_agent TEXT,
						created_at BIGINT NOT NULL,
						expires_at BIGINT NOT NULL,
						last_activity BIGINT
					);
					
					CREATE TABLE IF NOT EXISTS audit_logs (
						id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
						user_id UUID,
						action VARCHAR(50) NOT NULL,
						ip_address INET,
						user_agent TEXT,
						details TEXT,
						timestamp BIGINT NOT NULL,
						success BOOLEAN DEFAULT TRUE,
						product_id UUID
					);
					
					CREATE TABLE IF NOT EXISTS permissions (
						id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
						user_id UUID NOT NULL,
						permission VARCHAR(100) NOT NULL,
						granted_by UUID,
						granted_at BIGINT NOT NULL,
						expires_at BIGINT
					);
					
					CREATE TABLE IF NOT EXISTS rate_limits (
						id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
						key VARCHAR(255) NOT NULL UNIQUE,
						requests JSONB DEFAULT '[]',
						max_requests INTEGER DEFAULT 60,
						window_seconds INTEGER DEFAULT 60,
						blocked_until BIGINT
					);
					
					CREATE TABLE IF NOT EXISTS blocked_ips (
						id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
						ip_address INET NOT NULL UNIQUE,
						reason TEXT,
						blocked_by UUID,
						blocked_at BIGINT NOT NULL,
						expires_at BIGINT
					);
				`)
				return err
			},
			Rollback: func(db *sql.DB) error {
				_, err := db.Exec(`
					DROP TABLE IF EXISTS blocked_ips CASCADE;
					DROP TABLE IF EXISTS rate_limits CASCADE;
					DROP TABLE IF EXISTS permissions CASCADE;
					DROP TABLE IF EXISTS audit_logs CASCADE;
					DROP TABLE IF EXISTS sessions CASCADE;
					DROP TABLE IF EXISTS api_keys CASCADE;
					DROP TABLE IF EXISTS products CASCADE;
					DROP TABLE IF EXISTS users CASCADE;
				`)
				return err
			},
		},
		{
			Version: "2024060902",
			Name: "add_indexes",
			Migrate: func(db *sql.DB) error {
				_, err := db.Exec(`
					CREATE INDEX IF NOT EXISTS idx_users_email ON users(email);
					CREATE INDEX IF NOT EXISTS idx_users_username ON users(username);
					CREATE INDEX IF NOT EXISTS idx_users_role ON users(role);
					CREATE INDEX IF NOT EXISTS idx_users_status ON users(status);
					CREATE INDEX IF NOT EXISTS idx_users_product_id ON users(product_id);
					CREATE INDEX IF NOT EXISTS idx_products_owner_id ON products(owner_id);
					CREATE INDEX IF NOT EXISTS idx_products_status ON products(status);
					CREATE INDEX IF NOT EXISTS idx_products_domain ON products(domain);
					CREATE INDEX IF NOT EXISTS idx_api_keys_product_id ON api_keys(product_id);
					CREATE INDEX IF NOT EXISTS idx_api_keys_key_hash ON api_keys(key_hash);
					CREATE INDEX IF NOT EXISTS idx_sessions_user_id ON sessions(user_id);
					CREATE INDEX IF NOT EXISTS idx_sessions_token_hash ON sessions(token_hash);
					CREATE INDEX IF NOT EXISTS idx_sessions_expires_at ON sessions(expires_at);
					CREATE INDEX IF NOT EXISTS idx_audit_logs_user_id ON audit_logs(user_id);
					CREATE INDEX IF NOT EXISTS idx_audit_logs_timestamp ON audit_logs(timestamp DESC);
					CREATE INDEX IF NOT EXISTS idx_audit_logs_action ON audit_logs(action);
					CREATE INDEX IF NOT EXISTS idx_permissions_user_id ON permissions(user_id);
					CREATE INDEX IF NOT EXISTS idx_rate_limits_key ON rate_limits(key);
					CREATE INDEX IF NOT EXISTS idx_blocked_ips_ip ON blocked_ips(ip_address);
				`)
				return err
			},
			Rollback: func(db *sql.DB) error {
				_, err := db.Exec(`
					DROP INDEX IF EXISTS idx_users_email;
					DROP INDEX IF EXISTS idx_users_username;
					DROP INDEX IF EXISTS idx_users_role;
					DROP INDEX IF EXISTS idx_users_status;
					DROP INDEX IF EXISTS idx_users_product_id;
					DROP INDEX IF EXISTS idx_products_owner_id;
					DROP INDEX IF EXISTS idx_products_status;
					DROP INDEX IF EXISTS idx_products_domain;
					DROP INDEX IF EXISTS idx_api_keys_product_id;
					DROP INDEX IF EXISTS idx_api_keys_key_hash;
					DROP INDEX IF EXISTS idx_sessions_user_id;
					DROP INDEX IF EXISTS idx_sessions_token_hash;
					DROP INDEX IF EXISTS idx_sessions_expires_at;
					DROP INDEX IF EXISTS idx_audit_logs_user_id;
					DROP INDEX IF EXISTS idx_audit_logs_timestamp;
					DROP INDEX IF EXISTS idx_audit_logs_action;
					DROP INDEX IF EXISTS idx_permissions_user_id;
					DROP INDEX IF EXISTS idx_rate_limits_key;
					DROP INDEX IF EXISTS idx_blocked_ips_ip;
				`)
				return err
			},
		},
	}
}

// GetMigrations returns all migrations.
func GetMigrations() []Migration {
	return getMigrations()
}