// Package mevtracker provides MEV and whale transaction tracking
// Built with Rust for high performance and security
package main

import (
	"context"
	"encoding/json"
	"fmt"
	"math/big"
	"strconv"
	"strings"
	"sync"
	"time"

	"github.com/jackc/pgx/v5/pgxpool"
	"github.com/redis/go-redis/v9"
)

// TrackerConfig holds MEV tracker configuration
type TrackerConfig struct {
	DBURL          string
	RedisURL       string
	UpdateInterval time.Duration
	WhaleThreshold float64 // USD threshold for whale transactions
	// NativeToken is the wrapped-native token address whose USD price is read
	// from the token_prices table. Without it, values cannot be denominated in
	// USD and whale detection is skipped rather than guessing a price.
	NativeToken string
}

// MEVTransaction represents an MEV transaction
type MEVTransaction struct {
	Hash        string    `json:"hash"`
	Type        string    `json:"type"` // flash_loan, sandwich, arbitrage, liquidate
	Profit      float64   `json:"profit"`
	BlockNumber int64     `json:"blockNumber"`
	Timestamp   time.Time `json:"timestamp"`
	GasUsed     uint64    `json:"gasUsed"`
	GasPrice    uint64    `json:"gasPrice"`
	Contracts   []string  `json:"contracts"`
}

// WhaleTransaction represents a whale transaction
type WhaleTransaction struct {
	Hash        string    `json:"hash"`
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
	Hash        string    `json:"hash"`
	Protocol    string    `json:"protocol"`
	FromToken   string    `json:"fromToken"`
	ToToken     string    `json:"toToken"`
	FromAmount  float64   `json:"fromAmount"`
	ToAmount    float64   `json:"toAmount"`
	USDValue    float64   `json:"usdValue"`
	BlockNumber int64     `json:"blockNumber"`
	Timestamp   time.Time `json:"timestamp"`
}

// Server represents the MEV tracker server
type Server struct {
	cfg         *TrackerConfig
	pool        *pgxpool.Pool
	redis       *redis.Client
	mu          sync.RWMutex
	knownWhales map[string]bool
}

