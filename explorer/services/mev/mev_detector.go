// Package mev provides MEV (Maximal Extractable Value) detection services
// This service identifies sandwich attacks, front-running, and other MEV extraction patterns
package mev

import (
	"encoding/hex"
	"fmt"
	"math/big"
	"strings"
	"time"
)

// MEVType represents the type of MEV activity detected
type MEVType string

const (
	MEVSandwich     MEVType = "sandwich"
	MEVFrontRun    MEVType = "frontrun"
	MEVBackRun     MEVType = "backrun"
	MEVArbitrage   MEVType = "arbitrage"
	MEVLiquidation MEVType = "liquidation"
	MEVUnknown    MEVType = "unknown"
)

// DetectedMEV represents a detected MEV activity
type DetectedMEV struct {
	ID          string    `json:"id"`
	Type        MEVType   `json:"type"`
	Timestamp   time.Time `json:"timestamp"`
	BlockNumber uint64   `json:"blockNumber"`
	TxHash     string    `json:"txHash"`
	Profit     string   `json:"profit"`
	ProfitUSD  float64  `json:"profitUsd,omitempty"`
	Token0     string   `json:"token0"`
	Token1     string   `json:"token1"`
	Amount0    string   `json:"amount0"`
	Amount1    string   `json:"amount1"`
	PoolAddress string  `json:"poolAddress"`
	GasUsed    uint64   `json:"gasUsed"`
	GasPrice   string   `json:"gasPrice"`
	Signer     string   `json:"signer"`
	Confidence float64  `json:"confidence"`
}

// SandwichAttackDetector detects sandwich attack patterns
type SandwichAttackDetector struct {
	// Configuration thresholds
	MinProfitUSD        float64   `json:"minProfitUsd"`
	MaxBlockDelay       int      `json:"maxBlockDelay"`
	MinSwapAmount       *big.Int `json:"minSwapAmount"`
	FlashbotsAPI        string    `json:"flashbotsApi"`
	MEVProtectionAPI    string    `json:"mevProtectionApi"`
}

// NewSandwichAttackDetector creates a new detector with default settings
func NewSandwichAttackDetector() *SandwichAttackDetector {
	return &SandwichAttackDetector{
		MinProfitUSD:      100.0,           // Minimum $100 profit to flag
		MaxBlockDelay:    3,               // Within 3 blocks
		MinSwapAmount:   big.NewInt(1000),  // Min 1000 tokens
		FlashbotsAPI:   "https://relay.flashbots.net",
		MEVProtectionAPI: "https://api.mevprotection.io",
	}
}

// DetectSandwichAttack analyzes transactions for sandwich patterns
func (d *SandwichAttackDetector) DetectSandwichAttack(tx *Transaction, mempool *MempoolTxSet) (*DetectedMEV, error) {
	if tx == nil {
		return nil, fmt.Errorf("nil transaction")
	}
	
	// Check if transaction is a large swap
	if !d.isLargeSwap(tx) {
		return nil, nil
	}
	
	// Find front-running and back-running transactions
	frontRun := d.findFrontRun(tx, mempool)
	backRun := d.findBackRun(tx, mempool)
	
	if frontRun == nil || backRun == nil {
		return nil, nil
	}
	
	// Calculate profit
	profit := d.calculateProfit(frontRun, tx, backRun)
	if profit.Cmp(big.NewInt(0)) <= 0 {
		return nil, nil
	}
	
	profitUSD := d.estimateUSDProfit(profit, tx.Token0)
	
	return &DetectedMEV{
		ID:          fmt.Sprintf("sandwich_%d_%s", tx.BlockNumber, tx.Hash[:8]),
		Type:        MEVSandwich,
		Timestamp:   tx.Timestamp,
		BlockNumber: tx.BlockNumber,
		TxHash:     tx.Hash,
		Profit:     profit.String(),
		ProfitUSD:  profitUSD,
		Token0:     tx.Token0,
		Token1:     tx.Token1,
		Amount0:    tx.Amount0.String(),
		Amount1:    tx.Amount1.String(),
		PoolAddress: tx.To,
		GasUsed:    tx.GasUsed,
		GasPrice:   tx.GasPrice.String(),
		Signer:     tx.From,
		Confidence: d.calculateConfidence(frontRun, tx, backRun),
	}, nil
}

