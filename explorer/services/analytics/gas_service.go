// Package gas provides gas tracking and analytics for TigerScan.
package gas

import (
	"context"
	"fmt"
	"math/big"
	"sync"
	"time"

	"github.com/tigersmartchain/tigersmartchain/explorer/databases/postgres"
)

// =============================================================================
// GAS TRACKER SERVICE
// =============================================================================

// Service provides gas tracking and analytics
type Service struct {
	db           *postgres.DB
	mu           sync.RWMutex
	gasHistory   []GasData
	priceFeeds  map[string]GasPriceFeed
}

// GasData represents gas price data
type GasData struct {
	BlockNumber   uint64    `json:"blockNumber"`
	SlowPrice    uint64    `json:"slowPrice"`    // Gwei
	AveragePrice uint64    `json:"averagePrice"` // Gwei
	FastPrice   uint64    `json:"fastPrice"`    // Gwei
	BaseFee     uint64    `json:"baseFee"`
	Timestamp   time.Time `json:"timestamp"`
}

// GasPriceFeed represents a gas price feed
type GasPriceFeed interface {
	GetGasPrice() (uint64, uint64, uint64, error)
}

// EthGasStationFeed uses EthGasStation API
type EthGasStationFeed struct {
	APIURL string
}

// NewEthGasStationFeed creates a new EthGasStation feed
func NewEthGasStationFeed() *EthGasStationFeed {
	return &EthGasStationFeed{
		APIURL: "https://api.ethgasstation.info/v2/standard",
	}
}

// GetGasPrice returns gas prices from EthGasStation
func (f *EthGasStationFeed) GetGasPrice() (slow, avg, fast uint64, err error) {
	// Would fetch from API and convert
	// For now, return default values
	return 20, 30, 50, nil
}

// NewService creates a new gas service
func NewService(db *postgres.DB) *Service {
	return &Service{
		db:          db,
		gasHistory: make([]GasData, 0, 10000),
		priceFeeds: make(map[string]GasPriceFeed),
	}
}

// =============================================================================
// GAS PRICE OPERATIONS
// =============================================================================

// GetCurrentGasPrice returns current recommended gas prices
func (s *Service) GetCurrentGasPrice(ctx context.Context) (*GasData, error) {
	s.mu.RLock()
	defer s.mu.RUnlock()

	if len(s.gasHistory) > 0 {
		return &s.gasHistory[len(s.gasHistory)-1], nil
	}

	// Return default if no history
	return &GasData{
		SlowPrice:    20,
		AveragePrice: 30,
		FastPrice:   50,
		Timestamp:   time.Now(),
	}, nil
}

// GetGasHistory returns gas price history
func (s *Service) GetGasHistory(ctx context.Context, since time.Duration) ([]GasData, error) {
	s.mu.RLock()
	defer s.mu.RUnlock()

	cutoff := time.Now().Add(-since)
	result := make([]GasData, 0)

	for i := len(s.gasHistory) - 1; i >= 0; i-- {
		if s.gasHistory[i].Timestamp.After(cutoff) {
			result = append(result, s.gasHistory[i])
		}
	}

	return result, nil
}

// GetGasEstimates returns gas estimates for different speeds
func (s *Service) GetGasEstimates(ctx context.Context, txType string, gasLimit uint64) (*GasEstimates, error) {
	current, err := s.GetCurrentGasPrice(ctx)
	if err != nil {
		return nil, err
	}

	estimates := &GasEstimates{
		Slow:     current.SlowPrice * gasLimit,
		Standard: current.AveragePrice * gasLimit,
		Fast:     current.FastPrice * gasLimit,
	}

	return estimates, nil
}

// GasEstimates represents gas cost estimates
type GasEstimates struct {
	Slow     uint64 `json:"slow"`
	Standard uint64 `json:"standard"`
	Fast     uint64 `json:"fast"`
}

// =============================================================================
// GAS PREDICTION
// =============================================================================

