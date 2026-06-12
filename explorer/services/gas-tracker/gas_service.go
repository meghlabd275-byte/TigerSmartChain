// Package gastracker provides gas price tracking and predictions.
package gastracker

import (
	"context"
	"database/sql"
	"encoding/json"
	"fmt"
	"math"
	"math/big"
	"sort"
	"time"
)

// GasPrice represents a gas price entry
type GasPrice struct {
	ID                 int64     `json:"id"`
	LowGasPrice       int64     `json:"lowGasPrice"`
	MediumGasPrice   int64     `json:"mediumGasPrice"`
	HighGasPrice     int64     `json:"highGasPrice"`
	BaseFeePerGas    int64     `json:"baseFeePerGas"`
	PriorityFeeAvg   int64     `json:"priorityFeeAvg"`
	NetworkUtilization float64 `json:"networkUtilization"`
	PendingTxCount  int64     `json:"pendingTxCount"`
	Timestamp       time.Time `json:"timestamp"`
}

// GasPricePrediction represents predicted gas prices
type GasPricePrediction struct {
	Timestamp       time.Time `json:"timestamp"`
	PredictedLow   int64     `json:"predictedLow"`
	PredictedMedium int64     `json:"predictedMedium"`
	PredictedHigh  int64     `json:"predictedHigh"`
	Confidence     float64   `json:"confidence"`
}

// Service provides gas tracking functionality
type Service struct {
	db              *sql.DB
	rpcURL          string
	updateInterval time.Duration
	historyWindow  time.Duration
}

// Config holds service configuration
type Config struct {
	DB            *sql.DB
	RPCURL        string
	UpdateInterval time.Duration
	HistoryWindow  time.Duration
}

// NewService creates a new gas tracker service
func NewService(cfg *Config) *Service {
	interval := cfg.UpdateInterval
	if interval == 0 {
		interval = 15 * time.Second
	}
	window := cfg.HistoryWindow
	if window == 0 {
		window = 1 * time.Hour
	}

	return &Service{
		db:              cfg.DB,
		rpcURL:          cfg.RPCURL,
		updateInterval:  interval,
		historyWindow:  window,
	}
}

// UpdateGasPrices fetches and stores current gas prices
func (s *Service) UpdateGasPrices(ctx context.Context) error {
	prices, err := s.fetchGasPrices(ctx)
	if err != nil {
		return err
	}

	return s.storeGasPrices(ctx, prices)
}

// fetchGasPrices fetches current gas prices from RPC
func (s *Service) fetchGasPrices(ctx context.Context) (*GasPrice, error) {
	if s.rpcURL == "" {
		return nil, fmt.Errorf("RPC URL not configured")
	}

	// Get latest block to calculate base fee
	params := map[string]interface{}{
		"blockNumber": "latest",
		"fullTxns":   false,
	}

	result, err := s.callRPC(ctx, "eth_getBlockByNumber", params)
	if err != nil {
		return nil, err
	}

	var block struct {
		BaseFeePerGas string `json:"baseFeePerGas"`
		GasLimit     string `json:"gasLimit"`
		GasUsed     string `json:"gasUsed"`
	}

	if err := json.Unmarshal(result, &block); err != nil {
		return nil, err
	}

	baseFee := int64(0)
	if block.BaseFeePerGas != "" {
		fmt.Sscanf(block.BaseFeePerGas, "0x%x", &baseFee)
	}

	gasUsed := int64(0)
	gasLimit := int64(0)
	if block.GasUsed != "" {
		fmt.Sscanf(block.GasUsed, "0x%x", &gasUsed)
	}
	if block.GasLimit != "" {
		fmt.Sscanf(block.GasLimit, "0x%x", &gasLimit)
	}

	// Calculate network utilization
	utilization := float64(0)
	if gasLimit > 0 {
		utilization = float64(gasUsed) / float64(gasLimit) * 100
	}

	// Get pending tx count
	pendingCount, _ := s.getPendingTxCount(ctx)

	// Get priority fees from recent blocks
	priorityFees := s.calculatePriorityFees(ctx)

	// Calculate gas prices based on network conditions
	gasPrices := s.calculateGasPrices(baseFee, priorityFees, utilization)

	return &GasPrice{
		BaseFeePerGas:     baseFee,
		LowGasPrice:      gasPrices[0],
		MediumGasPrice:   gasPrices[1],
		HighGasPrice:     gasPrices[2],
		PriorityFeeAvg:   priorityFees[1],
		NetworkUtilization: utilization,
		PendingTxCount:   pendingCount,
		Timestamp:       time.Now(),
	}, nil
}

