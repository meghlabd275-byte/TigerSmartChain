// Package postgres provides PostgreSQL database operations for TigerScan Explorer.
// This package provides complete CRUD operations with proper security and performance.
package postgres

import (
	"context"
	"database/sql"
	"encoding/hex"
	"encoding/json"
	"fmt"
	"math/big"
	"strings"
	"time"

	"github.com/jackc/pgx/v5"
	"github.com/jackc/pgx/v5/pgxpool"
)

// Config holds database configuration
type Config struct {
	Host            string
	Port            int
	User           string
	Password       string
	Database       string
	MaxConnections int
	MinConnections int
}

// =============================================================================
// DATABASE CONNECTION
// =============================================================================

// DB wraps the database connection pool
type DB struct {
	pool *pgxpool.Pool
}

// NewDB creates a new database connection
func NewDB(ctx context.Context, config *Config) (*DB, error) {
	connStr := fmt.Sprintf(
		"postgres://%s:%s@%s:%d/%s?pool_max_conns=%d&pool_min_conns=%d",
		config.User, config.Password, config.Host, config.Port, config.Database,
		config.MaxConnections, config.MinConnections,
	)

	poolConfig, err := pgxpool.ParseConfig(connStr)
	if err != nil {
		return nil, fmt.Errorf("failed to parse config: %w", err)
	}

	pool, err := pgxpool.NewWithConfig(ctx, poolConfig)
	if err != nil {
		return nil, fmt.Errorf("failed to create pool: %w", err)
	}

	// Verify connection
	if err := pool.Ping(ctx); err != nil {
		return nil, fmt.Errorf("failed to ping database: %w", err)
	}

	return &DB{pool: pool}, nil
}

// Close closes the database connection
func (db *DB) Close() {
	if db.pool != nil {
		db.pool.Close()
	}
}

// =============================================================================
// BLOCK OPERATIONS
// =============================================================================

// Block represents a blockchain block
type Block struct {
	ID              int64     `json:"id"`
	Number         int64     `json:"number"`
	Hash           string   `json:"hash"`
	ParentHash     string   `json:"parentHash"`
	Nonce          string   `json:"nonce,omitempty"`
	SHA3Uncles     string   `json:"sha3Uncles,omitempty"`
	LogsBloom      string   `json:"logsBloom,omitempty"`
	TransactionsRoot string `json:"transactionsRoot,omitempty"`
	StateRoot      string   `json:"stateRoot,omitempty"`
	ReceiptsRoot   string   `json:"receiptsRoot,omitempty"`
	Miner          string   `json:"miner"`
	Difficulty     string   `json:"difficulty,omitempty"`
	TotalDifficulty string `json:"totalDifficulty,omitempty"`
	Size           int64    `json:"size,omitempty"`
	GasLimit       int64    `json:"gasLimit"`
	GasUsed        int64    `json:"gasUsed"`
	Timestamp      int64    `json:"timestamp"`
	ExtraData      string   `json:"extraData,omitempty"`
	BaseFeePerGas  int64    `json:"baseFeePerGas,omitempty"`
	CreatedAt      time.Time `json:"createdAt"`
	UpdatedAt      time.Time `json:"updatedAt"`
}

// InsertBlock inserts a new block
func (db *DB) InsertBlock(ctx context.Context, block *Block) error {
	query := `
		INSERT INTO blocks (number, hash, parent_hash, nonce, sha3_uncles, logs_bloom,
			transactions_root, state_root, receipts_root, miner, difficulty, total_difficulty,
			size, gas_limit, gas_used, timestamp, extra_data, base_fee_per_gas, created_at, updated_at)
		VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16, $17, $18, NOW(), NOW())
		ON CONFLICT (number) DO UPDATE SET
			hash = EXCLUDED.hash,
			parent_hash = EXCLUDED.parent_hash,
			nonce = EXCLUDED.nonce,
			sha3_uncles = EXCLUDED.sha3_uncles,
			logs_bloom = EXCLUDED.logs_bloom,
			transactions_root = EXCLUDED.transactions_root,
			state_root = EXCLUDED.state_root,
			receipts_root = EXCLUDED.receipts_root,
			miner = EXCLUDED.miner,
			difficulty = EXCLUDED.difficulty,
			total_difficulty = EXCLUDED.total_difficulty,
			size = EXCLUDED.size,
			gas_limit = EXCLUDED.gas_limit,
			gas_used = EXCLUDED.gas_used,
			timestamp = EXCLUDED.timestamp,
			extra_data = EXCLUDED.extra_data,
			base_fee_per_gas = EXCLUDED.base_fee_per_gas,
			updated_at = NOW()
	`

	_, err := db.pool.Exec(ctx, query,
		block.Number, block.Hash, block.ParentHash, block.Nonce, block.SHA3Uncles,
		block.LogsBloom, block.TransactionsRoot, block.StateRoot, block.ReceiptsRoot,
		block.Miner, block.Difficulty, block.TotalDifficulty, block.Size, block.GasLimit,
		block.GasUsed, block.Timestamp, block.ExtraData, block.BaseFeePerGas,
	)
	return err
}

// GetBlockByNumber retrieves a block by number
func (db *DB) GetBlockByNumber(ctx context.Context, number int64) (*Block, error) {
	query := `
		SELECT id, number, hash, parent_hash, nonce, sha3_uncles, logs_bloom, transactions_root,
			state_root, receipts_root, miner, difficulty, total_difficulty, size, gas_limit,
			gas_used, timestamp, extra_data, base_fee_per_gas, created_at, updated_at
		FROM blocks WHERE number = $1
	`

	var block Block
	err := db.pool.QueryRow(ctx, query, number).Scan(
		&block.ID, &block.Number, &block.Hash, &block.ParentHash, &block.Nonce, &block.SHA3Uncles,
		&block.LogsBloom, &block.TransactionsRoot, &block.StateRoot, &block.ReceiptsRoot,
		&block.Miner, &block.Difficulty, &block.TotalDifficulty, &block.Size, &block.GasLimit,
		&block.GasUsed, &block.Timestamp, &block.ExtraData, &block.BaseFeePerGas, &block.CreatedAt, &block.UpdatedAt,
	)
	if err == sql.ErrNoRows {
		return nil, nil
	}
	return &block, err
}

// GetBlockByHash retrieves a block by hash
func (db *DB) GetBlockByHash(ctx context.Context, hash string) (*Block, error) {
	query := `
		SELECT id, number, hash, parent_hash, nonce, sha3_uncles, logs_bloom, transactions_root,
			state_root, receipts_root, miner, difficulty, total_difficulty, size, gas_limit,
			gas_used, timestamp, extra_data, base_fee_per_gas, created_at, updated_at
		FROM blocks WHERE hash = $1
	`

	var block Block
	err := db.pool.QueryRow(ctx, query, hash).Scan(
		&block.ID, &block.Number, &block.Hash, &block.ParentHash, &block.Nonce, &block.SHA3Uncles,
		&block.LogsBloom, &block.TransactionsRoot, &block.StateRoot, &block.ReceiptsRoot,
		&block.Miner, &block.Difficulty, &block.TotalDifficulty, &block.Size, &block.GasLimit,
		&block.GasUsed, &block.Timestamp, &block.ExtraData, &block.BaseFeePerGas, &block.CreatedAt, &block.UpdatedAt,
	)
	if err == sql.ErrNoRows {
		return nil, nil
	}
	return &block, err
}

// GetLatestBlock retrieves the latest block
func (db *DB) GetLatestBlock(ctx context.Context) (*Block, error) {
	query := `
		SELECT id, number, hash, parent_hash, nonce, sha3_uncles, logs_bloom, transactions_root,
			state_root, receipts_root, miner, difficulty, total_difficulty, size, gas_limit,
			gas_used, timestamp, extra_data, base_fee_per_gas, created_at, updated_at
		FROM blocks ORDER BY number DESC LIMIT 1
	`

	var block Block
	err := db.pool.QueryRow(ctx, query).Scan(
		&block.ID, &block.Number, &block.Hash, &block.ParentHash, &block.Nonce, &block.SHA3Uncles,
		&block.LogsBloom, &block.TransactionsRoot, &block.StateRoot, &block.ReceiptsRoot,
		&block.Miner, &block.Difficulty, &block.TotalDifficulty, &block.Size, &block.GasLimit,
		&block.GasUsed, &block.Timestamp, &block.ExtraData, &block.BaseFeePerGas, &block.CreatedAt, &block.UpdatedAt,
	)
	if err == sql.ErrNoRows {
		return nil, nil
	}
	return &block, err
}

