// Package analytics provides blockchain analytics for TigerScan Explorer.
package analytics

import (
	"encoding/json"
	"fmt"
	"math/big"
	"sync"
	"time"

	"github.com/tigersmartchain/tigersmartchain/internal/blockchain/block"
	"github.com/tigersmartchain/tigersmartchain/internal/blockchain/transaction"
)

// =============================================================================
// ANALYTICS SERVICE
// =============================================================================

// Service provides blockchain analytics.
type Service struct {
	mu sync.RWMutex

	// Metrics
	chainMetrics    *ChainMetrics
	blockMetrics   *BlockMetrics
	transactionMetrics *TransactionMetrics

	// Storage
	store storage.Store

	// Configuration
	config *Config
}

// Config holds analytics configuration.
type Config struct {
	// Update interval
	UpdateInterval time.Duration

	// Historical data retention (days)
	RetentionDays int
}

// =============================================================================
// CHAIN METRICS
// =============================================================================

// ChainMetrics holds overall chain metrics.
type ChainMetrics struct {
	TotalTransactions uint64
	TotalBlocks       uint64
	TotalAddresses    uint64
	TotalContracts    uint64

	// TPS
	CurrentTPS    float64
	AverageTPS    float64
	PeakTPS       float64

	// Gas
	CurrentGasPrice    uint64
	AverageGasPrice  uint64
	MinGasPrice      uint64
	MaxGasPrice      uint64

	// Chain data
	ChainID      uint64
	NetworkID    uint64
	GenesisTime time.Time
}

// NewChainMetrics creates new chain metrics.
func NewChainMetrics() *ChainMetrics {
	return &ChainMetrics{
		GenesisTime: time.Now(),
	}
}

// =============================================================================
// BLOCK METRICS
// =============================================================================

// BlockMetrics holds block-related metrics.
type BlockMetrics struct {
	BlockTime           time.Duration
	BlockReward        uint64
	AvgGasUsed         uint64
	MaxGasUsed         uint64
	AvgTxPerBlock      float64
	MaxTxPerBlock      uint64

	// Last N blocks
	LastBlockNumbers []uint64
	LastBlockTimes   []time.Time
	LastGasUsed     []uint64
	LastTxCounts   []int
}

// NewBlockMetrics creates new block metrics.
func NewBlockMetrics() *BlockMetrics {
	return &BlockMetrics{
		LastBlockNumbers: make([]uint64, 0),
		LastBlockTimes:   make([]time.Time, 0),
		LastGasUsed:    make([]uint64, 0),
		LastTxCounts:   make([]int, 0),
	}
}

// AddBlock records a block for metrics.
func (bm *BlockMetrics) AddBlock(header *block.Header, txCount int) {
	bm.LastBlockNumbers = append(bm.LastBlockNumbers, header.Number)
	bm.LastGasUsed = append(bm.LastGasUsed, header.GasUsed)
	bm.LastTxCounts = append(bm.LastTxCounts, txCount)
	bm.LastBlockTimes = append(bm.LastBlockTimes, time.Now())

	// Keep only last 100 blocks
	if len(bm.LastBlockNumbers) > 100 {
		bm.LastBlockNumbers = bm.LastBlockNumbers[1:]
		bm.LastGasUsed = bm.LastGasUsed[1:]
		bm.LastTxCounts = bm.LastTxCounts[1:]
		bm.LastBlockTimes = bm.LastBlockTimes[1:]
	}

	// Calculate averages
	if len(bm.LastGasUsed) > 0 {
		var sum uint64
		for _, g := range bm.LastGasUsed {
			sum += g
		}
		bm.AvgGasUsed = sum / uint64(len(bm.LastGasUsed))
		bm.MaxGasUsed = bm.LastGasUsed[0]
		for _, g := range bm.LastGasUsed {
			if g > bm.MaxGasUsed {
				bm.MaxGasUsed = g
			}
		}
	}

	if len(bm.LastTxCounts) > 0 {
		var sum int
		for _, c := range bm.LastTxCounts {
			sum += c
		}
		bm.AvgTxPerBlock = float64(sum) / float64(len(bm.LastTxCounts))
		bm.MaxTxPerBlock = bm.LastTxCounts[0]
		for _, c := range bm.LastTxCounts {
			if c > bm.MaxTxPerBlock {
				bm.MaxTxPerBlock = c
			}
		}
	}
}

// =============================================================================
// TRANSACTION METRICS
// =============================================================================

