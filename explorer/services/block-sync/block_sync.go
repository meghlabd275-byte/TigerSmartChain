// Package blocksync provides block synchronization services for TigerScan Explorer.
package blocksync

import (
	"context"
	"encoding/json"
	"fmt"
	"sync"
	"time"

	"github.com/tigersmartchain/tigersmartchain/internal/blockchain/block"
	"github.com/tigersmartchain/tigersmartchain/internal/blockchain/transaction"
	"github.com/tigersmartchain/tigersmartchain/internal/storage"
)

// =============================================================================
// BLOCK SYNC SERVICE
// =============================================================================

// Service provides block synchronization for the explorer.
type Service struct {
	mu sync.RWMutex

	// Configuration
	config *Config

	// State
	ctx        context.Context
	cancel     context.CancelFunc
	started    bool
	lastBlock  uint64
	headBlock uint64

	// Dependencies
	blockStore  storage.Store
	txStore    storage.Store
	receiptStore storage.Store

	// Event handlers
	onBlock    func(*block.Block) error
	onNewBlock func(*block.Header) error

	// Metrics
	stats *Stats
}

// Config holds service configuration.
type Config struct {
	// RPC endpoint for fetching blocks
	RPCURL string

	// Batch size for fetching blocks
	BatchSize uint64

	// Polling interval
	PollInterval time.Duration

	// Start block (0 for genesis)
	StartBlock uint64

	// Max workers
	MaxWorkers int

	// Retry configuration
	MaxRetries int
	RetryDelay time.Duration
}

// Stats holds service statistics.
type Stats struct {
	BlocksProcessed  uint64
	TransactionsIndexed uint64
	LastProcessedAt time.Time
	Errors          uint64
}

// NewService creates a new block sync service.
func NewService(config *Config) *Service {
	ctx, cancel := context.WithCancel(context.Background())

	return &Service{
		ctx:      ctx,
		cancel:   cancel,
		config:   config,
		stats:    &Stats{},
		lastBlock: config.StartBlock,
	}
}

// SetBlockStore sets the block storage.
func (s *Service) SetBlockStore(store storage.Store) {
	s.mu.Lock()
	defer s.mu.Unlock()
	s.blockStore = store
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

// OnBlock registers a callback for new blocks.
func (s *Service) OnBlock(handler func(*block.Block) error) {
	s.mu.Lock()
	defer s.mu.Unlock()
	s.onBlock = handler
}

// OnNewBlock registers a callback for new block headers.
func (s *Service) OnNewBlock(handler func(*block.Header) error) {
	s.mu.Lock()
	defer s.mu.Unlock()
	s.onNewBlock = handler
}

// =============================================================================
// LIFECYCLE
// =============================================================================

// Start starts the block sync service.
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

// Stop stops the block sync service.
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
			s.processNextBatch()
		}
	}
}

// processNextBatch processes the next batch of blocks.
func (s *Service) processNextBatch() {
	s.mu.Lock()
	lastBlock := s.lastBlock
	s.mu.Unlock()

	// Fetch blocks
	from := lastBlock + 1
	to := from + s.config.BatchSize

	blocks, err := s.fetchBlocks(from, to)
	if err != nil {
		s.mu.Lock()
		s.stats.Errors++
		s.mu.Unlock()
		return
	}

	// Process each block
	for _, blk := range blocks {
		if err := s.processBlock(blk); err != nil {
			continue
		}

		s.mu.Lock()
		s.lastBlock = blk.Header.Number
		s.stats.BlocksProcessed++
		s.stats.LastProcessedAt = time.Now()
		s.mu.Unlock()
	}
}

// fetchBlocks fetches blocks from the RPC endpoint.
func (s *Service) fetchBlocks(from, to uint64) ([]*block.Block, error) {
	// In production, use actual RPC calls
	// For now, return empty
	return []*block.Block{}, nil
}

// processBlock processes a single block.
func (s *Service) processBlock(blk *block.Block) error {
	// Store block
	if s.blockStore != nil {
		key := fmt.Sprintf("block:%d", blk.Header.Number)
		data, _ := json.Marshal(blk)
		if err := s.blockStore.Put([]byte(key), data); err != nil {
			return err
		}
	}

	// Process transactions
	for i, tx := range blk.Body.Transactions {
		if err := s.processTransaction(tx, blk.Header.Number, uint64(i)); err != nil {
			continue
		}

		s.mu.Lock()
		s.stats.TransactionsIndexed++
		s.mu.Unlock()
	}

	// Fire callbacks
	if s.onBlock != nil {
		s.onBlock(blk)
	}

	if s.onNewBlock != nil {
		s.onNewBlock(blk.Header)
	}

	return nil
}