// GetBlocks retrieves blocks with pagination
func (db *DB) GetBlocks(ctx context.Context, limit, offset int) ([]*Block, error) {
	query := `
		SELECT id, number, hash, parent_hash, nonce, sha3_uncles, logs_bloom, transactions_root,
			state_root, receipts_root, miner, difficulty, total_difficulty, size, gas_limit,
			gas_used, timestamp, extra_data, base_fee_per_gas, created_at, updated_at
		FROM blocks ORDER BY number DESC LIMIT $1 OFFSET $2
	`

	rows, err := db.pool.Query(ctx, query, limit, offset)
	if err != nil {
		return nil, err
	}
	defer rows.Close()

	var blocks []*Block
	for rows.Next() {
		var block Block
		if err := rows.Scan(
			&block.ID, &block.Number, &block.Hash, &block.ParentHash, &block.Nonce, &block.SHA3Uncles,
			&block.LogsBloom, &block.TransactionsRoot, &block.StateRoot, &block.ReceiptsRoot,
			&block.Miner, &block.Difficulty, &block.TotalDifficulty, &block.Size, &block.GasLimit,
			&block.GasUsed, &block.Timestamp, &block.ExtraData, &block.BaseFeePerGas, &block.CreatedAt, &block.UpdatedAt,
		); err != nil {
			return nil, err
		}
		blocks = append(blocks, &block)
	}
	return blocks, rows.Err()
}

// =============================================================================
// TRANSACTION OPERATIONS
// =============================================================================

// Transaction represents a blockchain transaction
type Transaction struct {
	ID              int64     `json:"id"`
	Hash           string    `json:"hash"`
	BlockNumber    *int64    `json:"blockNumber,omitempty"`
	BlockHash     *string   `json:"blockHash,omitempty"`
	TransactionIndex *int64  `json:"transactionIndex,omitempty"`
	FromAddress  string    `json:"fromAddress"`
	ToAddress    *string   `json:"toAddress,omitempty"`
	Value        string    `json:"value"`
	GasPrice     int64     `json:"gasPrice"`
	GasLimit     int64     `json:"gasLimit"`
	GasUsed      *int64   `json:"gasUsed,omitempty"`
	Nonce        int64     `json:"nonce"`
	InputData    *string  `json:"inputData,omitempty"`
	V            *int64    `json:"v,omitempty"`
	R            *string   `json:"r,omitempty"`
	S            *string   `json:"s,omitempty"`
	Status      int       `json:"status"`
	Timestamp   *int64   `json:"timestamp,omitempty"`
	CreatedAt    time.Time `json:"createdAt"`
}

// InsertTransaction inserts a new transaction
func (db *DB) InsertTransaction(ctx context.Context, tx *Transaction) error {
	query := `
		INSERT INTO transactions (hash, block_number, block_hash, transaction_index, from_address,
			to_address, value, gas_price, gas_limit, gas_used, nonce, input_data, v, r, s, status, timestamp, created_at)
		VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16, $17, NOW())
		ON CONFLICT (hash) DO UPDATE SET
			block_number = EXCLUDED.block_number,
			block_hash = EXCLUDED.block_hash,
			transaction_index = EXCLUDED.transaction_index,
			to_address = EXCLUDED.to_address,
			gas_used = EXCLUDED.gas_used,
			status = EXCLUDED.status,
			timestamp = EXCLUDED.timestamp
	`

	_, err := db.pool.Exec(ctx, query,
		tx.Hash, tx.BlockNumber, tx.BlockHash, tx.TransactionIndex, tx.FromAddress,
		tx.ToAddress, tx.Value, tx.GasPrice, tx.GasLimit, tx.GasUsed, tx.Nonce,
		tx.InputData, tx.V, tx.R, tx.S, tx.Status, tx.Timestamp,
	)
	return err
}

// GetTransactionByHash retrieves a transaction by hash
func (db *DB) GetTransactionByHash(ctx context.Context, hash string) (*Transaction, error) {
	query := `
		SELECT id, hash, block_number, block_hash, transaction_index, from_address, to_address,
			value, gas_price, gas_limit, gas_used, nonce, input_data, v, r, s, status, timestamp, created_at
		FROM transactions WHERE hash = $1
	`

	var tx Transaction
	err := db.pool.QueryRow(ctx, query, hash).Scan(
		&tx.ID, &tx.Hash, &tx.BlockNumber, &tx.BlockHash, &tx.TransactionIndex, &tx.FromAddress,
		&tx.ToAddress, &tx.Value, &tx.GasPrice, &tx.GasLimit, &tx.GasUsed, &tx.Nonce,
		&tx.InputData, &tx.V, &tx.R, &tx.S, &tx.Status, &tx.Timestamp, &tx.CreatedAt,
	)
	if err == sql.ErrNoRows {
		return nil, nil
	}
	return &tx, err
}

// GetTransactionsByBlock retrieves transactions for a block
func (db *DB) GetTransactionsByBlock(ctx context.Context, blockNumber int64) ([]*Transaction, error) {
	query := `
		SELECT id, hash, block_number, block_hash, transaction_index, from_address, to_address,
			value, gas_price, gas_limit, gas_used, nonce, input_data, v, r, s, status, timestamp, created_at
		FROM transactions WHERE block_number = $1 ORDER BY transaction_index
	`

	rows, err := db.pool.Query(ctx, query, blockNumber)
	if err != nil {
		return nil, err
	}
	defer rows.Close()

	var txs []*Transaction
	for rows.Next() {
		var tx Transaction
		if err := rows.Scan(
			&tx.ID, &tx.Hash, &tx.BlockNumber, &tx.BlockHash, &tx.TransactionIndex, &tx.FromAddress,
			&tx.ToAddress, &tx.Value, &tx.GasPrice, &tx.GasLimit, &tx.GasUsed, &tx.Nonce,
			&tx.InputData, &tx.V, &tx.R, &tx.S, &tx.Status, &tx.Timestamp, &tx.CreatedAt,
		); err != nil {
			return nil, err
		}
		txs = append(txs, &tx)
	}
	return txs, rows.Err()
}

// GetLatestTransactions retrieves latest transactions
func (db *DB) GetLatestTransactions(ctx context.Context, limit int) ([]*Transaction, error) {
	query := `
		SELECT id, hash, block_number, block_hash, transaction_index, from_address, to_address,
			value, gas_price, gas_limit, gas_used, nonce, input_data, v, r, s, status, timestamp, created_at
		FROM transactions WHERE status != 0 ORDER BY timestamp DESC, transaction_index DESC LIMIT $1
	`

	rows, err := db.pool.Query(ctx, query, limit)
	if err != nil {
		return nil, err
	}
	defer rows.Close()

	var txs []*Transaction
	for rows.Next() {
		var tx Transaction
		if err := rows.Scan(
			&tx.ID, &tx.Hash, &tx.BlockNumber, &tx.BlockHash, &tx.TransactionIndex, &tx.FromAddress,
			&tx.ToAddress, &tx.Value, &tx.GasPrice, &tx.GasLimit, &tx.GasUsed, &tx.Nonce,
			&tx.InputData, &tx.V, &tx.R, &tx.S, &tx.Status, &tx.Timestamp, &tx.CreatedAt,
		); err != nil {
			return nil, err
		}
		txs = append(txs, &tx)
	}
	return txs, rows.Err()
}

// GetPendingTransactions retrieves pending transactions
func (db *DB) GetPendingTransactions(ctx context.Context, limit int) ([]*Transaction, error) {
	query := `
		SELECT id, hash, block_number, block_hash, transaction_index, from_address, to_address,
			value, gas_price, gas_limit, gas_used, nonce, input_data, v, r, s, status, timestamp, created_at
		FROM transactions WHERE status = 0 ORDER BY gas_price DESC, created_at ASC LIMIT $1
	`

	rows, err := db.pool.Query(ctx, query, limit)
	if err != nil {
		return nil, err
	}
	defer rows.Close()

	var txs []*Transaction
	for rows.Next() {
		var tx Transaction
		if err := rows.Scan(
			&tx.ID, &tx.Hash, &tx.BlockNumber, &tx.BlockHash, &tx.TransactionIndex, &tx.FromAddress,
			&tx.ToAddress, &tx.Value, &tx.GasPrice, &tx.GasLimit, &tx.GasUsed, &tx.Nonce,
			&tx.InputData, &tx.V, &tx.R, &tx.S, &tx.Status, &tx.Timestamp, &tx.CreatedAt,
		); err != nil {
			return nil, err
		}
		txs = append(txs, &tx)
	}
	return txs, rows.Err()
}

