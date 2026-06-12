// Package pendingtx provides pending transaction pool tracking.
package pendingtx

import (
	"context"
	"database/sql"
	"encoding/json"
	"fmt"
	"math/big"
	"strings"
	"sync"
	"time"
)

// PendingTransaction represents a pending transaction
type PendingTransaction struct {
	Hash           string    `json:"hash"`
	From           string    `json:"from"`
	To             string    `json:"to"`
	Value          string    `json:"value"`
	GasPrice       int64     `json:"gasPrice"`
	GasLimit       int64     `json:"gasLimit"`
	InputData      string    `json:"inputData"`
	Nonce          int64     `json:"nonce"`
	ArrivedAt      time.Time `json:"arrivedAt"`
	FirstSeenAt   time.Time `json:"firstSeenAt"`
	ReplaceByFee  bool      `json:"replaceByFee"`
	GasUsed        int64     `json:"gasUsed,omitempty"`
}

// PoolStats represents pool statistics
type PoolStats struct {
	TotalPending    int     `json:"totalPending"`
	AvgGasPrice    int64   `json:"avgGasPrice"`
	TotalValue     string  `json:"totalValue"`
	ByFromAddress  map[string]int `json:"byFromAddress"`
}

// Service provides pending transaction tracking
type Service struct {
	db           *sql.DB
	rpcURL       string
	pool         map[string]*PendingTransaction
	mu           sync.RWMutex
	pollInterval time.Duration
	maxPoolSize int
}

// Config holds service configuration
type Config struct {
	DB           *sql.DB
	RPCURL       string
	PollInterval time.Duration
	MaxPoolSize  int
}

// NewService creates a new pending transaction service
func NewService(cfg *Config) *Service {
	interval := cfg.PollInterval
	if interval == 0 {
		interval = 5 * time.Second
	}
	maxSize := cfg.MaxPoolSize
	if maxSize == 0 {
		maxSize = 10000
	}

	return &Service{
		db:           cfg.DB,
		rpcURL:       cfg.RPCURL,
		pool:         make(map[string]*PendingTransaction),
		pollInterval: interval,
		maxPoolSize: maxSize,
	}
}

// Start starts the pool monitoring
func (s *Service) Start(ctx context.Context) error {
	if s.rpcURL == "" {
		return fmt.Errorf("RPC URL not configured")
	}

	ticker := time.NewTicker(s.pollInterval)
	defer ticker.Stop()

	for {
		select {
		case <-ctx.Done():
			return ctx.Err()
		case <-ticker.C:
			if err := s.pollPendingTxs(ctx); err != nil {
				continue
			}
		}
	}
}

// pollPendingTxs polls for pending transactions
func (s *Service) pollPendingTxs(ctx context.Context) error {
	params := map[string]interface{}{
		"includeTransactions": true,
	}

	result, err := s.callRPC(ctx, "eth_newPendingTransactionFilter", params)
	if err != nil {
		return err
	}

	var filter struct {
		FilterId string `json:"filterId"`
	}

	if err := json.Unmarshal(result, &filter); err != nil {
		return err
	}

	// Get changes
	changes, err := s.getFilterChanges(ctx, filter.FilterId)
	if err != nil {
		return err
	}

	for _, txHash := range changes {
		if err := s.addPendingTx(ctx, txHash); err != nil {
			continue
		}
	}

	return nil
}

// addPendingTx adds a pending transaction
func (s *Service) addPendingTx(ctx context.Context, txHash string) error {
	txHash = strings.TrimPrefix(txHash, "0x")
	if txHash == "" {
		return nil
	}

	// Get transaction details
	params := map[string]interface{}{
		"transactionHash": fmt.Sprintf("0x%s", txHash),
	}

	result, err := s.callRPC(ctx, "eth_getTransactionByHash", params)
	if err != nil {
		return err
	}

	var tx struct {
		Hash      string `json:"hash"`
		From     string `json:"from"`
		To       string `json:"to"`
		Value    string `json:"value"`
		GasPrice string `json:"gasPrice"`
		Gas      string `json:"gas"`
		Input    string `json:"input"`
		Nonce    string `json:"nonce"`
	}

	if err := json.Unmarshal(result, &tx); err != nil {
		return err
	}

	gasPrice := int64(0)
	if tx.GasPrice != "" {
		fmt.Sscanf(tx.GasPrice, "0x%x", &gasPrice)
	}

	gasLimit := int64(0)
	if tx.Gas != "" {
		fmt.Sscanf(tx.Gas, "0x%x", &gasLimit)
	}

	nonce := int64(0)
	if tx.Nonce != "" {
		fmt.Sscanf(tx.Nonce, "0x%x", &nonce)
	}

	pending := &PendingTransaction{
		Hash:         "0x" + tx.Hash,
		From:        strings.ToLower(tx.From),
		To:          strings.ToLower(tx.To),
		Value:       tx.Value,
		GasPrice:    gasPrice,
		GasLimit:    gasLimit,
		InputData:  tx.Input,
		Nonce:      nonce,
		ArrivedAt:  time.Now(),
		FirstSeenAt: time.Now(),
	}

	s.mu.Lock()
	// Check if replacing existing tx (ReplaceByFee)
	if existing, ok := s.pool[pending.Hash]; ok {
		if pending.GasPrice > existing.GasPrice && pending.Nonce == existing.Nonce {
			pending.ReplaceByFee = true
			pending.FirstSeenAt = existing.FirstSeenAt
		}
	}

	s.pool[pending.Hash] = pending

	// Trim pool if too large
	if len(s.pool) > s.maxPoolSize {
		s.trimPool()
	}

	s.mu.Unlock()

	return nil
}

