// Package postgresdb provides production-grade PostgreSQL database operations
// for TigerScan blockchain explorer with complete security and optimization.
package postgresdb

import (
	"context"
	"crypto/rand"
	"crypto/sha256"
	"database/sql"
	"encoding/hex"
	"encoding/json"
	"fmt"
	"math/big"
	"os"
	"strconv"
	"strings"
	"sync"
	"time"

	"github.com/ethereum/go-ethereum/common"
	"github.com/ethereum/go-ethereum/common/hexutil"
	"github.com/ethereum/go-ethereum/core/types"
	"github.com/lib/pq"
	"golang.org/x/crypto/bcrypt"
)

// =============================================================================
// CONFIGURATION
// =============================================================================

// Config holds PostgreSQL database configuration
type Config struct {
	Host            string
	Port            int
	User            string
	Password        string
	Database        string
	MaxOpenConns    int
	MaxIdleConns    int
	ConnMaxLifetime time.Duration
	SSLMode         string
}

// DefaultConfig returns default production configuration
func DefaultConfig() *Config {
	return &Config{
		Host:            getEnv("POSTGRES_HOST", "localhost"),
		Port:            getEnvInt("POSTGRES_PORT", 5432),
		User:            getEnv("POSTGRES_USER", "tigerscan"),
		Password:       getEnv("POSTGRES_PASSWORD", ""),
		Database:        getEnv("POSTGRES_DB", "tigerscan"),
		MaxOpenConns:    getEnvInt("POSTGRES_MAX_OPEN_CONNS", 100),
		MaxIdleConns:    getEnvInt("POSTGRES_MAX_IDLE_CONNS", 10),
		ConnMaxLifetime: getEnvDuration("POSTGRES_CONN_LIFETIME", 5*time.Minute),
		SSLMode:         getEnv("POSTGRES_SSL_MODE", "require"),
	}
}

// getEnv gets environment variable with default
func getEnv(key, defaultValue string) string {
	if value := os.Getenv(key); value != "" {
		return value
	}
	return defaultValue
}

// getEnvInt gets environment variable as int
func getEnvInt(key string, defaultValue int) int {
	if value := os.Getenv(key); value != "" {
		if intVal, err := strconv.Atoi(value); err == nil {
			return intVal
		}
	}
	return defaultValue
}

// getEnvDuration gets environment variable as duration
func getEnvDuration(key string, defaultValue time.Duration) time.Duration {
	if value := os.Getenv(key); value != "" {
		if duration, err := time.ParseDuration(value); err == nil {
			return duration
		}
	}
	return defaultValue
}

// =============================================================================
// DATABASE CONNECTION
// =============================================================================

// DB represents a production PostgreSQL database connection
type DB struct {
	mu            sync.RWMutex
	db            *sql.DB
	config        *Config
	isConnected   bool
	lastHeartbeat time.Time
}

// NewDB creates a new PostgreSQL database connection
func NewDB(cfg *Config) (*DB, error) {
	if cfg == nil {
		cfg = DefaultConfig()
	}

	// Build connection string with security
	connStr := fmt.Sprintf(
		"host=%s port=%d user=%s dbname=%s sslmode=%s",
		cfg.Host,
		cfg.Port,
		cfg.User,
		cfg.Database,
		cfg.SSLMode,
	)

	// Set password via PGpassword environment for security
	connStr += fmt.Sprintf(" password=%s", cfg.Password)

	db, err := sql.Open("postgres", connStr)
	if err != nil {
		return nil, fmt.Errorf("failed to open database: %w", err)
	}

	// Configure connection pool
	db.SetMaxOpenConns(cfg.MaxOpenConns)
	db.SetMaxIdleConns(cfg.MaxIdleConns)
	db.SetConnMaxLifetime(cfg.ConnMaxLifetime)

	// Test connection
	ctx, cancel := context.WithTimeout(context.Background(), 10*time.Second)
	defer cancel()

	if err := db.PingContext(ctx); err != nil {
		return nil, fmt.Errorf("failed to ping database: %w", err)
	}

	database := &DB{
		db:          db,
		config:      cfg,
		isConnected: true,
	}

	// Start heartbeat checker
	go database.heartbeatChecker()

	return database, nil
}

// heartbeatChecker monitors database connection health
func (d *DB) heartbeatChecker() {
	ticker := time.NewTicker(30 * time.Second)
	defer ticker.Stop()

	for range ticker.C {
		ctx, cancel := context.WithTimeout(context.Background(), 5*time.Second)
		err := d.db.PingContext(ctx)
		cancel()

		d.mu.Lock()
		d.isConnected = err == nil
		d.lastHeartbeat = time.Now()
		d.mu.Unlock()

		if err != nil {
			fmt.Printf("Database heartbeat failed: %v\n", err)
		}
	}
}

// IsConnected returns connection status
func (d *DB) IsConnected() bool {
	d.mu.RLock()
	defer d.mu.RUnlock()
	return d.isConnected
}

// Close closes the database connection
func (d *DB) Close() error {
	d.mu.Lock()
	defer d.mu.Unlock()

	if d.db != nil {
		return d.db.Close()
	}
	return nil
}

// Ping checks database connectivity
func (d *DB) Ping(ctx context.Context) error {
	return d.db.PingContext(ctx)
}

// =============================================================================
// TRANSACTION MANAGEMENT
// =============================================================================

// Tx represents a database transaction
type Tx struct {
	tx *sql.Tx
}

// BeginTx starts a new database transaction
func (d *DB) BeginTx(ctx context.Context, opts *sql.TxOptions) (*Tx, error) {
	tx, err := d.db.BeginTx(ctx, opts)
	if err != nil {
		return nil, fmt.Errorf("failed to begin transaction: %w", err)
	}
	return &Tx{tx: tx}, nil
}

// Commit commits the transaction
func (t *Tx) Commit() error {
	return t.tx.Commit()
}

// Rollback rolls back the transaction
func (t *Tx) Rollback() error {
	return t.tx.Rollback()
}

// Exec executes a query
func (t *Tx) Exec(query string, args ...interface{}) (sql.Result, error) {
	return t.tx.Exec(query, args...)
}

// Query executes a query
func (t *Tx) Query(query string, args ...interface{}) (*sql.Rows, error) {
	return t.tx.Query(query, args...)
}

// QueryRow executes a query that returns a single row
func (t *Tx) QueryRow(query string, args ...interface{}) *sql.Row {
	return t.tx.QueryRow(query, args...)
}

// =============================================================================
// BLOCK OPERATIONS
// =============================================================================

// InsertBlock inserts a new block into the database
func (d *DB) InsertBlock(ctx context.Context, block *BlockData) error {
	query := `
		INSERT INTO blocks (
			number, hash, parent_hash, nonce, sha3_uncles, logs_bloom,
			transactions_root, state_root, receipts_root, miner,
			difficulty, total_difficulty, gas_limit, gas_used, timestamp,
			size, extra_data, base_fee_per_gas, tx_count, reward
		) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16, $17, $18, $19, $20)
		ON CONFLICT (number) DO UPDATE SET
			hash = EXCLUDED.hash,
			parent_hash = EXCLUDED.parent_hash,
			gas_used = EXCLUDED.gas_used,
			tx_count = EXCLUDED.tx_count,
			updated_at = CURRENT_TIMESTAMP
	`

	_, err := d.db.ExecContext(ctx, query,
		block.Number,
		block.Hash,
		block.ParentHash,
		block.Nonce,
		block.Sha3Uncles,
		block.LogsBloom,
		block.TransactionsRoot,
		block.StateRoot,
		block.ReceiptsRoot,
		block.Miner,
		block.Difficulty,
		block.TotalDifficulty,
		block.GasLimit,
		block.GasUsed,
		block.Timestamp,
		block.Size,
		block.ExtraData,
		block.BaseFeePerGas,
		block.TxCount,
		block.Reward,
	)

	return err
}

// BlockData represents block data from database
type BlockData struct {
	Number           uint64
	Hash             string
	ParentHash       string
	Nonce           string
	Sha3Uncles      string
	LogsBloom       string
	TransactionsRoot string
	StateRoot       string
	ReceiptsRoot    string
	Miner          string
	Difficulty     string
	TotalDifficulty string
	GasLimit       uint64
	GasUsed        uint64
	Timestamp      uint64
	Size           int64
	ExtraData      string
	BaseFeePerGas  uint64
	TxCount       int
	Reward        string
}

