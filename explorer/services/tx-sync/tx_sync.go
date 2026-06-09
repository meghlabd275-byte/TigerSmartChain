// Package txsync provides transaction synchronization for TigerScan Explorer.
package txsync

import (
	"context"
	"encoding/json"
	"fmt"
	"sync"
	"time"

	"github.com/tigersmartchain/tigersmartchain/internal/blockchain/transaction"
	"github.com/tigersmartchain/tigersmartchain/internal/storage"
)

// =============================================================================
// TRANSACTION SYNC SERVICE
// =============================================================================

// Service provides transaction synchronization for the explorer.
type Service struct {
	mu sync.RWMutex

	// Configuration
	config *Config

	// State
	ctx     context.Context
	cancel  context.CancelFunc
	started bool
	pending map[string]*PendingTx

	// Dependencies
	txStore    storage.Store
	receiptStore storage.Store
	eventStore storage.Store

	// Event handlers
	onTransaction    func(*transaction.Transaction) error
	onTransactionReceipt func(*TransactionReceipt) error

	// Metrics
	stats *Stats
}

// Config holds service configuration.
type Config struct {
	// RPC endpoint
	RPCURL string

	// Polling interval
	PollInterval time.Duration

	// Max pending transactions
	MaxPending int

	// Max workers
	MaxWorkers int
}

// PendingTx represents a pending transaction.
type PendingTx struct {
	Tx       *transaction.Transaction
	AddedAt  time.Time
	Retries  int
}

// Stats holds service statistics.
type Stats struct {
	TransactionsProcessed  uint64
	ReceiptsIndexed       uint64
	LastProcessedAt       time.Time
	Errors                uint64
}

// TransactionReceipt represents a transaction receipt.
type TransactionReceipt struct {
	TransactionHash   string   `json:"transactionHash"`
	BlockHash        string   `json:"blockHash"`
	BlockNumber      uint64   `json:"blockNumber"`
	TransactionIndex uint64   `json:"transactionIndex"`
	From             string   `json:"from"`
	To               string   `json:"to"`
	ContractAddress  string   `json:"contractAddress"`
	CumulativeGasUsed uint64  `json:"cumulativeGasUsed"`
	GasUsed         uint64   `json:"gasUsed"`
	Logs             []Log   `json:"logs"`
	LogsBloom        string   `json:"logsBloom"`
	Status           uint64   `json:"status"`
}

// Log represents a transaction log.
type Log struct {
	Address     string   `json:"address"`
	Topics      []string `json:"topics"`
	Data        string   `json:"data"`
	BlockNumber uint64   `json:"blockNumber"`
	TxHash     string   `json:"transactionHash"`
	TxIndex    uint64   `json:"transactionIndex"`
	LogIndex   uint64   `json:"logIndex"`
}

// NewService creates a new transaction sync service.
func NewService(config *Config) *Service {
	ctx, cancel := context.WithCancel(context.Background())

	return &Service{
		ctx:     ctx,
		cancel:  cancel,
		config:  config,
		stats:   &Stats{},
		pending: make(map[string]*PendingTx),
	}
}

// SetTransactionStore sets the transaction storage.
func (s *Service) SetTransactionStore(store storage.Store) {
	s.mu.Lock()
	defer s.mu.Unlock()
	s.txStore = store
}

// SetReceiptStore sets the receipt storage.
func (s *Service) SetReceiptStore(store storage.Store) {
	s.mu.Lock()
	defer s.mu.Unlock()
	s.receiptStore = store
}

// SetEventStore sets the event log storage.
func (s *Service) SetEventStore(store storage.Store) {
	s.mu.Lock()
	defer s.mu.Unlock()
	s.eventStore = store
}

// OnTransaction registers a callback for new transactions.
func (s *Service) OnTransaction(handler func(*transaction.Transaction) error) {
	s.mu.Lock()
	defer s.mu.Unlock()
	s.onTransaction = handler
}

// OnTransactionReceipt registers a callback for transaction receipts.
func (s *Service) OnTransactionReceipt(handler func(*TransactionReceipt) error) {
	s.mu.Lock()
	defer s.mu.Unlock()
	s.onTransactionReceipt = handler
}

// =============================================================================
// LIFECYCLE
// =============================================================================

