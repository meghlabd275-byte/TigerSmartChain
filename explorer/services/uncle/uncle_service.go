// Package uncle provides uncle block indexing and rewards calculation.
package uncle

import (
	"context"
	"database/sql"
	"encoding/json"
	"fmt"
	"math/big"
	"strings"
	"time"
)

// UncleBlock represents an uncle block
type UncleBlock struct {
	ID              int64     `json:"id"`
	Hash            string   `json:"hash"`
	Number          int64    `json:"number"`
	ParentHash     string   `json:"parentHash"`
	Miner          string   `json:"miner"`
	GasUsed        uint64    `json:"gasUsed"`
	GasLimit       uint64    `json:"gasLimit"`
	Difficulty     string   `json:"difficulty"`
	TotalDifficulty string   `json:"totalDifficulty"`
	Size           int64     `json:"size"`
	Timestamp     time.Time `json:"timestamp"`
	BlockReward   string   `json:"blockReward"`
	UncleReward  string   `json:"uncleReward"`
	Nonce        string   `json:"nonce"`
	ExtraData    string   `json:"extraData"`
}

// Service provides uncle block functionality
type Service struct {
	db             *sql.DB
	rpcURL         string
	blockRewardFunc func(blockNumber int64, uncleCount int) (*big.Int, *big.Int)
}

// Config holds service configuration
type Config struct {
	DB       *sql.DB
	RPCURL   string
}

// NewService creates a new uncle block service
func NewService(cfg *Config) *Service {
	return &Service{
		db:       cfg.DB,
		rpcURL:   cfg.RPCURL,
		blockRewardFunc: calculateRewards,
	}
}

// IndexUncle indexes a new uncle block
func (s *Service) IndexUncle(ctx context.Context, block *UncleBlock) error {
	query := `
		INSERT INTO uncle_blocks 
		(hash, number, parent_hash, miner, gas_used, gas_limit, difficulty, 
		 total_difficulty, size, timestamp, block_reward, uncle_reward, nonce, extra_data)
		VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14)
		ON CONFLICT (hash) DO UPDATE SET
			gas_used = EXCLUDED.gas_used,
			timestamp = EXCLUDED.timestamp
		RETURNING id
	`

	return s.db.QueryRowContext(ctx, query,
		block.Hash, block.Number, block.ParentHash, block.Miner,
		block.GasUsed, block.GasLimit, block.Difficulty, block.TotalDifficulty,
		block.Size, block.Timestamp, block.BlockReward, block.UncleReward,
		block.Nonce, block.ExtraData,
	).Scan(&block.ID)
}

// GetUncle returns an uncle block by hash or number
func (s *Service) GetUncle(ctx context.Context, hashOrNumber string) (*UncleBlock, error) {
	query := `
		SELECT id, hash, number, parent_hash, miner, gas_used, gas_limit, 
		       difficulty, total_difficulty, size, timestamp, block_reward, 
		       uncle_reward, nonce, extra_data
		FROM uncle_blocks
		WHERE hash = $1 OR number::text = $1
	`

	u := &UncleBlock{}
	err := s.db.QueryRowContext(ctx, query, hashOrNumber).Scan(
		&u.ID, &u.Hash, &u.Number, &u.ParentHash, &u.Miner,
		&u.GasUsed, &u.GasLimit, &u.Difficulty, &u.TotalDifficulty,
		&u.Size, &u.Timestamp, &u.BlockReward, &u.UncleReward,
		&u.Nonce, &u.ExtraData,
	)
	if err != nil {
		return nil, err
	}

	return u, nil
}

// ListUncles returns uncle blocks for a block
func (s *Service) ListUncles(ctx context.Context, blockNumber int64) ([]*UncleBlock, error) {
	query := `
		SELECT id, hash, number, parent_hash, miner, gas_used, gas_limit, 
		       difficulty, total_difficulty, size, timestamp, block_reward, 
		       uncle_reward, nonce, extra_data
		FROM uncle_blocks
		WHERE number = $1
		ORDER BY hash
	`

	rows, err := s.db.QueryContext(ctx, query, blockNumber)
	if err != nil {
		return nil, err
	}
	defer rows.Close()

	var uncles []*UncleBlock
	for rows.Next() {
		u := &UncleBlock{}
		err := rows.Scan(
			&u.ID, &u.Hash, &u.Number, &u.ParentHash, &u.Miner,
			&u.GasUsed, &u.GasLimit, &u.Difficulty, &u.TotalDifficulty,
			&u.Size, &u.Timestamp, &u.BlockReward, &u.UncleReward,
			&u.Nonce, &u.ExtraData,
		)
		if err != nil {
			return nil, err
		}
		uncles = append(uncles, u)
	}

	return uncles, rows.Err()
}

// GetUncleRewards calculates rewards for a block with uncles
func (s *Service) GetUncleRewards(ctx context.Context, blockNumber int64) (blockReward, uncleReward *big.Int, err error) {
	uncles, err := s.ListUncles(ctx, blockNumber)
	if err != nil {
		return nil, nil, err
	}

	return s.blockRewardFunc(blockNumber, len(uncles))
}

