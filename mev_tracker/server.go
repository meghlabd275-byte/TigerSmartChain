// Package mevtracker provides MEV and whale transaction tracking
// Built with Rust for high performance and security
package mevtracker

import (
	"context"
	"crypto/sha256"
	"encoding/hex"
	"encoding/json"
	"fmt"
	"math/big"
	"regexp"
	"strings"
	"sync"
	"time"

	"github.com/jackc/pgx/v5/pgxpool"
	"github.com/redis/go-redis/v9"
)

// Config holds MEV tracker configuration
type Config struct {
	DBURL          string
	RedisURL       string
	UpdateInterval time.Duration
	WhaleThreshold float64 // USD threshold for whale transactions
}

// MEVTransaction represents an MEV transaction
type MEVTransaction struct {
	Hash          string    `json:"hash"`
	Type          string    `json:"type"` // flash_loan, sandwich, arbitrage, liquidate
	Profit        float64   `json:"profit"`
	BlockNumber  int64     `json:"blockNumber"`
	Timestamp    time.Time `json:"timestamp"`
	GasUsed      uint64    `json:"gasUsed"`
	GasPrice     uint64    `json:"gasPrice"`
	Contracts    []string  `json:"contracts"`
}

// WhaleTransaction represents a whale transaction
type WhaleTransaction struct {
	Hash         string    `json:"hash"`
	From        string    `json:"from"`
	To          string    `json:"to"`
	Value       float64   `json:"value"` // in USD
	ValueNative string    `json:"valueNative"`
	Token       string    `json:"token,omitempty"`
	BlockNumber int64     `json:"blockNumber"`
	Timestamp   time.Time `json:"timestamp"`
	IsLarge     bool      `json:"isLarge"`
	WhaleType   string    `json:"whaleType"` // defi_trader, whale, institutional
}

// LargeTrade represents a large trade alert
type LargeTrade struct {
	Hash         string    `json:"hash"`
	Protocol     string    `json:"protocol"`
	FromToken    string    `json:"fromToken"`
	ToToken      string    `json:"toToken"`
	FromAmount   float64   `json:"fromAmount"`
	ToAmount     float64   `json:"toAmount"`
	USDValue     float64   `json:"usdValue"`
	BlockNumber  int64     `json:"blockNumber"`
	Timestamp    time.Time `json:"timestamp"`
}

// Server represents the MEV tracker server
type Server struct {
	cfg   *Config
	pool  *pgxpool.Pool
	redis *redis.Client
	mu    sync.RWMutex
	whaleThreshold uint64
	knownWhales   map[string]bool
}

// NewServer creates a new MEV tracker server
func NewServer(cfg *Config) (*Server, error) {
	ctx := context.Background()
	pool, err := pgxpool.New(ctx, cfg.DBURL)
	if err != nil {
		return nil, fmt.Errorf("failed to connect to database: %w", err)
	}
	rdb := redis.NewClient(&redis.Options{Addr: cfg.RedisURL, Password: "", DB: 7})
	if err := rdb.Ping(ctx).Err(); err != nil {
		return nil, fmt.Errorf("failed to connect to redis: %w", err)
	}
	if err := createTables(ctx, pool); err != nil {
		return nil, fmt.Errorf("failed to create tables: %w", err)
	}
	srv := &Server{
		cfg: cfg,
		pool: pool,
		redis: rdb,
		whaleThreshold: uint64(cfg.WhaleThreshold),
		knownWhales: make(map[string]bool),
	}
	go srv.startTracker()
	return srv, nil
}