// =============================================================================
// ACCOUNT OPERATIONS
// =============================================================================

// Account represents an account
type Account struct {
	ID                int64     `json:"id"`
	Address           string    `json:"address"`
	Balance          string    `json:"balance"`
	Nonce            int       `json:"nonce"`
	CodeHash         *string   `json:"codeHash,omitempty"`
	CodeLength      int       `json:"codeLength"`
	IsContract       bool      `json:"isContract"`
	IsVerified      bool      `json:"isVerified"`
	IsSelfDestructed bool      `json:"isSelfDestructed"`
	FirstBlockNumber *int64    `json:"firstBlockNumber,omitempty"`
	LastBlockNumber  *int64    `json:"lastBlockNumber,omitempty"`
	CreatedAt       time.Time `json:"createdAt"`
	UpdatedAt       time.Time `json:"updatedAt"`
}

// InsertAccount inserts or updates an account
func (db *DB) InsertAccount(ctx context.Context, acc *Account) error {
	query := `
		INSERT INTO accounts (address, balance, nonce, code_hash, code_length, is_contract,
			is_verified, is_self_destructed, first_block_number, last_block_number, created_at, updated_at)
		VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, NOW(), NOW())
		ON CONFLICT (address) DO UPDATE SET
			balance = EXCLUDED.balance,
			nonce = EXCLUDED.nonce,
			code_hash = EXCLUDED.code_hash,
			code_length = EXCLUDED.code_length,
			is_contract = EXCLUDED.is_contract,
			is_verified = EXCLUDED.is_verified,
			is_self_destructed = EXCLUDED.is_self_destructed,
			last_block_number = EXCLUDED.last_block_number,
			updated_at = NOW()
	`

	_, err := db.pool.Exec(ctx, query,
		acc.Address, acc.Balance, acc.Nonce, acc.CodeHash, acc.CodeLength,
		acc.IsContract, acc.IsVerified, acc.IsSelfDestructed, acc.FirstBlockNumber, acc.LastBlockNumber,
	)
	return err
}

// GetAccount retrieves an account by address
func (db *DB) GetAccount(ctx context.Context, address string) (*Account, error) {
	query := `
		SELECT id, address, balance, nonce, code_hash, code_length, is_contract,
			is_verified, is_self_destructed, first_block_number, last_block_number, created_at, updated_at
		FROM accounts WHERE address = $1
	`

	var acc Account
	err := db.pool.QueryRow(ctx, query, address).Scan(
		&acc.ID, &acc.Address, &acc.Balance, &acc.Nonce, &acc.CodeHash, &acc.CodeLength,
		&acc.IsContract, &acc.IsVerified, &acc.IsSelfDestructed, &acc.FirstBlockNumber,
		&acc.LastBlockNumber, &acc.CreatedAt, &acc.UpdatedAt,
	)
	if err == sql.ErrNoRows {
		return nil, nil
	}
	return &acc, err
}

// GetAccountTransactions retrieves transactions for an account
func (db *DB) GetAccountTransactions(ctx context.Context, address string, limit, offset int) ([]*Transaction, error) {
	query := `
		SELECT id, hash, block_number, block_hash, transaction_index, from_address, to_address,
			value, gas_price, gas_limit, gas_used, nonce, input_data, v, r, s, status, timestamp, created_at
		FROM transactions 
		WHERE from_address = $1 OR to_address = $1
		ORDER BY timestamp DESC, transaction_index DESC
		LIMIT $2 OFFSET $3
	`

	rows, err := db.pool.Query(ctx, query, address, limit, offset)
	if err != nil {
		return nil, err
	}
	defer rows.Close()

	var txs []*Transaction
	for rows.Next() {
		var tx Transaction
		if err := rows.Scan(
			&tx.ID, &tx.Hash, &tx.BlockNumber, &tx.BlockHash, &tx.TransactionIndex, &tx.FromAddress,
			&tx.ToAddress, &tx.Value, &tx.GasPrice, &tx.GasLimit, &tx.GasUsed, &tx.Nonce,
			&tx.InputData, &tx.V, &tx.R, &tx.S, &tx.Status, &tx.Timestamp, &tx.CreatedAt,
		); err != nil {
			return nil, err
		}
		txs = append(txs, &tx)
	}
	return txs, rows.Err()
}

// =============================================================================
// TOKEN OPERATIONS
// =============================================================================

// Token represents a TEP20 token
type Token struct {
	ID              int64     `json:"id"`
	Address         string    `json:"address"`
	Name           string    `json:"name"`
	Symbol         string    `json:"symbol"`
	Decimals       int       `json:"decimals"`
	TotalSupply    string    `json:"totalSupply"`
	HoldersCount   int       `json:"holdersCount"`
	TransfersCount int       `json:"transfersCount"`
	Creator       *string   `json:"creator,omitempty"`
	ContractType  string    `json:"contractType"`
	IsVerified    bool      `json:"isVerified"`
	IsActive      bool      `json:"isActive"`
	Price         *string   `json:"price,omitempty"`
	MarketCap     *string   `json:"marketCap,omitempty"`
	Volume24h     *string   `json:"volume24h,omitempty"`
	PriceChange24h *string  `json:"priceChange24h,omitempty"`
	TxHash        *string   `json:"txHash,omitempty"`
	BlockNumber   *int64    `json:"blockNumber,omitempty"`
	CreatedAt     time.Time `json:"createdAt"`
	UpdatedAt     time.Time `json:"updatedAt"`
}

// InsertToken inserts or updates a token
func (db *DB) InsertToken(ctx context.Context, token *Token) error {
	query := `
		INSERT INTO tokens (address, name, symbol, decimals, total_supply, holders_count, transfers_count,
			creator, contract_type, is_verified, is_active, price, market_cap, volume_24h, price_change_24h,
			tx_hash, block_number, created_at, updated_at)
		VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16, NOW(), NOW())
		ON CONFLICT (address) DO UPDATE SET
			name = EXCLUDED.name,
			symbol = EXCLUDED.symbol,
			decimals = EXCLUDED.decimals,
			total_supply = EXCLUDED.total_supply,
			holders_count = EXCLUDED.holders_count,
			transfers_count = EXCLUDED.transfers_count,
			is_verified = EXCLUDED.is_verified,
			is_active = EXCLUDED.is_active,
			price = EXCLUDED.price,
			market_cap = EXCLUDED.market_cap,
			volume_24h = EXCLUDED.volume_24h,
			price_change_24h = EXCLUDED.price_change_24h,
			updated_at = NOW()
	`

	_, err := db.pool.Exec(ctx, query,
		token.Address, token.Name, token.Symbol, token.Decimals, token.TotalSupply,
		token.HoldersCount, token.TransfersCount, token.Creator, token.ContractType,
		token.IsVerified, token.IsActive, token.Price, token.MarketCap, token.Volume24h, token.PriceChange24h,
		token.TxHash, token.BlockNumber,
	)
	return err
}

// GetToken retrieves a token by address
func (db *DB) GetToken(ctx context.Context, address string) (*Token, error) {
	query := `
		SELECT id, address, name, symbol, decimals, total_supply, holders_count, transfers_count,
			creator, contract_type, is_verified, is_active, price, market_cap, volume_24h, price_change_24h,
			tx_hash, block_number, created_at, updated_at
		FROM tokens WHERE address = $1
	`

	var token Token
	err := db.pool.QueryRow(ctx, query, address).Scan(
		&token.ID, &token.Address, &token.Name, &token.Symbol, &token.Decimals, &token.TotalSupply,
		&token.HoldersCount, &token.TransfersCount, &token.Creator, &token.ContractType,
		&token.IsVerified, &token.IsActive, &token.Price, &token.MarketCap, &token.Volume24h, &token.PriceChange24h,
		&token.TxHash, &token.BlockNumber, &token.CreatedAt, &token.UpdatedAt,
	)
	if err == sql.ErrNoRows {
		return nil, nil
	}
	return &token, err
}

