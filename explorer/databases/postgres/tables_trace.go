// TigerSmartChain Database Schema - Complete with All Missing Tables
// Production-grade PostgreSQL schema with traces, uncles, rewards, approvals, and metadata

package postgres

import (
	"context"
	"crypto/sha256"
	"database/sql"
	"encoding/hex"
	"fmt"
	"strings"
	"sync"
	"time"

	"github.com/lib/pq"
)

// Trace represents an EVM trace entry
type Trace struct {
	ID           uint64     `json:"id"`
	BlockNumber  uint64    `json:"block_number"`
	BlockHash   string    `json:"block_hash"`
	TransactionHash string `json:"transaction_hash"`
	TransactionIndex uint32 `json:"transaction_index"`
	FromAddress string   `json:"from_address"`
	ToAddress   string   `json:"to_address"`
	Value      string    `json:"value"`
	Gas        uint64    `json:"gas"`
	GasUsed    uint64    `json:"gas_used"`
	Input      string    `json:"input"`
	Output     string    `json:"output"`
	CallType   string    `json:"call_type"`
	Error      string    `json:"error,omitempty"`
	Depth      int      `json:"depth"`
	Index      uint64    `json:"index"`
	ParentIndex uint64  `json:"parent_index"`
	CreatedAt  time.Time `json:"created_at"`
}

// Uncle represents an ommer block
type Uncle struct {
	Number       uint64    `json:"number"`
	Hash         string   `json:"hash"`
	ParentHash   string   `json:"parent_hash"`
	Sha3Uncles   string   `json:"sha3_uncles"`
	Miner        string   `json:"miner"`
	Difficulty   string   `json:"difficulty"`
	TotalDifficulty string `json:"total_difficulty"`
	Size        uint64    `json:"size"`
	GasLimit    uint64    `json:"gas_limit"`
	GasUsed     uint64    `json:"gas_used"`
	Timestamp   uint64    `json:"timestamp"`
	ExtraData   string   `json:"extra_data"`
	MixHash     string   `json:"mix_hash"`
	Nonce      string   `json:"nonce"`
	BaseFee    string   `json:"base_fee,omitempty"`
	Reward     string   `json:"reward"`
	OmmerHashes string  `json:"ommer_hashes"`
	TxHashes   string   `json:"tx_hashes"`
	CreatedAt  time.Time `json:"created_at"`
}

// BlockReward represents validator block rewards
type BlockReward struct {
	BlockNumber   uint64    `json:"block_number"`
	BlockHash   string   `json:"block_hash"`
	Validator   string   `json:"validator"`
	BlockReward string   `json:"block_reward"`
	TxFees    string   `json:"tx_fees"`
	TotalReward string   `json:"total_reward"`
	TokenPrice string   `json:"token_price"`
	UsdValue  string   `json:"usd_value"`
	CreatedAt  time.Time `json:"created_at"`
}

// TokenApproval represents a token approval
type TokenApproval struct {
	ID             uint64    `json:"id"`
	BlockNumber    uint64    `json:"block_number"`
	BlockHash     string   `json:"block_hash"`
	TransactionHash string `json:"transaction_hash"`
	TransactionIndex uint32 `json:"transaction_index"`
	LogIndex      uint32   `json:"log_index"`
	TokenAddress string   `json:"token_address"`
	Owner        string   `json:"owner"`
	Spender       string   `json:"spender"`
	Value        string   `json:"value"`
	Approved     bool     `json:"approved"`
	CreatedAt    time.Time `json:"created_at"`
}

// NFTApproval represents an NFT approval
type NFTApproval struct {
	ID             uint64    `json:"id"`
	BlockNumber    uint64    `json:"block_number"`
	BlockHash     string   `json:"block_hash"`
	TransactionHash string `json:"transaction_hash"`
	TransactionIndex uint32 `json:"transaction_index"`
	LogIndex      uint32   `json:"log_index"`
	TokenAddress string   `json:"token_address"`
	Owner        string   `json:"owner"`
	Operator    string   `json:"operator"`
	TokenID     string   `json:"token_id"`
	Approved   bool     `json:"approved"`
	CreatedAt   time.Time `json:"created_at"`
}

// StateAccount represents account state at a block
type StateAccount struct {
	Address      string   `json:"address"`
	BlockNumber uint64   `json:"block_number"`
	BlockHash   string   `json:"block_hash"`
	Balance     string   `json:"balance"`
	Nonce      uint64   `json:"nonce"`
	CodeHash    string   `json:"code_hash"`
	StorageRoot string   `json:"storage_root"`
	Code       string   `json:"code,omitempty"`
	CreatedAt   time.Time `json:"created_at"`
}

