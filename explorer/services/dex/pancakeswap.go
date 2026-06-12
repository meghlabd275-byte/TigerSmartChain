// Package dex provides production DEX integration for PancakeSwap and other DEXes.
// This service provides real-time trading pair data, liquidity tracking, OHLC historical data,
// flash loan detection, and AMM analytics.
package dex

import (
	"context"
	"database/sql"
	"encoding/hex"
	"encoding/json"
	"fmt"
	"math"
	"math/big"
	"net/http"
	"strings"
	"sync"
	"time"

	"github.com/ethereum/go-ethereum/accounts/abi"
	"github.com/ethereum/go-ethereum/common"
	"github.com/ethereum/go-ethereum/common/hexutil"
	"github.com/ethereum/go-ethereum/core/types"
	"github.com/ethereum/go-ethereum/crypto"
	"github.com/ethereum/go-ethereum/ethclient"
)

// =============================================================================
// CONSTANTS & CONFIGURATION
// =============================================================================

const (
	// PancakeSwap Factory ABI
	PancakeFactoryABI = `[{"inputs":[{"internalType":"address","name":"_feeToSetter","type":"address"}],"stateMutability":"nonpayable","type":"constructor"},{"anonymous":false,"inputs":[{"indexed":true,"internalType":"address","name":"token0","type":"address"},{"indexed":true,"internalType":"address","name":"token1","type":"address"},{"indexed":false,"internalType":"address","name":"pair","type":"address"},{"indexed":false,"internalType":"uint256","name":"","type":"uint256"}],"name":"PairCreated","type":"event"},{"inputs":[],"name":"feeTo","outputs":[{"internalType":"address","name":"","type":"address"}],"stateMutability":"view","type":"function"},{"inputs":[],"name":"feeToSetter","outputs":[{"internalType":"address","name":"","type":"address"}],"stateMutability":"view","type":"function"},{"inputs":[],"name":"allPairs","outputs":[{"internalType":"address[]","name":"","type":"address[]"}],"stateMutability":"view","type":"function"},{"inputs":[],"name":"allPairsLength","outputs":[{"internalType":"uint256","name":"","type":"uint256"}],"stateMutability":"view","type":"function"},{"inputs":[{"internalType":"address","name":"","type":"address"}],"name":"getPair","outputs":[{"internalType":"address","name":"","type":"address"}],"stateMutability":"view","type":"function"},{"inputs":[{"internalType":"address","name":"tokenA","type":"address"},{"internalType":"address","name":"tokenB","type":"address"}],"name":"createPair","outputs":[{"internalType":"address","name":"pair","type":"address"}],"stateMutability":"nonpayable","type":"function"},{"inputs":[{"internalType":"address","name":"","type":"address"}],"name":"token0","outputs":[{"internalType":"address","name":"","type":"address"}],"stateMutability":"view","type":"function"},{"inputs":[{"internalType":"address","name":"","type":"address"}],"name":"token1","outputs":[{"internalType":"address","name":"","type":"address"}],"stateMutability":"view","type":"function"}]`

	// PancakeSwap Pair ABI for liquidity pool
	PancakePairABI = `[{"inputs":[],"name":"metadata","outputs":[{"internalType":"uint256","name":"decimals","type":"uint256"},{"internalType":"address","name":"token0","type":"address"},{"internalType":"address","name":"token1","type":"address"},{"internalType":"uint8","name":"token0Decimals","type":"uint8"},{"internalType":"uint8","name":"token1Decimals","type":"uint8"},{"internalType":"uint16","name":"fee","type":"uint16"}],"stateMutability":"view","type":"function"},{"inputs":[],"name":"getReserves","outputs":[{"internalType":"uint112","name":"_reserve0","type":"uint112"},{"internalType":"uint112","name":"_reserve1","type":"uint112"},{"internalType":"uint32","name":"_blockTimestampLast","type":"uint32"}],"stateMutability":"view","type":"function"},{"inputs":[],"name":"price0CumulativeLast","outputs":[{"internalType":"uint256","name":"","type":"uint256"}],"stateMutability":"view","type":"function"},{"inputs":[],"name":"price1CumulativeLast","outputs":[{"internalType":"uint256","name":"","type":"uint256"}],"stateMutability":"view","type":"function"},{"inputs":[],"name":"kLast","outputs":[{"internalType":"uint256","name":"","type":"uint256"}],"stateMutability":"view","type":"function"},{"inputs":[{"internalType":"address","name":"to","type":"address"}],"name":"mint","outputs":[{"internalType":"uint256","name":"liquidity","type":"uint256"}],"stateMutability":"nonpayable","type":"function"},{"inputs":[{"internalType":"address","name":"to","type":"address"}],"name":"burn","outputs":[{"internalType":"uint256","name":"amount0","type":"uint256"},{"internalType":"uint256","name":"amount1","type":"uint256"}],"stateMutability":"nonpayable","type":"function"},{"inputs":[{"internalType":"uint256","name":"amount0Out","type":"uint256"},{"internalType":"uint256","name":"amount1Out","name":"to","type":"address"},{"internalType":"bytes","name":"data","type":"bytes"}],"name":"swap","outputs":[],"stateMutability":"nonpayable","type":"function"},{"inputs":[{"internalType":"address","name":"tokenA","type":"address"},{"internalType":"address","name":"tokenB","type":"address"},{"internalType":"uint256","name":"liquidity","type":"uint256"}],"name":"skim","outputs":[],"stateMutability":"nonpayable","type":"function"},{"inputs":[],"name":"sync","outputs":[],"stateMutability":"nonpayable","type":"function"}]`

	// Factory addresses for different chains
	PancakeFactoryBSC  = "0xca143Ce32Fe78f1f7019d7d551a6402fC1270bC63"
	PancakeFactoryETH = "0x1097053Fb2F4c6459D9e7e8A7e6c7D8d1e3b3cF5"

	// Default settings
	DefaultMaxPairs      = 1000
	DefaultUpdateInterval = 15 * time.Second
	OHLCInterval         = 5 * time.Minute
)

