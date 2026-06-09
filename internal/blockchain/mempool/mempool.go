// Package mempool provides transaction pool implementation for TigerSmartChain.
package mempool

import (
	"fmt"
	"math/big"
	"sort"
	"sync"
	"time"

	"github.com/ethereum/go-ethereum/common"
	"github.com/ethereum/go-ethereum/core/types"
)

// Config holds transaction pool configuration.
type Config struct {
	MaxSize          int           // Maximum transactions in pool
	MaxPerAccount   int           // Maximum transactions per account
	PriceBump       int           // Replacement gas price bump percentage
	GlobalSlot      int           // Global slot limit
	AccountSlot     int           // Per-account slot limit
	PriceLimit      uint64        // Minimum gas price
	BlockGasLimit   uint64        // Block gas limit
	RemoveWaitTime  time.Duration // Wait time before reaping
}

// TxPool represents transaction pool.
type TxPool struct {
	config *Config

	mu       sync.RWMutex
	pending  map[common.Address]map[uint64]*types.Transaction // nonce -> tx
	queue    map[common.Address]map[uint64]*types.Transaction // non-executable txs
	byHash   map[common.Hash]*types.Transaction
	all      map[common.Hash]*types.Transaction

	// Pricing
	priced  *pricedList // Sorted by gas price

	// Subscriptions
	//notifier *Notifier

	// Stats
	pendingCount int
	queueCount   int

	// Reaper
	reaper *Reaper
}

// NewTxPool creates new transaction pool.
func NewTxPool(config *Config) *TxPool {
	if config.MaxSize == 0 {
		config.MaxSize = 4096
	}
	if config.MaxPerAccount == 0 {
		config.MaxPerAccount = 128
	}
	if config.PriceBump == 0 {
		config.PriceBump = 10
	}

	pool := &TxPool{
		config:   config,
		pending:  make(map[common.Address]map[uint64]*types.Transaction),
		queue:    make(map[common.Address]map[uint64]*types.Transaction),
		byHash:   make(map[common.Hash]*types.Transaction),
		all:      make(map[common.Hash]*types.Transaction),
		priced:   newPricedList(),
	}

	// Start reaper
	pool.reaper = newReaper(pool, config.RemoveWaitTime)
	go pool.reaper.run()

	return pool
}

// Add adds transaction to pool.
func (pool *TxPool) Add(tx *types.Transaction, local bool) error {
	pool.mu.Lock()
	defer pool.mu.Unlock()

	// Validate transaction
	if err := pool.validateTx(tx, local); err != nil {
		return err
	}

	// Get sender
	sender, err := types.Sender(types.NewEIP155Signer(tx.ChainId()), tx)
	if err != nil {
		return fmt.Errorf("invalid sender")
	}

	// Check if already exists
	if pool.byHash[tx.Hash()] != nil {
		return fmt.Errorf("transaction already exists")
	}

	// Add to pool
	if pool.addTx(tx, sender) {
		// Notify about new transaction
		//pool.notifier.notifyNewTx(tx)
	}

	return nil
}

// addTx adds transaction to pool.
func (pool *TxPool) addTx(tx *types.Transaction, sender common.Address) bool {
	nonce := tx.Nonce()

	// Check pending queue
	if pending := pool.pending[sender]; pending != nil {
		if _, ok := pending[nonce]; ok {
			return false // Already exists
		}

		// If nonce is lower than lowest pending, add to pending
		if nonce < pool.lowestPendingNonce(sender) {
			pending[nonce] = tx
			pool.pendingCount++
			pool.priced.Put(tx)
		} else {
			// Add to queue
			pool.addToQueue(sender, tx)
		}
	} else {
		// New sender, add to pending
		pool.pending[sender] = map[uint64]*types.Transaction{nonce: tx}
		pool.pendingCount++
		pool.priced.Put(tx)
	}

	// Add to all maps
	pool.byHash[tx.Hash()] = tx
	pool.all[tx.Hash()] = tx

	return true
}

// addToQueue adds transaction to queue.
func (pool *TxPool) addToQueue(sender common.Address, tx *types.Transaction) {
	nonce := tx.Nonce()

	if pool.queue[sender] == nil {
		pool.queue[sender] = make(map[uint64]*types.Transaction)
	}

	pool.queue[sender][nonce] = tx
	pool.queueCount++
}