// PredictGasPrice predicts future gas prices using simple moving average
func (s *Service) PredictGasPrice(ctx context.Context, blocksAhead int) (uint64, error) {
	s.mu.RLock()
	defer s.mu.RUnlock()

	if len(s.gasHistory) < 10 {
		// Not enough data, return current average
		if len(s.gasHistory) > 0 {
			return s.gasHistory[len(s.gasHistory)-1].AveragePrice, nil
		}
		return 30, nil
	}

	// Calculate moving average of last 10 blocks
	var sum uint64
	count := 10
	if count > len(s.gasHistory) {
		count = len(s.gasHistory)
	}

	for i := 0; i < count; i++ {
		idx := len(s.gasHistory) - 1 - i
		sum += s.gasHistory[idx].AveragePrice
	}

	avg := sum / uint64(count)

	// Simple prediction: adjust based on trend
	// If recent blocks are increasing, predict higher
	if count >= 2 {
		last := s.gasHistory[len(s.gasHistory)-1].AveragePrice
		prev := s.gasHistory[len(s.gasHistory)-2].AveragePrice
		if last > prev {
			avg = avg * 105 / 100 // 5% increase
		} else if last < prev {
			avg = avg * 95 / 100 // 5% decrease
		}
	}

	return avg, nil
}

// =============================================================================
// GAS ORACLE
// =============================================================================

// GasOracle represents an on-chain gas oracle
type GasOracle struct {
	mu            sync.RWMutex
	lastUpdate    time.Time
	suggestedGas  GasData
}

// NewGasOracle creates a new gas oracle
func NewGasOracle() *GasOracle {
	return &GasOracle{
		lastUpdate: time.Now(),
		suggestedGas: GasData{
			SlowPrice:    20,
			AveragePrice: 30,
			FastPrice:   50,
		},
	}
}

// GetSuggestedGas returns suggested gas prices
func (o *GasOracle) GetSuggestedGas() GasData {
	o.mu.RLock()
	defer o.mu.RUnlock()
	return o.suggestedGas
}

// UpdateGas updates gas prices
func (o *GasOracle) UpdateGas(gas GasData) {
	o.mu.Lock()
	defer o.mu.Unlock()
	o.lastUpdate = time.Now()
	o.suggestedGas = gas
}

// =============================================================================
// GAS CALCULATOR
// =============================================================================

// Calculator provides gas cost calculations
type Calculator struct{}

// NewCalculator creates a new calculator
func NewCalculator() *Calculator {
	return &Calculator{}
}

// CalculateGasCost calculates total gas cost
func (c *Calculator) CalculateGasCost(gasPrice uint64, gasLimit uint64) *big.Int {
	return new(big.Int).Mul(
		new(big.Int).SetUint64(gasPrice),
		new(big.Int).SetUint64(gasLimit),
	)
}

// GweiToWei converts Gwei to Wei
func (c *Calculator) GweiToWei(gwei uint64) *big.Int {
	return new(big.Int).Mul(
		new(big.Int).SetUint64(gwei),
		big.NewInt(1e9),
	)
}

// WeiToGwei converts Wei to Gwei
func (c *Calculator) WeiToGwei(wei *big.Int) uint64 {
	return wei.Div(wei, big.NewInt(1e9)).Uint64()
}

// EstimateTransferCost estimates cost for a standard transfer
func (c *Calculator) EstimateTransferCost(gasPrice uint64) uint64 {
	// Standard transfer uses 21000 gas
	return gasPrice * 21000
}

// EstimateContractDeployCost estimates cost for contract deployment
func (c *Calculator) EstimateContractDeployCost(gasPrice uint64, codeLength int) uint64 {
	// Contract deployment: 21000 + code length * 200 (approximate)
	return gasPrice * (21000 + uint64(codeLength)*200)
}

// EstimateSwapCost estimates cost for DEX swap
func (c *Calculator) EstimateSwapCost(gasPrice uint64) uint64 {
	// Uniswap swap: ~150000-250000 gas
	return gasPrice * 200000
}