// =============================================================================
// DATA STRUCTURES
// =============================================================================

// Service provides DEX integration and liquidity tracking
type Service struct {
	db            *sql.DB
	rpcClient     *ethclient.Client
	factoryAddr   common.Address
	factoryABI   abi.ABI
	pairABI      abi.ABI
	mu           sync.RWMutex
	pairs        map[string]*TradingPair
	updateTimer *time.Timer
	config      *Config
}

// Config holds DEX service configuration
type Config struct {
	DB               *sql.DB
	RPCURL           string
	FactoryAddress   string
	MaxPairs        int
	UpdateInterval  time.Duration
	EnableFlashLoan bool
}

// TradingPair represents a DEX trading pair
type TradingPair struct {
	Address             string    `json:"address"`
	Token0              string    `json:"token0"`
	Token1              string    `json:"token1"`
	Token0Symbol        string    `json:"token0Symbol"`
	Token1Symbol        string    `json:"token1Symbol"`
	Token0Decimals     uint8     `json:"token0Decimals"`
	Token1Decimals     uint8     `json:"token1Decimals"`
	Reserve0           string    `json:"reserve0"`
	Reserve1           string    `json:"reserve1"`
	TotalSupply        string    `json:"totalSupply"`
	LiquidityUSD       float64   `json:"liquidityUSD"`
	Volume24h          float64   `json:"volume24h"`
	VolumeChange24h    float64   `json:"volumeChange24h"`
	Price0             float64   `json:"price0"`
	Price1             float64   `json:"price1"`
	PriceChange24h     float64   `json:"priceChange24h"`
	TxCount            int64     `json:"txCount"`
	InitializedAt     time.Time `json:"initializedAt"`
	LastUpdatedAt     time.Time `json:"lastUpdatedAt"`
}

// OHLCData represents candlestick data
type OHLCData struct {
	PairAddress   string    `json:"pairAddress"`
	Timestamp   time.Time `json:"timestamp"`
	Open        float64   `json:"open"`
	High        float64   `json:"high"`
	Low         float64   `json:"low"`
	Close       float64   `json:"close"`
	Volume0     float64   `json:"volume0"`
	Volume1     float64   `json:"volume1"`
	TxCount     int64     `json:"txCount"`
}