// Start starts the transaction sync service.
func (s *Service) Start() error {
	s.mu.Lock()
	defer s.mu.Unlock()

	if s.started {
		return fmt.Errorf("already started")
	}

	s.started = true
	s.ctx, s.cancel = context.WithCancel(context.Background())

	// Start sync loop
	go s.syncLoop()

	return nil
}

// Stop stops the transaction sync service.
func (s *Service) Stop() error {
	s.mu.Lock()
	defer s.mu.Unlock()

	if !s.started {
		return fmt.Errorf("not started")
	}

	s.cancel()
	s.started = false

	return nil
}

// syncLoop runs the main synchronization loop.
func (s *Service) syncLoop() {
	ticker := time.NewTicker(s.config.PollInterval)
	defer ticker.Stop()

	for {
		select {
		case <-s.ctx.Done():
			return
		case <-ticker.C:
			s.processPending()
		}
	}
}

// processPending processes pending transactions.
func (s *Service) processPending() {
	s.mu.Lock()
	pending := make([]*PendingTx, 0)
	for _, ptx := range s.pending {
		pending = append(pending, ptx)
	}
	s.mu.Unlock()

	// Process each pending transaction
	for _, ptx := range pending {
		receipt, err := s.fetchReceipt(ptx.Tx.Hash)
		if err != nil {
			// Transaction not confirmed yet
			ptx.Retries++
			continue
		}

		// Store receipt
		if err := s.storeReceipt(receipt); err != nil {
			s.mu.Lock()
			s.stats.Errors++
			s.mu.Unlock()
			continue
		}

		// Remove from pending
		s.mu.Lock()
		delete(s.pending, ptx.Tx.Hash)
		s.mu.Unlock()

		// Update stats
		s.mu.Lock()
		s.stats.TransactionsProcessed++
		s.stats.ReceiptsIndexed++
		s.stats.LastProcessedAt = time.Now()
		s.mu.Unlock()

		// Fire callback
		if s.onTransactionReceipt != nil {
			s.onTransactionReceipt(receipt)
		}
	}
}

// fetchReceipt fetches a transaction receipt.
func (s *Service) fetchReceipt(txHash string) (*TransactionReceipt, error) {
	// In production, use actual RPC call
	// For now, return error
	return nil, fmt.Errorf("not found")
}

// storeReceipt stores a transaction receipt.
func (s *Service) storeReceipt(receipt *TransactionReceipt) error {
	if s.receiptStore == nil {
		return fmt.Errorf("no receipt store configured")
	}

	key := fmt.Sprintf("receipt:%s", receipt.TransactionHash)
	data, err := json.Marshal(receipt)
	if err != nil {
		return err
	}

	return s.receiptStore.Put([]byte(key), data)
}

// =============================================================================
// TRANSACTION MANAGEMENT
// =============================================================================

// AddPending adds a transaction to pending list.
func (s *Service) AddPending(tx *transaction.Transaction) error {
	s.mu.Lock()
	defer s.mu.Unlock()

	if len(s.pending) >= s.config.MaxPending {
		return fmt.Errorf("too many pending transactions")
	}

	s.pending[tx.Hash] = &PendingTx{
		Tx:      tx,
		AddedAt: time.Now(),
	}

	return nil
}

// GetPending returns pending transactions.
func (s *Service) GetPending() []*PendingTx {
	s.mu.RLock()
	defer s.mu.RUnlock()

	result := make([]*PendingTx, 0, len(s.pending))
	for _, ptx := range s.pending {
		result = append(result, ptx)
	}

	return result
}

// IsPending checks if a transaction is pending.
func (s *Service) IsPending(txHash string) bool {
	s.mu.RLock()
	defer s.mu.RUnlock()

	_, ok := s.pending[txHash]
	return ok
}

// =============================================================================
// STORAGE OPERATIONS
// =============================================================================

// StoreTransaction stores a transaction.
func (s *Service) StoreTransaction(tx *transaction.Transaction) error {
	if s.txStore == nil {
		return fmt.Errorf("no transaction store configured")
	}

	key := fmt.Sprintf("tx:%s", tx.Hash)
	data, err := json.Marshal(tx)
	if err != nil {
		return err
	}

	return s.txStore.Put([]byte(key), data)
}