// GetTokens retrieves tokens with pagination
func (db *DB) GetTokens(ctx context.Context, limit, offset int) ([]*Token, error) {
	query := `
		SELECT id, address, name, symbol, decimals, total_supply, holders_count, transfers_count,
			creator, contract_type, is_verified, is_active, price, market_cap, volume_24h, price_change_24h,
			tx_hash, block_number, created_at, updated_at
		FROM tokens ORDER BY transfers_count DESC LIMIT $1 OFFSET $2
	`

	rows, err := db.pool.Query(ctx, query, limit, offset)
	if err != nil {
		return nil, err
	}
	defer rows.Close()

	var tokens []*Token
	for rows.Next() {
		var token Token
		if err := rows.Scan(
			&token.ID, &token.Address, &token.Name, &token.Symbol, &token.Decimals, &token.TotalSupply,
			&token.HoldersCount, &token.TransfersCount, &token.Creator, &token.ContractType,
			&token.IsVerified, &token.IsActive, &token.Price, &token.MarketCap, &token.Volume24h, &token.PriceChange24h,
			&token.TxHash, &token.BlockNumber, &token.CreatedAt, &token.UpdatedAt,
		); err != nil {
			return nil, err
		}
		tokens = append(tokens, &token)
	}
	return tokens, rows.Err()
}

// =============================================================================
// TOKEN HOLDER OPERATIONS
// =============================================================================

// TokenHolder represents a token holder
type TokenHolder struct {
	ID            int64     `json:"id"`
	TokenAddress  string    `json:"tokenAddress"`
	Address      string    `json:"address"`
	Balance      string    `json:"balance"`
	Percent      float64   `json:"percent"`
	CreatedAt    time.Time `json:"createdAt"`
	UpdatedAt    time.Time `json:"updatedAt"`
}

// InsertTokenHolder inserts or updates a token holder
func (db *DB) InsertTokenHolder(ctx context.Context, holder *TokenHolder) error {
	query := `
		INSERT INTO token_holders (token_address, address, balance, percent, created_at, updated_at)
		VALUES ($1, $2, $3, $4, NOW(), NOW())
		ON CONFLICT (token_address, address) DO UPDATE SET
			balance = EXCLUDED.balance,
			percent = EXCLUDED.percent,
			updated_at = NOW()
	`

	_, err := db.pool.Exec(ctx, query,
		holder.TokenAddress, holder.Address, holder.Balance, holder.Percent,
	)
	return err
}

// GetTokenHolders retrieves holders for a token
func (db *DB) GetTokenHolders(ctx context.Context, tokenAddress string, limit, offset int) ([]*TokenHolder, error) {
	query := `
		SELECT id, token_address, address, balance, percent, created_at, updated_at
		FROM token_holders WHERE token_address = $1
		ORDER BY balance DESC LIMIT $2 OFFSET $3
	`

	rows, err := db.pool.Query(ctx, query, tokenAddress, limit, offset)
	if err != nil {
		return nil, err
	}
	defer rows.Close()

	var holders []*TokenHolder
	for rows.Next() {
		var holder TokenHolder
		if err := rows.Scan(
			&holder.ID, &holder.TokenAddress, &holder.Address, &holder.Balance,
			&holder.Percent, &holder.CreatedAt, &holder.UpdatedAt,
		); err != nil {
			return nil, err
		}
		holders = append(holders, &holder)
	}
	return holders, rows.Err()
}

// =============================================================================
// TOKEN TRANSFER OPERATIONS
// =============================================================================

// TokenTransfer represents a token transfer
type TokenTransfer struct {
	ID            int64     `json:"id"`
	TokenAddress  string    `json:"tokenAddress"`
	Hash         string    `json:"hash"`
	BlockNumber  int64     `json:"blockNumber"`
	TransactionHash string  `json:"transactionHash"`
	FromAddress string    `json:"fromAddress"`
	ToAddress   string    `json:"toAddress"`
	Value      string    `json:"value"`
	LogIndex   *int64    `json:"logIndex,omitempty"`
	Timestamp  int64     `json:"timestamp"`
	CreatedAt  time.Time `json:"createdAt"`
}

// InsertTokenTransfer inserts a token transfer
func (db *DB) InsertTokenTransfer(ctx context.Context, transfer *TokenTransfer) error {
	query := `
		INSERT INTO token_transfers (token_address, hash, block_number, transaction_hash,
			from_address, to_address, value, log_index, timestamp, created_at)
		VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, NOW())
		ON CONFLICT (hash) DO NOTHING
	`

	_, err := db.pool.Exec(ctx, query,
		transfer.TokenAddress, transfer.Hash, transfer.BlockNumber, transfer.TransactionHash,
		transfer.FromAddress, transfer.ToAddress, transfer.Value, transfer.LogIndex, transfer.Timestamp,
	)
	return err
}

// GetTokenTransfers retrieves transfers for a token
func (db *DB) GetTokenTransfers(ctx context.Context, tokenAddress string, limit, offset int) ([]*TokenTransfer, error) {
	query := `
		SELECT id, token_address, hash, block_number, transaction_hash, from_address, to_address,
			value, log_index, timestamp, created_at
		FROM token_transfers WHERE token_address = $1
		ORDER BY timestamp DESC LIMIT $2 OFFSET $3
	`

	rows, err := db.pool.Query(ctx, query, tokenAddress, limit, offset)
	if err != nil {
		return nil, err
	}
	defer rows.Close()

	var transfers []*TokenTransfer
	for rows.Next() {
		var transfer TokenTransfer
		if err := rows.Scan(
			&transfer.ID, &transfer.TokenAddress, &transfer.Hash, &transfer.BlockNumber,
			&transfer.TransactionHash, &transfer.FromAddress, &transfer.ToAddress,
			&transfer.Value, &transfer.LogIndex, &transfer.Timestamp, &transfer.CreatedAt,
		); err != nil {
			return nil, err
		}
		transfers = append(transfers, &transfer)
	}
	return transfers, rows.Err()
}

// =============================================================================
// NFT OPERATIONS
// =============================================================================

// NFTCollection represents an NFT collection
type NFTCollection struct {
	ID            int64     `json:"id"`
	Address       string    `json:"address"`
	Name          string    `json:"name"`
	Symbol        *string   `json:"symbol,omitempty"`
	Description  *string   `json:"description,omitempty"`
	ImageURL     *string   `json:"imageURL,omitempty"`
	ExternalURL  *string   `json:"externalURL,omitempty"`
	ContractType string   `json:"contractType"`
	TotalSupply int64     `json:"totalSupply"`
	OwnersCount int64     `json:"ownersCount"`
	NFTsCount   int64     `json:"nftsCount"`
	FloorPrice  *string   `json:"floorPrice,omitempty"`
	Volume24h   *string   `json:"volume24h,omitempty"`
	VolumeTotal *string   `json:"volumeTotal,omitempty"`
	Creator     *string   `json:"creator,omitempty"`
	IsVerified   bool      `json:"isVerified"`
	IsActive    bool      `json:"isActive"`
	TxHash      *string   `json:"txHash,omitempty"`
	BlockNumber *int64    `json:"blockNumber,omitempty"`
	CreatedAt   time.Time `json:"createdAt"`
	UpdatedAt   time.Time `json:"updatedAt"`
}

// InsertCollection inserts or updates an NFT collection
func (db *DB) InsertCollection(ctx context.Context, col *NFTCollection) error {
	query := `
		INSERT INTO collections (address, name, symbol, description, image_url, external_url, contract_type,
			total_supply, owners_count, nfts_count, floor_price, volume_24h, volume_total, creator,
			is_verified, is_active, tx_hash, block_number, created_at, updated_at)
		VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16, $17, NOW(), NOW())
		ON CONFLICT (address) DO UPDATE SET
			name = EXCLUDED.name,
			symbol = EXCLUDED.symbol,
			description = EXCLUDED.description,
			image_url = EXCLUDED.image_url,
			external_url = EXCLUDED.external_url,
			total_supply = EXCLUDED.total_supply,
			owners_count = EXCLUDED.owners_count,
			nfts_count = EXCLUDED.nfts_count,
			floor_price = EXCLUDED.floor_price,
			volume_24h = EXCLUDED.volume_24h,
			volume_total = EXCLUDED.volume_total,
			is_verified = EXCLUDED.is_verified,
			is_active = EXCLUDED.is_active,
			updated_at = NOW()
	`

	_, err := db.pool.Exec(ctx, query,
		col.Address, col.Name, col.Symbol, col.Description, col.ImageURL, col.ExternalURL,
		col.ContractType, col.TotalSupply, col.OwnersCount, col.NFTsCount, col.FloorPrice,
		col.Volume24h, col.VolumeTotal, col.Creator, col.IsVerified, col.IsActive,
		col.TxHash, col.BlockNumber,
	)
	return err
}