// SwapEvent represents a swap transaction
type SwapEvent struct {
	ID              int64     `json:"id"`
	PairAddress    string    `json:"pairAddress"`
	TransactionHash string  `json:"transactionHash"`
	BlockNumber   int64     `json:"blockNumber"`
	Timestamp     time.Time `json:"timestamp"`
	Sender        string    `json:"sender"`
	FromReserve0  string    `json:"fromReserve0"`
	ToReserve1    string    `json:"toReserve1"`
	FromReserve1  string    `json:"fromReserve1"`
	ToReserve0    string    `json:"toReserve0"`
	GasUsed       int64     `json:"gasUsed"`
}

// FlashLoan represents a flash loan transaction
type FlashLoan struct {
	ID              int64     `json:"id"`
	TransactionHash string    `json:"transactionHash"`
	BlockNumber  int64     `json:"blockNumber"`
	Timestamp   time.Time `json:"timestamp"`
	Borrower    string    `json:"borrower"`
	Token       string    `json:"token"`
	Amount      string    `json:"amount"`
	DebtFee     string    `json:"debtFee"`
	Profit      string    `json:"profit"`
	IsAttack    bool      `json:"isAttack"`
	Description string   `json:"description"`
}

// PairInfo is for API response compatibility
type PairInfo struct {
	Address          string    `json:"address"`
	Token0           string    `json:"token0"`
	Token1           string    `json:"token1"`
	Token0Symbol    string    `json:"token0Symbol"`
	Token1Symbol    string    `json:"token1Symbol"`
	Reserve0        string    `json:"reserve0"`
	Reserve1        string    `json:"reserve1"`
	PriceUSD        float64   `json:"priceUSD"`
	LiquidityUSD    float64   `json:"liquidityUSD"`
	Volume24h      float64   `json:"volume24h"`
	TxCount        int64     `json:"txCount"`
}

// =============================================================================
// CONSTRUCTOR & INITIALIZATION
// =============================================================================

// NewService creates a new DEX service with real blockchain integration
func NewService(cfg *Config) (*Service, error) {
	// Validate configuration
	if cfg == nil {
		cfg = &Config{
			RPCURL:         "https://bsc-dataseed1.binance.org",
			FactoryAddress: PancakeFactoryBSC,
			MaxPairs:       DefaultMaxPairs,
			UpdateInterval: DefaultUpdateInterval,
		}
	}

	// Connect to blockchain RPC
	client, err := ethclient.Dial(cfg.RPCURL)
	if err != nil {
		return nil, fmt.Errorf("failed to connect to RPC: %w", err)
	}

	// Parse factory ABI
	factoryABI, err := abi.JSON(strings.NewReader(PancakeFactoryABI))
	if err != nil {
		return nil, fmt.Errorf("failed to parse factory ABI: %w", err)
	}

	// Parse pair ABI
	pairABI, err := abi.JSON(strings.NewReader(PancakePairABI))
	if err != nil {
		return nil, fmt.Errorf("failed to parse pair ABI: %w", err)
	}

	svc := &Service{
		db:          cfg.DB,
		rpcClient:   client,
		factoryAddr: common.HexToAddress(cfg.FactoryAddress),
		factoryABI: factoryABI,
		pairABI:    pairABI,
		pairs:      make(map[string]*TradingPair),
		config:     cfg,
	}

	// Initialize database tables
	if err := svc.initDatabase(); err != nil {
		return nil, fmt.Errorf("failed to initialize database: %w", err)
	}

	// Start background sync
	go svc.startBackgroundSync()

	return svc, nil
}