// =============================================================================
// GAS ANALYTICS
// =============================================================================

// GetGasAnalytics returns detailed gas analytics
func (s *Service) GetGasAnalytics(ctx context.Context, period time.Duration) (*GasAnalytics, error) {
	history, err := s.GetGasHistory(ctx, period)
	if err != nil {
		return nil, err
	}

	if len(history) == 0 {
		return &GasAnalytics{
			Period:         period.String(),
			AveragePrice:   30,
			MinPrice:       20,
			MaxPrice:       50,
			MedianPrice:    30,
			StdDeviation:   5,
		}, nil
	}

	var sum, min, max uint64
	var prices []uint64

	for _, g := range history {
		sum += g.AveragePrice
		prices = append(prices, g.AveragePrice)
		if min == 0 || g.AveragePrice < min {
			min = g.AveragePrice
		}
		if g.AveragePrice > max {
			max = g.AveragePrice
		}
	}

	avg := sum / uint64(len(history))

	// Calculate median
	var median uint64
	mid := len(prices) / 2
	if len(prices)%2 == 0 {
		median = (prices[mid-1] + prices[mid]) / 2
	} else {
		median = prices[mid]
	}

	// Calculate standard deviation
	var varianceSum uint64
	for _, p := range prices {
		diff := p - avg
		varianceSum += diff * diff
	}
	stdDev := 0
	if len(prices) > 1 {
		stdDev = int(varianceSum / uint64(len(prices)-1))
	}

	return &GasAnalytics{
		Period:        period.String(),
		AveragePrice: avg,
		MinPrice:     min,
		MaxPrice:     max,
		MedianPrice:  median,
		StdDeviation: uint64(stdDev),
		DataPoints:   len(history),
	}, nil
}

// GasAnalytics represents gas analytics
type GasAnalytics struct {
	Period        string `json:"period"`
	AveragePrice uint64  `json:"averagePrice"`
	MinPrice     uint64  `json:"minPrice"`
	MaxPrice     uint64  `json:"maxPrice"`
	MedianPrice  uint64  `json:"medianPrice"`
	StdDeviation uint64  `json:"stdDeviation"`
	DataPoints   int     `json:"dataPoints"`
}

// =============================================================================
// GAS ALERTS
// =============================================================================

// AlertThreshold represents gas price alert
type AlertThreshold struct {
	ID          string
	Condition   string // "above" or "below"
	Price       uint64
	NotifyURL  string
	Active     bool
	LastTrigger time.Time
}

// =============================================================================
// STORAGE
// =============================================================================

// StoreGasPrice stores gas price in database
func (s *Service) StoreGasPrice(ctx context.Context, gas *GasData) error {
	dbGas := &postgres.GasPrice{
		BlockNumber:    int64(gas.BlockNumber),
		SlowGasPrice:   int64(gas.SlowPrice),
		AvgGasPrice:    int64(gas.AveragePrice),
		FastGasPrice:   int64(gas.FastPrice),
		BaseFeePerGas:  int64(gas.BaseFee),
		Timestamp:      gas.Timestamp.Unix(),
	}

	if err := s.db.InsertGasPrice(ctx, dbGas); err != nil {
		return err
	}

	// Update in-memory history
	s.mu.Lock()
	s.gasHistory = append(s.gasHistory, *gas)
	if len(s.gasHistory) > 10000 {
		s.gasHistory = s.gasHistory[len(s.gasHistory)-10000:]
	}
	s.mu.Unlock()

	return nil
}

// =============================================================================
// WEBSOCKET BROADCAST
// =============================================================================

// BroadcastGasUpdate broadcasts gas update to subscribers
func (s *Service) BroadcastGasUpdate(gas *GasData) {
	// Would broadcast via WebSocket
	fmt.Printf("Gas update: slow=%d, avg=%d, fast=%d\n", gas.SlowPrice, gas.AveragePrice, gas.FastPrice)
}