// GetCollection retrieves a collection by address
func (db *DB) GetCollection(ctx context.Context, address string) (*NFTCollection, error) {
	query := `
		SELECT id, address, name, symbol, description, image_url, external_url, contract_type,
			total_supply, owners_count, nfts_count, floor_price, volume_24h, volume_total, creator,
			is_verified, is_active, tx_hash, block_number, created_at, updated_at
		FROM collections WHERE address = $1
	`

	var col NFTCollection
	err := db.pool.QueryRow(ctx, query, address).Scan(
		&col.ID, &col.Address, &col.Name, &col.Symbol, &col.Description, &col.ImageURL,
		&col.ExternalURL, &col.ContractType, &col.TotalSupply, &col.OwnersCount, &col.NFTsCount,
		&col.FloorPrice, &col.Volume24h, &col.VolumeTotal, &col.Creator, &col.IsVerified,
		&col.IsActive, &col.TxHash, &col.BlockNumber, &col.CreatedAt, &col.UpdatedAt,
	)
	if err == sql.ErrNoRows {
		return nil, nil
	}
	return &col, err
}

// GetCollections retrieves collections with pagination
func (db *DB) GetCollections(ctx context.Context, limit, offset int) ([]*NFTCollection, error) {
	query := `
		SELECT id, address, name, symbol, description, image_url, external_url, contract_type,
			total_supply, owners_count, nfts_count, floor_price, volume_24h, volume_total, creator,
			is_verified, is_active, tx_hash, block_number, created_at, updated_at
		FROM collections ORDER BY volume_total DESC LIMIT $1 OFFSET $2
	`

	rows, err := db.pool.Query(ctx, query, limit, offset)
	if err != nil {
		return nil, err
	}
	defer rows.Close()

	var cols []*NFTCollection
	for rows.Next() {
		var col NFTCollection
		if err := rows.Scan(
			&col.ID, &col.Address, &col.Name, &col.Symbol, &col.Description, &col.ImageURL,
			&col.ExternalURL, &col.ContractType, &col.TotalSupply, &col.OwnersCount, &col.NFTsCount,
			&col.FloorPrice, &col.Volume24h, &col.VolumeTotal, &col.Creator, &col.IsVerified,
			&col.IsActive, &col.TxHash, &col.BlockNumber, &col.CreatedAt, &col.UpdatedAt,
		); err != nil {
			return nil, err
		}
		cols = append(cols, &col)
	}
	return cols, rows.Err()
}

// =============================================================================
// NFT TOKEN OPERATIONS
// =============================================================================

// NFTToken represents an NFT
type NFTToken struct {
	ID              int64     `json:"id"`
	Address         string    `json:"address"`
	TokenID         string    `json:"tokenId"`
	Owner           string    `json:"owner"`
	Creator        *string   `json:"creator,omitempty"`
	Name           *string   `json:"name,omitempty"`
	Description    *string   `json:"description,omitempty"`
	ImageURL       *string   `json:"imageURL,omitempty"`
	AnimationURL   *string   `json:"animationURL,omitempty"`
	ExternalURL    *string   `json:"externalURL,omitempty"`
	Attributes     *string   `json:"attributes,omitempty"`
	ContractType   string    `json:"contractType"`
	TokenURI       *string   `json:"tokenURI,omitempty"`
	CollectionAddress *string `json:"collectionAddress,omitempty"`
	BlockNumber    *int64    `json:"blockNumber,omitempty"`
	BlockHash      *string   `json:"blockHash,omitempty"`
	TransactionHash *string  `json:"transactionHash,omitempty"`
	Timestamp      *int64    `json:"timestamp,omitempty"`
	CreatedAt      time.Time `json:"createdAt"`
	UpdatedAt      time.Time `json:"updatedAt"`
}

// InsertNFT inserts or updates an NFT
func (db *DB) InsertNFT(ctx context.Context, nft *NFTToken) error {
	query := `
		INSERT INTO nfts (address, token_id, owner, creator, name, description, image_url,
			animation_url, external_url, attributes, contract_type, token_uri, collection_address,
			block_number, block_hash, transaction_hash, timestamp, created_at, updated_at)
		VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16, $17, NOW(), NOW())
		ON CONFLICT (address, token_id) DO UPDATE SET
			owner = EXCLUDED.owner,
			name = EXCLUDED.name,
			description = EXCLUDED.description,
			image_url = EXCLUDED.image_url,
			animation_url = EXCLUDED.animation_url,
			external_url = EXCLUDED.external_url,
			attributes = EXCLUDED.attributes,
			token_uri = EXCLUDED.token_uri,
			block_hash = EXCLUDED.block_hash,
			transaction_hash = EXCLUDED.transaction_hash,
			timestamp = EXCLUDED.timestamp,
			updated_at = NOW()
	`

	_, err := db.pool.Exec(ctx, query,
		nft.Address, nft.TokenID, nft.Owner, nft.Creator, nft.Name, nft.Description,
		nft.ImageURL, nft.AnimationURL, nft.ExternalURL, nft.Attributes, nft.ContractType,
		nft.TokenURI, nft.CollectionAddress, nft.BlockNumber, nft.BlockHash,
		nft.TransactionHash, nft.Timestamp,
	)
	return err
}

// GetNFT retrieves an NFT by address and token ID
func (db *DB) GetNFT(ctx context.Context, address, tokenID string) (*NFTToken, error) {
	query := `
		SELECT id, address, token_id, owner, creator, name, description, image_url, animation_url,
			external_url, attributes, contract_type, token_uri, collection_address,
			block_number, block_hash, transaction_hash, timestamp, created_at, updated_at
		FROM nfts WHERE address = $1 AND token_id = $2
	`

	var nft NFTToken
	err := db.pool.QueryRow(ctx, query, address, tokenID).Scan(
		&nft.ID, &nft.Address, &nft.TokenID, &nft.Owner, &nft.Creator, &nft.Name,
		&nft.Description, &nft.ImageURL, &nft.AnimationURL, &nft.ExternalURL, &nft.Attributes,
		&nft.ContractType, &nft.TokenURI, &nft.CollectionAddress, &nft.BlockNumber,
		&nft.BlockHash, &nft.TransactionHash, &nft.Timestamp, &nft.CreatedAt, &nft.UpdatedAt,
	)
	if err == sql.ErrNoRows {
		return nil, nil
	}
	return &nft, err
}

// GetNFTsByCollection retrieves NFTs for a collection
func (db *DB) GetNFTsByCollection(ctx context.Context, address string, limit, offset int) ([]*NFTToken, error) {
	query := `
		SELECT id, address, token_id, owner, creator, name, description, image_url, animation_url,
			external_url, attributes, contract_type, token_uri, collection_address,
			block_number, block_hash, transaction_hash, timestamp, created_at, updated_at
		FROM nfts WHERE address = $1 ORDER BY token_id LIMIT $2 OFFSET $3
	`

	rows, err := db.pool.Query(ctx, query, address, limit, offset)
	if err != nil {
		return nil, err
	}
	defer rows.Close()

	var nfts []*NFTToken
	for rows.Next() {
		var nft NFTToken
		if err := rows.Scan(
			&nft.ID, &nft.Address, &nft.TokenID, &nft.Owner, &nft.Creator, &nft.Name,
			&nft.Description, &nft.ImageURL, &nft.AnimationURL, &nft.ExternalURL, &nft.Attributes,
			&nft.ContractType, &nft.TokenURI, &nft.CollectionAddress, &nft.BlockNumber,
			&nft.BlockHash, &nft.TransactionHash, &nft.Timestamp, &nft.CreatedAt, &nft.UpdatedAt,
		); err != nil {
			return nil, err
		}
		nfts = append(nfts, &nft)
	}
	return nfts, rows.Err()
}

// =============================================================================
// CONTRACT OPERATIONS
// =============================================================================