// isLargeSwap checks if transaction is a large swap
func (d *SandwichAttackDetector) isLargeSwap(tx *Transaction) bool {
	if tx == nil {
		return false
	}
	
	// Check for swap function signatures
	swapSigs := []string{
		"0x7ff36ab5", // swapExactETHForTokens
		"0xb88a802f", // swapETHForExactTokens
		"0x38ed1739", // swapExactTokensForETH
		0x3cb4b7a4", // swapTokensForExactETH
		0x8803dbee", // swapExactTokensForTokens
		0x4e1cff86", // swapTokensForExactTokens
		0x5c11d495", // multicall
	}
	
	for _, sig := range swapSigs {
		if strings.HasPrefix(tx.Data, sig) {
			return true
		}
	}
	
	// Check amount threshold
	if tx.Amount0 != nil && tx.Amount0.Cmp(d.MinSwapAmount) >= 0 {
		return true
	}
	
	return false
}

// findFrontRun finds potential front-running transactions
func (d *SandwichAttackDetector) findFrontRun(target *Transaction, mempool *MempoolTxSet) *Transaction {
	if mempool == nil {
		return nil
	}
	
	// Look for same token pair with higher gas price in same block
	for _, tx := range mempool.Transactions {
		if tx.BlockNumber > target.BlockNumber {
			continue
		}
		if tx.BlockNumber < target.BlockNumber {
			continue
		}
		
		// Check same pool and token pair
		if tx.Token0 == target.Token0 && tx.Token1 == target.Token1 {
			// Check if higher gas price (front-running indicator)
			if tx.GasPrice.Cmp(target.GasPrice) > 0 {
				return tx
			}
		}
	}
	
	return nil
}

// findBackRun finds potential back-running transactions
func (d *SandwichAttackDetector) findBackRun(target *Transaction, mempool *MempoolTxSet) *Transaction {
	if mempool == nil {
		return nil
	}
	
	// Look for same token pair after target transaction
	for _, tx := range mempool.Transactions {
		if tx.BlockNumber != target.BlockNumber {
			continue
		}
		
		// Check timestamp after target
		if tx.Timestamp.After(target.Timestamp) {
			if tx.Token0 == target.Token0 && tx.Token1 == target.Token1 {
				return tx
			}
		}
	}
	
	return nil
}

// calculateProfit calculates the sandwich attack profit
func (d *SandwichAttackDetector) calculateProfit(frontRun, target, backRun *Transaction) *big.Int {
	// Simplified profit calculation
	// In production, would use actual exchange rates
	
	if frontRun == nil || backRun == nil {
		return big.NewInt(0)
	}
	
	// Example: front-run buys low, target trades, back-run sells high
	// Profit = backRun amount - frontRun amount
	profit := new(big.Int)
	if backRun.Amount1 != nil && frontRun.Amount1 != nil {
		profit.Sub(backRun.Amount1, frontRun.Amount1)
	}
	
	return profit
}

// estimateUSDProfit converts profit to USD
func (d *SandwichAttackDetector) estimateUSDProfit(profit *big.Int, token string) float64 {
	// Simplified - in production would fetch real prices
	if profit.Sign() <= 0 {
		return 0
	}
	
	// Assume $1 per token for estimation
	f := new(big.Float).SetInt(profit)
	
	// This would normally fetch real token prices
	return 0.0
}

// calculateConfidence calculates confidence score
func (d *SandwichAttackDetector) calculateConfidence(frontRun, target, backRun *Transaction) float64 {
	confidence := 0.5 // Base confidence
	
	if frontRun != nil && frontRun.GasPrice.Cmp(target.GasPrice) > 0 {
		confidence += 0.2
	}
	
	if backRun != nil && target.Timestamp.Before(backRun.Timestamp) {
		confidence += 0.2
	}
	
	// Check for exact token match
	if frontRun != nil && backRun != nil {
		if frontRun.Token0 == target.Token0 && backRun.Token1 == target.Token1 {
			confidence += 0.1
		}
	}
	
	return confidence
}