// calculateGasPrices calculates low/medium/high gas prices
func (s *Service) calculateGasPrices(baseFee int64, priorityFees []int64, utilization float64) []int64 {
	result := make([]int64, 3)

	// Multiplier based on network utilization
	multiplier := 1.0
	switch {
	case utilization > 90:
		multiplier = 2.0
	case utilization > 70:
		multiplier = 1.5
	case utilization > 50:
		multiplier = 1.2
	default:
		multiplier = 1.0
	}

	// Low: base fee + small priority fee
	result[0] = baseFee + int64(float64(priorityFees[0])*multiplier)
	
	// Medium: base fee + average priority fee
	result[1] = baseFee + int64(float64(priorityFees[1])*multiplier)
	
	// High: base fee + high priority fee
	result[2] = baseFee + int64(float64(priorityFees[2])*multiplier)

	return result
}

// getPendingTxCount returns the count of pending transactions
func (s *Service) getPendingTxCount(ctx context.Context) (int64, error) {
	params := map[string]interface{}{
		"includeTransactions": true,
	}

	result, err := s.callRPC(ctx, "eth_newBlockFilter", params)
	if err != nil {
		return 0, err
	}

	// This would need proper implementation with filter polling
	return 0, nil
}

// calculatePriorityFees calculates priority fees from recent blocks
func (s *Service) calculatePriorityFees(ctx context.Context) []int64 {
	// Get recent transactions and calculate priority fees
	// Simplified: return default values
	return []int64{1000000000, 2000000000, 5000000000}
}

// storeGasPrices stores gas prices in database
func (s *Service) storeGasPrices(ctx context.Context, prices *GasPrice) error {
	query := `
		INSERT INTO gas_price_history 
		(low_gas_price, medium_gas_price, high_gas_price, base_fee_per_gas, 
		 priority_fee_avg, network_utilization, pending_tx_count, timestamp)
		VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
	`

	_, err := s.db.ExecContext(ctx, query,
		prices.LowGasPrice, prices.MediumGasPrice, prices.HighGasPrice,
		prices.BaseFeePerGas, prices.PriorityFeeAvg, prices.NetworkUtilization,
		prices.PendingTxCount, prices.Timestamp,
	)

	return err
}

// GetGasPrices returns current gas prices
func (s *Service) GetGasPrices(ctx context.Context) (*GasPrice, error) {
	query := `
		SELECT id, low_gas_price, medium_gas_price, high_gas_price, 
		       base_fee_per_gas, priority_fee_avg, network_utilization, 
		       pending_tx_count, timestamp
		FROM gas_price_history
		ORDER BY timestamp DESC
		LIMIT 1
	`

	p := &GasPrice{}
	err := s.db.QueryRowContext(ctx, query).Scan(
		&p.ID, &p.LowGasPrice, &p.MediumGasPrice, &p.HighGasPrice,
		&p.BaseFeePerGas, &p.PriorityFeeAvg, &p.NetworkUtilization,
		&p.PendingTxCount, &p.Timestamp,
	)
	if err != nil {
		return nil, err
	}

	return p, nil
}