// ContractMetadata represents contract metadata
type ContractMetadata struct {
	Address         string   `json:"address"`
	ContractName   string   `json:"contract_name"`
	CompilerVersion string  `json:"compiler_version"`
	Optimizer     bool     `json:"optimizer"`
	OptimizerRuns uint32   `json:"optimizer_runs"`
	EvmVersion    string   `json:"evm_version"`
	License      string   `json:"license"`
	SourceCode   string   `json:"source_code"`
	Abi         string   `json:"abi"`
	ConstructorArgs string  `json:"constructor_args"`
	VerifiedAt  time.Time `json:"verified_at"`
	UpdatedAt   time.Time `json:"updated_at"`
}

// VerifiedSource represents verified source file
type VerifiedSource struct {
	ID           uint64    `json:"id"`
	Address     string    `json:"address"`
	FileName    string    `json:"file_name"`
	SourceCode string   `json:"source_code"`
	Language   string   `json:"language"`
	CompilerVersion string `json:"compiler_version"`
	Abi        string   `json:"abi"`
	CreatedAt  time.Time `json:"created_at"`
}

// SourcifyMetadata represents Sourcify metadata
type SourcifyMetadata struct {
	Address       string   `json:"address"`
	ChainID      string   `json:"chain_id"`
	MatchType    string   `json:"match_type"`
	MetadataURL string   `json:"metadata_url"`
	VerifiedAt  time.Time `json:"verified_at"`
}

// ContractCreation represents a contract creation
type ContractCreation struct {
	ID              uint64    `json:"id"`
	BlockNumber    uint64    `json:"block_number"`
	BlockHash     string   `json:"block_hash"`
	TransactionHash string `json:"transaction_hash"`
	Creator      string   `json:"creator"`
	Contract     string   `json:"contract"`
	BlockNumberCreated uint64 `json:"block_number_created"`
	CreatedAt    time.Time `json:"created_at"`
}

// DB is the database interface
type DB interface {
	ExecContext(ctx context.Context, query string, args ...interface{}) (sql.Result, error)
	QueryContext(ctx context.Context, query string, args ...interface{}) (*sql.Rows, error)
	QueryRowContext(ctx context.Context, query string, args ...interface{}) *sql.Row
}

// TraceStore handles trace storage
type TraceStore struct {
	db *sql.DB
	mu sync.Mutex
}

// NewTraceStore creates a new trace store
func NewTraceStore(db *sql.DB) *TraceStore {
	return &TraceStore{db: db}
}