// Transaction represents a blockchain transaction
type Transaction struct {
	Hash        string
	From       string
	To         string
	Value      *big.Int
	Data       string
	GasUsed    uint64
	GasPrice   *big.Int
	BlockNumber uint64
	Timestamp time.Time
	Token0    string
	Token1    string
	Amount0   *big.Int
	Amount1  *big.Int
}

// MempoolTxSet represents a set of transactions in the mempool
type MempoolTxSet struct {
	Transactions []*Transaction
	Timestamp   time.Time
}

// AddTransaction adds a transaction to the mempool
func (m *MempoolTxSet) AddTransaction(tx *Transaction) {
	if m.Transactions == nil {
		m.Transactions = make([]*Transaction, 0)
	}
	m.Transactions = append(m.Transactions, tx)
}

// DecodeTransaction decodes transaction data to extract swap details
func DecodeTransaction(data string, to string) (*Transaction, error) {
	data = strings.TrimPrefix(data, "0x")
	
	tx := &Transaction{
		Data: data,
		To:   to,
	}
	
	if len(data) < 8 {
		return tx, nil
	}
	
	sig := data[:8]
	
	// Common DEX function signatures
	switch sig {
	case "7ff36ab5": // swapExactETHForTokens
		tx.Token0 = "0x000000000000000000000000000000000000000000" // ETH
	case "b88a802f": // swapETHForExactTokens
		tx.Token0 = "0x000000000000000000000000000000000000000000"
	case "38ed1739": // swapExactTokensForETH
		tx.Token1 = "0x000000000000000000000000000000000000000000"
	case "8803dbee": // swapExactTokensForTokens
		// Would need to decode token addresses from data
	case "5c11d495": // multicall
		// Would need to decode calls
	}
	
	return tx, nil
}

// ScanMempool scans the mempool for MEV opportunities
func (d *SandwichAttackDetector) ScanMempool(mempool *MempoolTxSet) ([]*DetectedMEV, error) {
	var detections []*DetectedMEV
	
	if mempool == nil {
		return detections, nil
	}
	
	for _, tx := range mempool.Transactions {
		detection, err := d.DetectSandwichAttack(tx, mempool)
		if err != nil {
			continue
		}
		if detection != nil && detection.ProfitUSD >= d.MinProfitUSD {
			detections = append(detections, detection)
		}
	}
	
	return detections, nil
}

// GetRecentMEV retrieves recent MEV activities
func (d *SandwichAttackDetector) GetRecentMEV(blockRange uint64) ([]*DetectedMEV, error) {
	// In production, would query database
	return []*DetectedMEV{}, nil
}

// MEVStats provides MEV statistics
type MEVStats struct {
	TotalSandwiches   int     `json:"totalSandwiches"`
	TotalProfitUSD float64 `json:"totalProfitUsd"`
	TopMEVBot    string `json:"topMEVBot"`
	TopPool     string `json:"topPool"`
}

// GetStats returns MEV statistics
func (d *SandwichAttackDetector) GetStats() (*MEVStats, error) {
	// In production, would query database
	return &MEVStats{
		TotalSandwiches:   0,
		TotalProfitUSD: 0,
		TopMEVBot:     "0x0000000000000000000000000000000000000000",
		TopPool:      "0x0000000000000000000000000000000000000000",
	}, nil
}

// InitMEVService initializes the MEV detection service
func InitMEVService() (*SandwichAttackDetector, error) {
	return NewSandwichAttackDetector(), nil
}

// Helper to decode hex string
func decodeHex(hexStr string) ([]byte, error) {
	hexStr = strings.TrimPrefix(hexStr, "0x")
	return hex.DecodeString(hexStr)
}