// Contract represents a verified smart contract
type Contract struct {
	ID                  int64     `json:"id"`
	Address            string    `json:"address"`
	Name               string    `json:"name"`
	Compiler           string    `json:"compiler"`
	Version            string    `json:"version"`
	OptimizationEnabled bool     `json:"optimizationEnabled"`
	OptimizationRuns  int       `json:"optimizationRuns"`
	SourceCode         string    `json:"sourceCode"`
	ABI                *string   `json:"abi,omitempty"`
	Bytecode           *string   `json:"bytecode,omitempty"`
	ConstructorArgs   *string   `json:"constructorArgs,omitempty"`
	EVMVersion        *string   `json:"evmVersion,omitempty"`
	LibraryRefs       *string   `json:"libraryRefs,omitempty"`
	IsVerified        bool      `json:"isVerified"`
	VerificationDate *time.Time `json:"verificationDate,omitempty"`
	VerifiedBy       *string   `json:"verifiedBy,omitempty"`
	IsProxy           bool      `json:"isProxy"`
	ProxyImplementation *string `json:"proxyImplementation,omitempty"`
	IsUpgradable      bool      `json:"isUpgradable"`
	License           *string   `json:"license,omitempty"`
	ExternalLibs      *string   `json:"externalLibs,omitempty"`
	HitsCount         int       `json:"hitsCount"`
	CreatedAt         time.Time `json:"createdAt"`
	UpdatedAt         time.Time `json:"updatedAt"`
}

// InsertContract inserts or updates a contract
func (db *DB) InsertContract(ctx context.Context, contract *Contract) error {
	query := `
		INSERT INTO contracts (address, name, compiler, version, optimization_enabled,
			optimization_runs, source_code, abi, bytecode, constructor_args, evm_version,
			library_refs, is_verified, verification_date, verified_by, is_proxy, proxy_implementation,
			is_upgradable, license, external_libs, hits_count, created_at, updated_at)
		VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16, $17, $18, $19, $20, $21, NOW(), NOW())
		ON CONFLICT (address) DO UPDATE SET
			name = EXCLUDED.name,
			source_code = EXCLUDED.source_code,
			abi = EXCLUDED.abi,
			is_verified = EXCLUDED.is_verified,
			is_proxy = EXCLUDED.is_proxy,
			proxy_implementation = EXCLUDED.proxy_implementation,
			is_upgradable = EXCLUDED.is_upgradable,
			hits_count = contracts.hits_count + 1,
			updated_at = NOW()
	`

	_, err := db.pool.Exec(ctx, query,
		contract.Address, contract.Name, contract.Compiler, contract.Version,
		contract.OptimizationEnabled, contract.OptimizationRuns, contract.SourceCode, contract.ABI,
		contract.Bytecode, contract.ConstructorArgs, contract.EVMVersion, contract.LibraryRefs,
		contract.IsVerified, contract.VerificationDate, contract.VerifiedBy, contract.IsProxy,
		contract.ProxyImplementation, contract.IsUpgradable, contract.License, contract.ExternalLibs,
		contract.HitsCount,
	)
	return err
}

// GetContract retrieves a contract by address
func (db *DB) GetContract(ctx context.Context, address string) (*Contract, error) {
	// Increment hits count
	db.pool.Exec(ctx, "UPDATE contracts SET hits_count = hits_count + 1 WHERE address = $1", address)

	query := `
		SELECT id, address, name, compiler, version, optimization_enabled, optimization_runs,
			source_code, abi, bytecode, constructor_args, evm_version, library_refs, is_verified,
			verification_date, verified_by, is_proxy, proxy_implementation, is_upgradable,
			license, external_libs, hits_count, created_at, updated_at
		FROM contracts WHERE address = $1 AND is_verified = TRUE
	`

	var contract Contract
	err := db.pool.QueryRow(ctx, query, address).Scan(
		&contract.ID, &contract.Address, &contract.Name, &contract.Compiler, &contract.Version,
		&contract.OptimizationEnabled, &contract.OptimizationRuns, &contract.SourceCode, &contract.ABI,
		&contract.Bytecode, &contract.ConstructorArgs, &contract.EVMVersion, &contract.LibraryRefs,
		&contract.IsVerified, &contract.VerificationDate, &contract.VerifiedBy, &contract.IsProxy,
		&contract.ProxyImplementation, &contract.IsUpgradable, &contract.License, &contract.ExternalLibs,
		&contract.HitsCount, &contract.CreatedAt, &contract.UpdatedAt,
	)
	if err == sql.ErrNoRows {
		return nil, nil
	}
	return &contract, err
}

// GetVerifiedContracts retrieves verified contracts
func (db *DB) GetVerifiedContracts(ctx context.Context, limit, offset int) ([]*Contract, error) {
	query := `
		SELECT id, address, name, compiler, version, optimization_enabled, optimization_runs,
			source_code, abi, bytecode, constructor_args, evm_version, library_refs, is_verified,
			verification_date, verified_by, is_proxy, proxy_implementation, is_upgradable,
			license, external_libs, hits_count, created_at, updated_at
		FROM contracts WHERE is_verified = TRUE ORDER BY hits_count DESC LIMIT $1 OFFSET $2
	`

	rows, err := db.pool.Query(ctx, query, limit, offset)
	if err != nil {
		return nil, err
	}
	defer rows.Close()

	var contracts []*Contract
	for rows.Next() {
		var contract Contract
		if err := rows.Scan(
			&contract.ID, &contract.Address, &contract.Name, &contract.Compiler, &contract.Version,
			&contract.OptimizationEnabled, &contract.OptimizationRuns, &contract.SourceCode, &contract.ABI,
			&contract.Bytecode, &contract.ConstructorArgs, &contract.EVMVersion, &contract.LibraryRefs,
			&contract.IsVerified, &contract.VerificationDate, &contract.VerifiedBy, &contract.IsProxy,
			&contract.ProxyImplementation, &contract.IsUpgradable, &contract.License, &contract.ExternalLibs,
			&contract.HitsCount, &contract.CreatedAt, &contract.UpdatedAt,
		); err != nil {
			return nil, err
		}
		contracts = append(contracts, &contract)
	}
	return contracts, rows.Err()
}

// =============================================================================
// LOG OPERATIONS
// =============================================================================

// Log represents an EVM event log
type Log struct {
	ID            int64     `json:"id"`
	TransactionHash string  `json:"transactionHash"`
	BlockNumber  int64     `json:"blockNumber"`
	Address      string    `json:"address"`
	Topics       string    `json:"topics"`
	Data         string    `json:"data"`
	LogIndex     int64     `json:"logIndex"`
	Timestamp    int64     `json:"timestamp"`
	CreatedAt    time.Time `json:"createdAt"`
}

// InsertLog inserts a log
func (db *DB) InsertLog(ctx context.Context, log *Log) error {
	query := `
		INSERT INTO logs (transaction_hash, block_number, address, topics, data, log_index, timestamp, created_at)
		VALUES ($1, $2, $3, $4, $5, $6, $7, NOW())
		ON CONFLICT (transaction_hash, log_index) DO NOTHING
	`

	_, err := db.pool.Exec(ctx, query,
		log.TransactionHash, log.BlockNumber, log.Address, log.Topics, log.Data, log.LogIndex, log.Timestamp,
	)
	return err
}

// GetLogsByTransaction retrieves logs for a transaction
func (db *DB) GetLogsByTransaction(ctx context.Context, txHash string) ([]*Log, error) {
	query := `
		SELECT id, transaction_hash, block_number, address, topics, data, log_index, timestamp, created_at
		FROM logs WHERE transaction_hash = $1 ORDER BY log_index
	`

	rows, err := db.pool.Query(ctx, query, txHash)
	if err != nil {
		return nil, err
	}
	defer rows.Close()

	var logs []*Log
	for rows.Next() {
		var log Log
		if err := rows.Scan(
			&log.ID, &log.TransactionHash, &log.BlockNumber, &log.Address,
			&log.Topics, &log.Data, &log.LogIndex, &log.Timestamp, &log.CreatedAt,
		); err != nil {
			return nil, err
		}
		logs = append(logs, &log)
	}
	return logs, rows.Err()
}

// =============================================================================
// GAS PRICE OPERATIONS
// =============================================================================

// GasPrice represents gas price data
type GasPrice struct {
	ID              int64     `json:"id"`
	BlockNumber     int64     `json:"blockNumber"`
	SlowGasPrice    int64     `json:"slowGasPrice"`
	AvgGasPrice     int64     `json:"avgGasPrice"`
	FastGasPrice   int64     `json:"fastGasPrice"`
	BaseFeePerGas  *int64    `json:"baseFeePerGas,omitempty"`
	Timestamp      int64     `json:"timestamp"`
	CreatedAt      time.Time `json:"createdAt"`
}