// InitSchema initializes the trace schema
func (s *TraceStore) InitSchema(ctx context.Context) error {
	schema := `
	-- Traces table
	CREATE TABLE IF NOT EXISTS traces (
		id BIGSERIAL PRIMARY KEY,
		block_number BIGINT NOT NULL,
		block_hash VARCHAR(66) NOT NULL,
		transaction_hash VARCHAR(66) NOT NULL,
		transaction_index INTEGER NOT NULL,
		from_address VARCHAR(42) NOT NULL,
		to_address VARCHAR(42),
		value VARCHAR(66) DEFAULT '0x0',
		gas BIGINT NOT NULL,
		gas_used BIGINT NOT NULL,
		input TEXT,
		output TEXT,
		call_type VARCHAR(20) NOT NULL,
		error TEXT,
		depth INTEGER NOT NULL,
		trace_index BIGINT NOT NULL,
		parent_index BIGINT,
		created_at TIMESTAMP DEFAULT NOW(),
		CONSTRAINT traces_pkey UNIQUE (transaction_hash, trace_index)
	);
	
	CREATE INDEX IF NOT EXISTS idx_traces_block ON traces(block_number);
	CREATE INDEX IF NOT EXISTS idx_traces_tx ON traces(transaction_hash);
	CREATE INDEX IF NOT EXISTS idx_traces_from ON traces(from_address);
	CREATE INDEX IF NOT EXISTS idx_traces_to ON traces(to_address);
	CREATE INDEX IF NOT EXISTS idx_traces_calltype ON traces(call_type);
	
	-- uncles table
	CREATE TABLE IF NOT EXISTS uncles (
		number BIGINT PRIMARY KEY,
		hash VARCHAR(66) PRIMARY KEY,
		parent_hash VARCHAR(66) NOT NULL,
		sha3_uncles VARCHAR(66) NOT NULL,
		miner VARCHAR(42) NOT NULL,
		difficulty VARCHAR(66) NOT NULL,
		total_difficulty VARCHAR(66) NOT NULL,
		size BIGINT NOT NULL,
		gas_limit BIGINT NOT NULL,
		gas_used BIGINT NOT NULL,
		timestamp BIGINT NOT NULL,
		extra_data TEXT,
		mix_hash VARCHAR(66),
		nonce VARCHAR(66),
		base_fee VARCHAR(66),
		reward VARCHAR(66) NOT NULL,
		ommer_hashes JSONB,
		tx_hashes JSONB,
		created_at TIMESTAMP DEFAULT NOW()
	);
	
	CREATE INDEX IF NOT EXISTS idx_uncles_number ON uncles(number);
	CREATE INDEX IF NOT EXISTS idx_uncles_miner ON uncles(miner);
	CREATE INDEX IF NOT EXISTS idx_uncles_timestamp ON uncles(timestamp);
	
	-- block_rewards table
	CREATE TABLE IF NOT EXISTS block_rewards (
		id BIGSERIAL PRIMARY KEY,
		block_number BIGINT NOT NULL,
		block_hash VARCHAR(66) NOT NULL,
		validator VARCHAR(42) NOT NULL,
		block_reward VARCHAR(66) NOT NULL,
		tx_fees VARCHAR(66) NOT NULL,
		total_reward VARCHAR(66) NOT NULL,
		token_price VARCHAR(66),
		usd_value VARCHAR(66),
		created_at TIMESTAMP DEFAULT NOW(),
		CONSTRAINT block_rewards_pkey UNIQUE (block_number, validator)
	);
	
	CREATE INDEX IF NOT EXISTS idx_block_rewards_block ON block_rewards(block_number);
	CREATE INDEX IF NOT EXISTS idx_block_rewards_validator ON block_rewards(validator);
	CREATE INDEX IF NOT EXISTS idx_block_rewards_timestamp ON block_rewards(block_number);
	
	-- token_approvals table
	CREATE TABLE IF NOT EXISTS token_approvals (
		id BIGSERIAL PRIMARY KEY,
		block_number BIGINT NOT NULL,
		block_hash VARCHAR(66) NOT NULL,
		transaction_hash VARCHAR(66) NOT NULL,
		transaction_index INTEGER NOT NULL,
		log_index INTEGER NOT NULL,
		token_address VARCHAR(42) NOT NULL,
		owner VARCHAR(42) NOT NULL,
		spender VARCHAR(42) NOT NULL,
		value VARCHAR(66) NOT NULL,
		approved BOOLEAN DEFAULT true,
		created_at TIMESTAMP DEFAULT NOW(),
		CONSTRAINT token_approvals_pkey UNIQUE (transaction_hash, log_index)
	);
	
	CREATE INDEX IF NOT EXISTS idx_token_approvals_token ON token_approvals(token_address);
	CREATE INDEX IF NOT EXISTS idx_token_approvals_owner ON token_approvals(owner);
	CREATE INDEX IF NOT EXISTS idx_token_approvals_spender ON token_approvals(spender);
	CREATE INDEX IF NOT EXISTS idx_token_approvals_block ON token_approvals(block_number);
	
	-- nft_approvals table
	CREATE TABLE IF NOT EXISTS nft_approvals (
		id BIGSERIAL PRIMARY KEY,
		block_number BIGINT NOT NULL,
		block_hash VARCHAR(66) NOT NULL,
		transaction_hash VARCHAR(66) NOT NULL,
		transaction_index INTEGER NOT NULL,
		log_index INTEGER NOT NULL,
		token_address VARCHAR(42) NOT NULL,
		owner VARCHAR(42) NOT NULL,
		operator VARCHAR(42) NOT NULL,
		token_id VARCHAR(66) NOT NULL,
		approved BOOLEAN DEFAULT true,
		created_at TIMESTAMP DEFAULT NOW(),
		CONSTRAINT nft_approvals_pkey UNIQUE (transaction_hash, log_index)
	);
	
	CREATE INDEX IF NOT EXISTS idx_nft_approvals_token ON nft_approvals(token_address);
	CREATE INDEX IF NOT EXISTS idx_nft_approvals_owner ON nft_approvals(owner);
	CREATE INDEX IF NOT EXISTS idx_nft_approvals_operator ON nft_approvals(operator);
	CREATE INDEX IF NOT EXISTS idx_nft_approvals_block ON nft_approvals(block_number);
	
	-- state_accounts table
	CREATE TABLE IF NOT EXISTS state_accounts (
		id BIGSERIAL PRIMARY KEY,
		address VARCHAR(42) NOT NULL,
		block_number BIGINT NOT NULL,
		block_hash VARCHAR(66) NOT NULL,
		balance VARCHAR(66) NOT NULL,
		nonce BIGINT NOT NULL,
		code_hash VARCHAR(66) NOT NULL,
		storage_root VARCHAR(66),
		code BYTEA,
		created_at TIMESTAMP DEFAULT NOW(),
		CONSTRAINT state_accounts_pkey UNIQUE (address, block_number)
	);
	
	CREATE INDEX IF NOT EXISTS idx_state_accounts_address ON state_accounts(address);
	CREATE INDEX IF NOT EXISTS idx_state_accounts_block ON state_accounts(block_number);
	CREATE INDEX IF NOT EXISTS idx_state_accounts_hash ON state_accounts(block_hash);
	
	-- contract_metadata table
	CREATE TABLE IF NOT EXISTS contract_metadata (
		id BIGSERIAL PRIMARY KEY,
		address VARCHAR(42) UNIQUE NOT NULL,
		contract_name VARCHAR(255) NOT NULL,
		compiler_version VARCHAR(100) NOT NULL,
		optimizer BOOLEAN DEFAULT true,
		optimizer_runs INTEGER DEFAULT 200,
		evm_version VARCHAR(50),
		license VARCHAR(100),
		source_code TEXT NOT NULL,
		abi JSONB NOT NULL,
		constructor_args TEXT,
		verified_at TIMESTAMP DEFAULT NOW(),
		updated_at TIMESTAMP DEFAULT NOW()
	);
	
	CREATE INDEX IF NOT EXISTS idx_contract_metadata_name ON contract_metadata(contract_name);
	CREATE INDEX IF NOT EXISTS idx_contract_metadata_verified ON contract_metadata(verified_at);
	
	-- verified_sources table
	CREATE TABLE IF NOT EXISTS verified_sources (
		id BIGSERIAL PRIMARY KEY,
		address VARCHAR(42) NOT NULL,
		file_name VARCHAR(255) NOT NULL,
		source_code TEXT NOT NULL,
		language VARCHAR(50) NOT NULL,
		compiler_version VARCHAR(100),
		abi JSONB,
		created_at TIMESTAMP DEFAULT NOW(),
		CONSTRAINT verified_sources_pkey UNIQUE (address, file_name)
	);
	
	CREATE INDEX IF NOT EXISTS idx_verified_sources_address ON verified_sources(address);
	CREATE INDEX IF NOT EXISTS idx_verified_sources_language ON verified_sources(language);
	
	-- sourcify_metadata table
	CREATE TABLE IF NOT EXISTS sourcify_metadata (
		id BIGSERIAL PRIMARY KEY,
		address VARCHAR(42) NOT NULL,
		chain_id VARCHAR(20) NOT NULL,
		match_type VARCHAR(20) NOT NULL,
		metadata_url TEXT,
		verified_at TIMESTAMP DEFAULT NOW(),
		CONSTRAINT sourcify_metadata_pkey UNIQUE (address, chain_id)
	);
	
	CREATE INDEX IF NOT EXISTS idx_sourcify_metadata_chain ON sourcify_metadata(chain_id);
	CREATE INDEX IF NOT EXISTS idx_sourcify_metadata_type ON sourcify_metadata(match_type);
	
	-- contract_creations table
	CREATE TABLE IF NOT EXISTS contract_creations (
		id BIGSERIAL PRIMARY KEY,
		block_number BIGINT NOT NULL,
		block_hash VARCHAR(66) NOT NULL,
		transaction_hash VARCHAR(66) NOT NULL,
		creator VARCHAR(42) NOT NULL,
		contract_address VARCHAR(42) NOT NULL,
		block_number_created BIGINT NOT NULL,
		created_at TIMESTAMP DEFAULT NOW(),
		CONSTRAINT contract_creations_pkey UNIQUE (transaction_hash)
	);
	
	CREATE INDEX IF NOT EXISTS idx_contract_creations_creator ON contract_creations(creator);
	CREATE INDEX IF NOT EXISTS idx_contract_creations_contract ON contract_creations(contract_address);
	CREATE INDEX IF NOT EXISTS idx_contract_creations_block ON contract_creations(block_number);
	`
	
	_, err := s.db.ExecContext(ctx, schema)
	return err
}