// GetGasPriceHistory returns historical gas prices
func (s *Service) GetGasPriceHistory(ctx context.Context, since time.Time) ([]*GasPrice, error) {
	query := `
		SELECT id, low_gas_price, medium_gas_price, high_gas_price, 
		       base_fee_per_gas, priority_fee_avg, network_utilization, 
		       pending_tx_count, timestamp
		FROM gas_price_history
		WHERE timestamp >= $1
		ORDER BY timestamp DESC
	`

	rows, err := s.db.QueryContext(ctx, query, since)
	if err != nil {
		return nil, err
	}
	defer rows.Close()

	var prices []*GasPrice
	for rows.Next() {
		p := &GasPrice{}
		err := rows.Scan(
			&p.ID, &p.LowGasPrice, &p.MediumGasPrice, &p.HighGasPrice,
			&p.BaseFeePerGas, &p.PriorityFeeAvg, &p.NetworkUtilization,
			&p.PendingTxCount, &p.Timestamp,
		)
		if err != nil {
			return nil, err
		}
		prices = append(prices, p)
	}

	return prices, rows.Err()
}

// PredictGasPrices predicts future gas prices
func (s *Service) PredictGasPrices(ctx context.Context, hoursAhead int) ([]*GasPricePrediction, error) {
	// Get historical data for prediction
	since := time.Now().Add(-24 * time.Hour)
	history, err := s.GetGasPriceHistory(ctx, since)
	if err != nil {
		return nil, err
	}

	if len(history) < 10 {
		return nil, fmt.Errorf("insufficient historical data")
	}

	// Calculate trend
	trend := s.calculateTrend(history)

	// Generate predictions
	predictions := make([]*GasPricePrediction, hoursAhead)
	current := history[0]

	for i := 0; i < hoursAhead; i++ {
		predictions[i] = &GasPricePrediction{
			Timestamp:      time.Now().Add(time.Duration(i) * time.Hour),
			PredictedLow:   current.LowGasPrice + int64(float64(trend)*float64(i)),
			PredictedMedium: current.MediumGasPrice + int64(float64(trend)*float64(i)),
			PredictedHigh:  current.HighGasPrice + int64(float64(trend)*float64(i)),
			Confidence:    s.calculateConfidence(history, i),
		}
	}

	return predictions, nil
}

// calculateTrend calculates gas price trend
func (s *Service) calculateTrend(history []*GasPrice) float64 {
	if len(history) < 2 {
		return 0
	}

	var sum float64
	for i := 1; i < len(history); i++ {
		sum += float64(history[i-1].MediumGasPrice - history[i].MediumGasPrice)
	}

	return sum / float64(len(history)-1)
}

// calculateConfidence calculates prediction confidence
func (s *Service) calculateConfidence(history []*GasPrice, hoursAhead int) float64 {
	// Calculate standard deviation
	var sum float64
	var mean float64
	for _, p := range history {
		mean += float64(p.MediumGasPrice)
	}
	mean /= float64(len(history))

	for _, p := range history {
		diff := float64(p.MediumGasPrice) - mean
		sum += diff * diff
	}

	stdDev := math.Sqrt(sum / float64(len(history)))
	
	// Confidence decreases with time
	confidence := 1.0 - (float64(hoursAhead) * 0.1)
	if confidence < 0.1 {
		confidence = 0.1
	}

	// Adjust by volatility
	volatility := stdDev / mean
	confidence *= (1.0 - volatility)

	if confidence < 0 {
		confidence = 0
	}

	return confidence
}

// GetGasEstimates returns gas estimates for different urgency levels
func (s *Service) GetGasEstimates(ctx context.Context) (map[string]int64, error) {
	current, err := s.GetGasPrices(ctx)
	if err != nil {
		return nil, err
	}

	return map[string]int64{
		"slow":     current.LowGasPrice,
		"standard": current.MediumGasPrice,
		"fast":     current.HighGasPrice,
		"baseFee":  current.BaseFeePerGas,
	}, nil
}

// GetNetworkUtilization returns current network utilization
func (s *Service) GetNetworkUtilization(ctx context.Context) (float64, error) {
	current, err := s.GetGasPrices(ctx)
	if err != nil {
		return 0, err
	}

	return current.NetworkUtilization, nil
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
var _ = sort.Ints       // Use sort