// InsertGasPrice inserts gas price data
func (db *DB) InsertGasPrice(ctx context.Context, gp *GasPrice) error {
	query := `
		INSERT INTO gas_prices (block_number, slow_gas_price, avg_gas_price, fast_gas_price, base_fee_per_gas, timestamp, created_at)
		VALUES ($1, $2, $3, $4, $5, $6, NOW())
	`

	_, err := db.pool.Exec(ctx, query,
		gp.BlockNumber, gp.SlowGasPrice, gp.AvgGasPrice, gp.FastGasPrice, gp.BaseFeePerGas, gp.Timestamp,
	)
	return err
}

// GetLatestGasPrice retrieves the latest gas price
func (db *DB) GetLatestGasPrice(ctx context.Context) (*GasPrice, error) {
	query := `
		SELECT id, block_number, slow_gas_price, avg_gas_price, fast_gas_price, base_fee_per_gas, timestamp, created_at
		FROM gas_prices ORDER BY timestamp DESC LIMIT 1
	`

	var gp GasPrice
	err := db.pool.QueryRow(ctx, query).Scan(
		&gp.ID, &gp.BlockNumber, &gp.SlowGasPrice, &gp.AvgGasPrice, &gp.FastGasPrice,
		&gp.BaseFeePerGas, &gp.Timestamp, &gp.CreatedAt,
	)
	if err == sql.ErrNoRows {
		return nil, nil
	}
	return &gp, err
}

// GetGasPriceHistory retrieves gas price history
func (db *DB) GetGasPriceHistory(ctx context.Context, since time.Time) ([]*GasPrice, error) {
	query := `
		SELECT id, block_number, slow_gas_price, avg_gas_price, fast_gas_price, base_fee_per_gas, timestamp, created_at
		FROM gas_prices WHERE timestamp >= $1 ORDER BY timestamp DESC
	`

	rows, err := db.pool.Query(ctx, query, since.Unix())
	if err != nil {
		return nil, err
	}
	defer rows.Close()

	var prices []*GasPrice
	for rows.Next() {
		var gp GasPrice
		if err := rows.Scan(
			&gp.ID, &gp.BlockNumber, &gp.SlowGasPrice, &gp.AvgGasPrice, &gp.FastGasPrice,
			&gp.BaseFeePerGas, &gp.Timestamp, &gp.CreatedAt,
		); err != nil {
			return nil, err
		}
		prices = append(prices, &gp)
	}
	return prices, rows.Err()
}

// =============================================================================
// VALIDATOR OPERATIONS
// =============================================================================

// Validator represents a validator
type Validator struct {
	ID              int64     `json:"id"`
	Address         string    `json:"address"`
	Name            *string   `json:"name,omitempty"`
	Website         *string   `json:"website,omitempty"`
	Email          *string   `json:"email,omitempty"`
	Description    *string   `json:"description,omitempty"`
	LogoURL        *string   `json:"logoURL,omitempty"`
	CommissionRate float64   `json:"commissionRate"`
	TotalStake     *string   `json:"totalStake,omitempty"`
	SelfStake      *string   `json:"selfStake,omitempty"`
	DelegatorsCount int      `json:"delegatorsCount"`
	BlocksProposed int      `json:"blocksProposed"`
	BlocksMissed   int      `json:"blocksMissed"`
	Uptime         float64   `json:"uptime"`
	IsActive       bool      `json:"isActive"`
	IsJailed       bool      `json:"isJailed"`
	JailReason    *string   `json:"jailReason,omitempty"`
	JailedUntil   *int64    `json:"jailedUntil,omitempty"`
	CreatedAt      time.Time `json:"createdAt"`
	UpdatedAt      time.Time `json:"updatedAt"`
}

// InsertValidator inserts or updates a validator
func (db *DB) InsertValidator(ctx context.Context, val *Validator) error {
	query := `
		INSERT INTO validators (address, name, website, email, description, logo_url, commission_rate,
			total_stake, self_stake, delegators_count, blocks_proposed, blocks_missed, uptime,
			is_active, is_jailed, jail_reason, jailed_until, created_at, updated_at)
		VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16, $17, NOW(), NOW())
		ON CONFLICT (address) DO UPDATE SET
			name = EXCLUDED.name,
			website = EXCLUDED.website,
			email = EXCLUDED.email,
			description = EXCLUDED.description,
			logo_url = EXCLUDED.logo_url,
			commission_rate = EXCLUDED.commission_rate,
			total_stake = EXCLUDED.total_stake,
			self_stake = EXCLUDED.self_stake,
			delegators_count = EXCLUDED.delegators_count,
			blocks_proposed = EXCLUDED.blocks_proposed,
			blocks_missed = EXCLUDED.blocks_missed,
			uptime = EXCLUDED.uptime,
			is_active = EXCLUDED.is_active,
			is_jailed = EXCLUDED.is_jailed,
			jail_reason = EXCLUDED.jail_reason,
			jailed_until = EXCLUDED.jailed_until,
			updated_at = NOW()
	`

	_, err := db.pool.Exec(ctx, query,
		val.Address, val.Name, val.Website, val.Email, val.Description, val.LogoURL,
		val.CommissionRate, val.TotalStake, val.SelfStake, val.DelegatorsCount, val.BlocksProposed,
		val.BlocksMissed, val.Uptime, val.IsActive, val.IsJailed, val.JailReason, val.JailedUntil,
	)
	return err
}

// GetValidators retrieves validators
func (db *DB) GetValidators(ctx context.Context, limit, offset int) ([]*Validator, error) {
	query := `
		SELECT id, address, name, website, email, description, logo_url, commission_rate,
			total_stake, self_stake, delegators_count, blocks_proposed, blocks_missed, uptime,
			is_active, is_jailed, jail_reason, jailed_until, created_at, updated_at
		FROM validators WHERE is_active = TRUE ORDER BY total_stake DESC LIMIT $1 OFFSET $2
	`

	rows, err := db.pool.Query(ctx, query, limit, offset)
	if err != nil {
		return nil, err
	}
	defer rows.Close()

	var validators []*Validator
	for rows.Next() {
		var val Validator
		if err := rows.Scan(
			&val.ID, &val.Address, &val.Name, &val.Website, &val.Email, &val.Description,
			&val.LogoURL, &val.CommissionRate, &val.TotalStake, &val.SelfStake, &val.DelegatorsCount,
			&val.BlocksProposed, &val.BlocksMissed, &val.Uptime, &val.IsActive, &val.IsJailed,
			&val.JailReason, &val.JailedUntil, &val.CreatedAt, &val.UpdatedAt,
		); err != nil {
			return nil, err
		}
		validators = append(validators, &val)
	}
	return validators, rows.Err()
}

// =============================================================================
// SEARCH OPERATIONS
// =============================================================================

// SearchResult represents a search result
type SearchResult struct {
	Type      string          `json:"type"`
	ID       string          `json:"id"`
	Data     json.RawMessage `json:"data"`
	Rank     float64         `json:"rank"`
}

// Search searches the explorer
func (db *DB) Search(ctx context.Context, query string, limit int) ([]*SearchResult, error) {
	// Try to parse as number (block)
	var blockNum int64
	if _, err := fmt.Sscanf(query, "%d", &blockNum); err == nil {
		// It's a number, check if it's a block
		if block, err := db.GetBlockByNumber(ctx, blockNum); err == nil && block != nil {
			data, _ := json.Marshal(block)
			return []*SearchResult{{Type: "block", ID: fmt.Sprintf("%d", blockNum), Data: data, Rank: 1.0}}, nil
		}
	}

	// Try as hash (transaction or block)
	if len(query) >= 64 && (strings.HasPrefix(query, "0x") || len(query) == 64) {
		hash := query
		if strings.HasPrefix(hash, "0x") {
			hash = hash[2:]
		}

		// Check transaction
		if tx, err := db.GetTransactionByHash(ctx, hash); err == nil && tx != nil {
			data, _ := json.Marshal(tx)
			return []*SearchResult{{Type: "transaction", ID: hash, Data: data, Rank: 1.0}}, nil
		}

		// Check block
		if block, err := db.GetBlockByHash(ctx, hash); err == nil && block != nil {
			data, _ := json.Marshal(block)
			return []*SearchResult{{Type: "block", ID: fmt.Sprintf("%d", block.Number), Data: data, Rank: 1.0}}, nil
		}
	}

	// Try as address
	if len(query) == 42 && strings.HasPrefix(query, "0x") {
		// Check account
		if acc, err := db.GetAccount(ctx, query); err == nil && acc != nil {
			data, _ := json.Marshal(acc)
			return []*SearchResult{{Type: "account", ID: query, Data: data, Rank: 0.8}}, nil
		}

		// Check token
		if token, err := db.GetToken(ctx, query); err == nil && token != nil {
			data, _ := json.Marshal(token)
			return []*SearchResult{{Type: "token", ID: query, Data: data, Rank: 1.0}}, nil
		}

		// Check contract
		if contract, err := db.GetContract(ctx, query); err == nil && contract != nil {
			data, _ := json.Marshal(contract)
			return []*SearchResult{{Type: "contract", ID: query, Data: data, Rank: 1.0}}, nil
		}
	}

	// Try as token symbol or name
	queryLower := strings.ToLower(query)
	tokens, err := db.GetTokens(ctx, limit, 0)
	if err == nil {
		for _, token := range tokens {
			if strings.EqualFold(token.Symbol, query) || strings.Contains(strings.ToLower(token.Name), queryLower) {
				data, _ := json.Marshal(token)
				return []*SearchResult{{Type: "token", ID: token.Address, Data: data, Rank: 0.9}}, nil
			}
		}
	}

	return nil, nil
}