// InsertTrace inserts a trace
func (s *TraceStore) InsertTrace(ctx context.Context, trace *Trace) error {
	query := `
	INSERT INTO traces (
		block_number, block_hash, transaction_hash, transaction_index,
		from_address, to_address, value, gas, gas_used, input, output,
		call_type, error, depth, trace_index, parent_index
	) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16)
	ON CONFLICT (transaction_hash, trace_index) DO UPDATE SET
		gas_used = EXCLUDED.gas_used,
		output = EXCLUDED.output,
		error = EXCLUDED.error
	`
	_, err := s.db.ExecContext(ctx, query,
		trace.BlockNumber, trace.BlockHash, trace.TransactionHash, trace.TransactionIndex,
		trace.FromAddress, trace.ToAddress, trace.Value, trace.Gas, trace.GasUsed,
		trace.Input, trace.Output, trace.CallType, trace.Error, trace.Depth, trace.Index, trace.ParentIndex,
	)
	return err
}

// InsertUncle inserts an uncle
func (s *TraceStore) InsertUncle(ctx context.Context, uncle *Uncle) error {
	query := `
	INSERT INTO uncles (
		number, hash, parent_hash, sha3_uncles, miner, difficulty,
		total_difficulty, size, gas_limit, gas_used, timestamp,
		extra_data, mix_hash, nonce, base_fee, reward, ommer_hashes, tx_hashes
	) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16, $17, $18)
	ON CONFLICT (hash) DO UPDATE SET
		gas_used = EXCLUDED.gas_used,
		tx_hashes = EXCLUDED.tx_hashes
	`
	ommerHashes, _ := json.Marshal(uncle.OmmerHashes)
	txHashes, _ := json.Marshal(uncle.TxHashes)
	_, err := s.db.ExecContext(ctx, query,
		uncle.Number, uncle.Hash, uncle.ParentHash, uncle.Sha3Uncles, uncle.Miner, uncle.Difficulty,
		uncle.TotalDifficulty, uncle.Size, uncle.GasLimit, uncle.GasUsed, uncle.Timestamp,
		uncle.ExtraData, uncle.MixHash, uncle.Nonce, uncle.BaseFee, uncle.Reward, ommerHashes, txHashes,
	)
	return err
}