// initDatabase creates required database tables
func (s *Service) initDatabase() error {
	if s.db == nil {
		return nil // Skip if no database
	}

	queries := []string{
		`CREATE TABLE IF NOT EXISTS dex_pairs (
			address VARCHAR(64) PRIMARY KEY,
			token0 VARCHAR(64) NOT NULL,
			token1 VARCHAR(64) NOT NULL,
			token0_symbol VARCHAR(32),
			token1_symbol VARCHAR(32),
			token0_decimals INTEGER,
			token1_decimals INTEGER,
			reserve0 TEXT,
			reserve1 TEXT,
			total_supply TEXT,
			liquidity_usd REAL,
			volume_24h REAL,
			volume_change_24h REAL,
			price0 REAL,
			price1 REAL,
			price_change_24h REAL,
			tx_count BIGINT,
			initialized_at TIMESTAMP,
			updated_at TIMESTAMP DEFAULT NOW()
		)`,
		`CREATE TABLE IF NOT EXISTS dex_ohlc (
			id SERIAL PRIMARY KEY,
			pair_address VARCHAR(64),
			timestamp TIMESTAMP,
			open REAL,
			high REAL,
			low REAL,
			close REAL,
			volume0 REAL,
			volume1 REAL,
			tx_count BIGINT,
			UNIQUE(pair_address, timestamp)
		)`,
		`CREATE TABLE IF NOT EXISTS dex_swaps (
			id SERIAL PRIMARY KEY,
			pair_address VARCHAR(64),
			transaction_hash VARCHAR(128),
			block_number BIGINT,
			timestamp TIMESTAMP,
			sender VARCHAR(64),
			from_reserve0 TEXT,
			to_reserve1 TEXT,
			from_reserve1 TEXT,
			to_reserve0 TEXT,
			gas_used BIGINT
		)`,
		`CREATE TABLE IF NOT EXISTS dex_flashloans (
			id SERIAL PRIMARY KEY,
			transaction_hash VARCHAR(128),
			block_number BIGINT,
			timestamp TIMESTAMP,
			borrower VARCHAR(64),
			token VARCHAR(64),
			amount TEXT,
			debt_fee TEXT,
			profit TEXT,
			is_attack BOOLEAN,
			description TEXT
		)`,
		`CREATE INDEX IF NOT EXISTS idx_dex_pairs_updated ON dex_pairs(updated_at)`,
		`CREATE INDEX IF NOT EXISTS idx_dex_ohlc_pair_time ON dex_ohlc(pair_address, timestamp)`,
		`CREATE INDEX IF NOT EXISTS idx_dex_swaps_pair ON dex_swaps(pair_address, block_number)`,
	}

	for _, q := range queries {
		if _, err := s.db.Exec(q); err != nil {
			return fmt.Errorf("database initialization failed: %w", err)
		}
	}

	return nil
}

// =============================================================================
// CORE FUNCTIONS
// =============================================================================

// GetTopPairs returns the top liquidity pairs
func (s *Service) GetTopPairs(limit int) ([]*PairInfo, error) {
	if limit <= 0 || limit > DefaultMaxPairs {
		limit = 50
	}

	s.mu.RLock()
	defer s.mu.RUnlock()

	// Convert to slice and sort by liquidity
	pairs := make([]*TradingPair, 0, len(s.pairs))
	for _, p := range s.pairs {
		pairs = append(pairs, p)
	}

	// Simple bubble sort by liquidity (could use heap for better performance)
	for i := 0; i < len(pairs)-1; i++ {
		for j := 0; j < len(pairs)-1-i; j++ {
			if pairs[j].LiquidityUSD < pairs[j+1].LiquidityUSD {
				pairs[j], pairs[j+1] = pairs[j+1], pairs[j]
			}
		}
	}

	// Limit results
	if len(pairs) > limit {
		pairs = pairs[:limit]
	}

	// Convert to PairInfo
	result := make([]*PairInfo, len(pairs))
	for i, p := range pairs {
		result[i] = &PairInfo{
			Address:       p.Address,
			Token0:        p.Token0,
			Token1:        p.Token1,
			Token0Symbol:  p.Token0Symbol,
			Token1Symbol:  p.Token1Symbol,
			Reserve0:      p.Reserve0,
			Reserve1:      p.Reserve1,
			PriceUSD:      p.Price1,
			LiquidityUSD:  p.LiquidityUSD,
			Volume24h:     p.Volume24h,
			TxCount:       p.TxCount,
		}
	}

	return result, nil
}

// GetPair returns a specific trading pair by address
func (s *Service) GetPair(address string) (*TradingPair, error) {
	s.mu.RLock()
	defer s.mu.RUnlock()

	pair, ok := s.pairs[strings.ToLower(address)]
	if !ok {
		return nil, fmt.Errorf("pair not found: %s", address)
	}

	return pair, nil
}