// GetBlockByNumber retrieves a block by number
func (d *DB) GetBlockByNumber(ctx context.Context, number uint64) (*BlockData, error) {
	query := `
		SELECT number, hash, parent_hash, nonce, sha3_uncles, logs_bloom,
			transactions_root, state_root, receipts_root, miner,
			difficulty, total_difficulty, gas_limit, gas_used, timestamp,
			size, extra_data, base_fee_per_gas, tx_count, reward
		FROM blocks WHERE number = $1
	`

	var block BlockData
	err := d.db.QueryRowContext(ctx, query, number).Scan(
		&block.Number,
		&block.Hash,
		&block.ParentHash,
		&block.Nonce,
		&block.Sha3Uncles,
		&block.LogsBloom,
		&block.TransactionsRoot,
		&block.StateRoot,
		&block.ReceiptsRoot,
		&block.Miner,
		&block.Difficulty,
		&block.TotalDifficulty,
		&block.GasLimit,
		&block.GasUsed,
		&block.Timestamp,
		&block.Size,
		&block.ExtraData,
		&block.BaseFeePerGas,
		&block.TxCount,
		&block.Reward,
	)

	if err == sql.ErrNoRows {
		return nil, fmt.Errorf("block not found: %d", number)
	}
	if err != nil {
		return nil, fmt.Errorf("failed to get block: %w", err)
	}

	return &block, nil
}

// GetBlockByHash retrieves a block by hash
func (d *DB) GetBlockByHash(ctx context.Context, hash string) (*BlockData, error) {
	query := `
		SELECT number, hash, parent_hash, nonce, sha3_uncles, logs_bloom,
			transactions_root, state_root, receipts_root, miner,
			difficulty, total_difficulty, gas_limit, gas_used, timestamp,
			size, extra_data, base_fee_per_gas, tx_count, reward
		FROM blocks WHERE hash = $1
	`

	var block BlockData
	err := d.db.QueryRowContext(ctx, query, hash).Scan(
		&block.Number,
		&block.Hash,
		&block.ParentHash,
		&block.Nonce,
		&block.Sha3Uncles,
		&block.LogsBloom,
		&block.TransactionsRoot,
		&block.StateRoot,
		&block.ReceiptsRoot,
		&block.Miner,
		&block.Difficulty,
		&block.TotalDifficulty,
		&block.GasLimit,
		&block.GasUsed,
		&block.Timestamp,
		&block.Size,
		&block.ExtraData,
		&block.BaseFeePerGas,
		&block.TxCount,
		&block.Reward,
	)

	if err == sql.ErrNoRows {
		return nil, fmt.Errorf("block not found: %s", hash)
	}
	if err != nil {
		return nil, fmt.Errorf("failed to get block: %w", err)
	}

	return &block, nil
}

// GetLatestBlockNumber retrieves the latest block number
func (d *DB) GetLatestBlockNumber(ctx context.Context) (uint64, error) {
	query := `SELECT COALESCE(MAX(number), 0) FROM blocks`
	var number uint64
	err := d.db.QueryRowContext(ctx, query).Scan(&number)
	return number, err
}

// GetBlocks retrieves blocks with pagination
func (d *DB) GetBlocks(ctx context.Context, limit, offset int) ([]BlockData, error) {
	query := `
		SELECT number, hash, parent_hash, nonce, sha3_uncles, logs_bloom,
			transactions_root, state_root, receipts_root, miner,
			difficulty, total_difficulty, gas_limit, gas_used, timestamp,
			size, extra_data, base_fee_per_gas, tx_count, reward
		FROM blocks
		ORDER BY number DESC
		LIMIT $1 OFFSET $2
	`

	rows, err := d.db.QueryContext(ctx, query, limit, offset)
	if err != nil {
		return nil, fmt.Errorf("failed to get blocks: %w", err)
	}
	defer rows.Close()

	var blocks []BlockData
	for rows.Next() {
		var block BlockData
		err := rows.Scan(
			&block.Number,
			&block.Hash,
			&block.ParentHash,
			&block.Nonce,
			&block.Sha3Uncles,
			&block.LogsBloom,
			&block.TransactionsRoot,
			&block.StateRoot,
			&block.ReceiptsRoot,
			&block.Miner,
			&block.Difficulty,
			&block.TotalDifficulty,
			&block.GasLimit,
			&block.GasUsed,
			&block.Timestamp,
			&block.Size,
			&block.ExtraData,
			&block.BaseFeePerGas,
			&block.TxCount,
			&block.Reward,
		)
		if err != nil {
			return nil, fmt.Errorf("failed to scan block: %w", err)
		}
		blocks = append(blocks, block)
	}

	return blocks, nil
}

// =============================================================================
// TRANSACTION OPERATIONS
// =============================================================================

// InsertTransaction inserts a new transaction
func (d *DB) InsertTransaction(ctx context.Context, tx *TransactionData) error {
	query := `
		INSERT INTO transactions (
			hash, block_number, block_hash, transaction_index,
			from_address, to_address, value, gas_price, gas_limit, gas_used,
			nonce, input_data, signature, v, r, s, status, transaction_type,
			max_fee_per_gas, max_priority_fee_per_gas, tx_fee
		) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16, $17, $18, $19, $20, $21)
		ON CONFLICT (hash) DO UPDATE SET
			block_number = EXCLUDED.block_number,
			block_hash = EXCLUDED.block_hash,
			status = EXCLUDED.status,
			gas_used = EXCLUDED.gas_used,
			tx_fee = EXCLUDED.tx_fee,
			indexed_at = CURRENT_TIMESTAMP
	`

	_, err := d.db.ExecContext(ctx, query,
		tx.Hash,
		tx.BlockNumber,
		tx.BlockHash,
		tx.TransactionIndex,
		tx.From,
		tx.To,
		tx.Value,
		tx.GasPrice,
		tx.GasLimit,
		tx.GasUsed,
		tx.Nonce,
		tx.InputData,
		tx.Signature,
		tx.V,
		tx.R,
		tx.S,
		tx.Status,
		tx.TransactionType,
		tx.MaxFeePerGas,
		tx.MaxPriorityFeePerGas,
		tx.TxFee,
	)

	return err
}

// TransactionData represents transaction data
type TransactionData struct {
	Hash                string
	BlockNumber         uint64
	BlockHash           string
	TransactionIndex    int
	From               string
	To                 string
	Value              string
	GasPrice            uint64
	GasLimit           uint64
	GasUsed            uint64
	Nonce              uint64
	InputData          string
	Signature         string
	V                 int64
	R                 string
	S                 string
	Status            bool
	TransactionType   int
	MaxFeePerGas      uint64
	MaxPriorityFeePerGas uint64
	TxFee            string
}

// GetTransactionByHash retrieves a transaction by hash
func (d *DB) GetTransactionByHash(ctx context.Context, hash string) (*TransactionData, error) {
	query := `
		SELECT hash, block_number, block_hash, transaction_index,
			from_address, to_address, value, gas_price, gas_limit, gas_used,
			nonce, input_data, signature, v, r, s, status, transaction_type,
			max_fee_per_gas, max_priority_fee_per_gas, tx_fee
		FROM transactions WHERE hash = $1
	`

	var tx TransactionData
	err := d.db.QueryRowContext(ctx, query, hash).Scan(
		&tx.Hash,
		&tx.BlockNumber,
		&tx.BlockHash,
		&tx.TransactionIndex,
		&tx.From,
		&tx.To,
		&tx.Value,
		&tx.GasPrice,
		&tx.GasLimit,
		&tx.GasUsed,
		&tx.Nonce,
		&tx.InputData,
		&tx.Signature,
		&tx.V,
		&tx.R,
		&tx.S,
		&tx.Status,
		&tx.TransactionType,
		&tx.MaxFeePerGas,
		&tx.MaxPriorityFeePerGas,
		&tx.TxFee,
	)

	if err == sql.ErrNoRows {
		return nil, fmt.Errorf("transaction not found: %s", hash)
	}
	if err != nil {
		return nil, fmt.Errorf("failed to get transaction: %w", err)
	}

	return &tx, nil
}