// processTransaction processes a transaction.
func (s *Service) processTransaction(tx *transaction.Transaction, blockNumber, txIndex uint64) error {
	if s.txStore == nil {
		return nil
	}

	// Store transaction
	key := fmt.Sprintf("tx:%s", tx.Hash)
	data, _ := json.Marshal(tx)
	return s.txStore.Put([]byte(key), data)
}

// =============================================================================
// STATUS
// =============================================================================

// GetStatus returns the current service status.
func (s *Service) GetStatus() *Status {
	s.mu.RLock()
	defer s.mu.RUnlock()

	return &Status{
		Started:         s.started,
		LastBlock:       s.lastBlock,
		HeadBlock:      s.headBlock,
		BlocksProcessed: s.stats.BlocksProcessed,
		Errors:         s.stats.Errors,
	}
}

// Status holds the service status.
type Status struct {
	Started         bool
	LastBlock      uint64
	HeadBlock     uint64
	BlocksProcessed uint64
	Errors         uint64
}

// GetLastBlock returns the last processed block.
func (s *Service) GetLastBlock() uint64 {
	s.mu.RLock()
	defer s.mu.RUnlock()
	return s.lastBlock
}

// SetHeadBlock sets the known head block.
func (s *Service) SetHeadBlock(blockNum uint64) {
	s.mu.Lock()
	defer s.mu.Unlock()
	s.headBlock = blockNum
}

// =============================================================================
// DATABASE STORAGE
// =============================================================================

// StoreBlock stores a block in the database.
func (s *Service) StoreBlock(blk *block.Block) error {
	if s.blockStore == nil {
		return fmt.Errorf("no block store configured")
	}

	key := fmt.Sprintf("block:%s", blk.Header.Hash)
	data, err := json.Marshal(blk)
	if err != nil {
		return err
	}

	return s.blockStore.Put([]byte(key), data)
}

// GetBlock retrieves a block from the database.
func (s *Service) GetBlock(blockHash string) (*block.Block, error) {
	if s.blockStore == nil {
		return nil, fmt.Errorf("no block store configured")
	}

	key := fmt.Sprintf("block:%s", blockHash)
	data, err := s.blockStore.Get([]byte(key))
	if err != nil {
		return nil, err
	}

	blk := &block.Block{}
	if err := json.Unmarshal(data, blk); err != nil {
		return nil, err
	}

	return blk, nil
}

// GetBlockByNumber retrieves a block by number.
func (s *Service) GetBlockByNumber(blockNum uint64) (*block.Block, error) {
	if s.blockStore == nil {
		return nil, fmt.Errorf("no block store configured")
	}

	key := fmt.Sprintf("block:%d", blockNum)
	data, err := s.blockStore.Get([]byte(key))
	if err != nil {
		return nil, err
	}

	blk := &block.Block{}
	if err := json.Unmarshal(data, blk); err != nil {
		return nil, err
	}

	return blk, nil
}

// StoreTransaction stores a transaction in the database.
func (s *Service) StoreTransaction(tx *transaction.Transaction, blockNumber, txIndex uint64) error {
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

// GetTransaction retrieves a transaction from the database.
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

// =============================================================================
// BATCH OPERATIONS
// =============================================================================

// StoreBlocks stores multiple blocks.
func (s *Service) StoreBlocks(blocks []*block.Block) error {
	for _, blk := range blocks {
		if err := s.StoreBlock(blk); err != nil {
			return err
		}
	}
	return nil
}

// GetBlocksByRange returns blocks in a range.
func (s *Service) GetBlocksByRange(from, to uint64) ([]*block.Block, error) {
	result := make([]*block.Block, 0, to-from+1)

	for i := from; i <= to; i++ {
		blk, err := s.GetBlockByNumber(i)
		if err != nil {
			continue
		}
		result = append(result, blk)
	}

	return result, nil
}

// =============================================================================
// UTILITY METHODS
// =============================================================================

// WaitForBlock waits for a specific block to be synced.
func (s *Service) WaitForBlock(blockNum uint64, timeout time.Duration) error {
	deadline := time.Now().Add(timeout)

	for time.Now().Before(deadline) {
		s.mu.RLock()
		currentBlock := s.lastBlock
		s.mu.RUnlock()

		if currentBlock >= blockNum {
			return nil
		}

		time.Sleep(100 * time.Millisecond)
	}

	return fmt.Errorf("timeout waiting for block %d", blockNum)
}

// GetRecentBlocks returns the N most recent blocks.
func (s *Service) GetRecentBlocks(count int) ([]*block.Block, error) {
	s.mu.RLock()
	currentBlock := s.lastBlock
	s.mu.RUnlock()

	from := uint64(0)
	if currentBlock > uint64(count) {
		from = currentBlock - uint64(count)
	}

	return s.GetBlocksByRange(from, currentBlock)
}