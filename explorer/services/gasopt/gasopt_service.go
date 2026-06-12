// Package gasopt provides gas optimization and analysis services
package gasopt

import (
	"encoding/hex"
	"fmt"
	"math/big"
	"strings"
)

// GasOptService provides gas optimization analysis
type GasOptService struct {
	historicalData map[string][]*GasAnalysis
}

// GasAnalysis represents gas analysis for a transaction
type GasAnalysis struct {
	TxHash       string   `json:"txHash"`
	GasUsed      uint64   `json:"gasUsed"`
	GasLimit     uint64   `json:"gasLimit"`
	GasPrice     *big.Int `json:"gasPrice"`
	BlockNumber  uint64   `json:"blockNumber"`
	Function    string   `json:"function"`
	Calls       int      `json:"calls"`
	StorageReads int     `json:"storageReads"`
	StorageWrites int    `json:"storageWrites"`
}

// GasOptimization represents optimization suggestions
type GasOptimization struct {
	TxHash          string         `json:"txHash"`
	Suggestions     []string       `json:"suggestions"`
	SavingsEstimate *big.Int       `json:"savingsEstimate"`
	SavingsPercent float64       `json:"savingsPercent"`
	Optimizations []Optimization `json:"optimizations"`
}

// Optimization represents a single optimization
type Optimization struct {
	Type        string `json:"type"`
	Description string `json:"description"`
	Savings     string `json:"savings"`
	Priority   int    `json:"priority"`
}

// NewGasOptService creates a new gas optimization service
func NewGasOptService() *GasOptService {
	return &GasOptService{
		historicalData: make(map[string][]*GasAnalysis),
	}
}

// AnalyzeGas analyzes gas usage for a transaction
func (g *GasOptService) AnalyzeGas(txHash, bytecode string) (*GasOptimization, error) {
	opt := &GasOptimization{
		TxHash:          txHash,
		Suggestions:     []string{},
		SavingsEstimate: big.NewInt(0),
	}
	
	// Analyze bytecode for optimization opportunities
	bytecode = strings.TrimPrefix(bytecode, "0x")
	
	// Check for redundant SLOAD
	if strings.Contains(bytecode, "54") { // SLOAD opcode
		opt.Suggestions = append(opt.Suggestions, "Cache storage values in memory to reduce SLOADs")
		opt.Optimizations = append(opt.Optimizations, Optimization{
			Type:        "storage",
			Description: "Cache storage values in memory",
			Savings:     "~2000 gas per cached read",
			Priority:   1,
		})
	}
	
	// Check for redundant storage writes
	if strings.Contains(bytecode, "55") { // SSTORE opcode
		opt.Suggestions = append(opt.Suggestions, "Avoid redundant storage writes")
		opt.Optimizations = append(opt.Optimizations, Optimization{
			Type:        "storage",
			Description: "Check value before writing",
			Savings:     "~2900 gas per write avoided",
			Priority:   1,
		})
	}
	
	// Check for loops
	if strings.Contains(bytecode, "19") || strings.Contains(bytecode, "18") { // XOR, OR in loops
		opt.Suggestions = append(opt.Suggestions, "Optimize loop operations")
		opt.Optimizations = append(opt.Optimizations, Optimization{
			Type:        "loop",
			Description: "Unroll or cache loop bounds",
			Savings:     "Variable",
			Priority:   2,
		})
	}
	
	// Check for multiple calls
	callCount := strings.Count(bytecode, "f1") + strings.Count(bytecode, "fa")
	if callCount > 1 {
		opt.Suggestions = append(opt.Suggestions, "Bundle external calls")
		opt.Optimizations = append(opt.Optimizations, Optimization{
			Type:        "calls",
			Description: "Use multicall to batch calls",
			Savings:     "~21000 gas per call saved",
			Priority:   2,
		})
	}
	
	// Calculate potential savings
	// This is simplified - in production would use historical data
	opt.SavingsPercent = 15.0
	opt.SavingsEstimate = big.NewInt(5000)
	
	return opt, nil
}

// GetHistoricalGas gets historical gas data for a contract
func (g *GasOptService) GetHistoricalGas(contract string) ([]*GasAnalysis, error) {
	return g.historicalData[contract], nil
}

// RecordGas records gas analysis data
func (g *GasOptService) RecordGas(contract string, analysis *GasAnalysis) error {
	if g.historicalData == nil {
		g.historicalData = make(map[string][]*GasAnalysis)
	}
	
	g.historicalData[contract] = append(g.historicalData[contract], analysis)
	
	return nil
}

// EstimateGas estimates gas for a function call
func (g *GasOptService) EstimateGas(data string, to string) (*GasEstimate, error) {
	estimate := &GasEstimate{
		CallData: data,
		To:      to,
	}
	
	// Base gas for transaction
	estimate.BaseGas = 21000
	
	// Add function call gas
	if len(data) >= 10 {
		estimate.FunctionGas = 21000
		estimate.CallDataGas = int64(len(data)-10) / 2 * 68 // 68 gas per byte of call data
	}
	
	// Estimate total
	estimate.TotalGas = estimate.BaseGas + estimate.FunctionGas + estimate.CallDataGas
	
	return estimate, nil
}

// GasEstimate represents a gas estimate
type GasEstimate struct {
	CallData     string `json:"callData"`
	To           string `json:"to"`
	BaseGas     int64  `json:"baseGas"`
	FunctionGas int64  `json:"functionGas"`
	CallDataGas int64  `json:"callDataGas"`
	TotalGas    int64  `json:"totalGas"`
}

// GetOptimizedGas gets optimized gas recommendation
func (g *GasOptService) GetOptimizedGas(txHash string) (*GasRecommendation, error) {
	return &GasRecommendation{
		Slow:     GasLevel{GasPrice: "20", WaitTime: "5m", MaxCost: "$0.50"},
		Standard: GasLevel{GasPrice: "30", WaitTime: "1m", MaxCost: "$1.00"},
		Fast:     GasLevel{GasPrice: "50", WaitTime: "15s", MaxCost: "$2.00"},
	}, nil
}

// GasRecommendation represents gas price recommendations
type GasRecommendation struct {
	Slow     GasLevel `json:"slow"`
	Standard GasLevel `json:"standard"`
	Fast     GasLevel `json:"fast"`
}

// GasLevel represents a gas level
type GasLevel struct {
	GasPrice string `json:"gasPrice"`
	WaitTime string `json:"waitTime"`
	MaxCost string `json:"maxCost"`
}

// DecodeBytecode decodes bytecode for analysis
func DecodeBytecode(bytecodeHex string) ([]byte, error) {
	bytecodeHex = strings.TrimPrefix(bytecodeHex, "0x")
	return hex.DecodeString(bytecodeHex)
}

// GetGasStats returns gas statistics
func (g *GasOptService) GetGasStats() (*GasStats, error) {
	return &GasStats{
		AvgGasPrice:    "30",
		AvgGasUsed:     65000,
		AvgTxCost:      "$2.00",
		FastGasPrice:   "50",
		SlowGasPrice:   "20",
	}, nil
}

// GasStats represents gas statistics
type GasStats struct {
	AvgGasPrice  string `json:"avgGasPrice"`
	AvgGasUsed  uint64 `json:"avgGasUsed"`
	AvgTxCost   string `json:"avgTxCost"`
	FastGasPrice string `json:"fastGasPrice"`
	SlowGasPrice string `json:"slowGasPrice"`
}

// InitGasOptService initializes the service
func InitGasOptService() (*GasOptService, error) {
	return NewGasOptService(), nil
}