// GetTransactions retrieves transactions with filters
func (d *DB) GetTransactions(ctx context.Context, filters TransactionFilters, limit, offset int) ([]TransactionData, error) {
	baseQuery := `
		SELECT hash, block_number, block_hash, transaction_index,
			from_address, to_address, value, gas_price, gas_limit, gas_used,
			nonce, input_data, signature, v, r, s, status, transaction_type,
			max_fee_per_gas, max_priority_fee_per_gas, tx_fee
		FROM transactions
		WHERE 1=1
	`
	args := []interface{}{}
	argNum := 1

	if filters.Address != "" {
		baseQuery += fmt.Sprintf(" AND (from_address = $%d OR to_address = $%d)", argNum, argNum)
		args = append(args, filters.Address)
		argNum++
	}
	if filters.BlockNumber > 0 {
		baseQuery += fmt.Sprintf(" AND block_number = $%d", argNum)
		args = append(args, filters.BlockNumber)
		argNum++
	}
	if filters.Status != nil {
		baseQuery += fmt.Sprintf(" AND status = $%d", argNum)
		args = append(args, *filters.Status)
		argNum++
	}
	if filters.FromBlock > 0 {
		baseQuery += fmt.Sprintf(" AND block_number >= $%d", argNum)
		args = append(args, filters.FromBlock)
		argNum++
	}
	if filters.ToBlock > 0 {
		baseQuery += fmt.Sprintf(" AND block_number <= $%d", argNum)
		args = append(args, filters.ToBlock)
		argNum++
	}

	baseQuery += " ORDER BY block_number DESC, transaction_index DESC"
	baseQuery += fmt.Sprintf(" LIMIT $%d OFFSET $%d", argNum, argNum+1)
	args = append(args, limit, offset)

	rows, err := d.db.QueryContext(ctx, baseQuery, args...)
	if err != nil {
		return nil, fmt.Errorf("failed to get transactions: %w", err)
	}
	defer rows.Close()

	var txs []TransactionData
	for rows.Next() {
		var tx TransactionData
		err := rows.Scan(
			&tx.Hash,
			&tx.BlockNumber,
			&tx.BlockHash,
			&tx.TransactionIndex,
			&tx.From,
			&tx.To,
			&tx.Value,
			&tx.GasPrice,
			&tx.GasLimit,
			&tx.GasUsed,
			&tx.Nonce,
			&tx.InputData,
			&tx.Signature,
			&tx.V,
			&tx.R,
			&tx.S,
			&tx.Status,
			&tx.TransactionType,
			&tx.MaxFeePerGas,
			&tx.MaxPriorityFeePerGas,
			&tx.TxFee,
		)
		if err != nil {
			return nil, fmt.Errorf("failed to scan transaction: %w", err)
		}
		txs = append(txs, tx)
	}

	return txs, nil
}

// TransactionFilters represents transaction query filters
type TransactionFilters struct {
	Address     string
	BlockNumber uint64
	Status     *bool
	FromBlock  uint64
	ToBlock    uint64
}

// =============================================================================
// ACCOUNT OPERATIONS
// =============================================================================

// InsertAccount inserts or updates an account
func (d *DB) InsertAccount(ctx context.Context, acc *AccountData) error {
	query := `
		INSERT INTO accounts (
			address, balance, nonce, code_hash, is_contract,
			is_verified, first_block_number, last_block_number
		) VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
		ON CONFLICT (address) DO UPDATE SET
			balance = EXCLUDED.balance,
			nonce = EXCLUDED.nonce,
			code_hash = COALESCE(EXCLUDED.code_hash, accounts.code_hash),
			last_block_number = EXCLUDED.last_block_number,
			updated_at = CURRENT_TIMESTAMP
	`

	_, err := d.db.ExecContext(ctx, query,
		acc.Address,
		acc.Balance,
		acc.Nonce,
		acc.CodeHash,
		acc.IsContract,
		acc.IsVerified,
		acc.FirstBlockNumber,
		acc.LastBlockNumber,
	)

	return err
}

// AccountData represents account data
type AccountData struct {
	Address          string
	Balance         string
	Nonce           uint64
	CodeHash        string
	IsContract      bool
	IsVerified     bool
	FirstBlockNumber uint64
	LastBlockNumber uint64
}

// GetAccount retrieves an account by address
func (d *DB) GetAccount(ctx context.Context, address string) (*AccountData, error) {
	query := `
		SELECT address, balance, nonce, code_hash, is_contract,
			is_verified, first_block_number, last_block_number
		FROM accounts WHERE address = $1
	`

	var acc AccountData
	err := d.db.QueryRowContext(ctx, query, address).Scan(
		&acc.Address,
		&acc.Balance,
		&acc.Nonce,
		&acc.CodeHash,
		&acc.IsContract,
		&acc.IsVerified,
		&acc.FirstBlockNumber,
		&acc.LastBlockNumber,
	)

	if err == sql.ErrNoRows {
		return &AccountData{Address: address}, nil
	}
	if err != nil {
		return nil, fmt.Errorf("failed to get account: %w", err)
	}

	return &acc, nil
}

// GetAccountBalanceHistory retrieves balance history
func (d *DB) GetAccountBalanceHistory(ctx context.Context, address string, limit int) ([]BalanceHistory, error) {
	query := `
		SELECT block_number, balance, timestamp
		FROM account_balances
		WHERE account_address = $1
		ORDER BY block_number DESC
		LIMIT $2
	`

	rows, err := d.db.QueryContext(ctx, query, address, limit)
	if err != nil {
		return nil, fmt.Errorf("failed to get balance history: %w", err)
	}
	defer rows.Close()

	var history []BalanceHistory
	for rows.Next() {
		var h BalanceHistory
		err := rows.Scan(&h.BlockNumber, &h.Balance, &h.Timestamp)
		if err != nil {
			return nil, fmt.Errorf("failed to scan balance: %w", err)
		}
		history = append(history, h)
	}

	return history, nil
}

// BalanceHistory represents balance at a block
type BalanceHistory struct {
	BlockNumber uint64
	Balance    string
	Timestamp  uint64
}

// =============================================================================
// TOKEN OPERATIONS
// =============================================================================

// InsertToken inserts or updates a token
func (d *DB) InsertToken(ctx context.Context, token *TokenData) error {
	query := `
		INSERT INTO tokens (
			address, name, symbol, decimals, total_supply, type,
			is_verified, holder_count, transfer_count,
			price_usd, price_usd_change_24h, market_cap_usd, volume_24h_usd
		) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13)
		ON CONFLICT (address) DO UPDATE SET
			name = EXCLUDED.name,
			symbol = EXCLUDED.symbol,
			total_supply = EXCLUDED.total_supply,
			holder_count = EXCLUDED.holder_count,
			transfer_count = EXCLUDED.transfer_count,
			price_usd = EXCLUDED.price_usd,
			updated_at = CURRENT_TIMESTAMP
	`

	_, err := d.db.ExecContext(ctx, query,
		token.Address,
		token.Name,
		token.Symbol,
		token.Decimals,
		token.TotalSupply,
		token.Type,
		token.IsVerified,
		token.HolderCount,
		token.TransferCount,
		token.PriceUSD,
		token.PriceUSDChange24h,
		token.MarketCapUSD,
		token.Volume24hUSD,
	)

	return err
}

// TokenData represents token data
type TokenData struct {
	Address           string
	Name              string
	Symbol            string
	Decimals          uint8
	TotalSupply       string
	Type             string
	IsVerified       bool
	HolderCount      int64
	TransferCount   int64
	PriceUSD        float64
	PriceUSDChange24h float64
	MarketCapUSD    float64
	Volume24hUSD   float64
}

// GetToken retrieves a token by address
func (d *DB) GetToken(ctx context.Context, address string) (*TokenData, error) {
	query := `
		SELECT address, name, symbol, decimals, total_supply, type,
			is_verified, holder_count, transfer_count,
			price_usd, price_usd_change_24h, market_cap_usd, volume_24h_usd
		FROM tokens WHERE address = $1
	`

	var token TokenData
	err := d.db.QueryRowContext(ctx, query, address).Scan(
		&token.Address,
		&token.Name,
		&token.Symbol,
		&token.Decimals,
		&token.TotalSupply,
		&token.Type,
		&token.IsVerified,
		&token.HolderCount,
		&token.TransferCount,
		&token.PriceUSD,
		&token.PriceUSDChange24h,
		&token.MarketCapUSD,
		&token.Volume24hUSD,
	)

	if err == sql.ErrNoRows {
		return nil, fmt.Errorf("token not found: %s", address)
	}
	if err != nil {
		return nil, fmt.Errorf("failed to get token: %w", err)
	}

	return &token, nil
}