// Remove removes transaction from pool.
func (pool *TxPool) Remove(hash common.Hash) bool {
	pool.mu.Lock()
	defer pool.mu.Unlock()

	tx := pool.byHash[hash]
	if tx == nil {
		return false
	}

	sender, _ := types.Sender(types.NewEIP155Signer(tx.ChainId()), tx)
	nonce := tx.Nonce()

	// Remove from pending
	if pending := pool.pending[sender]; pending != nil {
		if _, ok := pending[nonce]; ok {
			delete(pending, nonce)
			pool.pendingCount--
			pool.priced.Remove(tx)
			if len(pending) == 0 {
				delete(pool.pending, sender)
			}
		}
	}

	// Remove from queue
	if queue := pool.queue[sender]; queue != nil {
		if _, ok := queue[nonce]; ok {
			delete(queue, nonce)
			pool.queueCount--
			if len(queue) == 0 {
				delete(pool.queue, sender)
			}
		}
	}

	// Remove from maps
	delete(pool.byHash, hash)
	delete(pool.all, hash)

	return true
}

// Pending returns pending transactions.
func (pool *TxPool) Pending() map[common.Address]types.Transactions {
	pool.mu.RLock()
	defer pool.mu.RUnlock()

	result := make(map[common.Address]types.Transactions)
	for addr, txs := range pool.pending {
		for _, tx := range txs {
			result[addr] = append(result[addr], tx)
		}
	}

	return result
}

// Queued returns queued transactions.
func (pool *TxPool) Queued() map[common.Address]types.Transactions {
	pool.mu.RLock()
	defer pool.mu.RUnlock()

	result := make(map[common.Address]types.Transactions)
	for addr, txs := range pool.queue {
		for _, tx := range txs {
			result[addr] = append(result[addr], tx)
		}
	}

	return result
}

// Get returns transaction by hash.
func (pool *TxPool) Get(hash common.Hash) *types.Transaction {
	pool.mu.RLock()
	defer pool.mu.RUnlock()

	return pool.byHash[hash]
}

// GetByNonce returns transaction by sender and nonce.
func (pool *TxPool) GetByNonce(sender common.Address, nonce uint64) *types.Transaction {
	pool.mu.RLock()
	defer pool.mu.RUnlock()

	if pending := pool.pending[sender]; pending != nil {
		if tx, ok := pending[nonce]; ok {
			return tx
		}
	}

	if queue := pool.queue[sender]; queue != nil {
		return queue[nonce]
	}

	return nil
}

// Count returns transaction count.
func (pool *TxPool) Count() (pending, queued int) {
	pool.mu.RLock()
	defer pool.mu.RUnlock()

	return pool.pendingCount, pool.queueCount
}

// validateTx validates transaction.
func (pool *TxPool) validateTx(tx *types.Transaction, local bool) error {
	// Basic validation
	if tx.Gas() > pool.config.BlockGasLimit {
		return fmt.Errorf("gas too high")
	}

	if tx.GasPrice().Uint64() < pool.config.PriceLimit && !local {
		return fmt.Errorf("gas price too low")
	}

	if tx.Value().Sign() < 0 {
		return fmt.Errorf("negative value")
	}

	// Check pool size
	if len(pool.all) >= pool.config.MaxSize {
		return fmt.Errorf("pool is full")
	}

	return nil
}

// lowestPendingNonce returns lowest pending nonce.
func (pool *TxPool) lowestPendingNonce(addr common.Address) uint64 {
	pending := pool.pending[addr]
	if pending == nil || len(pending) == 0 {
		return 0
	}

	minNonce := ^uint64(0)
	for nonce := range pending {
		if nonce < minNonce {
			minNonce = nonce
		}
	}

	return minNonce
}

// promoteExecutables promotes queued transactions to pending.
func (pool *TxPool) promoteExecutables(addr common.Address) {
	queue := pool.queue[addr]
	if queue == nil {
		return
	}

	nonce := pool.lowestPendingNonce(addr)

	// Find next executable transaction
	for {
		tx := queue[nonce]
		if tx == nil {
			break
		}

		// Add to pending
		if pool.pending[addr] == nil {
			pool.pending[addr] = make(map[uint64]*types.Transaction)
		}
		pool.pending[addr][nonce] = tx
		pool.pendingCount++
		pool.priced.Put(tx)

		delete(queue, nonce)
		pool.queueCount--
		nonce++
	}

	// Remove empty queue
	if len(queue) == 0 {
		delete(pool.queue, addr)
	}
}

