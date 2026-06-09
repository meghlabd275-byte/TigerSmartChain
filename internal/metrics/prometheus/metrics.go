// Package metrics provides Prometheus metrics for TigerSmartChain.
package metrics

import (
	"sync"
)

// Global metrics instance
var (
	instance *Metrics
	once    sync.Once
)

// Metrics holds the global metrics.
type Metrics struct {
	// Block metrics
	BlockCounter    uint64
	BlockGauge     uint64
	BlockGasUsed   uint64

	// Transaction metrics
	TxCounter      uint64
	TxGasUsed     uint64

	// Validator metrics
	ValidatorCount uint64

	// Network metrics
	PeerCount     uint64
	PendingTx    uint64

	// Memory metrics
	MemAlloc      uint64
	MemTotalAlloc uint64
}

// Init initializes the metrics.
func Init() {
	once.Do(func() {
		instance = &Metrics{}
	})
}

// Get returns the global metrics.
func Get() *Metrics {
	if instance == nil {
		Init()
	}
	return instance
}

// IncBlockCounter increments the block counter.
func IncBlockCounter() {
	if instance != nil {
		instance.BlockCounter++
	}
}

// SetBlockGauge sets the current block number.
func SetBlockGauge(block uint64) {
	if instance != nil {
		instance.BlockGauge = block
	}
}

// IncTxCounter increments the transaction counter.
func IncTxCounter() {
	if instance != nil {
		instance.TxCounter++
	}
}

// SetValidatorCount sets the validator count.
func SetValidatorCount(count uint64) {
	if instance != nil {
		instance.ValidatorCount = count
	}
}

// SetPeerCount sets the peer count.
func SetPeerCount(count uint64) {
	if instance != nil {
		instance.PeerCount = count
	}
}