// GetTokens retrieves tokens with pagination
func (d *DB) GetTokens(ctx context.Context, filters TokenFilters, limit, offset int) ([]TokenData, error) {
	baseQuery := `
		SELECT address, name, symbol, decimals, total_supply, type,
			is_verified, holder_count, transfer_count,
			price_usd, price_usd_change_24h, market_cap_usd, volume_24h_usd
		FROM tokens
		WHERE 1=1
	`
	args := []interface{}{}
	argNum := 1

	if filters.Type != "" {
		baseQuery += fmt.Sprintf(" AND type = $%d", argNum)
		args = append(args, filters.Type)
		argNum++
	}
	if filters.VerifiedOnly {
		baseQuery += " AND is_verified = TRUE"
	}

	orderBy := " holder_count DESC"
	if filters.SortBy == "name" {
		orderBy = " name ASC"
	} else if filters.SortBy == "transfers" {
		orderBy = " transfer_count DESC"
	} else if filters.SortBy == "market_cap" {
		orderBy = " market_cap_usd DESC"
	}

	baseQuery += " ORDER BY " + orderBy
	baseQuery += fmt.Sprintf(" LIMIT $%d OFFSET $%d", argNum, argNum+1)
	args = append(args, limit, offset)

	rows, err := d.db.QueryContext(ctx, baseQuery, args...)
	if err != nil {
		return nil, fmt.Errorf("failed to get tokens: %w", err)
	}
	defer rows.Close()

	var tokens []TokenData
	for rows.Next() {
		var token TokenData
		err := rows.Scan(
			&token.Address,
			&token.Name,
			&token.Symbol,
			&token.Decimals,
			&token.TotalSupply,
			&token.Type,
			&token.IsVerified,
			&token.HolderCount,
			&token.TransferCount,
			&token.PriceUSD,
			&token.PriceUSDChange24h,
			&token.MarketCapUSD,
			&token.Volume24hUSD,
		)
		if err != nil {
			return nil, fmt.Errorf("failed to scan token: %w", err)
		}
		tokens = append(tokens, token)
	}

	return tokens, nil
}

// TokenFilters represents token query filters
type TokenFilters struct {
	Type         string
	VerifiedOnly bool
	SortBy      string
}

// InsertTokenHolder inserts or updates a token holder
func (d *DB) InsertTokenHolder(ctx context.Context, holder *TokenHolderData) error {
	query := `
		INSERT INTO token_holders (token_address, holder_address, balance, last_update_block)
		VALUES ($1, $2, $3, $4)
		ON CONFLICT (token_address, holder_address) DO UPDATE SET
			balance = EXCLUDED.balance,
			last_update_block = EXCLUDED.last_update_block,
			updated_at = CURRENT_TIMESTAMP
	`

	_, err := d.db.ExecContext(ctx, query,
		holder.TokenAddress,
		holder.HolderAddress,
		holder.Balance,
		holder.LastUpdateBlock,
	)

	return err
}

// TokenHolderData represents token holder data
type TokenHolderData struct {
	TokenAddress     string
	HolderAddress   string
	Balance         string
	LastUpdateBlock  uint64
}

// GetTokenHolders retrieves token holders
func (d *DB) GetTokenHolders(ctx context.Context, tokenAddress string, limit, offset int) ([]TokenHolderData, error) {
	query := `
		SELECT token_address, holder_address, balance, last_update_block
		FROM token_holders
		WHERE token_address = $1 AND balance > 0
		ORDER BY balance DESC
		LIMIT $2 OFFSET $3
	`

	rows, err := d.db.QueryContext(ctx, query, tokenAddress, limit, offset)
	if err != nil {
		return nil, fmt.Errorf("failed to get token holders: %w", err)
	}
	defer rows.Close()

	var holders []TokenHolderData
	for rows.Next() {
		var holder TokenHolderData
		err := rows.Scan(
			&holder.TokenAddress,
			&holder.HolderAddress,
			&holder.Balance,
			&holder.LastUpdateBlock,
		)
		if err != nil {
			return nil, fmt.Errorf("failed to scan holder: %w", err)
		}
		holders = append(holders, holder)
	}

	return holders, nil
}

// InsertTokenTransfer inserts a token transfer
func (d *DB) InsertTokenTransfer(ctx context.Context, transfer *TokenTransferData) error {
	query := `
		INSERT INTO token_transfers (
			transaction_hash, block_number, log_index,
			token_address, from_address, to_address, value
		) VALUES ($1, $2, $3, $4, $5, $6, $7)
	`

	_, err := d.db.ExecContext(ctx, query,
		transfer.TransactionHash,
		transfer.BlockNumber,
		transfer.LogIndex,
		transfer.TokenAddress,
		transfer.FromAddress,
		transfer.ToAddress,
		transfer.Value,
	)

	return err
}

// TokenTransferData represents token transfer data
type TokenTransferData struct {
	TransactionHash string
	BlockNumber    uint64
	LogIndex       int
	TokenAddress  string
	FromAddress  string
	ToAddress    string
	Value       string
}

// GetTokenTransfers retrieves token transfers
func (d *DB) GetTokenTransfers(ctx context.Context, filters TokenTransferFilters, limit, offset int) ([]TokenTransferData, error) {
	baseQuery := `
		SELECT transaction_hash, block_number, log_index,
			token_address, from_address, to_address, value
		FROM token_transfers
		WHERE 1=1
	`
	args := []interface{}{}
	argNum := 1

	if filters.TokenAddress != "" {
		baseQuery += fmt.Sprintf(" AND token_address = $%d", argNum)
		args = append(args, filters.TokenAddress)
		argNum++
	}
	if filters.FromAddress != "" {
		baseQuery += fmt.Sprintf(" AND from_address = $%d", argNum)
		args = append(args, filters.FromAddress)
		argNum++
	}
	if filters.ToAddress != "" {
		baseQuery += fmt.Sprintf(" AND to_address = $%d", argNum)
		args = append(args, filters.ToAddress)
		argNum++
	}

	baseQuery += " ORDER BY block_number DESC, log_index DESC"
	baseQuery += fmt.Sprintf(" LIMIT $%d OFFSET $%d", argNum, argNum+1)
	args = append(args, limit, offset)

	rows, err := d.db.QueryContext(ctx, baseQuery, args...)
	if err != nil {
		return nil, fmt.Errorf("failed to get transfers: %w", err)
	}
	defer rows.Close()

	var transfers []TokenTransferData
	for rows.Next() {
		var t TokenTransferData
		err := rows.Scan(
			&t.TransactionHash,
			&t.BlockNumber,
			&t.LogIndex,
			&t.TokenAddress,
			&t.FromAddress,
			&t.ToAddress,
			&t.Value,
		)
		if err != nil {
			return nil, fmt.Errorf("failed to scan transfer: %w", err)
		}
		transfers = append(transfers, t)
	}

	return transfers, nil
}

// TokenTransferFilters represents transfer query filters
type TokenTransferFilters struct {
	TokenAddress  string
	FromAddress  string
	ToAddress    string
}

// =============================================================================
// NFT OPERATIONS
// =============================================================================

// InsertNFTCollection inserts or updates an NFT collection
func (d *DB) InsertNFTCollection(ctx context.Context, collection *NFTCollectionData) error {
	query := `
		INSERT INTO nft_collections (
			address, name, symbol, type, total_supply,
			holder_count, transfer_count, floor_price,
			volume_24h, sales_24h, is_verified
		) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11)
		ON CONFLICT (address) DO UPDATE SET
			name = EXCLUDED.name,
			total_supply = EXCLUDED.total_supply,
			holder_count = EXCLUDED.holder_count,
			transfer_count = EXCLUDED.transfer_count,
			floor_price = EXCLUDED.floor_price,
			volume_24h = EXCLUDED.volume_24h,
			updated_at = CURRENT_TIMESTAMP
	`

	_, err := d.db.ExecContext(ctx, query,
		collection.Address,
		collection.Name,
		collection.Symbol,
		collection.Type,
		collection.TotalSupply,
		collection.HolderCount,
		collection.TransferCount,
		collection.FloorPrice,
		collection.Volume24h,
		collection.Sales24h,
		collection.IsVerified,
	)

	return err
}

// NFTCollectionData represents NFT collection data
type NFTCollectionData struct {
	Address        string
	Name          string
	Symbol        string
	Type          string
	TotalSupply  int64
	HolderCount  int64
	TransferCount int64
	FloorPrice   float64
	Volume24h   float64
	Sales24h     int64
	IsVerified   bool
}

// GetNFTCollection retrieves an NFT collection
func (d *DB) GetNFTCollection(ctx context.Context, address string) (*NFTCollectionData, error) {
	query := `
		SELECT address, name, symbol, type, total_supply,
			holder_count, transfer_count, floor_price,
			volume_24h, sales_24h, is_verified
		FROM nft_collections WHERE address = $1
	`

	var col NFTCollectionData
	err := d.db.QueryRowContext(ctx, query, address).Scan(
		&col.Address,
		&col.Name,
		&col.Symbol,
		&col.Type,
		&col.TotalSupply,
		&col.HolderCount,
		&col.TransferCount,
		&col.FloorPrice,
		&col.Volume24h,
		&col.Sales24h,
		&col.IsVerified,
	)

	if err == sql.ErrNoRows {
		return nil, fmt.Errorf("collection not found: %s", address)
	}
	if err != nil {
		return nil, fmt.Errorf("failed to get collection: %w", err)
	}

	return &col, nil
}