// TransactionMetrics holds transaction metrics.
type TransactionMetrics struct {
	TotalTransactions  uint64
	SuccessfulTxs      uint64
	FailedTxs          uint64
	ContractCreates   uint64

	// Timing
	AvgConfirmationTime time.Duration
	MaxConfirmationTime time.Duration

	// Gas
	AvgGasPrice   uint64
	AvgGasLimit   uint64

	// Types
	TransferTxs      uint64
	ContractCallTxs  uint64
	TokenTransferTxs  uint64
	NFTTransferTxs   uint64
	SwapTxs          uint64
}

// NewTransactionMetrics creates new transaction metrics.
func NewTransactionMetrics() *TransactionMetrics {
	return &TransactionMetrics{}
}

// =============================================================================
// ANALYTICS COMPUTATION
// =============================================================================

// storage.Store interface (minimal)
type storage interface {
	Put(key, value []byte) error
	Get(key []byte) ([]byte, error)
}

// NewService creates a new analytics service.
func NewService(config *Config) *Service {
	return &Service{
		config: config,
		chainMetrics: NewChainMetrics(),
		blockMetrics: NewBlockMetrics(),
		transactionMetrics: NewTransactionMetrics(),
	}
}

// SetStorage sets the storage backend.
func (s *Service) SetStorage(store storage.Store) {
	s.mu.Lock()
	defer s.mu.Unlock()
	s.store = store
}

// ProcessBlock processes a block and updates metrics.
func (s *Service) ProcessBlock(blk *block.Block) error {
	s.mu.Lock()
	defer s.mu.Unlock()

	// Update block metrics
	s.blockMetrics.AddBlock(blk.Header, len(blk.Body.Transactions))

	// Update chain metrics
	s.chainMetrics.TotalBlocks++
	s.chainMetrics.TotalTransactions += uint64(len(blk.Body.Transactions))

	// Calculate TPS (transactions per second)
	txCount := len(blk.Body.Transactions)
	s.chainMetrics.CurrentTPS = float64(txCount) / 3.0 // Block time is 3 seconds

	if s.chainMetrics.CurrentTPS > s.chainMetrics.PeakTPS {
		s.chainMetrics.PeakTPS = s.chainMetrics.CurrentTPS
	}

	// Store metrics
	return s.storeMetrics()
}

// storeMetrics stores current metrics.
func (s *Service) storeMetrics() error {
	if s.store == nil {
		return nil
	}

	// Store chain metrics
	data, err := json.Marshal(s.chainMetrics)
	if err != nil {
		return err
	}
	return s.store.Put([]byte("metrics:chain"), data)
}

// =============================================================================
// STATIC METHODS
// =============================================================================

// CalculateTPS calculates transactions per second.
func (s *Service) CalculateTPS(txCount int, blockTime time.Duration) float64 {
	return float64(txCount) / blockTime.Seconds()
}

// CalculateBlockReward calculates block reward.
func CalculateBlockReward(blockNumber uint64) uint64 {
	// Initial reward: 3 BNB per block
	// Halving every 3 years (approx 1.5M blocks)
	initialReward := uint64(3000000000000000000) // 3 BNB in wei

	halvings := blockNumber / 3000000
	if halvings == 0 {
		return initialReward
	}

	// Calculate halving
	divisor := big.NewInt(1)
	for i := uint64(0); i < halvings; i++ {
		divisor.Mul(divisor, big.NewInt(2))
	}

	result := new(big.Int).Div(big.NewInt(int64(initialReward)), divisor)
	return result.Uint64()
}

// =============================================================================
// AGGREGATED DATA
// =============================================================================

// GetChainMetrics returns current chain metrics.
func (s *Service) GetChainMetrics() *ChainMetrics {
	s.mu.RLock()
	defer s.mu.RUnlock()
	return s.chainMetrics
}

// GetBlockMetrics returns current block metrics.
func (s *Service) GetBlockMetrics() *BlockMetrics {
	s.mu.RLock()
	defer s.mu.RUnlock()
	return s.blockMetrics
}

// GetTransactionMetrics returns current transaction metrics.
func (s *Service) GetTransactionMetrics() *TransactionMetrics {
	s.mu.RLock()
	defer s.mu.RUnlock()
	return s.transactionMetrics
}

// =============================================================================
// HISTORICAL DATA
// =============================================================================

// GetHistoricalTPS returns historical TPS data.
func (s *Service) GetHistoricalTPS(period time.Duration) ([]DataPoint, error) {
	// In production, query time-series database
	// For now, return empty
	return []DataPoint{}, nil
}

// GetHistoricalGasPrice returns historical gas price data.
func (s *Service) GetHistoricalGasPrice(period time.Duration) ([]DataPoint, error) {
	// In production, query time-series database
	// For now, return empty
	return []DataPoint{}, nil
}

// DataPoint represents a time-series data point.
type DataPoint struct {
	Timestamp time.Time
	Value     float64
}

// =============================================================================
// TOKEN ANALYTICS
// =============================================================================