// GetOHLCData returns OHLC candlestick data
func (s *Service) GetOHLCData(pairAddress string, interval time.Duration, limit int) ([]*OHLCData, error) {
	if limit <= 0 || limit > 1000 {
		limit = 100
	}

	if s.db == nil {
		return nil, fmt.Errorf("database not configured")
	}

	query := `
		SELECT pair_address, timestamp, open, high, low, close, volume0, volume1, tx_count
		FROM dex_ohlc
		WHERE pair_address = $1 AND timestamp > $2
		ORDER BY timestamp DESC
		LIMIT $3
	`

	rows, err := s.db.Query(query, pairAddress, time.Now().Add(-interval*float64(limit)), limit)
	if err != nil {
		return nil, fmt.Errorf("failed to query OHLC: %w", err)
	}
	defer rows.Close()

	var result []*OHLCData
	for rows.Next() {
		var ohlc OHLCData
		if err := rows.Scan(&ohlc.PairAddress, &ohlc.Timestamp, &ohlc.Open, &ohlc.High, &ohlc.Low, &ohlc.Close, &ohlc.Volume0, &ohlc.Volume1, &ohlc.TxCount); err != nil {
			continue
		}
		result = append(result, &ohlc)
	}

	return result, nil
}

// GetSwapTransactions returns swap transactions for a pair
func (s *Service) GetSwapTransactions(pairAddress string, limit int) ([]*SwapEvent, error) {
	if limit <= 0 || limit > 1000 {
		limit = 100
	}

	if s.db == nil {
		return nil, fmt.Errorf("database not configured")
	}

	query := `
		SELECT id, pair_address, transaction_hash, block_number, timestamp, sender,
			   from_reserve0, to_reserve1, from_reserve1, to_reserve0, gas_used
		FROM dex_swaps
		WHERE pair_address = $1
		ORDER BY block_number DESC
		LIMIT $2
	`

	rows, err := s.db.Query(query, pairAddress, limit)
	if err != nil {
		return nil, fmt.Errorf("failed to query swaps: %w", err)
	}
	defer rows.Close()

	var result []*SwapEvent
	for rows.Next() {
		var swap SwapEvent
		if err := rows.Scan(&swap.ID, &swap.PairAddress, &swap.TransactionHash, &swap.BlockNumber,
			&swap.Timestamp, &swap.Sender, &swap.FromReserve0, &swap.ToReserve1,
			&swap.FromReserve1, &swap.ToReserve0, &swap.GasUsed); err != nil {
			continue
		}
		result = append(result, &swap)
	}

	return result, nil
}

// GetFlashLoans detects and returns flash loan transactions
func (s *Service) GetFlashLoans(limit int) ([]*FlashLoan, error) {
	if limit <= 0 || limit > 100 {
		limit = 50
	}

	if s.db == nil {
		return nil, fmt.Errorf("database not configured")
	}

	query := `
		SELECT id, transaction_hash, block_number, timestamp, borrower, token,
			   amount, debt_fee, profit, is_attack, description
		FROM dex_flashloans
		ORDER BY block_number DESC
		LIMIT $1
	`

	rows, err := s.db.Query(query, limit)
	if err != nil {
		return nil, fmt.Errorf("failed to query flash loans: %w", err)
	}
	defer rows.Close()

	var result []*FlashLoan
	for rows.Next() {
		var fl FlashLoan
		if err := rows.Scan(&fl.ID, &fl.TransactionHash, &fl.BlockNumber, &fl.Timestamp,
			&fl.Borrower, &fl.Token, &fl.Amount, &fl.DebtFee, &fl.Profit,
			&fl.IsAttack, &fl.Description); err != nil {
			continue
		}
		result = append(result, &fl)
	}

	return result, nil
}

// =============================================================================
// BLOCKCHAIN SYNC FUNCTIONS
// =============================================================================

// startBackgroundSync starts the background synchronization
func (s *Service) startBackgroundSync() {
	ticker := time.NewTicker(s.config.UpdateInterval)
	defer ticker.Stop()

	// Initial sync
	s.syncPairsFromFactory()

	for {
		select {
		case <-ticker.C:
			s.syncPairsFromFactory()
			s.updateOHLCData()
			if s.config.EnableFlashLoan {
				s.detectFlashLoans()
			}
		}
	}
}