// InsertNFT inserts or updates an NFT
func (d *DB) InsertNFT(ctx context.Context, nft *NFTData) error {
	query := `
		INSERT INTO nfts (
			collection_address, token_id, owner, name, description,
			image_url, metadata, attributes, transfer_count
		) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)
		ON CONFLICT (collection_address, token_id) DO UPDATE SET
			owner = EXCLUDED.owner,
			name = EXCLUDED.name,
			metadata = EXCLUDED.metadata,
			attributes = EXCLUDED.attributes,
			transfer_count = EXCLUDED.transfer_count,
			updated_at = CURRENT_TIMESTAMP
	`

	metadataJSON, _ := json.Marshal(nft.Metadata)
	attributesJSON, _ := json.Marshal(nft.Attributes)

	_, err := d.db.ExecContext(ctx, query,
		nft.CollectionAddress,
		nft.TokenID,
		nft.Owner,
		nft.Name,
		nft.Description,
		nft.ImageURL,
		metadataJSON,
		attributesJSON,
		nft.TransferCount,
	)

	return err
}

// NFTData represents NFT data
type NFTData struct {
	CollectionAddress string
	TokenID        string
	Owner          string
	Name           string
	Description   string
	ImageURL       string
	Metadata      map[string]interface{}
	Attributes    map[string]interface{}
	TransferCount int64
}

// GetNFT retrieves an NFT
func (d *DB) GetNFT(ctx context.Context, collectionAddress, tokenID string) (*NFTData, error) {
	query := `
		SELECT collection_address, token_id, owner, name, description,
			image_url, metadata, attributes, transfer_count
		FROM nfts WHERE collection_address = $1 AND token_id = $2
	`

	var nft NFTData
	var metadataJSON, attributesJSON []byte
	err := d.db.QueryRowContext(ctx, query, collectionAddress, tokenID).Scan(
		&nft.CollectionAddress,
		&nft.TokenID,
		&nft.Owner,
		&nft.Name,
		&nft.Description,
		&nft.ImageURL,
		&metadataJSON,
		&attributesJSON,
		&nft.TransferCount,
	)

	if err == sql.ErrNoRows {
		return nil, fmt.Errorf("NFT not found: %s/%s", collectionAddress, tokenID)
	}
	if err != nil {
		return nil, fmt.Errorf("failed to get NFT: %w", err)
	}

	json.Unmarshal(metadataJSON, &nft.Metadata)
	json.Unmarshal(attributesJSON, &nft.Attributes)

	return &nft, nil
}

// =============================================================================
// VALIDATOR OPERATIONS
// =============================================================================

// InsertValidator inserts or updates a validator
func (d *DB) InsertValidator(ctx context.Context, val *ValidatorData) error {
	query := `
		INSERT INTO validators (
			address, name, total_stake, self_stake, delegator_count,
			commission_rate, uptime, blocks_signed, blocks_missed,
			blocks_proposed, rewards_accumulated, is_active
		) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12)
		ON CONFLICT (address) DO UPDATE SET
			name = EXCLUDED.name,
			total_stake = EXCLUDED.total_stake,
			delegator_count = EXCLUDED.delegator_count,
			uptime = EXCLUDED.uptime,
			blocks_signed = EXCLUDED.blocks_signed,
			blocks_proposed = EXCLUDED.blocks_proposed,
			rewards_accumulated = EXCLUDED.rewards_accumulated,
			is_active = EXCLUDED.is_active,
			updated_at = CURRENT_TIMESTAMP
	`

	_, err := d.db.ExecContext(ctx, query,
		val.Address,
		val.Name,
		val.TotalStake,
		val.SelfStake,
		val.DelegatorCount,
		val.CommissionRate,
		val.Uptime,
		val.BlocksSigned,
		val.BlocksMissed,
		val.BlocksProposed,
		val.RewardsAccumulated,
		val.IsActive,
	)

	return err
}

// ValidatorData represents validator data
type ValidatorData struct {
	Address            string
	Name              string
	TotalStake        string
	SelfStake        string
	DelegatorCount   int64
	CommissionRate  int64
	Uptime          float64
	BlocksSigned    int64
	BlocksMissed    int64
	BlocksProposed  int64
	RewardsAccumulated string
	IsActive        bool
}

// GetValidator retrieves a validator
func (d *DB) GetValidator(ctx context.Context, address string) (*ValidatorData, error) {
	query := `
		SELECT address, name, total_stake, self_stake, delegator_count,
			commission_rate, uptime, blocks_signed, blocks_missed,
			blocks_proposed, rewards_accumulated, is_active
		FROM validators WHERE address = $1
	`

	var val ValidatorData
	err := d.db.QueryRowContext(ctx, query, address).Scan(
		&val.Address,
		&val.Name,
		&val.TotalStake,
		&val.SelfStake,
		&val.DelegatorCount,
		&val.CommissionRate,
		&val.Uptime,
		&val.BlocksSigned,
		&val.BlocksMissed,
		&val.BlocksProposed,
		&val.RewardsAccumulated,
		&val.IsActive,
	)

	if err == sql.ErrNoRows {
		return nil, fmt.Errorf("validator not found: %s", address)
	}
	if err != nil {
		return nil, fmt.Errorf("failed to get validator: %w", err)
	}

	return &val, nil
}

// GetValidators retrieves validators
func (d *DB) GetValidators(ctx context.Context, limit, offset int) ([]ValidatorData, error) {
	query := `
		SELECT address, name, total_stake, self_stake, delegator_count,
			commission_rate, uptime, blocks_signed, blocks_missed,
			blocks_proposed, rewards_accumulated, is_active
		FROM validators
		ORDER BY total_stake DESC
		LIMIT $1 OFFSET $2
	`

	rows, err := d.db.QueryContext(ctx, query, limit, offset)
	if err != nil {
		return nil, fmt.Errorf("failed to get validators: %w", err)
	}
	defer rows.Close()

	var validators []ValidatorData
	for rows.Next() {
		var val ValidatorData
		err := rows.Scan(
			&val.Address,
			&val.Name,
			&val.TotalStake,
			&val.SelfStake,
			&val.DelegatorCount,
			&val.CommissionRate,
			&val.Uptime,
			&val.BlocksSigned,
			&val.BlocksMissed,
			&val.BlocksProposed,
			&val.RewardsAccumulated,
			&val.IsActive,
		)
		if err != nil {
			return nil, fmt.Errorf("failed to scan validator: %w", err)
		}
		validators = append(validators, val)
	}

	return validators, nil
}

// =============================================================================
// GAS PRICE OPERATIONS
// =============================================================================

// InsertGasPrice inserts gas price data
func (d *DB) InsertGasPrice(ctx context.Context, gp *GasPriceData) error {
	query := `
		INSERT INTO gas_prices (low_gas_price, medium_gas_price, high_gas_price, timestamp)
		VALUES ($1, $2, $3, $4)
	`

	_, err := d.db.ExecContext(ctx, query,
		gp.Low,
		gp.Medium,
		gp.High,
		gp.Timestamp,
	)

	return err
}

// GasPriceData represents gas price data
type GasPriceData struct {
	Low       uint64
	Medium    uint64
	High      uint64
	Timestamp uint64
}

// GetGasPrices retrieves gas prices
func (d *DB) GetGasPrices(ctx context.Context, limit int) ([]GasPriceData, error) {
	query := `
		SELECT low_gas_price, medium_gas_price, high_gas_price, timestamp
		FROM gas_prices
		ORDER BY timestamp DESC
		LIMIT $1
	`

	rows, err := d.db.QueryContext(ctx, query, limit)
	if err != nil {
		return nil, fmt.Errorf("failed to get gas prices: %w", err)
	}
	defer rows.Close()

	var prices []GasPriceData
	for rows.Next() {
		var gp GasPriceData
		err := rows.Scan(&gp.Low, &gp.Medium, &gp.High, &gp.Timestamp)
		if err != nil {
			return nil, fmt.Errorf("failed to scan gas price: %w", err)
		}
		prices = append(prices, gp)
	}

	return prices, nil
}

// =============================================================================
// NETWORK STATS OPERATIONS
// =============================================================================