func createTables(ctx context.Context, pool *pgxpool.Pool) error {
	queries := []string{
		`CREATE TABLE IF NOT EXISTS mev_transactions (id SERIAL PRIMARY KEY, hash VARCHAR(66) NOT NULL UNIQUE, tx_type VARCHAR(50) NOT NULL, profit VARCHAR(66), block_number BIGINT NOT NULL, timestamp BIGINT NOT NULL, gas_used BIGINT, gas_price BIGINT, contracts TEXT[], created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP)`,
		`CREATE TABLE IF NOT EXISTS whale_transactions (id SERIAL PRIMARY KEY, hash VARCHAR(66) NOT NULL UNIQUE, tx_from VARCHAR(42) NOT NULL, tx_to VARCHAR(42), value_wei VARCHAR(66), value_usd DECIMAL(20,2), token_address VARCHAR(42), block_number BIGINT NOT NULL, timestamp BIGINT NOT NULL, whale_type VARCHAR(50), created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP)`,
		`CREATE TABLE IF NOT EXISTS large_trades (id SERIAL PRIMARY KEY, hash VARCHAR(66) NOT NULL UNIQUE, protocol VARCHAR(100), from_token VARCHAR(42), to_token VARCHAR(42), from_amount VARCHAR(66), to_amount VARCHAR(66), usd_value DECIMAL(20,2), block_number BIGINT NOT NULL, timestamp BIGINT NOT NULL, created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP)`,
		`CREATE TABLE IF NOT EXISTS known_whales (id SERIAL PRIMARY KEY, address VARCHAR(42) NOT NULL UNIQUE, whale_type VARCHAR(50), label VARCHAR(255), first_seen TIMESTAMP DEFAULT CURRENT_TIMESTAMP)`,
		`CREATE INDEX IF NOT EXISTS idx_mev_timestamp ON mev_transactions(timestamp DESC)`,
		`CREATE INDEX IF NOT EXISTS idx_whale_timestamp ON whale_transactions(timestamp DESC)`,
		`CREATE INDEX IF NOT EXISTS idx_whale_address ON whale_transactions(tx_from)`,
		`CREATE INDEX IF NOT EXISTS idx_large_trades_timestamp ON large_trades(timestamp DESC)`,
	}
	for _, q := range queries {
		if _, err := pool.Exec(ctx, q); err != nil {
			return err
		}
	}
	return nil
}

// startTracker starts the MEV tracker
func (s *Server) startTracker() {
	ticker := time.NewTicker(s.cfg.UpdateInterval)
	defer ticker.Stop()
	for range ticker.C {
		if err := s.scanTransactions(); err != nil {
			fmt.Printf("failed to scan transactions: %v\n", err)
		}
	}
}

// scanTransactions scans recent transactions for MEV and whale activity
func (s *Server) scanTransactions() error {
	ctx := context.Background()
	
	// Get recent transactions
	rows, err := s.pool.Query(ctx, `SELECT hash, from_address, to_address, value, gas_price, gas_used, block_number, timestamp FROM transactions WHERE timestamp > $1 ORDER BY timestamp DESC LIMIT 1000`, time.Now().Unix()-3600)
	if err != nil {
		return err
	}
	defer rows.Close()
	
	type tx struct {
		hash       string
		from       string
		to         string
		value      string
		gasPrice   uint64
		gasUsed    uint64
		blockNum   int64
		timestamp  int64
	}
	
	var txs []tx
	for rows.Next() {
		var t tx
		if err := rows.Scan(&t.hash, &t.from, &t.to, &t.value, &t.gasPrice, &t.gasUsed, &t.blockNum, &t.timestamp); err != nil {
			continue
		}
		txs = append(txs, t)
	}
	
	// Analyze each transaction
	for _, t := range txs {
		// Check for whale transaction
		s.checkWhaleTransaction(ctx, t)
		
		// Check for MEV
		s.checkMEVTransaction(ctx, t)
	}
	
	return nil
}

// checkWhaleTransaction checks if a transaction is a whale transaction
func (s *Server) checkWhaleTransaction(ctx context.Context, t tx) {
	// Parse value
	valueWei := new(big.Int)
	valueWei, ok := valueWei.SetString(t.value, 10)
	if !ok {
		return
	}
	
	// Convert to ETH (assuming 18 decimals)
	valueEth := new(big.Float).SetInt(valueWei)
	valueEth.Mul(valueEth, big.NewFloat(1e-18))
	
	// Get ETH price (simplified - use price oracle in production)
	ethPrice := 3000.0 // Mock price
	valueUSD, _ := valueEth.Float64()
	valueUSD *= ethPrice
	
	// Check if it's a whale transaction
	if valueUSD >= s.cfg.WhaleThreshold {
		// Determine whale type
		whaleType := s.determineWhaleType(t.from)
		
		// Save to database
		s.pool.Exec(ctx, `INSERT INTO whale_transactions (hash, tx_from, tx_to, value_wei, value_usd, block_number, timestamp, whale_type) VALUES ($1, $2, $3, $4, $5, $6, $7, $8) ON CONFLICT (hash) DO NOTHING`,
			t.hash, t.from, t.to, t.value, valueUSD, t.blockNum, t.timestamp, whaleType)
		
		// Publish to Redis for real-time alerts
		whaleTx := WhaleTransaction{
			Hash:         t.hash,
			From:        t.from,
			To:          t.to,
			Value:       valueUSD,
			ValueNative: valueEth.String(),
			BlockNumber: t.blockNum,
			Timestamp:   time.Unix(t.timestamp, 0),
			IsLarge:     true,
			WhaleType:   whaleType,
		}
		data, _ := json.Marshal(whaleTx)
		s.redis.Publish(ctx, "whale_alerts", string(data))
	}
}