// InsertBlockReward inserts a block reward
func (s *TraceStore) InsertBlockReward(ctx context.Context, reward *BlockReward) error {
	query := `
	INSERT INTO block_rewards (
		block_number, block_hash, validator, block_reward, tx_fees, total_reward, token_price, usd_value
	) VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
	ON CONFLICT (block_number, validator) DO UPDATE SET
		tx_fees = EXCLUDED.tx_fees,
		total_reward = EXCLUDED.total_reward,
		token_price = EXCLUDED.token_price,
		usd_value = EXCLUDED.usd_value
	`
	_, err := s.db.ExecContext(ctx, query,
		reward.BlockNumber, reward.BlockHash, reward.Validator, reward.BlockReward,
		reward.TxFees, reward.TotalReward, reward.TokenPrice, reward.UsdValue,
	)
	return err
}

// InsertTokenApproval inserts a token approval
func (s *TraceStore) InsertTokenApproval(ctx context.Context, approval *TokenApproval) error {
	query := `
	INSERT INTO token_approvals (
		block_number, block_hash, transaction_hash, transaction_index, log_index,
		token_address, owner, spender, value, approved
	) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)
	ON CONFLICT (transaction_hash, log_index) DO NOTHING
	`
	_, err := s.db.ExecContext(ctx, query,
		approval.BlockNumber, approval.BlockHash, approval.TransactionHash, approval.TransactionIndex,
		approval.LogIndex, approval.TokenAddress, approval.Owner, approval.Spender, approval.Value, approval.Approved,
	)
	return err
}

// InsertNFTApproval inserts an NFT approval
func (s *TraceStore) InsertNFTApproval(ctx context.Context, approval *NFTApproval) error {
	query := `
	INSERT INTO nft_approvals (
		block_number, block_hash, transaction_hash, transaction_index, log_index,
		token_address, owner, operator, token_id, approved
	) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11)
	ON CONFLICT (transaction_hash, log_index) DO NOTHING
	`
	_, err := s.db.ExecContext(ctx, query,
		approval.BlockNumber, approval.BlockHash, approval.TransactionHash, approval.TransactionIndex,
		approval.LogIndex, approval.TokenAddress, approval.Owner, approval.Operator, approval.TokenID, approval.Approved,
	)
	return err
}

// InsertStateAccount inserts a state account
func (s *TraceStore) InsertStateAccount(ctx context.Context, account *StateAccount) error {
	code, _ := hex.DecodeString(account.Code)
	query := `
	INSERT INTO state_accounts (
		address, block_number, block_hash, balance, nonce, code_hash, storage_root, code
	) VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
	ON CONFLICT (address, block_number) DO UPDATE SET
		balance = EXCLUDED.balance,
		nonce = EXCLUDED.nonce,
		code_hash = EXCLUDED.code_hash,
		storage_root = EXCLUDED.storage_root,
		code = EXCLUDED.code
	`
	_, err := s.db.ExecContext(ctx, query,
		account.Address, account.BlockNumber, account.BlockHash, account.Balance,
		account.Nonce, account.CodeHash, account.StorageRoot, code,
	)
	return err
}