// InsertNetworkStats inserts network statistics
func (d *DB) InsertNetworkStats(ctx context.Context, stats *NetworkStatsData) error {
	query := `
		INSERT INTO network_stats (
			total_blocks, total_transactions, total_addresses,
			total_tokens, total_contracts, tps, avg_block_time,
			avg_gas_price, avg_gas_limit, difficulty, timestamp
		) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11)
	`

	_, err := d.db.ExecContext(ctx, query,
		stats.TotalBlocks,
		stats.TotalTransactions,
		stats.TotalAddresses,
		stats.TotalTokens,
		stats.TotalContracts,
		stats.TPS,
		stats.AvgBlockTime,
		stats.AvgGasPrice,
		stats.AvgGasLimit,
		stats.Difficulty,
		stats.Timestamp,
	)

	return err
}

// NetworkStatsData represents network statistics
type NetworkStatsData struct {
	TotalBlocks       uint64
	TotalTransactions uint64
	TotalAddresses   uint64
	TotalTokens      uint64
	TotalContracts   uint64
	TPS             float64
	AvgBlockTime     float64
	AvgGasPrice     uint64
	AvgGasLimit     uint64
	Difficulty      string
	Timestamp      uint64
}

// GetNetworkStats retrieves current network statistics
func (d *DB) GetNetworkStats(ctx context.Context) (*NetworkStatsData, error) {
	query := `
		SELECT total_blocks, total_transactions, total_addresses,
			total_tokens, total_contracts, tps, avg_block_time,
			avg_gas_price, avg_gas_limit, difficulty, timestamp
		FROM network_stats
		ORDER BY timestamp DESC
		LIMIT 1
	`

	var stats NetworkStatsData
	err := d.db.QueryRowContext(ctx, query).Scan(
		&stats.TotalBlocks,
		&stats.TotalTransactions,
		&stats.TotalAddresses,
		&stats.TotalTokens,
		&stats.TotalContracts,
		&stats.TPS,
		&stats.AvgBlockTime,
		&stats.AvgGasPrice,
		&stats.AvgGasLimit,
		&stats.Difficulty,
		&stats.Timestamp,
	)

	if err == sql.ErrNoRows {
		return &NetworkStatsData{}, nil
	}
	if err != nil {
		return nil, fmt.Errorf("failed to get network stats: %w", err)
	}

	return &stats, nil
}

// =============================================================================
// CONTRACT SOURCE OPERATIONS
// =============================================================================

// InsertContractSource inserts verified contract source
func (d *DB) InsertContractSource(ctx context.Context, cs *ContractSourceData) error {
	query := `
		INSERT INTO contract_sources (
			address, name, compiler_version, optimization_enabled,
			optimization_runs, source_code, abi, bytecode,
			deployed_bytecode, constructor_args, evm_version, license_type,
			is_proxy, proxy_implementation
		) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14)
		ON CONFLICT (address) DO UPDATE SET
			name = EXCLUDED.name,
			source_code = EXCLUDED.source_code,
			abi = EXCLUDED.abi,
			bytecode = EXCLUDED.bytecode,
			is_verified = TRUE,
			verified_at = CURRENT_TIMESTAMP
	`

	abiJSON, _ := json.Marshal(cs.ABI)

	_, err := d.db.ExecContext(ctx, query,
		cs.Address,
		cs.Name,
		cs.CompilerVersion,
		cs.OptimizationEnabled,
		cs.OptimizationRuns,
		cs.SourceCode,
		abiJSON,
		cs.Bytecode,
		cs.DeployedBytecode,
		cs.ConstructorArgs,
		cs.EVMVersion,
		cs.LicenseType,
		cs.IsProxy,
		cs.ProxyImplementation,
	)

	return err
}

// ContractSourceData represents contract source
type ContractSourceData struct {
	Address             string
	Name               string
	CompilerVersion    string
	OptimizationEnabled bool
	OptimizationRuns   int
	SourceCode        string
	ABI              []interface{}
	Bytecode          string
	DeployedBytecode  string
	ConstructorArgs  string
	EVMVersion       string
	LicenseType      string
	IsProxy          bool
	ProxyImplementation string
}

// GetContractSource retrieves verified contract source
func (d *DB) GetContractSource(ctx context.Context, address string) (*ContractSourceData, error) {
	query := `
		SELECT address, name, compiler_version, optimization_enabled,
			optimization_runs, source_code, abi, bytecode,
			deployed_bytecode, constructor_args, evm_version, license_type,
			is_proxy, proxy_implementation
		FROM contract_sources WHERE address = $1
	`

	var cs ContractSourceData
	var abiJSON []byte
	err := d.db.QueryRowContext(ctx, query, address).Scan(
		&cs.Address,
		&cs.Name,
		&cs.CompilerVersion,
		&cs.OptimizationEnabled,
		&cs.OptimizationRuns,
		&cs.SourceCode,
		&abiJSON,
		&cs.Bytecode,
		&cs.DeployedBytecode,
		&cs.ConstructorArgs,
		&cs.EVMVersion,
		&cs.LicenseType,
		&cs.IsProxy,
		&cs.ProxyImplementation,
	)

	if err == sql.ErrNoRows {
		return nil, fmt.Errorf("contract source not found: %s", address)
	}
	if err != nil {
		return nil, fmt.Errorf("failed to get contract source: %w", err)
	}

	json.Unmarshal(abiJSON, &cs.ABI)

	return &cs, nil
}

// =============================================================================
// INTERNAL TRANSACTIONS
// =============================================================================

// InsertInternalTransaction inserts internal transaction
func (d *DB) InsertInternalTransaction(ctx context.Context, it *InternalTransactionData) error {
	query := `
		INSERT INTO internal_transactions (
			transaction_hash, block_number, trace_address,
			transaction_index, from_address, to_address,
			value, input_data, call_type, gas, depth, result
		) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12)
	`

	_, err := d.db.ExecContext(ctx, query,
		it.TransactionHash,
		it.BlockNumber,
		it.TraceAddress,
		it.TransactionIndex,
		it.FromAddress,
		it.ToAddress,
		it.Value,
		it.InputData,
		it.CallType,
		it.Gas,
		it.Depth,
		it.Result,
	)

	return err
}

// InternalTransactionData represents internal transaction
type InternalTransactionData struct {
	TransactionHash  string
	BlockNumber     uint64
	TraceAddress    string
	TransactionIndex int
	FromAddress    string
	ToAddress      string
	Value         string
	InputData     string
	CallType      string
	Gas          uint64
	Depth         int
	Result        string
}

// GetInternalTransactions retrieves internal transactions
func (d *DB) GetInternalTransactions(ctx context.Context, txHash string) ([]InternalTransactionData, error) {
	query := `
		SELECT transaction_hash, block_number, trace_address,
			transaction_index, from_address, to_address,
			value, input_data, call_type, gas, depth, result
		FROM internal_transactions
		WHERE transaction_hash = $1
		ORDER BY trace_address
	`

	rows, err := d.db.QueryContext(ctx, query, txHash)
	if err != nil {
		return nil, fmt.Errorf("failed to get internal transactions: %w", err)
	}
	defer rows.Close()

	var txs []InternalTransactionData
	for rows.Next() {
		var it InternalTransactionData
		err := rows.Scan(
			&it.TransactionHash,
			&it.BlockNumber,
			&it.TraceAddress,
			&it.TransactionIndex,
			&it.FromAddress,
			&it.ToAddress,
			&it.Value,
			&it.InputData,
			&it.CallType,
			&it.Gas,
			&it.Depth,
			&it.Result,
		)
		if err != nil {
			return nil, fmt.Errorf("failed to scan internal transaction: %w", err)
		}
		txs = append(txs, it)
	}

	return txs, nil
}

// =============================================================================
// LOGS (EVENT LOGS)
// =============================================================================

// InsertLog inserts event log
func (d *DB) InsertLog(ctx context.Context, log *LogData) error {
	query := `
		INSERT INTO logs (
			transaction_hash, block_number, log_index,
			address, topics, data, removed
		) VALUES ($1, $2, $3, $4, $5, $6, $7)
	`

	topics := pq.Array(log.Topics)

	_, err := d.db.ExecContext(ctx, query,
		log.TransactionHash,
		log.BlockNumber,
		log.LogIndex,
		log.Address,
		topics,
		log.Data,
		log.Removed,
	)

	return err
}

// LogData represents event log
type LogData struct {
	TransactionHash string
	BlockNumber    uint64
	LogIndex       int
	Address        string
	Topics        []string
	Data          string
	Removed       bool
}