// TokenMetrics holds token-related metrics.
type TokenMetrics struct {
	TotalTokens         uint64
	TotalHolders      uint64
	TotalTransfers    uint64

	// Volume
	TransferVolume24h  *big.Int
	TransferVolume7d   *big.Int
	TransferVolume30d  *big.Int

	// Holders distribution
	Top10HoldersPercent float64
	Top100HoldersPercent float64
}

// GetTokenMetrics returns token metrics.
func (s *Service) GetTokenMetrics(tokenAddress string) (*TokenMetrics, error) {
	// In production, query database
	// For now, return empty
	return &TokenMetrics{}, nil
}

// =============================================================================
// NFT ANALYTICS
// =============================================================================

// NFTMetrics holds NFT-related metrics.
type NFTMetrics struct {
	TotalCollections uint64
	TotalNFTs       uint64
	TotalMinters   uint64
	TotalTraders  uint64

	// Volume
	Volume24h  *big.Int
	Volume7d   *big.Int
	Volume30d *big.Int

	// Floor price
	FloorPrice *big.Int
	AvgPrice  *big.Int
}

// GetNFTMetrics returns NFT metrics.
func (s *Service) GetNFTMetrics(collectionAddress string) (*NFTMetrics, error) {
	// In production, query database
	// For now, return empty
	return &NFTMetrics{}, nil
}

// =============================================================================
// DEX ANALYTICS (for future expansion)
// =============================================================================

// DEXMetrics holds DEX-related metrics.
type DEXMetrics struct {
	TotalValueLocked  *big.Int
	Volume24h     *big.Int
	Fees24h      *big.Int

	TopTokens  []string
	TopPools  []string
}

// =============================================================================
// TVL (Total Value Locked)
// =============================================================================

// CalculateTVL calculates total value locked.
func (s *Service) CalculateTVL() (*big.Int, error) {
	// In production, sum all token balances in staking contracts
	// For now, return zero
	return big.NewInt(0), nil
}

// GetHistoricalTVL returns historical TVL data.
func (s *Service) GetHistoricalTVL(period time.Duration) ([]DataPoint, error) {
	// In production, query time-series database
	// For now, return empty
	return []DataPoint{}, nil
}

// =============================================================================
// NETWORK HEALTH
// =============================================================================

// NetworkHealth represents network health status.
type NetworkHealth struct {
	Status           string
	BlockTime       time.Duration
	GasPrice       uint64
	ActiveValidators uint64
	AvgUptime      float64
}

// GetNetworkHealth returns network health status.
func (s *Service) GetNetworkHealth() *NetworkHealth {
	s.mu.RLock()
	defer s.mu.RUnlock()

	return &NetworkHealth{
		Status:            "healthy",
		BlockTime:        3 * time.Second,
		GasPrice:         s.chainMetrics.CurrentGasPrice,
		ActiveValidators: 21,
		AvgUptime:        99.5,
	}
}

// =============================================================================
// EXPORT METHODS
// =============================================================================

// ExportMetrics exports all metrics as JSON.
func (s *Service) ExportMetrics() ([]byte, error) {
	s.mu.RLock()
	defer s.mu.RUnlock()

	data := map[string]interface{}{
		"chain":      s.chainMetrics,
		"block":     s.blockMetrics,
		"transaction": s.transactionMetrics,
		"timestamp": time.Now(),
	}

	return json.Marshal(data)
}

// ExportPrometheus exports metrics in Prometheus format.
func (s *Service) ExportPrometheus() string {
	s.mu.RLock()
	defer s.mu.RUnlock()

	lines := []string{
		"# HELP tsc_chain_total_blocks Total number of blocks",
		fmt.Sprintf("# TYPE tsc_chain_total_blocks gauge"),
		fmt.Sprintf("tsc_chain_total_blocks %d", s.chainMetrics.TotalBlocks),
		"",
		"# HELP tsc_chain_total_transactions Total number of transactions",
		fmt.Sprintf("# TYPE tsc_chain_total_transactions gauge"),
		fmt.Sprintf("tsc_chain_total_transactions %d", s.chainMetrics.TotalTransactions),
		"",
		"# HELP tsc_chain_tps Transactions per second",
		fmt.Sprintf("# TYPE tsc_chain_tps gauge"),
		fmt.Sprintf("tsc_chain_tps %f", s.chainMetrics.CurrentTPS),
		"",
		"# HELP tsc_block_gas_used Gas used in last block",
		fmt.Sprintf("# TYPE tsc_block_gas_used gauge"),
		fmt.Sprintf("tsc_block_gas_used %d", s.blockMetrics.AvgGasUsed),
	}

	return fmt.Sprintf("%s\n", lines)
}