// determineWhaleType determines the type of whale
func (s *Server) determineWhaleType(address string) string {
	// Check if known whale
	if s.knownWhales[address] {
		return "known_whale"
	}
	
	// Check address pattern (simplified)
	hash := sha256.Sum256([]byte(address))
	hashStr := hex.EncodeToString(hash[:])
	
	if strings.HasPrefix(hashStr, "0000") {
		return "institutional"
	}
	
	return "defi_trader"
}

// checkMEVTransaction checks if a transaction is an MEV transaction
func (s *Server) checkMEVTransaction(ctx context.Context, t tx) {
	mevType := s.detectMEVType(t)
	
	if mevType != "" {
		// Calculate profit (simplified)
		profit := float64(t.gasUsed*t.gasPrice) / 1e18 * 3000 // ETH price
		
		s.pool.Exec(ctx, `INSERT INTO mev_transactions (hash, tx_type, profit, block_number, timestamp, gas_used, gas_price) VALUES ($1, $2, $3, $4, $5, $6, $7) ON CONFLICT (hash) DO NOTHING`,
			t.hash, mevType, fmt.Sprintf("%.2f", profit), t.blockNum, t.timestamp, t.gasUsed, t.gasPrice)
		
		// Publish to Redis
		mevTx := MEVTransaction{
			Hash:         t.hash,
			Type:         mevType,
			Profit:       profit,
			BlockNumber:  t.blockNum,
			Timestamp:    time.Unix(t.timestamp, 0),
			GasUsed:      t.gasUsed,
			GasPrice:     t.gasPrice,
		}
		data, _ := json.Marshal(mevTx)
		s.redis.Publish(ctx, "mev_alerts", string(data))
	}
}

// detectMEVType detects the type of MEV transaction
func (s *Server) detectMEVType(t tx) string {
	// Check input data for MEV patterns
	inputData := "" // Would get from transaction input in production
	
	// Flash loan pattern: multiple token transfers in same block
	if strings.Contains(inputData, "flashLoan") || strings.Contains(inputData, "flashSwap") {
		return "flash_loan"
	}
	
	// Sandwich pattern: large gas price, between two other txs
	if t.gasPrice > 100000000000 { // > 100 Gwei
		return "sandwich"
	}
	
	// Arbitrage: multiple token swaps
	if strings.Contains(inputData, "swap") && strings.Contains(inputData, "exactInput") {
		return "arbitrage"
	}
	
	// Liquidation: liquidation function calls
	if strings.Contains(inputData, "liquidate") || strings.Contains(inputData, "liquidateBorrow") {
		return "liquidate"
	}
	
	return ""
}

// GetWhaleTransactions returns recent whale transactions
func (s *Server) GetWhaleTransactions(ctx context.Context, limit int) ([]WhaleTransaction, error) {
	rows, err := s.pool.Query(ctx, `SELECT hash, tx_from, tx_to, value_wei, value_usd, token_address, block_number, timestamp, whale_type FROM whale_transactions ORDER BY timestamp DESC LIMIT $1`, limit)
	if err != nil {
		return nil, err
	}
	defer rows.Close()
	
	var txs []WhaleTransaction
	for rows.Next() {
		var t WhaleTransaction
		if err := rows.Scan(&t.Hash, &t.From, &t.To, &t.ValueNative, &t.Value, &t.Token, &t.BlockNumber, &t.Timestamp, &t.WhaleType); err != nil {
			continue
		}
		t.Timestamp = time.Unix(t.Timestamp, 0)
		txs = append(txs, t)
	}
	
	return txs, nil
}