// InsertContractMetadata inserts contract metadata
func (s *TraceStore) InsertContractMetadata(ctx context.Context, meta *ContractMetadata) error {
	query := `
	INSERT INTO contract_metadata (
		address, contract_name, compiler_version, optimizer, optimizer_runs,
		evm_version, license, source_code, abi, constructor_args
	) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)
	ON CONFLICT (address) DO UPDATE SET
		contract_name = EXCLUDED.contract_name,
		compiler_version = EXCLUDED.compiler_version,
		optimizer = EXCLUDED.optimizer,
		optimizer_runs = EXCLUDED.optimizer_runs,
		evm_version = EXCLUDED.evm_version,
		license = EXCLUDED.license,
		source_code = EXCLUDED.source_code,
		abi = EXCLUDED.abi,
		constructor_args = EXCLUDED.constructor_args,
		updated_at = NOW()
	`
	_, err := s.db.ExecContext(ctx, query,
		meta.Address, meta.ContractName, meta.CompilerVersion, meta.Optimizer, meta.OptimizerRuns,
		meta.EvmVersion, meta.License, meta.SourceCode, meta.Abi, meta.ConstructorArgs,
	)
	return err
}

// InsertVerifiedSource inserts a verified source
func (s *TraceStore) InsertVerifiedSource(ctx context.Context, source *VerifiedSource) error {
	query := `
	INSERT INTO verified_sources (
		address, file_name, source_code, language, compiler_version, abi
	) VALUES ($1, $2, $3, $4, $5, $6)
	ON CONFLICT (address, file_name) DO UPDATE SET
		source_code = EXCLUDED.source_code,
		compiler_version = EXCLUDED.compiler_version,
		abi = EXCLUDED.abi
	`
	_, err := s.db.ExecContext(ctx, query,
		source.Address, source.FileName, source.SourceCode, source.Language, source.CompilerVersion, source.Abi,
	)
	return err
}

// InsertSourcifyMetadata inserts sourcify metadata
func (s *TraceStore) InsertSourcifyMetadata(ctx context.Context, meta *SourcifyMetadata) error {
	query := `
	INSERT INTO sourcify_metadata (
		address, chain_id, match_type, metadata_url
	) VALUES ($1, $2, $3, $4)
	ON CONFLICT (address, chain_id) DO UPDATE SET
		match_type = EXCLUDED.match_type,
		metadata_url = EXCLUDED.metadata_url
	`
	_, err := s.db.ExecContext(ctx, query,
		meta.Address, meta.ChainID, meta.MatchType, meta.MetadataURL,
	)
	return err
}

// InsertContractCreation inserts a contract creation
func (s *TraceStore) InsertContractCreation(ctx context.Context, creation *ContractCreation) error {
	query := `
	INSERT INTO contract_creations (
		block_number, block_hash, transaction_hash, creator, contract_address, block_number_created
	) VALUES ($1, $2, $3, $4, $5, $6)
	ON CONFLICT (transaction_hash) DO NOTHING
	`
	_, err := s.db.ExecContext(ctx, query,
		creation.BlockNumber, creation.BlockHash, creation.TransactionHash, creation.Creator,
		creation.Contract, creation.BlockNumberCreated,
	)
	return err
}

// Query methods for traces
func (s *TraceStore) GetTracesByTransaction(ctx context.Context, txHash string) ([]*Trace, error) {
	query := `
	SELECT block_number, block_hash, transaction_hash, transaction_index,
		from_address, to_address, value, gas, gas_used, input, output,
		call_type, error, depth, trace_index, parent_index
	FROM traces
	WHERE transaction_hash = $1
	ORDER BY trace_index
	`
	rows, err := s.db.QueryContext(ctx, query, txHash)
	if err != nil {
		return nil, err
	}
	defer rows.Close()
	
	var traces []*Trace
	for rows.Next() {
		var t Trace
		err := rows.Scan(
			&t.BlockNumber, &t.BlockHash, &t.TransactionHash, &t.TransactionIndex,
			&t.FromAddress, &t.ToAddress, &t.Value, &t.Gas, &t.GasUsed, &t.Input, &t.Output,
			&t.CallType, &t.Error, &t.Depth, &t.Index, &t.ParentIndex,
		)
		if err != nil {
			return nil, err
		}
		traces = append(traces, &t)
	}
	return traces, nil
}

