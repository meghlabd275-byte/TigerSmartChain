// Package mempool provides transaction pool management with RBF and reaping.
package mempool

import (
	"fmt"
	"math/big"
	"sync"
	"time"

	"github.com/tigersmartchain/tigersmartchain/internal/blockchain/transaction"
)

// =============================================================================
// ADVANCED TRANSACTION POOL WITH RBF AND REAPING
// =============================================================================

// TxPoolConfig represents transaction pool configuration.
type TxPoolConfig struct {
	// Global slots
	GlobalSlots uint64
	// Local slots
	LocalSlots uint64
	// Max account slots
	MaxAccountSlots uint64
	// Minimum gas price
	MinGasPrice *big.Int
	// Maximum gas price
	MaxGasPrice *big.Int
	// Reaping interval
	ReapingInterval time.Duration
	// PriceBump for RBF
	PriceBump *big.Int
}

// DefaultTxPoolConfig returns default configuration.
func DefaultTxPoolConfig() *TxPoolConfig {
	return &TxPoolConfig{
		GlobalSlots:     5000,
		LocalSlots:    100,
		MaxAccountSlots: 16,
		MinGasPrice:   big.NewInt(1000000000),  // 1 Gwei
		MaxGasPrice:   big.NewInt(1000000000000), // 1000 Gwei
		ReapingInterval: 1 * time.Hour,
		PriceBump:    big.NewInt(110), // 10% bump
	}
}

// TxPool represents advanced transaction pool.
type TxPool struct {
	mu sync.RWMutex

	config *TxPoolConfig

	// Pending transactions (ready to be mined)
	pending map[string]*PoolTransaction
	// Queued transactions (not ready)
	queued map[string]*PoolTransaction
	// By account and nonce
	byAccount map[string]map[uint64]*PoolTransaction

	// Pricing
	gasPrice *big.Int
	headNum  uint64

	// Statistics
	stats TxPoolStats
}

// PoolTransaction represents a transaction in the pool.
type PoolTransaction struct {
	Tx       *transaction.Transaction
	Hash     string
	From     string
	Nonce    uint64
	GasPrice *big.Int
	GasLimit uint64
	EntryRank uint64 // For price ordering
	Inserted time.Time
	IsLocal  bool
}

// TxPoolStats represents pool statistics.
type TxPoolStats struct {
	Pending uint64
	Queued  uint64
	Local   uint64
}

// NewTxPool creates a new transaction pool.
func NewTxPool(config *TxPoolConfig) *TxPool {
	if config == nil {
		config = DefaultTxPoolConfig()
	}

	return &TxPool{
		config:     config,
		pending:   make(map[string]*PoolTransaction),
		queued:    make(map[string]*PoolTransaction),
		byAccount: make(map[string]map[uint64]*PoolTransaction),
		gasPrice:  config.MinGasPrice,
	}
}

// AddTransaction adds a transaction to the pool.
func (tp *TxPool) AddTransaction(tx *transaction.Transaction, isLocal bool) error {
	if tx == nil {
		return fmt.Errorf("nil transaction")
	}

	tp.mu.Lock()
	defer tp.mu.Unlock()

	// Validate gas price
	if tx.GasPrice.Cmp(tp.config.MinGasPrice) < 0 {
		return fmt.Errorf("gas price below minimum")
	}

	if tx.GasPrice.Cmp(tp.config.MaxGasPrice) > 0 {
		return fmt.Errorf("gas price above maximum")
	}

	// Check if sender has enough balance
	if !tp.hasBalance(tx.From, tx.Value) {
		return fmt.Errorf("insufficient balance")
	}

	// Create pool transaction
	ptx := &PoolTransaction{
		Tx:        tx,
		Hash:      tx.Hash,
		From:     tx.From,
		Nonce:    tx.Nonce,
		GasPrice: tx.GasPrice,
		GasLimit: tx.GasLimit,
		EntryRank: 0,
		Inserted: time.Now(),
		IsLocal:  isLocal,
	}

	// Determine if pending or queued
	if tp.isPending(tx.From, tx.Nonce) {
		// Check for RBF
		if err := tp.checkReplaceByFee(ptx); err != nil {
			return err
		}
		tp.pending[tx.Hash] = ptx
	} else {
		tp.queued[tx.Hash] = ptx
	}

	// Update account map
	if _, ok := tp.byAccount[tx.From]; !ok {
		tp.byAccount[tx.From] = make(map[uint64]*PoolTransaction)
	}
	tp.byAccount[tx.From][tx.Nonce] = ptx

	// Update stats
	tp.updateStats()

	return nil
}

// isPending checks if nonce is pending.
func (tp *TxPool) isPending(from string, nonce uint64) bool {
	if accTxs, ok := tp.byAccount[from]; ok {
		if ptx, ok := accTxs[nonce]; ok {
			return ptx != nil
		}
	}
	return false
}

// hasBalance checks if account has enough balance.
func (tp *TxPool) hasBalance(from string, value *big.Int) bool {
	// Simplified - would check state
	return true
}