// =============================================================================
// NETWORK STATS
// =============================================================================

// NetworkStats represents network statistics
type NetworkStats struct {
	TotalBlocks      int64   `json:"totalBlocks"`
	TotalTransactions int64   `json:"totalTransactions"`
	UniqueSenders    int64   `json:"uniqueSenders"`
	TotalTokens     int64   `json:"totalTokens"`
	TotalCollections int64   `json:"totalCollections"`
	TotalNFTs       int64   `json:"totalNFTs"`
	TotalGasUsed    int64   `json:"totalGasUsed"`
	CurrentGasPrice int64   `json:"currentGasPrice"`
}

// GetNetworkStats retrieves network statistics
func (db *DB) GetNetworkStats(ctx context.Context) (*NetworkStats, error) {
	stats := &NetworkStats{}

	err := db.pool.QueryRow(ctx, "SELECT COUNT(*) FROM blocks").Scan(&stats.TotalBlocks)
	if err != nil {
		return nil, err
	}

	err = db.pool.QueryRow(ctx, "SELECT COUNT(*) FROM transactions WHERE status = 1").Scan(&stats.TotalTransactions)
	if err != nil {
		return nil, err
	}

	err = db.pool.QueryRow(ctx, "SELECT COUNT(DISTINCT from_address) FROM transactions").Scan(&stats.UniqueSenders)
	if err != nil {
		return nil, err
	}

	err = db.pool.QueryRow(ctx, "SELECT COUNT(*) FROM tokens").Scan(&stats.TotalTokens)
	if err != nil {
		return nil, err
	}

	err = db.pool.QueryRow(ctx, "SELECT COUNT(*) FROM collections").Scan(&stats.TotalCollections)
	if err != nil {
		return nil, err
	}

	err = db.pool.QueryRow(ctx, "SELECT COUNT(*) FROM nfts").Scan(&stats.TotalNFTs)
	if err != nil {
		return nil, err
	}

	err = db.pool.QueryRow(ctx, "SELECT COALESCE(SUM(gas_used), 0) FROM blocks").Scan(&stats.TotalGasUsed)
	if err != nil {
		return nil, err
	}

	if gp, err := db.GetLatestGasPrice(ctx); err == nil && gp != nil {
		stats.CurrentGasPrice = gp.AvgGasPrice
	}

	return stats, nil
}

// =============================================================================
// BATCH OPERATIONS (For High Performance)
// =============================================================================

// BatchInsertBlocks inserts multiple blocks in a batch
func (db *DB) BatchInsertBlocks(ctx context.Context, blocks []*Block) error {
	if len(blocks) == 0 {
		return nil
	}

	batch := &pgx.Batch{}
	for _, block := range blocks {
		batch.Queue(`
			INSERT INTO blocks (number, hash, parent_hash, nonce, sha3_uncles, logs_bloom,
				transactions_root, state_root, receipts_root, miner, difficulty, total_difficulty,
				size, gas_limit, gas_used, timestamp, extra_data, base_fee_per_gas, created_at, updated_at)
			VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16, $17, $18, NOW(), NOW())
			ON CONFLICT (number) DO UPDATE SET
				hash = EXCLUDED.hash,
				gas_used = EXCLUDED.gas_used,
				transactions_root = EXCLUDED.transactions_root,
				timestamp = EXCLUDED.timestamp,
				updated_at = NOW()
		`,
			block.Number, block.Hash, block.ParentHash, block.Nonce, block.SHA3Uncles,
			block.LogsBloom, block.TransactionsRoot, block.StateRoot, block.ReceiptsRoot,
			block.Miner, block.Difficulty, block.TotalDifficulty, block.Size, block.GasLimit,
			block.GasUsed, block.Timestamp, block.ExtraData, block.BaseFeePerGas,
		)
	}

	br := db.pool.SendBatch(ctx, batch)
	defer br.Close()

	for _, block := range blocks {
		_, err := br.Exec()
		if err != nil {
			return err
		}
	}

	return nil
}

// BatchInsertTransactions inserts multiple transactions in a batch
func (db *DB) BatchInsertTransactions(ctx context.Context, txs []*Transaction) error {
	if len(txs) == 0 {
		return nil
	}

	batch := &pgx.Batch{}
	for _, tx := range txs {
		batch.Queue(`
			INSERT INTO transactions (hash, block_number, block_hash, transaction_index, from_address,
				to_address, value, gas_price, gas_limit, gas_used, nonce, input_data, v, r, s, status, timestamp, created_at)
			VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16, $17, NOW())
			ON CONFLICT (hash) DO UPDATE SET
				block_number = EXCLUDED.block_number,
				block_hash = EXCLUDED.block_hash,
				transaction_index = EXCLUDED.transaction_index,
				gas_used = EXCLUDED.gas_used,
				status = EXCLUDED.status,
				timestamp = EXCLUDED.timestamp
		`,
			tx.Hash, tx.BlockNumber, tx.BlockHash, tx.TransactionIndex, tx.FromAddress,
			tx.ToAddress, tx.Value, tx.GasPrice, tx.GasLimit, tx.GasUsed, tx.Nonce,
			tx.InputData, tx.V, tx.R, tx.S, tx.Status, tx.Timestamp,
		)
	}

	br := db.pool.SendBatch(ctx, batch)
	defer br.Close()

	for _, tx := range txs {
		_, err := br.Exec()
		if err != nil {
			return err
		}
	}

	return nil
}

// =============================================================================
// UTILITY FUNCTIONS
// =============================================================================

// NormalizeAddress normalizes an Ethereum address to lowercase with 0x prefix
func NormalizeAddress(addr string) string {
	addr = strings.ToLower(addr)
	if !strings.HasPrefix(addr, "0x") {
		addr = "0x" + addr
	}
	return addr
}

// ValidateAddress validates an Ethereum address
func ValidateAddress(addr string) bool {
	if !strings.HasPrefix(addr, "0x") {
		addr = "0x" + addr
	}
	if len(addr) != 42 {
		return false
	}
	addr = addr[2:]
	for _, c := range addr {
		if !((c >= '0' && c <= '9') || (c >= 'a' && c <= 'f')) {
			return false
		}
	}
	return true
}

// NormalizeHash normalizes a transaction/block hash
func NormalizeHash(hash string) string {
	hash = strings.ToLower(hash)
	if strings.HasPrefix(hash, "0x") {
		hash = hash[2:]
	}
	return hash
}

// ValidateHash validates a transaction/block hash
func ValidateHash(hash string) bool {
	if strings.HasPrefix(hash, "0x") {
		hash = hash[2:]
	}
	if len(hash) != 64 {
		return false
	}
	for _, c := range hash {
		if !((c >= '0' && c <= '9') || (c >= 'a' && c <= 'f')) {
			return false
		}
	}
	return true
}

// HexToBigInt converts a hex string to big int
func HexToBigInt(hex string) string {
	if strings.HasPrefix(hex, "0x") {
		hex = hex[2:]
	}
	if hex == "" {
		return "0"
	}
	val, err := new(big.Int).SetString(hex, 16)
	if err != nil {
		return "0"
	}
	return val.String()
}

// BigIntToHex converts a big int to hex
func BigIntToHex(val string) string {
	n := new(big.Int)
	n.SetString(val, 10)
	return "0x" + n.Text(16)
}

// MustNewDB creates a new database or panics
func MustNewDB(ctx context.Context, config *Config) *DB {
	db, err := NewDB(ctx, config)
	if err != nil {
		panic(fmt.Sprintf("failed to create database: %v", err))
	}
	return db
}