// trimPool removes oldest transactions
func (s *Service) trimPool() {
	now := time.Now()
	minAge := now.Add(-1 * time.Hour)

	for hash, tx := range s.pool {
		if tx.ArrivedAt.Before(minAge) {
			delete(s.pool, hash)
		}

		if len(s.pool) <= s.maxPoolSize/2 {
			break
		}
	}
}

// getFilterChanges gets filter changes
func (s *Service) getFilterChanges(ctx context.Context, filterID string) ([]string, error) {
	params := map[string]interface{}{
		"filterId": filterID,
	}

	result, err := s.callRPC(ctx, "eth_getFilterChanges", params)
	if err != nil {
		return nil, err
	}

	var changes []string
	json.Unmarshal(result, &changes)
	return changes, nil
}

// GetPendingTx returns a pending transaction
func (s *Service) GetPendingTx(hash string) (*PendingTransaction, bool) {
	s.mu.RLock()
	defer s.mu.RUnlock()

	tx, ok := s.pool[hash]
	return tx, ok
}

// GetPendingTxs returns all pending transactions
func (s *Service) GetPendingTxs() []*PendingTransaction {
	s.mu.RLock()
	defer s.mu.RUnlock()

	txs := make([]*PendingTransaction, 0, len(s.pool))
	for _, tx := range s.pool {
		txs = append(txs, tx)
	}

	return txs
}

// GetPendingTxsByAddress returns pending transactions from an address
func (s *Service) GetPendingTxsByAddress(address string) []*PendingTransaction {
	s.mu.RLock()
	defer s.mu.RUnlock()

	address = strings.ToLower(address)
	var txs []*PendingTransaction

	for _, tx := range s.pool {
		if tx.From == address {
			txs = append(txs, tx)
		}
	}

	return txs
}

// GetPoolStats returns pool statistics
func (s *Service) GetPoolStats() *PoolStats {
	s.mu.RLock()
	defer s.mu.RUnlock()

	stats := &PoolStats{
		TotalPending:   len(s.pool),
		ByFromAddress: make(map[string]int),
	}

	var totalGas int64
	var totalValue big.Int

	for _, tx := range s.pool {
		totalGas += tx.GasPrice

		if tx.Value != "" {
			val := big.NewInt(0)
			val.SetString(tx.Value, 0)
			totalValue.Add(&totalValue, val)
		}

		stats.ByFromAddress[tx.From]++
	}

	if len(s.pool) > 0 {
		stats.AvgGasPrice = totalGas / int64(len(s.pool))
	}

	stats.TotalValue = totalValue.String()

	return stats
}

// RemovePendingTx removes a pending transaction (when mined)
func (s *Service) RemovePendingTx(hash string) {
	s.mu.Lock()
	defer s.mu.Unlock()

	delete(s.pool, hash)
}

// MarkAsMined marks a transaction as mined
func (s *Service) MarkAsMined(ctx context.Context, hash string, blockNumber int64, gasUsed int64) error {
	s.mu.Lock()
	defer s.mu.Unlock()

	if tx, ok := s.pool[hash]; ok {
		tx.GasUsed = gasUsed
	}

	delete(s.pool, hash)

	// Update in database if tracked
	if s.db != nil {
		query := `
			INSERT INTO pending_transactions (hash, from_address, to_address, value, gas_price, 
			                         gas_limit, nonce, first_seen, mined_at, gas_used)
			VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)
			ON CONFLICT (hash) DO UPDATE SET mined_at = EXCLUDED.mined_at,
			                         gas_used = EXCLUDED.gas_used
		`

		_, err := s.db.ExecContext(ctx, query)
		return err
	}

	return nil
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
var _ = big.NewInt       // Use big.Int
var _ = fmt.Sprintf    // Use fmt