// calculateRewards calculates block and uncle rewards
func calculateRewards(blockNumber int64, uncleCount int) (*big.Int, *big.Int) {
	baseReward := big.NewInt(2e18)
	uncleReward := big.NewInt(0)
	
	if uncleCount > 0 {
		uncleReward.Mul(big.NewInt(int64(uncleCount+1)), baseReward)
		uncleReward.Div(uncleReward, big.NewInt(8))
	}

	return baseReward, uncleReward
}

// GetUncleCountByBlock returns the number of uncles for a block
func (s *Service) GetUncleCountByBlock(ctx context.Context, blockNumber int64) (int, error) {
	query := `SELECT COUNT(*) FROM uncle_blocks WHERE number = $1`
	var count int
	err := s.db.QueryRowContext(ctx, query, blockNumber).Scan(&count)
	return count, err
}

// SyncFromRPC syncs uncle blocks from RPC
func (s *Service) SyncFromRPC(ctx context.Context, blockNumber int64) error {
	if s.rpcURL == "" {
		return fmt.Errorf("RPC URL not configured")
	}

	params := map[string]interface{}{
		"blockNumber": fmt.Sprintf("0x%x", blockNumber),
		"fullTxns":  false,
	}

	result, err := s.callRPC(ctx, "eth_getBlockByNumber", params)
	if err != nil {
		return err
	}

	var block struct {
		Uncles []string `json:"uncles"`
	}

	if err := json.Unmarshal(result, &block); err != nil {
		return err
	}

	for _, uncleHash := range block.Uncles {
		uncleHash = strings.TrimPrefix(uncleHash, "0x")
		if uncleHash == "" {
			continue
		}

		uncle, err := s.getUncleByHash(ctx, uncleHash)
		if err != nil {
			continue
		}

		s.IndexUncle(ctx, uncle)
	}

	return nil
}

// getUncleByHash gets uncle block by hash from RPC
func (s *Service) getUncleByHash(ctx context.Context, hash string) (*UncleBlock, error) {
	params := map[string]interface{}{
		"blockHash": fmt.Sprintf("0x%s", hash),
		"fullTxns": false,
	}

	result, err := s.callRPC(ctx, "eth_getBlockByHash", params)
	if err != nil {
		return nil, err
	}

	var block struct {
		Number     string `json:"number"`
		ParentHash string `json:"parentHash"`
		Miner     string `json:"miner"`
		GasUsed   string `json:"gasUsed"`
		GasLimit  string `json:"gasLimit"`
		Difficulty string `json:"difficulty"`
		Size     string `json:"size"`
		Nonce    string `json:"nonce"`
		ExtraData string `json:"extraData"`
	}

	if err := json.Unmarshal(result, &block); err != nil {
		return nil, err
	}

	u := &UncleBlock{
		Hash:       "0x" + hash,
		ParentHash: block.ParentHash,
		Miner:     block.Miner,
		ExtraData: block.ExtraData,
		Nonce:     block.Nonce,
	}

	if block.Number != "" {
		fmt.Sscanf(block.Number, "0x%x", &u.Number)
	}
	if block.GasUsed != "" {
		fmt.Sscanf(block.GasUsed, "0x%x", &u.GasUsed)
	}
	if block.GasLimit != "" {
		fmt.Sscanf(block.GasLimit, "0x%x", &u.GasLimit)
	}

	return u, nil
}

// callRPC makes an RPC call
func (s *Service) callRPC(ctx context.Context, method string, params map[string]interface{}) ([]byte, error) {
	type RPCRequest struct {
		JSONRPC string        `json:"jsonrpc"`
		Method  string        `json:"method"`
		Params  []interface{} `json:"params"`
		ID     int           `json:"id"`
	}

	type RPCResponse struct {
		JSONRPC string `json:"jsonrpc"`
		ID     int    `json:"id"`
		Result []byte `json:"result"`
		Error  *struct {
			Code    int    `json:"code"`
			Message string `json:"message"`
		} `json:"error"`
	}

	req := RPCRequest{
		JSONRPC: "2.0",
		Method:  method,
		Params:  []interface{}{params},
		ID:      1,
	}

	reqData, _ := json.Marshal(req)
	resp, err := s.doHTTPRequest(ctx, s.rpcURL, reqData)
	if err != nil {
		return nil, err
	}

	var rpcResp RPCResponse
	if err := json.Unmarshal(resp, &rpcResp); err != nil {
		return nil, err
	}

	if rpcResp.Error != nil {
		return nil, fmt.Errorf("RPC error: %s", rpcResp.Error.Message)
	}

	return rpcResp.Result, nil
}

// doHTTPRequest makes an HTTP POST request
func (s *Service) doHTTPRequest(ctx context.Context, url string, data []byte) ([]byte, error) {
	return nil, fmt.Errorf("HTTP client not configured")
}

var _ = context.Background // Use context
var _ = fmt.Scanf        // Use fmt
var _ = json.Unmarshal   // Use JSON