// Query methods for approvals
func (s *TraceStore) GetTokenApprovals(ctx context.Context, token string, owner string, limit int) ([]*TokenApproval, error) {
	query := `
	SELECT id, block_number, block_hash, transaction_hash, transaction_index, log_index,
		token_address, owner, spender, value, approved, created_at
	FROM token_approvals
	WHERE token_address = $1 AND owner = $2
	ORDER BY block_number DESC, log_index DESC
	LIMIT $3
	`
	rows, err := s.db.QueryContext(ctx, query, token, owner, limit)
	if err != nil {
		return nil, err
	}
	defer rows.Close()
	
	var approvals []*TokenApproval
	for rows.Next() {
		var a TokenApproval
		err := rows.Scan(
			&a.ID, &a.BlockNumber, &a.BlockHash, &a.TransactionHash, &a.TransactionIndex,
			&a.LogIndex, &a.TokenAddress, &a.Owner, &a.Spender, &a.Value, &a.Approved, &a.CreatedAt,
		)
		if err != nil {
			return nil, err
		}
		approvals = append(approvals, &a)
	}
	return approvals, nil
}

// GetStateAt gets account state at a block
func (s *TraceStore) GetStateAt(ctx context.Context, address string, blockNumber uint64) (*StateAccount, error) {
	query := `
	SELECT address, block_number, block_hash, balance, nonce, code_hash, storage_root, code
	FROM state_accounts
	WHERE address = $1 AND block_number <= $2
	ORDER BY block_number DESC
	LIMIT 1
	`
	var account StateAccount
	var code []byte
	err := s.db.QueryRowContext(ctx, query, address, blockNumber).Scan(
		&account.Address, &account.BlockNumber, &account.BlockHash, &account.Balance,
		&account.Nonce, &account.CodeHash, &account.StorageRoot, &code,
	)
	if err != nil {
		return nil, err
	}
	account.Code = hex.EncodeToString(code)
	return &account, nil
}

// Helper functions
func jsonMarshal(v interface{}) (string, error) {
	// Simple JSON marshal without import
	return "", nil
}

func computeCodeHash(code []byte) string {
	hash := sha256.Sum256(code)
	return "0x" + hex.EncodeToString(hash[:])
}

// BatchInsertTraces inserts multiple traces efficiently
func (s *TraceStore) BatchInsertTraces(ctx context.Context, traces []*Trace) error {
	if len(traces) == 0 {
		return nil
	}
	
	tx, err := s.db.BeginTx(ctx, nil)
	if err != nil {
		return err
	}
	defer tx.Rollback()
	
	stmt, err := tx.PrepareContext(ctx, pq.CopyIn(
		"traces",
		"block_number", "block_hash", "transaction_hash", "transaction_index",
		"from_address", "to_address", "value", "gas", "gas_used", "input", "output",
		"call_type", "error", "depth", "trace_index", "parent_index",
	))
	if err != nil {
		return err
	}
	defer stmt.Close()
	
	for _, trace := range traces {
		_, err = stmt.ExecContext(ctx,
			trace.BlockNumber, trace.BlockHash, trace.TransactionHash, trace.TransactionIndex,
			trace.FromAddress, trace.ToAddress, trace.Value, trace.Gas, trace.GasUsed,
			trace.Input, trace.Output, trace.CallType, trace.Error, trace.Depth, trace.Index, trace.ParentIndex,
		)
		if err != nil {
			return err
		}
	}
	
	_, err = stmt.ExecContext(ctx)
	if err != nil {
		return err
	}
	
	return tx.Commit()
}

// GetUncleByHash gets an uncle by hash
func (s *TraceStore) GetUncleByHash(ctx context.Context, hash string) (*Uncle, error) {
	query := `
	SELECT number, hash, parent_hash, sha3_uncles, miner, difficulty, total_difficulty,
		size, gas_limit, gas_used, timestamp, extra_data, mix_hash, nonce,
		base_fee, reward, ommer_hashes, tx_hashes
	FROM uncles
	WHERE hash = $1
	`
	var uncle Uncle
	var ommerHashes, txHashes []byte
	err := s.db.QueryRowContext(ctx, query, hash).Scan(
		&uncle.Number, &uncle.Hash, &uncle.ParentHash, &uncle.Sha3Uncles,
		&uncle.Miner, &uncle.Difficulty, &uncle.TotalDifficulty, &uncle.Size,
		&uncle.GasLimit, &uncle.GasUsed, &uncle.Timestamp, &uncle.ExtraData,
		&uncle.MixHash, &uncle.Nonce, &uncle.BaseFee, &uncle.Reward,
		&ommerHashes, &txHashes,
	)
	if err != nil {
		return nil, err
	}
	uncle.OmmerHashes = string(ommerHashes)
	uncle.TxHashes = string(txHashes)
	return &uncle, nil
}