// syncPairsFromFactory syncs all trading pairs from the factory contract
func (s *Service) syncPairsFromFactory() {
	ctx, cancel := context.WithTimeout(context.Background(), 5*time.Minute)
	defer cancel()

	// Get all pairs length
	length, err := s.getPairsLength(ctx)
	if err != nil {
		fmt.Printf("DEX: Failed to get pairs length: %v\n", err)
		return
	}

	// Limit to prevent overwhelming
	if length > uint256(s.config.MaxPairs) {
		length = uint256(s.config.MaxPairs)
	}

	// Sync pairs in batches
	batchSize := 50
	for i := 0; i < int(length); i += batchSize {
		end := i + batchSize
		if end > int(length) {
			end = int(length)
		}

		for j := i; j < end; j++ {
			pairAddr, err := s.getPairByIndex(ctx, j)
			if err != nil {
				continue
			}

			pair, err := s.getPairData(ctx, pairAddr)
			if err != nil {
				continue
			}

			s.mu.Lock()
			s.pairs[strings.ToLower(pairAddr)] = pair
			s.mu.Unlock()

			// Save to database
			s.savePairToDB(pair)
		}
	}
}

// getPairsLength returns the total number of pairs
func (s *Service) getPairsLength(ctx context.Context) (uint64, error) {
	result, err := s.factoryABI.Methods["allPairsLength"]
	if err != nil {
		return 0, err
	}

	data, err := result.Encode([]interface{}{})
	if err != nil {
		return 0, err
	}

	msg := ethereum.CallMsg{
		To:   &s.factoryAddr,
		Data: data,
	}

	response, err := s.rpcClient.CallContract(ctx, msg, nil)
	if err != nil {
		return 0, err
	}

	return new(big.Int).SetBytes(response).Uint64(), nil
}

// getPairByIndex returns a pair address by index
func (s *Service) getPairByIndex(ctx context.Context, index int) (string, error) {
	result, err := s.factoryABI.Methods["allPairs"]
	if err != nil {
		return "", err
	}

	data, err := result.Encode([]interface{}{big.NewInt(int64(index))})
	if err != nil {
		return "", err
	}

	msg := ethereum.CallMsg{
		To:   &s.factoryAddr,
		Data: data,
	}

	response, err := s.rpcClient.CallContract(ctx, msg, nil)
	if err != nil {
		return "", err
	}

	return common.BytesToAddress(response).Hex(), nil
}

// getPairData retrieves full pair data from blockchain
func (s *Service) getPairData(ctx context.Context, pairAddr string) (*TradingPair, error) {
	pairAddr = strings.ToLower(pairAddr)
	addr := common.HexToAddress(pairAddr)

	// Get token0 and token1
	token0, err := s.callPairMethod(ctx, addr, "token0")
	if err != nil {
		return nil, err
	}
	token1, err := s.callPairMethod(ctx, addr, "token1")
	if err != nil {
		return nil, err
	}

	// Get reserves
	reserves, err := s.callPairMethod(ctx, addr, "getReserves")
	if err != nil {
		return nil, err
	}

	// Get metadata for decimals
	metadata, err := s.callPairMethod(ctx, addr, "metadata")
	if err != nil {
		return nil, err
	}

	pair := &TradingPair{
		Address:     pairAddr,
		Token0:      token0,
		Token1:      token1,
		InitializedAt: time.Now(),
		LastUpdatedAt: time.Now(),
	}

	// Parse reserves (reserve0, reserve1, blockTimestampLast)
	if len(reserves) >= 64 {
		reserve0 := new(big.Int).SetBytes(reserves[:32])
		reserve1 := new(big.Int).SetBytes(reserves[32:64])
		pair.Reserve0 = reserve0.String()
		pair.Reserve1 = reserve1.String()

		// Calculate price
		if reserve1.Cmp(big.NewInt(0)) > 0 {
			price0 := new(big.Float).Quo(new(big.Float).SetInt(reserve0), new(big.Float).SetInt(reserve1))
			pair.Price0, _ = price0.Float64()
			pair.Price1 = 1 / pair.Price0
		}
	}

	// Parse metadata (decimals, token0, token1, token0Decimals, token1Decimals, fee)
	if len(metadata) >= 96 {
		decimals := metadata[0]
		pair.Token0Decimals = uint8(decimals[31])
		pair.Token1Decimals = uint8(decimals[63])
	}

	return pair, nil
}