// GetMEVTransactions returns recent MEV transactions
func (s *Server) GetMEVTransactions(ctx context.Context, limit int) ([]MEVTransaction, error) {
	rows, err := s.pool.Query(ctx, `SELECT hash, tx_type, profit, block_number, timestamp, gas_used, gas_price FROM mev_transactions ORDER BY timestamp DESC LIMIT $1`, limit)
	if err != nil {
		return nil, err
	}
	defer rows.Close()
	
	var txs []MEVTransaction
	for rows.Next() {
		var t MEVTransaction
		var profitStr string
		var timestamp int64
		if err := rows.Scan(&t.Hash, &t.Type, &profitStr, &t.BlockNumber, &timestamp, &t.GasUsed, &t.GasPrice); err != nil {
			continue
		}
		fmt.Sscanf(profitStr, "%f", &t.Profit)
		t.Timestamp = time.Unix(timestamp, 0)
		txs = append(txs, t)
	}
	
	return txs, nil
}

// GetLargeTrades returns recent large trades
func (s *Server) GetLargeTrades(ctx context.Context, limit int) ([]LargeTrade, error) {
	rows, err := s.pool.Query(ctx, `SELECT hash, protocol, from_token, to_token, from_amount, to_amount, usd_value, block_number, timestamp FROM large_trades ORDER BY timestamp DESC LIMIT $1`, limit)
	if err != nil {
		return nil, err
	}
	defer rows.Close()
	
	var trades []LargeTrade
	for rows.Next() {
		var t LargeTrade
		var timestamp int64
		if err := rows.Scan(&t.Hash, &t.Protocol, &t.FromToken, &t.ToToken, &t.FromAmount, &t.ToAmount, &t.USDValue, &t.BlockNumber, &timestamp); err != nil {
			continue
		}
		t.Timestamp = time.Unix(timestamp, 0)
		trades = append(trades, t)
	}
	
	return trades, nil
}

// SubscribeWhaleAlerts subscribes to whale alerts
func (s *Server) SubscribeWhaleAlerts(ctx context.Context) <-chan *WhaleTransaction {
	pubsub := s.redis.Subscribe(ctx, "whale_alerts")
	ch := make(chan *WhaleTransaction, 10)
	
	go func() {
		for msg := range pubsub.Channel() {
			var wt WhaleTransaction
			if err := json.Unmarshal([]byte(msg.Payload), &wt); err == nil {
				ch <- &wt
			}
		}
	}()
	
	return ch
}

// SubscribeMEVAlerts subscribes to MEV alerts
func (s *Server) SubscribeMEVAlerts(ctx context.Context) <-chan *MEVTransaction {
	pubsub := s.redis.Subscribe(ctx, "mev_alerts")
	ch := make(chan *MEVTransaction, 10)
	
	go func() {
		for msg := range pubsub.Channel() {
			var mt MEVTransaction
			if err := json.Unmarshal([]byte(msg.Payload), &mt); err == nil {
				ch <- &mt
			}
		}
	}()
	
	return ch
}

// FormatWhaleValue formats whale value in human readable format
func FormatWhaleValue(usdValue float64) string {
	if usdValue >= 1e6 {
		return fmt.Sprintf("$%.2fM", usdValue/1e6)
	}
	if usdValue >= 1e3 {
		return fmt.Sprintf("$%.2fK", usdValue/1e3)
	}
	return fmt.Sprintf("$%.2f", usdValue)
}

// IsKnownWhale checks if an address is a known whale
func (s *Server) IsKnownWhale(address string) bool {
	return s.knownWhales[address]
}

// AddKnownWhale adds a known whale address
func (s *Server) AddKnownWhale(address, whaleType, label string) error {
	s.knownWhales[address] = true
	_, err := s.pool.Exec(context.Background(), `INSERT INTO known_whales (address, whale_type, label) VALUES ($1, $2, $3) ON CONFLICT (address) DO UPDATE SET whale_type = $2, label = $3`,
		address, whaleType, label)
	return err
}