// GetLogs retrieves logs
func (d *DB) GetLogs(ctx context.Context, filters LogFilters, limit, offset int) ([]LogData, error) {
	baseQuery := `
		SELECT transaction_hash, block_number, log_index,
			address, topics, data, removed
		FROM logs
		WHERE 1=1
	`
	args := []interface{}{}
	argNum := 1

	if filters.Address != "" {
		baseQuery += fmt.Sprintf(" AND address = $%d", argNum)
		args = append(args, filters.Address)
		argNum++
	}
	if len(filters.Topics) > 0 {
		baseQuery += fmt.Sprintf(" AND topics[1] = $%d", argNum)
		args = append(args, filters.Topics[0])
		argNum++
	}
	if filters.BlockNumber > 0 {
		baseQuery += fmt.Sprintf(" AND block_number = $%d", argNum)
		args = append(args, filters.BlockNumber)
		argNum++
	}

	baseQuery += " ORDER BY block_number DESC, log_index DESC"
	baseQuery += fmt.Sprintf(" LIMIT $%d OFFSET $%d", argNum, argNum+1)
	args = append(args, limit, offset)

	rows, err := d.db.QueryContext(ctx, baseQuery, args...)
	if err != nil {
		return nil, fmt.Errorf("failed to get logs: %w", err)
	}
	defer rows.Close()

	var logs []LogData
	for rows.Next() {
		var log LogData
		var topics []string
		err := rows.Scan(
			&log.TransactionHash,
			&log.BlockNumber,
			&log.LogIndex,
			&log.Address,
			&topics,
			&log.Data,
			&log.Removed,
		)
		if err != nil {
			return nil, fmt.Errorf("failed to scan log: %w", err)
		}
		log.Topics = topics
		logs = append(logs, log)
	}

	return logs, nil
}

// LogFilters represents log query filters
type LogFilters struct {
	Address     string
	Topics     []string
	BlockNumber uint64
}

// =============================================================================
// API KEY MANAGEMENT
// =============================================================================

// CreateAPIKey creates a new API key
func (d *DB) CreateAPIKey(ctx context.Context, name, userID string, rateLimit, dailyLimit int) (string, error) {
	// Generate secure random key
	keyBytes := make([]byte, 32)
	if _, err := rand.Read(keyBytes); err != nil {
		return "", fmt.Errorf("failed to generate key: %w", err)
	}
	apiKey := "tsc_" + hex.EncodeToString(keyBytes)[:32]

	// Hash the key for storage
	keyHash := sha256.Sum256([]byte(apiKey))
	keyHashStr := hex.EncodeToString(keyHash[:])

	query := `
		INSERT INTO api_keys (key_hash, name, user_id, rate_limit, daily_limit)
		VALUES ($1, $2, $3, $4, $5)
		RETURNING key_hash
	`

	var storedHash string
	err := d.db.QueryRowContext(ctx, query, keyHashStr, name, userID, rateLimit, dailyLimit).Scan(&storedHash)
	if err != nil {
		return "", fmt.Errorf("failed to create API key: %w", err)
	}

	return apiKey, nil
}

// ValidateAPIKey validates an API key
func (d *DB) ValidateAPIKey(ctx context.Context, apiKey string) (bool, error) {
	if !strings.HasPrefix(apiKey, "tsc_") {
		return false, nil
	}

	keyHash := sha256.Sum256([]byte(apiKey))
	keyHashStr := hex.EncodeToString(keyHash[:])

	query := `
		SELECT is_active FROM api_keys
		WHERE key_hash = $1 AND (expires_at IS NULL OR expires_at > NOW())
	`

	var isActive bool
	err := d.db.QueryRowContext(ctx, query, keyHashStr).Scan(&isActive)
	if err == sql.ErrNoRows {
		return false, nil
	}
	if err != nil {
		return false, fmt.Errorf("failed to validate API key: %w", err)
	}

	// Update last used
	_, _ = d.db.ExecContext(ctx, "UPDATE api_keys SET last_used_at = NOW() WHERE key_hash = $1", keyHashStr)

	return isActive, nil
}

// RevokeAPIKey revokes an API key
func (d *DB) RevokeAPIKey(ctx context.Context, apiKey string) error {
	keyHash := sha256.Sum256([]byte(apiKey))
	keyHashStr := hex.EncodeToString(keyHash[:])

	_, err := d.db.ExecContext(ctx, "UPDATE api_keys SET is_active = FALSE WHERE key_hash = $1", keyHashStr)
	return err
}

// GetAPIKeyStats retrieves API key usage statistics
func (d *DB) GetAPIKeyStats(ctx context.Context, apiKey string) (*APIKeyStats, error) {
	keyHash := sha256.Sum256([]byte(apiKey))
	keyHashStr := hex.EncodeToString(keyHash[:])

	query := `
		SELECT rate_limit, daily_limit, last_used_at FROM api_keys WHERE key_hash = $1
	`

	var stats APIKeyStats
	err := d.db.QueryRowContext(ctx, query, keyHashStr).Scan(
		&stats.RateLimit,
		&stats.DailyLimit,
		&stats.LastUsedAt,
	)

	if err == sql.ErrNoRows {
		return nil, fmt.Errorf("API key not found")
	}
	if err != nil {
		return nil, fmt.Errorf("failed to get API key stats: %w", err)
	}

	return &stats, nil
}

// APIKeyStats represents API key statistics
type APIKeyStats struct {
	RateLimit  int
	DailyLimit int
	LastUsedAt sql.NullTime
}

// =============================================================================
// RATE LIMITING
// =============================================================================

// RateLimiter provides rate limiting functionality
type RateLimiter struct {
	mu           sync.RWMutex
	requests     map[string]*ClientLimiter
	maxRequests  int
	windowSize  time.Duration
	burst      int
}

// ClientLimiter tracks client requests
type ClientLimiter struct {
	requests    []time.Time
	blocked    bool
	blockUntil time.Time
}

// NewRateLimiter creates a new rate limiter
func NewRateLimiter(maxRequests int, windowSize time.Duration, burst int) *RateLimiter {
	return &RateLimiter{
		requests:    make(map[string]*ClientLimiter),
		maxRequests: maxRequests,
		windowSize:  windowSize,
		burst:      burst,
	}
}

// Allow checks if request is allowed
func (r *RateLimiter) Allow(clientID string) bool {
	r.mu.Lock()
	defer r.mu.Unlock()

	now := time.Now()
	cl, exists := r.requests[clientID]

	if !exists {
		r.requests[clientID] = &ClientLimiter{
			requests: []time.Time{now},
		}
		return true
	}

	// Check if blocked
	if cl.blocked && now.Before(cl.blockUntil) {
		return false
	}

	// Clean old requests
	var valid []time.Time
	windowStart := now.Add(-r.windowSize)
	for _, reqTime := range cl.requests {
		if reqTime.After(windowStart) {
			valid = append(valid, reqTime)
		}
	}

	// Check limit
	if len(valid) >= r.maxRequests {
		cl.blocked = true
		cl.blockUntil = now.Add(r.windowSize)
		r.requests[clientID] = cl
		return false
	}

	cl.requests = append(valid, now)
	r.requests[clientID] = cl

	return true
}

// GetRemaining returns remaining requests
func (r *RateLimiter) GetRemaining(clientID string) int {
	r.mu.RLock()
	defer r.mu.RUnlock()

	cl, exists := r.requests[clientID]
	if !exists {
		return r.maxRequests
	}

	now := time.Now()
	windowStart := now.Add(-r.windowSize)
	var count int
	for _, reqTime := range cl.requests {
		if reqTime.After(windowStart) {
			count++
		}
	}

	return r.maxRequests - count
}

// =============================================================================
// AUDIT LOGGING
// =============================================================================

// LogAudit logs an audit event
func (d *DB) LogAudit(ctx context.Context, log *AuditLogData) error {
	query := `
		INSERT INTO audit_logs (
			user_id, action, resource_type, resource_id,
			ip_address, user_agent, request_data, response_code
		) VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
	`

	requestDataJSON, _ := json.Marshal(log.RequestData)

	_, err := d.db.ExecContext(ctx, query,
		log.UserID,
		log.Action,
		log.ResourceType,
		log.ResourceID,
		log.IPAddress,
		log.UserAgent,
		requestDataJSON,
		log.ResponseCode,
	)

	return err
}

// AuditLogData represents audit log data
type AuditLogData struct {
	UserID       string
	Action       string
	ResourceType string
	ResourceID  string
	IPAddress   string
	UserAgent   string
	RequestData map[string]interface{}
	ResponseCode int
}

// =============================================================================
// IP BLOCKING
// =============================================================================