// checkReplaceByFee checks if transaction can replace existing one (RBF).
func (tp *TxPool) checkReplaceByFee(newTx *PoolTransaction) error {
	oldTx, ok := tp.pending[newTx.Hash]
	if !ok {
		return nil
	}

	// Check price bump
	oldPrice := oldTx.GasPrice
	newPrice := newTx.GasPrice

	minPrice := new(big.Int).Mul(oldPrice, tp.config.PriceBump)
	minPrice.Div(minPrice, big.NewInt(100))

	if newPrice.Cmp(minPrice) < 0 {
		return fmt.Errorf("replacement price too low")
	}

	// Replace
	tp.pending[newTx.Hash] = newTx
	return nil
}

// GetTransaction gets a transaction by hash.
func (tp *TxPool) GetTransaction(hash string) (*PoolTransaction, bool) {
	tp.mu.RLock()
	defer tp.mu.RUnlock()

	if ptx, ok := tp.pending[hash]; ok {
		return ptx, true
	}

	if ptx, ok := tp.queued[hash]; ok {
		return ptx, true
	}

	return nil, false
}

// GetTransactions returns pending transactions.
func (tp *TxPool) GetTransactions() []*PoolTransaction {
	tp.mu.RLock()
	defer tp.mu.RUnlock()

	result := make([]*PoolTransaction, 0, len(tp.pending))
	for _, ptx := range tp.pending {
		result = append(result, ptx)
	}

	return result
}

// RemoveTransaction removes a transaction from the pool.
func (tp *TxPool) RemoveTransaction(hash string) {
	tp.mu.Lock()
	defer tp.mu.Unlock()

	// Try pending
	if ptx, ok := tp.pending[hash]; ok {
		delete(tp.pending, hash)
		if accTxs, ok := tp.byAccount[ptx.From]; ok {
			delete(accTxs, ptx.Nonce)
		}
	}

	// Try queued
	if ptx, ok := tp.queued[hash]; ok {
		delete(tp.queued, hash)
		if accTxs, ok := tp.byAccount[ptx.From]; ok {
			delete(accTxs, ptx.Nonce)
		}
	}

	tp.updateStats()
}

// RemoveByNonce removes all transactions from an account with nonce >= specified.
func (tp *TxPool) RemoveByNonce(from string, nonce uint64) {
	tp.mu.Lock()
	defer tp.mu.Unlock()

	if accTxs, ok := tp.byAccount[from]; ok {
		for n, ptx := range accTxs {
			if n >= nonce {
				delete(tp.pending, ptx.Hash)
				delete(tp.queued, ptx.Hash)
				delete(accTxs, n)
			}
		}
	}

	tp.updateStats()
}

// Reap removes old transactions (gas price too low).
func (tp *TxPool) Reap() {
	tp.mu.Lock()
	defer tp.mu.Unlock()

	cutoff := time.Now().Add(-tp.config.ReapingInterval)

	// Remove old queued transactions
	for hash, ptx := range tp.queued {
		if ptx.Inserted.Before(cutoff) && !ptx.IsLocal {
			delete(tp.queued, hash)
			if accTxs, ok := tp.byAccount[ptx.From]; ok {
				delete(accTxs, ptx.Nonce)
			}
		}
	}

	tp.updateStats()
}

// GetPendingCount returns count of pending transactions.
func (tp *TxPool) GetPendingCount() uint64 {
	tp.mu.RLock()
	defer tp.mu.RUnlock()
	return uint64(len(tp.pending))
}

// GetQueuedCount returns count of queued transactions.
func (tp *TxPool) GetQueuedCount() uint64 {
	tp.mu.RLock()
	defer tp.mu.RUnlock()
	return uint64(len(tp.queued))
}

// GetGasPrice returns current recommended gas price.
func (tp *TxPool) GetGasPrice() *big.Int {
	tp.mu.RLock()
	defer tp.mu.RUnlock()
	return new(big.Int).Set(tp.gasPrice)
}

// SetHead sets the current head block number.
func (tp *TxPool) SetHead(blockNum uint64) {
	tp.mu.Lock()
	defer tp.mu.Unlock()
	tp.headNum = blockNum

	// Promote queued to pending
	tp.promote()
}

// promote promotes queued transactions to pending.
func (tp *TxPool) promote() {
	// Simplified - would check nonce continuity
}

// updateStats updates pool statistics.
func (tp *TxPool) updateStats() {
	tp.stats.Pending = uint64(len(tp.pending))
	tp.stats.Queued = uint64(len(tp.queued))

	var local uint64
	for _, ptx := range tp.pending {
		if ptx.IsLocal {
			local++
		}
	}
	for _, ptx := range tp.queued {
		if ptx.IsLocal {
			local++
		}
	}
	tp.stats.Local = local
}

// GetStats returns pool statistics.
func (tp *TxPool) GetStats() TxPoolStats {
	tp.mu.RLock()
	defer tp.mu.RUnlock()
	return tp.stats
}

var _ = fmt.Sprintf // Use fmt
var _ = big.NewInt // Use big.Int