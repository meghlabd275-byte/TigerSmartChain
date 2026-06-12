// Package whale provides whale alert and large transaction detection services
package whale

import (
	"fmt"
	"math/big"
	"strings"
	"time"
)

// WhaleAlert represents a detected whale transaction
type WhaleAlert struct {
	ID          string    `json:"id"`
	Hash        string    `json:"hash"`
	From        string    `json:"from"`
	To          string    `json:"to"`
	Value       string    `json:"value"`
	ValueUSD    float64   `json:"valueUsd"`
	Token       string    `json:"token"`
	TokenSymbol string    `json:"tokenSymbol"`
	BlockNumber uint64    `json:"blockNumber"`
	Timestamp   time.Time `json:"timestamp"`
	AlertType   string    `json:"alertType"`
	GasUsed    uint64    `json:"gasUsed"`
	IsDEXTrade  bool      `json:"isDEXTrade"`
	IsNFT      bool      `json:"isNFT"`
	IsContract bool    `json:"isContract"`
}

// WhaleService handles whale transaction detection
type WhaleService struct {
	minUSDThreshold float64
	alertChannels   []string
	tokenPrices    map[string]float64
}

// NewWhaleService creates a new whale service
func NewWhaleService() *WhaleService {
	return &WhaleService{
		minUSDThreshold: 10000.0, // $10k minimum
		alertChannels:   []string{},
		tokenPrices: map[string]float64{
			"ETH":  3000.0,
			"WETH": 3000.0,
			"BNB":  600.0,
			"MATIC": 1.0,
			"USDC": 1.0,
			"USDT": 1.0,
			"DAI":  1.0,
		},
	}
}

// SetThreshold sets the minimum USD threshold
func (w *WhaleService) SetThreshold(threshold float64) {
	w.minUSDThreshold = threshold
}

// DetectWhaleTransaction detects whale transactions
func (w *WhaleService) DetectWhaleTransaction(tx *WhaleTx) (*WhaleAlert, error) {
	if tx == nil {
		return nil, nil
	}
	
	valueUSD := w.calculateUSDValue(tx.Value, tx.Token)
	
	if valueUSD < w.minUSDThreshold {
		return nil, nil
	}
	
	alertType := w.determineAlertType(tx)
	
	return &WhaleAlert{
		ID:          fmt.Sprintf("whale_%d_%s", tx.BlockNumber, tx.Hash[:8]),
		Hash:        tx.Hash,
		From:        tx.From,
		To:          tx.To,
		Value:       tx.Value,
		ValueUSD:    valueUSD,
		Token:       tx.Token,
		TokenSymbol: tx.TokenSymbol,
		BlockNumber: tx.BlockNumber,
		Timestamp:  tx.Timestamp,
		AlertType:  alertType,
		GasUsed:    tx.GasUsed,
		IsDEXTrade:  alertType == "dex_trade",
		IsNFT:      alertType == "nft_trade",
		IsContract: tx.IsContractCall,
	}, nil
}

// calculateUSDValue calculates USD value of a transaction
func (w *WhaleService) calculateUSDValue(value *big.Int, token string) float64 {
	if value == nil {
		return 0
	}
	
	price, ok := w.tokenPrices[token]
	if !ok {
		// Default price for unknown tokens
		price = 0.0
	}
	
	// Convert wei to ether (assuming 18 decimals)
	ethValue := new(big.Float).SetInt(value)
	ethValue.Mul(ethValue, big.NewFloat(1e-18))
	
	usdValue, _ := ethValue.Float64()
	
	return usdValue * price
}

// determineAlertType determines the type of whale alert
func (w *WhaleService) determineAlertType(tx *WhaleTx) string {
	if tx.IsNFT {
		return "nft_trade"
	}
	
	if tx.IsContractCall {
		if strings.HasPrefix(tx.Data, "0x7ff36ab5") ||
			strings.HasPrefix(tx.Data, "0x38ed1739") {
			return "dex_trade"
		}
		if strings.HasPrefix(tx.Data, "0xf242432a") ||
			strings.HasPrefix(tx.Data, "0x959e104e") {
			return "nft_mint"
		}
		return "contract_interaction"
	}
	
	if tx.Value != nil && tx.Token == "" {
		return "large_transfer"
	}
	
	return "unknown"
}

// WhaleTx represents a transaction to analyze
type WhaleTx struct {
	Hash          string
	From         string
	To           string
	Value        *big.Int
	Token        string
	TokenSymbol  string
	Data         string
	GasUsed      uint64
	BlockNumber  uint64
	Timestamp    time.Time
	IsNFT        bool
	IsContractCall bool
}

// ScanBlock scans a block for whale transactions
func (w *WhaleService) ScanBlock(blockTxs []*WhaleTx) ([]*WhaleAlert, error) {
	var alerts []*WhaleAlert
	
	for _, tx := range blockTxs {
		alert, err := w.DetectWhaleTransaction(tx)
		if err != nil {
			continue
		}
		if alert != nil {
			alerts = append(alerts, alert)
		}
	}
	
	return alerts, nil
}

// GetWhaleStats returns whale statistics
func (w *WhaleService) GetWhaleStats() (*WhaleStats, error) {
	return &WhaleStats{
		TotalAlerts:      0,
		TotalVolumeUSD:  0,
		AvgTransaction:  0,
		TopTokens:       []string{},
	}, nil
}

// WhaleStats represents whale statistics
type WhaleStats struct {
	TotalAlerts     int      `json:"totalAlerts"`
	TotalVolumeUSD float64  `json:"totalVolumeUsd"`
	AvgTransaction float64 `json:"avgTransaction"`
	TopTokens      []string `json:"topTokens"`
}

// GetWhaleMovement gets whale movements for an address
func (w *WhaleService) GetWhaleMovement(address string) ([]*WhaleAlert, error) {
	// In production, would query database
	return []*WhaleAlert{}, nil
}

// TrackWhale tracks a new whale address
func (w *WhaleService) TrackWhale(address string, threshold float64) error {
	_ = address
	_ = threshold
	// In production, would add to tracking list
	return nil
}

// GetTopWhales returns top whale addresses
func (w *WhaleService) GetTopWhales(limit int) ([]*WhaleAddress, error) {
	return []*WhaleAddress{}, nil
}

// WhaleAddress represents a tracked whale address
type WhaleAddress struct {
	Address      string    `json:"address"`
	TotalVolume float64   `json:"totalVolume"`
	TxCount     int       `json:"txCount"`
	LastActive  time.Time `json:"lastActive"`
	Tags        []string  `json:"tags"`
}

// InitWhaleService initializes the service
func InitWhaleService() (*WhaleService, error) {
	return NewWhaleService(), nil
}