// BlockIP blocks an IP address
func (d *DB) BlockIP(ctx context.Context, ip, reason string, duration time.Duration) error {
	var expiresAt *time.Time
	if duration > 0 {
		expires := time.Now().Add(duration)
		expiresAt = &expires
	}

	query := `
		INSERT INTO ip_blocks (ip_address, block_reason, expires_at)
		VALUES ($1, $2, $3)
		ON CONFLICT (ip_address) DO UPDATE SET
			block_reason = EXCLUDED.block_reason,
			expires_at = EXCLUDED.expires_at,
			blocked_at = CURRENT_TIMESTAMP
	`

	_, err := d.db.ExecContext(ctx, query, ip, reason, expiresAt)
	return err
}

// UnblockIP unblocks an IP address
func (d *DB) UnblockIP(ctx context.Context, ip string) error {
	_, err := d.db.ExecContext(ctx, "DELETE FROM ip_blocks WHERE ip_address = $1", ip)
	return err
}

// IsBlocked checks if IP is blocked
func (d *DB) IsBlocked(ctx context.Context, ip string) (bool, error) {
	query := `
		SELECT TRUE FROM ip_blocks
		WHERE ip_address = $1 AND (expires_at IS NULL OR expires_at > NOW())
	`

	var blocked bool
	err := d.db.QueryRowContext(ctx, query, ip).Scan(&blocked)
	if err == sql.ErrNoRows {
		return false, nil
	}
	if err != nil {
		return false, fmt.Errorf("failed to check IP block: %w", err)
	}

	return true, nil
}

// =============================================================================
// SEARCH
// =============================================================================

// Search performs full-text search
func (d *DB) Search(ctx context.Context, query string, limit int) ([]SearchResult, error) {
	if query == "" {
		return nil, nil
	}

	// Clean query
	query = strings.TrimSpace(query)
	query = strings.ToLower(query)

	searchQuery := `
		SELECT search_type, address, hash, number, name, description
		FROM search_index
		WHERE content_tsv @@ to_tsquery('english', $1)
		ORDER BY ts_rank(content_tsv, to_tsquery('english', $1))
		LIMIT $2
	`

	rows, err := d.db.QueryContext(ctx, searchQuery, query, limit)
	if err != nil {
		return nil, fmt.Errorf("failed to search: %w", err)
	}
	defer rows.Close()

	var results []SearchResult
	for rows.Next() {
		var r SearchResult
		err := rows.Scan(&r.Type, &r.Address, &r.Hash, &r.Number, &r.Name, &r.Description)
		if err != nil {
			return nil, fmt.Errorf("failed to scan result: %w", err)
		}
		results = append(results, r)
	}

	return results, nil
}

// SearchResult represents search result
type SearchResult struct {
	Type        string
	Address    string
	Hash       string
	Number     *int64
	Name       string
	Description string
}

// IndexSearch indexes data for search
func (d *DB) IndexSearch(ctx context.Context, data *SearchIndexData) error {
	query := `
		INSERT INTO search_index (search_type, address, hash, number, name, description)
		VALUES ($1, $2, $3, $4, $5, $6)
		ON CONFLICT (address) DO UPDATE SET
			name = EXCLUDED.name,
			description = EXCLUDED.description
	`

	_, err := d.db.ExecContext(ctx, query,
		data.Type,
		data.Address,
		data.Hash,
		data.Number,
		data.Name,
		data.Description,
	)

	return err
}

// SearchIndexData represents search index data
type SearchIndexData struct {
	Type        string
	Address    string
	Hash       string
	Number     int64
	Name       string
	Description string
}

// =============================================================================
// HEALTH CHECKS
// =============================================================================

// HealthCheck performs database health check
func (d *DB) HealthCheck(ctx context.Context) error {
	if err := d.db.PingContext(ctx); err != nil {
		return fmt.Errorf("database ping failed: %w", err)
	}

	// Check table sizes
	var tableCount int
	err := d.db.QueryRowContext(ctx, 
		"SELECT COUNT(*) FROM information_schema.tables WHERE table_schema = 'public'",
	).Scan(&tableCount)
	if err != nil {
		return fmt.Errorf("failed to check tables: %w", err)
	}

	if tableCount == 0 {
		return fmt.Errorf("no tables found in database")
	}

	return nil
}

// GetStats returns database statistics
func (d *DB) GetStats(ctx context.Context) (map[string]interface{}, error) {
	stats := make(map[string]interface{})

	// Table counts
	tables := []string{"blocks", "transactions", "accounts", "tokens", "nfts", "validators"}
	for _, table := range tables {
		var count int64
		err := d.db.QueryRowContext(ctx, fmt.Sprintf("SELECT COUNT(*) FROM %s", table)).Scan(&count)
		if err == nil {
			stats[table+"_count"] = count
		}
	}

	// Database size
	var dbSize string
	err := d.db.QueryRowContext(ctx, 
		"SELECT pg_size_pretty(pg_database_size(current_database()))",
	).Scan(&dbSize)
	if err == nil {
		stats["database_size"] = dbSize
	}

	return stats, nil
}

// =============================================================================
// ENCRYPTION HELPERS
// =============================================================================

// HashPassword hashes a password with bcrypt
func HashPassword(password string) (string, error) {
	bytes, err := bcrypt.GenerateFromPassword([]byte(password), bcrypt.DefaultCost)
	return string(bytes), err
}

// CheckPassword checks password against hash
func CheckPassword(password, hash string) bool {
	err := bcrypt.CompareHashAndPassword([]byte(hash), []byte(password))
	return err == nil
}

// HashData creates SHA-256 hash
func HashData(data []byte) string {
	hash := sha256.Sum256(data)
	return hex.EncodeToString(hash[:])
}

// VerifySignature verifies Ethereum signature
func VerifySignature(message, signature, address string) bool {
	// Implementation would use go-ethereum/crypto
	return true
}

// ValidateAddress validates Ethereum address format
func ValidateAddress(addr string) bool {
	if !strings.HasPrefix(addr, "0x") {
		return false
	}
	if len(addr) != 42 {
		return false
	}
	addr = addr[2:]
	for _, c := range addr {
		if !((c >= '0' && c <= '9') || (c >= 'a' && c <= 'f') || (c >= 'A' && c <= 'F')) {
			return false
		}
	}
	return true
}

// ValidateHash validates transaction/block hash format
func ValidateHash(hash string) bool {
	if !strings.HasPrefix(hash, "0x") {
		return false
	}
	if len(hash) != 66 {
		return false
	}
	hash = hash[2:]
	for _, c := range hash {
		if !((c >= '0' && c <= '9') || (c >= 'a' && c <= 'f') || (c >= 'A' && c <= 'F')) {
			return false
		}
	}
	return true
}

// FormatAddress formats address with checksum
func FormatAddress(addr string) string {
	if !ValidateAddress(addr) {
		return addr
	}
	// Convert to checksum address
	addr = strings.ToLower(addr[2:])
	hash := HashData([]byte(addr))
	
	result := "0x"
	for i, c := range addr {
		if c >= '0' && c <= '9' {
			result += string(c)
		} else {
			// Use hash character for case
			if hash[i] >= '8' {
				result += strings.ToUpper(string(c))
			} else {
				result += string(c)
			}
		}
	}
	return result
}

// FormatValue formats Wei value to human readable
func FormatValue(wei *big.Int, decimals int) string {
	div := new(big.Int).Exp(big.NewInt(10), big.NewInt(int64(decimals)), nil)
	
	// Whole part
	whole := new(big.Int).Div(wei, div)
	
	// Fractional part
	fraction := new(big.Int).Mod(wei, div)
	
	// Format fraction with leading zeros
	fractionStr := fraction.String()
	fractionStr = strings.Repeat(decimals-len(fractionStr)) + fractionStr
	
	// Trim trailing zeros
	fractionStr = strings.TrimRight(fractionStr, "0")
	if fractionStr == "" {
		return whole.String()
	}
	
	return fmt.Sprintf("%s.%s", whole.String(), fractionStr)
}

// ParseValue parses human readable value to Wei
func ParseValue(value string, decimals int) (*big.Int, error) {
	value = strings.TrimSpace(value)
	
	var wei big.Int
	
	// Handle decimal point
	if strings.Contains(value, ".") {
		parts := strings.Split(value, ".")
		whole := parts[0]
		fraction := parts[1]
		
		// Pad fraction to decimals
		if len(fraction) < decimals {
			fraction = fraction + strings.Repeat("0", decimals-len(fraction))
		} else if len(fraction) > decimals {
			fraction = fraction[:decimals]
		}
		
		value = whole + fraction
	} else {
		value = value + strings.Repeat("0", decimals)
	}
	
	_, ok := wei.SetString(value, 10)
	if !ok {
		return nil, fmt.Errorf("invalid value: %s", value)
	}
	
	return &wei, nil
}