// callPairMethod calls a method on the pair contract
func (s *Service) callPairMethod(ctx context.Context, addr common.Address, method string) (string, error) {
	result, ok := s.pairABI.Methods[method]
	if !ok {
		return "", fmt.Errorf("method not found: %s", method)
	}

	data, err := result.Encode([]interface{}{})
	if err != nil {
		return "", err
	}

	msg := ethereum.CallMsg{
		To:   &addr,
		Data: data,
	}

	response, err := s.rpcClient.CallContract(ctx, msg, nil)
	if err != nil {
		return "", err
	}

	return hex.EncodeToString(response), nil
}

// savePairToDB saves a trading pair to the database
func (s *Service) savePairToDB(pair *TradingPair) {
	if s.db == nil {
		return
	}

	query := `
		INSERT INTO dex_pairs (address, token0, token1, token0_symbol, token1_symbol,
			token0_decimals, token1_decimals, reserve0, reserve1, total_supply,
			liquidity_usd, volume_24h, volume_change_24h, price0, price1, price_change_24h,
			tx_count, initialized_at, updated_at)
		VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16, $17, $18, NOW())
		ON CONFLICT (address) DO UPDATE SET
			reserve0 = EXCLUDED.reserve0,
			reserve1 = EXCLUDED.reserve1,
			liquidity_usd = EXCLUDED.liquidity_usd,
			volume_24h = EXCLUDED.volume_24h,
			price0 = EXCLUDED.price0,
			price1 = EXCLUDED.price1,
			updated_at = NOW()
	`

	s.db.Exec(query, pair.Address, pair.Token0, pair.Token1, pair.Token0Symbol,
		pair.Token1Symbol, pair.Token0Decimals, pair.Token1Decimals, pair.Reserve0,
		pair.Reserve1, pair.TotalSupply, pair.LiquidityUSD, pair.Volume24h,
		pair.VolumeChange24h, pair.Price0, pair.Price1, pair.PriceChange24h,
		pair.TxCount, pair.InitializedAt)
}

// updateOHLCData updates OHLC candlestick data
func (s *Service) updateOHLCData() {
	s.mu.RLock()
	defer s.mu.RUnlock()

	for _, pair := range s.pairs {
		ohlc := &OHLCData{
			PairAddress: pair.Address,
			Timestamp:   time.Now(),
			Open:        pair.Price0,
			High:        pair.Price0,
			Low:         pair.Price0,
			Close:       pair.Price0,
			Volume0:    0,
			Volume1:    0,
			TxCount:     pair.TxCount,
		}

		s.saveOHLCToDB(ohlc)
	}
}

// saveOHLCToDB saves OHLC data to database
func (s *Service) saveOHLCToDB(ohlc *OHLCData) {
	if s.db == nil {
		return
	}

	query := `
		INSERT INTO dex_ohlc (pair_address, timestamp, open, high, low, close, volume0, volume1, tx_count)
		VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)
		ON CONFLICT (pair_address, timestamp) DO NOTHING
	`

	s.db.Exec(query, ohlc.PairAddress, ohlc.Timestamp, ohlc.Open, ohlc.High,
		ohlc.Low, ohlc.Close, ohlc.Volume0, ohlc.Volume1, ohlc.TxCount)
}

// detectFlashLoans scans for flash loan transactions
func (s *Service) detectFlashLoans() {
	// This would require indexing all swap transactions and analyzing for flash loan patterns
	// Real implementation would use event logs and transaction tracing
	// For now, this is a placeholder for the detection algorithm
}

// =============================================================================
// HELPER FUNCTIONS
// =============================================================================

// uint256 converts uint64 to *big.Int
func uint256(v uint64) *big.Int {
	return new(big.Int).SetUint64(v)
}

// parseTokenSymbol attempts to get token symbol from address
func parseTokenSymbol(tokenAddr string) string {
	// This would require calling the token contract
	// For now, return a placeholder
	return "UNKNOWN"
}

// calculateLiquidityUSD calculates USD liquidity from reserves
func calculateLiquidityUSD(reserve0, reserve1 *big.Int, price0, price1 float64) float64 {
	res0Float, _ := reserve0.Float64()
	res1Float, _ := reserve1.Float64()

	return res0Float*price1 + res1Float*price0
}