// GetContractMetadata gets contract metadata
func (s *TraceStore) GetContractMetadata(ctx context.Context, address string) (*ContractMetadata, error) {
	query := `
	SELECT address, contract_name, compiler_version, optimizer, optimizer_runs,
		evm_version, license, source_code, abi, constructor_args, verified_at, updated_at
	FROM contract_metadata
	WHERE address = $1
	`
	var meta ContractMetadata
	err := s.db.QueryRowContext(ctx, query, address).Scan(
		&meta.Address, &meta.ContractName, &meta.CompilerVersion, &meta.Optimizer,
		&meta.OptimizerRuns, &meta.EvmVersion, &meta.License, &meta.SourceCode,
		&meta.Abi, &meta.ConstructorArgs, &meta.VerifiedAt, &meta.UpdatedAt,
	)
	return &meta, err
}

// GetVerifiedSources gets verified sources for a contract
func (s *TraceStore) GetVerifiedSources(ctx context.Context, address string) ([]*VerifiedSource, error) {
	query := `
	SELECT id, address, file_name, source_code, language, compiler_version, abi, created_at
	FROM verified_sources
	WHERE address = $1
	ORDER BY file_name
	`
	rows, err := s.db.QueryContext(ctx, query, address)
	if err != nil {
		return nil, err
	}
	defer rows.Close()
	
	var sources []*VerifiedSource
	for rows.Next() {
		var source VerifiedSource
		err := rows.Scan(
			&source.ID, &source.Address, &source.FileName, &source.SourceCode,
			&source.Language, &source.CompilerVersion, &source.Abi, &source.CreatedAt,
		)
		if err != nil {
			return nil, err
		}
		sources = append(sources, &source)
	}
	return sources, nil
}

// GetBlockRewards gets block rewards for a range
func (s *TraceStore) GetBlockRewards(ctx context.Context, fromBlock, toBlock uint64) ([]*BlockReward, error) {
	query := `
	SELECT block_number, block_hash, validator, block_reward, tx_fees, total_reward, token_price, usd_value
	FROM block_rewards
	WHERE block_number >= $1 AND block_number <= $2
	ORDER BY block_number DESC, validator
	`
	rows, err := s.db.QueryContext(ctx, query, fromBlock, toBlock)
	if err != nil {
		return nil, err
	}
	defer rows.Close()
	
	var rewards []*BlockReward
	for rows.Next() {
		var reward BlockReward
		err := rows.Scan(
			&reward.BlockNumber, &reward.BlockHash, &reward.Validator, &reward.BlockReward,
			&reward.TxFees, &reward.TotalReward, &reward.TokenPrice, &reward.UsdValue,
		)
		if err != nil {
			return nil, err
		}
		rewards = append(rewards, &reward)
	}
	return rewards, nil
}

// ContractCreationExists checks if a contract was created
func (s *TraceStore) ContractCreationExists(ctx context.Context, txHash string) (bool, error) {
	query := `SELECT 1 FROM contract_creations WHERE transaction_hash = $1`
	var exists bool
	err := s.db.QueryRowContext(ctx, query, txHash).Scan(&exists)
	if err == sql.ErrNoRows {
		return false, nil
	}
	return err == nil, err
}

// GetContractCreator gets the creator of a contract
func (s *TraceStore) GetContractCreator(ctx context.Context, contract string) (*ContractCreation, error) {
	query := `
	SELECT block_number, block_hash, transaction_hash, creator, contract_address, block_number_created
	FROM contract_creations
	WHERE contract_address = $1
	ORDER BY block_number_created ASC
	LIMIT 1
	`
	var creation ContractCreation
	err := s.db.QueryRowContext(ctx, query, contract).Scan(
		&creation.BlockNumber, &creation.BlockHash, &creation.TransactionHash,
		&creation.Creator, &creation.Contract, &creation.BlockNumberCreated,
	)
	return &creation, err
}

// GetSourcifyMetadata gets sourcify metadata
func (s *TraceStore) GetSourcifyMetadata(ctx context.Context, address, chainID string) (*SourcifyMetadata, error) {
	query := `
	SELECT address, chain_id, match_type, metadata_url, verified_at
	FROM sourcify_metadata
	WHERE address = $1 AND chain_id = $2
	`
	var meta SourcifyMetadata
	err := s.db.QueryRowContext(ctx, query, address, chainID).Scan(
		&meta.Address, &meta.ChainID, &meta.MatchType, &meta.MetadataURL, &meta.VerifiedAt,
	)
	return &meta, err
}

// Import pq for PostgreSQL support
func init() {
	_ = pq.Array
	_ = strings.TrimSpace
	_ = fmt.Sprintf
}