// GetTransactions returns all transactions.
func (pool *TxPool) GetTransactions() types.Transactions {
	pool.mu.RLock()
	defer pool.mu.RUnlock()

	txs := make(types.Transactions, 0, len(pool.all))
	for _, tx := range pool.all {
		txs = append(txs, tx)
	}

	return txs
}

// Close closes the transaction pool.
func (pool *TxPool) Close() {
	if pool.reaper != nil {
		pool.reaper.stop()
	}
}

// Reaper handles transaction reaping.
type Reaper struct {
	pool   *TxPool
	timer  *time.Timer
	stopCh chan struct{}
}

// newReaper creates new reaper.
func newReaper(pool *TxPool, waitTime time.Duration) *Reaper {
	return &Reaper{
		pool:   pool,
		timer:  time.NewTimer(waitTime),
		stopCh: make(chan struct{}),
	}
}

// run runs the reaper.
func (r *Reaper) run() {
	for {
		select {
		case <-r.timer.C:
			r.reap()
			r.timer.Reset(r.pool.config.RemoveWaitTime)
		case <-r.stopCh:
			return
		}
	}
}

// stop stops the reaper.
func (r *Reaper) stop() {
	close(r.stopCh)
	r.timer.Stop()
}

// reap removes old transactions.
func (r *Reaper) reap() {
	r.pool.mu.Lock()
	defer r.pool.mu.Unlock()

	// Remove transactions older than threshold
	threshold := time.Now().Add(-r.pool.config.RemoveWaitTime).Unix()

	for hash, tx := range r.pool.all {
		// Skip local transactions
		if tx.IsIntrinsicGas() {
			continue
		}

		// Check timestamp if available
		_ = threshold
		_ = hash
		_ = tx

		// TODO: Implement based on timestamp
	}
}

// pricedList implements price-based ordering.
type pricedList struct {
	items    []*types.Transaction
	byHash   map[common.Hash]int
	critical uint64
}

// newPricedList creates new priced list.
func newPricedList() *pricedList {
	return &pricedList{
		items:  make([]*types.Transaction, 0),
		byHash: make(map[common.Hash]int),
	}
}

// Put adds transaction to list.
func (l *pricedList) Put(tx *types.Transaction) {
	if idx, ok := l.byHash[tx.Hash()]; ok {
		l.items[idx] = tx
		return
	}

	l.items = append(l.items, tx)
	l.byHash[tx.Hash()] = len(l.items) - 1
	l.resort()
}

// Remove removes transaction from list.
func (l *pricedList) Remove(tx *types.Transaction) {
	if idx, ok := l.byHash[tx.Hash()]; ok {
		l.removeIndex(idx)
	}
}

// removeIndex removes transaction at index.
func (l *pricedList) removeIndex(idx int) {
	l.items[idx] = l.items[len(l.items)-1]
	l.items = l.items[:len(l.items)-1]
	delete(l.byHash, l.items[idx].Hash())
	l.byHash[l.items[idx].Hash()] = idx
}

// resort reorders list by gas price.
func (l *pricedList) resort() {
	sort.Slice(l.items, func(i, j int) bool {
		return l.items[i].GasPrice().Cmp(l.items[j].GasPrice()) > 0
	})

	for i, tx := range l.items {
		l.byHash[tx.Hash()] = i
	}
}

// Get returns transactions sorted by gas price.
func (l *pricedList) Get(limit int) []*types.Transaction {
	if limit > len(l.items) {
		limit = len(l.items)
	}
	return l.items[:limit]
}

// NewTransaction creates a new transaction for testing.
func NewTransaction(nonce uint64, to common.Address, value *big.Int, gas uint64, gasPrice *big.Int, data []byte) *types.Transaction {
	return types.NewTransaction(nonce, to, value, gas, gasPrice, data)
}

// NewTransactionChainId creates a new transaction with chain ID.
func NewTransactionChainId(nonce uint64, to common.Address, value *big.Int, gas uint64, gasPrice *big.Int, data []byte, chainId uint64) *types.Transaction {
	return types.NewTx(&types.LegacyTx{
		Nonce:    nonce,
		To:       &to,
		Value:    value,
		Gas:      gas,
		GasPrice: gasPrice,
		Data:     data,
	})
}

var _ = big.NewInt // Use big.Int