// NewServer creates a new MEV tracker server
func NewServer(cfg *TrackerConfig) (*Server, error) {
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
		cfg:         cfg,
		pool:        pool,
		redis:       rdb,
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

// chainTx is the subset of an indexed transaction needed for MEV and whale
// analysis.
type chainTx struct {
	hash      string
	from      string
	to        string
	value     string
	input     string
	gasPrice  uint64
	gasUsed   uint64
	blockNum  int64
	txIndex   int
	timestamp int64
}

// scanTransactions scans recent transactions for MEV and whale activity
func (s *Server) scanTransactions() error {
	ctx := context.Background()

	rows, err := s.pool.Query(ctx, `
		SELECT hash, from_address, COALESCE(to_address, ''), value, gas_price,
		       COALESCE(gas_used, 0), block_number,
		       COALESCE(transaction_index, 0),
		       COALESCE(EXTRACT(EPOCH FROM block_timestamp)::bigint, 0),
		       COALESCE(input, '')
		FROM transactions
		WHERE block_timestamp > NOW() - INTERVAL '1 hour'
		ORDER BY block_number DESC, transaction_index ASC
		LIMIT 1000`)
	if err != nil {
		return err
	}
	defer rows.Close()

	var txs []chainTx
	for rows.Next() {
		var t chainTx
		if err := rows.Scan(&t.hash, &t.from, &t.to, &t.value, &t.gasPrice, &t.gasUsed,
			&t.blockNum, &t.txIndex, &t.timestamp, &t.input); err != nil {
			continue
		}
		txs = append(txs, t)
	}

	for _, t := range txs {
		s.checkWhaleTransaction(ctx, t)
		s.checkMEVTransaction(ctx, t, txs)
	}

	return nil
}

// checkWhaleTransaction records a transaction whose native value exceeds the
// configured USD threshold. The USD value uses the indexed native token price;
// when no price is available the transaction is skipped instead of being
// assigned an invented value.
func (s *Server) checkWhaleTransaction(ctx context.Context, t chainTx) {
	valueWei, ok := new(big.Int).SetString(t.value, 10)
	if !ok {
		return
	}

	price, err := s.nativePriceUSD(ctx)
	if err != nil || price <= 0 {
		return
	}

	valueNative := new(big.Float).Quo(new(big.Float).SetInt(valueWei), big.NewFloat(1e18))
	valueFloat, _ := valueNative.Float64()
	valueUSD := valueFloat * price

	if valueUSD < s.cfg.WhaleThreshold {
		return
	}

	whaleType := s.determineWhaleType(ctx, t.from)

	if _, err := s.pool.Exec(ctx, `
		INSERT INTO whale_transactions (hash, tx_from, tx_to, value_wei, value_usd, block_number, timestamp, whale_type)
		VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
		ON CONFLICT (hash) DO NOTHING`,
		t.hash, t.from, t.to, t.value, valueUSD, t.blockNum, t.timestamp, whaleType); err != nil {
		return
	}

	whaleTx := WhaleTransaction{
		Hash:        t.hash,
		From:        t.from,
		To:          t.to,
		Value:       valueUSD,
		ValueNative: valueNative.String(),
		BlockNumber: t.blockNum,
		Timestamp:   time.Unix(t.timestamp, 0),
		IsLarge:     true,
		WhaleType:   whaleType,
	}
	data, _ := json.Marshal(whaleTx)
	s.redis.Publish(ctx, "whale_alerts", string(data))
}

// nativePriceUSD returns the most recent indexed USD price of the configured
// native token.
func (s *Server) nativePriceUSD(ctx context.Context) (float64, error) {
	if s.cfg.NativeToken == "" {
		return 0, fmt.Errorf("native token address not configured")
	}
	var price float64
	err := s.pool.QueryRow(ctx, `
		SELECT price_usd::float8 FROM token_prices
		WHERE token_address = $1
		ORDER BY timestamp DESC
		LIMIT 1`, s.cfg.NativeToken).Scan(&price)
	if err != nil {
		return 0, fmt.Errorf("no indexed price for native token %s: %w", s.cfg.NativeToken, err)
	}
	return price, nil
}

// determineWhaleType classifies an address using the operator-maintained
// known_whales table.
func (s *Server) determineWhaleType(ctx context.Context, address string) string {
	s.mu.RLock()
	known := s.knownWhales[address]
	s.mu.RUnlock()
	if known {
		return "known_whale"
	}

	var whaleType string
	if err := s.pool.QueryRow(ctx, `SELECT COALESCE(whale_type, '') FROM known_whales WHERE address = $1`, address).Scan(&whaleType); err == nil && whaleType != "" {
		return whaleType
	}
	return "unclassified"
}

// mevSelectors maps the leading four bytes of a transaction's calldata to the
// class of activity that entry point performs.
var mevSelectors = map[string]string{
	"0xab9c4b5d": "flash_loan", // Aave flashLoan(address,address[],uint256[],uint256[],address,bytes,uint16)
	"0x5c38449e": "flash_loan", // Balancer flashLoan(address,address[],uint256[],bytes)
	"0x00a718a9": "liquidate",  // Aave liquidationCall(address,address,address,uint256,bool)
	"0x414bf389": "arbitrage",  // Uniswap V3 exactInputSingle
	"0xc04b8d59": "arbitrage",  // Uniswap V3 exactInput
	"0x38ed1739": "swap",       // Uniswap V2 swapExactTokensForTokens
	"0x7ff36ab5": "swap",       // Uniswap V2 swapExactETHForTokens
	"0x18cbafe5": "swap",       // Uniswap V2 swapExactTokensForETH
	"0xfb3bdb41": "swap",       // Uniswap V2 swapETHForExactTokens
	"0x8803dbee": "swap",       // Uniswap V2 swapTokensForExactTokens
	"0x5ae401dc": "swap",       // Uniswap V3 multicall
}

// selector returns the leading four bytes of hex-encoded calldata.
func selector(input string) string {
	hexData := strings.TrimPrefix(input, "0x")
	if len(hexData) < 8 {
		return ""
	}
	return "0x" + strings.ToLower(hexData[:8])
}

// checkMEVTransaction classifies a transaction and records it when it performs
// MEV-generating activity.
func (s *Server) checkMEVTransaction(ctx context.Context, t chainTx, all []chainTx) {
	mevType := s.detectMEVType(t, all)
	if mevType == "" {
		return
	}

	price, err := s.nativePriceUSD(ctx)
	if err != nil {
		return
	}

	// The minimum extractable value is the priority fee the searcher actually
	// paid; a precise profit figure requires trace-level accounting.
	profitWei := new(big.Int).Mul(new(big.Int).SetUint64(t.gasUsed), new(big.Int).SetUint64(t.gasPrice))
	profitNative, _ := new(big.Float).Quo(new(big.Float).SetInt(profitWei), big.NewFloat(1e18)).Float64()
	profit := profitNative * price

	if _, err := s.pool.Exec(ctx, `
		INSERT INTO mev_transactions (hash, tx_type, profit, block_number, timestamp, gas_used, gas_price)
		VALUES ($1, $2, $3, $4, $5, $6, $7)
		ON CONFLICT (hash) DO NOTHING`,
		t.hash, mevType, strconv.FormatFloat(profit, 'f', 18, 64), t.blockNum, t.timestamp, t.gasUsed, t.gasPrice); err != nil {
		return
	}

	mevTx := MEVTransaction{
		Hash:        t.hash,
		Type:        mevType,
		Profit:      profit,
		BlockNumber: t.blockNum,
		Timestamp:   time.Unix(t.timestamp, 0),
		GasUsed:     t.gasUsed,
		GasPrice:    t.gasPrice,
	}
	data, _ := json.Marshal(mevTx)
	s.redis.Publish(ctx, "mev_alerts", string(data))
}

// detectMEVType classifies a transaction from its calldata selector. A swap is
// reported as a sandwich only when it is bracketed by two other swaps in the
// same block and pays a strictly higher gas price than both, which is the
// on-chain signature of a sandwich attack.
func (s *Server) detectMEVType(t chainTx, all []chainTx) string {
	kind, ok := mevSelectors[selector(t.input)]
	if !ok {
		return ""
	}
	if kind != "swap" {
		return kind
	}

	prev, next := neighboursInBlock(t, all)
	if prev != nil && next != nil &&
		mevSelectors[selector(prev.input)] == "swap" &&
		mevSelectors[selector(next.input)] == "swap" &&
		t.gasPrice > prev.gasPrice && t.gasPrice > next.gasPrice {
		return "sandwich"
	}
	return ""
}

// neighboursInBlock returns the transactions immediately preceding and
// following t in its block, or nil at a block boundary.
func neighboursInBlock(t chainTx, all []chainTx) (prev, next *chainTx) {
	for i := range all {
		cand := &all[i]
		if cand.blockNum != t.blockNum {
			continue
		}
		switch {
		case cand.txIndex < t.txIndex:
			if prev == nil || cand.txIndex > prev.txIndex {
				prev = cand
			}
		case cand.txIndex > t.txIndex:
			if next == nil || cand.txIndex < next.txIndex {
				next = cand
			}
		}
	}
	return prev, next
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
		var timestamp int64
		if err := rows.Scan(&t.Hash, &t.From, &t.To, &t.ValueNative, &t.Value, &t.Token, &t.BlockNumber, &timestamp, &t.WhaleType); err != nil {
			continue
		}
		t.Timestamp = time.Unix(timestamp, 0)
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
	s.mu.RLock()
	defer s.mu.RUnlock()
	return s.knownWhales[address]
}

// AddKnownWhale adds a known whale address
func (s *Server) AddKnownWhale(address, whaleType, label string) error {
	_, err := s.pool.Exec(context.Background(), `INSERT INTO known_whales (address, whale_type, label) VALUES ($1, $2, $3) ON CONFLICT (address) DO UPDATE SET whale_type = $2, label = $3`,
		address, whaleType, label)
	if err != nil {
		return err
	}
	s.mu.Lock()
	s.knownWhales[address] = true
	s.mu.Unlock()
	return nil
}