// GetTransaction retrieves a transaction.
func (s *Service) GetTransaction(txHash string) (*transaction.Transaction, error) {
	if s.txStore == nil {
		return nil, fmt.Errorf("no transaction store configured")
	}

	key := fmt.Sprintf("tx:%s", txHash)
	data, err := s.txStore.Get([]byte(key))
	if err != nil {
		return nil, err
	}

	tx := &transaction.Transaction{}
	if err := json.Unmarshal(data, tx); err != nil {
		return nil, err
	}

	return tx, nil
}

// GetReceipt retrieves a transaction receipt.
func (s *Service) GetReceipt(txHash string) (*TransactionReceipt, error) {
	if s.receiptStore == nil {
		return nil, fmt.Errorf("no receipt store configured")
	}

	key := fmt.Sprintf("receipt:%s", txHash)
	data, err := s.receiptStore.Get([]byte(key))
	if err != nil {
		return nil, err
	}

	receipt := &TransactionReceipt{}
	if err := json.Unmarshal(data, receipt); err != nil {
		return nil, err
	}

	return receipt, nil
}

// GetTransactionsByAddress returns transactions for an address.
func (s *Service) GetTransactionsByAddress(address string) ([]*transaction.Transaction, error) {
	if s.txStore == nil {
		return nil, fmt.Errorf("no transaction store configured")
	}

	// In production, use database query
	// For now, return empty
	return []*transaction.Transaction{}, nil
}

// GetTransactionsByBlock returns transactions for a block.
func (s *Service) GetTransactionsByBlock(blockNumber uint64) ([]*transaction.Transaction, error) {
	if s.txStore == nil {
		return nil, fmt.Errorf("no transaction store configured")
	}

	// In production, use database query
	// For now, return empty
	return []*transaction.Transaction{}, nil
}

// =============================================================================
// LOGS
// =============================================================================

// StoreLogs stores transaction logs.
func (s *Service) StoreLogs(receipt *TransactionReceipt) error {
	if s.eventStore == nil {
		return fmt.Errorf("no event store configured")
	}

	for _, log := range receipt.Logs {
		key := fmt.Sprintf("log:%s:%d", log.TxHash, log.LogIndex)
		data, err := json.Marshal(log)
		if err != nil {
			return err
		}

		if err := s.eventStore.Put([]byte(key), data); err != nil {
			return err
		}
	}

	return nil
}

// GetLogs returns logs matching a filter.
func (s *Service) GetLogs(filter *LogFilter) ([]*Log, error) {
	if s.eventStore == nil {
		return nil, fmt.Errorf("no event store configured")
	}

	// In production, use database query
	// For now, return empty
	return []*Log{}, nil
}

// LogFilter represents a log filter.
type LogFilter struct {
	Address   string
	FromBlock uint64
	ToBlock   uint64
	Topics    []string
}

// =============================================================================
// STATUS
// =============================================================================

// GetStatus returns the service status.
func (s *Service) GetStatus() *Status {
	s.mu.RLock()
	defer s.mu.RUnlock()

	return &Status{
		Started:              s.started,
		PendingCount:         len(s.pending),
		TransactionsProcessed: s.stats.TransactionsProcessed,
		ReceiptsIndexed:      s.stats.ReceiptsIndexed,
		Errors:               s.stats.Errors,
	}
}

// Status holds the service status.
type Status struct {
	Started               bool
	PendingCount          int
	TransactionsProcessed uint64
	ReceiptsIndexed      uint64
	Errors                uint64
}

// =============================================================================
// UTILITY METHODS
// =============================================================================

// WaitForReceipt waits for a transaction receipt.
func (s *Service) WaitForReceipt(txHash string, timeout time.Duration) (*TransactionReceipt, error) {
	deadline := time.Now().Add(timeout)

	for time.Now().Before(deadline) {
		receipt, err := s.GetReceipt(txHash)
		if err == nil && receipt != nil {
			return receipt, nil
		}

		time.Sleep(100 * time.Millisecond)
	}

	return nil, fmt.Errorf("timeout waiting for receipt")
}

// GetRecentTransactions returns recent transactions.
func (s *Service) GetRecentTransactions(count int) ([]*transaction.Transaction, error) {
	if s.txStore == nil {
		return nil, fmt.Errorf("no transaction store configured")
	}

	// In production, use database query with LIMIT
	// For now, return empty
	return []*transaction.Transaction{}, nil
}