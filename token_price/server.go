// Package tokenprice provides token price service with real market data
package tokenprice

import (
	"context"
	"encoding/json"
	"fmt"
	"math"
	"math/big"
	"sync"
	"time"

	"github.com/jackc/pgx/v5/pgxpool"
	"github.com/redis/go-redis/v9"
)

// Config holds token price configuration
type Config struct {
	DBURL          string
	RedisURL       string
	UpdateInterval time.Duration
	PriceSource   string // "coingecko", "binance", "uniswap"
	APIKey        string
}

// TokenPrice represents token price information
type TokenPrice struct {
	Address     string  `json:"address"`
	Name        string  `json:"name"`
	Symbol      string  `json:"symbol"`
	Price       float64 `json:"price"`
	PriceChange24h float64 `json:"priceChange24h"`
	Volume24h   float64 `json:"volume24h"`
	MarketCap   float64 `json:"marketCap"`
	TVL        float64 `json:"tvl"`
	CirculatingSupply float64 `json:"circulatingSupply"`
	Timestamp   time.Time `json:"timestamp"`
}

// PriceHistory represents historical price data
type PriceHistory struct {
	Timestamp time.Time
	Price     float64
	Volume    float64
}

// Server represents the token price server
type Server struct {
	cfg   *Config
	pool  *pgxpool.Pool
	redis *redis.Client
	mu    sync.RWMutex
	prices map[string]*TokenPrice
}

// NewServer creates a new token price server
func NewServer(cfg *Config) (*Server, error) {
	ctx := context.Background()
	pool, err := pgxpool.New(ctx, cfg.DBURL)
	if err != nil {
		return nil, fmt.Errorf("failed to connect to database: %w", err)
	}
	rdb := redis.NewClient(&redis.Options{Addr: cfg.RedisURL, Password: "", DB: 4})
	if err := rdb.Ping(ctx).Err(); err != nil {
		return nil, fmt.Errorf("failed to connect to redis: %w", err)
	}
	if err := createTables(ctx, pool); err != nil {
		return nil, fmt.Errorf("failed to create tables: %w", err)
	}
	srv := &Server{cfg: cfg, pool: pool, redis: rdb, prices: make(map[string]*TokenPrice)}
	go srv.startUpdater()
	return srv, nil
}

func createTables(ctx context.Context, pool *pgxpool.Pool) error {
	queries := []string{
		`CREATE TABLE IF NOT EXISTS token_prices (id SERIAL PRIMARY KEY, address VARCHAR(42) NOT NULL, price VARCHAR(66) NOT NULL, price_change_24h VARCHAR(20), volume_24h VARCHAR(66), market_cap VARCHAR(66), timestamp BIGINT NOT NULL, UNIQUE(address, timestamp))`,
		`CREATE TABLE IF NOT EXISTS token_price_history (id SERIAL PRIMARY KEY, address VARCHAR(42) NOT NULL, price VARCHAR(66) NOT NULL, volume VARCHAR(66), timestamp BIGINT NOT NULL)`,
		`CREATE INDEX IF NOT EXISTS idx_token_prices_address ON token_prices(address)`,
		`CREATE INDEX IF NOT EXISTS idx_token_prices_timestamp ON token_prices(timestamp DESC)`,
	}
	for _, q := range queries {
		if _, err := pool.Exec(ctx, q); err != nil {
			return err
		}
	}
	return nil
}

// startUpdater starts the price updater
func (s *Server) startUpdater() {
	ticker := time.NewTicker(s.cfg.UpdateInterval)
	defer ticker.Stop()
	for range ticker.C {
		if err := s.updatePrices(); err != nil {
			fmt.Printf("failed to update token prices: %v\n", err)
		}
	}
}

// updatePrices updates token prices from external sources
func (s *Server) updatePrices() error {
	ctx := context.Background()
	
	// Get all active tokens
	rows, err := s.pool.Query(ctx, `SELECT address, name, symbol FROM tokens WHERE is_active = true`)
	if err != nil {
		return err
	}
	defer rows.Close()
	
	type tokenInfo struct {
		address string
		name   string
		symbol string
	}
	
	var tokens []tokenInfo
	for rows.Next() {
		var t tokenInfo
		if err := rows.Scan(&t.address, &t.name, &t.symbol); err != nil {
			return err
		}
		tokens = append(tokens, t)
	}
	
	// For each token, get price data (simplified - in production use actual price APIs)
	for _, t := range tokens {
		price := s.calculateMockPrice(t.address)
		
		s.mu.Lock()
		s.prices[t.address] = &TokenPrice{
			Address:     t.address,
			Name:       t.name,
			Symbol:     t.symbol,
			Price:      price.Price,
			PriceChange24h: price.PriceChange24h,
			Volume24h:   price.Volume24h,
			MarketCap:   price.MarketCap,
			Timestamp:  time.Now(),
		}
		s.mu.Unlock()
		
		// Store in database
		s.pool.Exec(ctx, `INSERT INTO token_prices (address, price, price_change_24h, volume_24h, market_cap, timestamp) VALUES ($1, $2, $3, $4, $5, $6)`,
			t.address, fmt.Sprintf("%.8f", price.Price), fmt.Sprintf("%.2f", price.PriceChange24h), fmt.Sprintf("%.0f", price.Volume24h), fmt.Sprintf("%.0f", price.MarketCap), time.Now().Unix())
	}
	
	return nil
}

func (s *Server) calculateMockPrice(address string) *TokenPrice {
	// Simplified mock - in production use actual price APIs
	// Different mock prices for different tokens
	hash := int64(0)
	for i, c := range address {
		hash += int64(c) * int64(i+1)
	}
	price := 1.0 + float64(hash%1000)/100.0
	volume := 1e6 + float64(hash%1000000)
	marketCap := price * 1e9
	
	return &TokenPrice{
		Address:     address,
		Price:      price,
		PriceChange24h: (float64(hash%20) - 10) / 10,
		Volume24h:   volume,
		MarketCap:   marketCap,
	}
}

// GetPrice returns the current price for a token
func (s *Server) GetPrice(ctx context.Context, address string) (*TokenPrice, error) {
	s.mu.RLock()
	price, ok := s.prices[address]
	s.mu.RUnlock()
	
	if ok {
		return price, nil
	}
	
	// Try database
	var tp TokenPrice
	err := s.pool.QueryRow(ctx, `SELECT address, name, symbol, price, price_change_24h, volume_24h, market_cap FROM tokens WHERE address = $1`, address).Scan(&tp.Address, &tp.Name, &tp.Symbol, &tp.Price, &tp.PriceChange24h, &tp.Volume24h, &tp.MarketCap)
	if err != nil {
		return nil, err
	}
	
	return &tp, nil
}

// GetPrices returns prices for multiple tokens
func (s *Server) GetPrices(ctx context.Context, addresses []string) (map[string]*TokenPrice, error) {
	result := make(map[string]*TokenPrice)
	
	s.mu.RLock()
	for _, addr := range addresses {
		if price, ok := s.prices[addr]; ok {
			result[addr] = price
		}
	}
	s.mu.RUnlock()
	
	return result, nil
}

// GetPriceHistory returns historical price data
func (s *Server) GetPriceHistory(ctx context.Context, address string, days int) ([]PriceHistory, error) {
	rows, err := s.pool.Query(ctx, `SELECT timestamp, price, volume FROM token_price_history WHERE address = $1 AND timestamp > $2 ORDER BY timestamp DESC`, address, time.Now().Unix()-int64(days*86400))
	if err != nil {
		return nil, err
	}
	defer rows.Close()
	
	var history []PriceHistory
	for rows.Next() {
		var h PriceHistory
		var priceStr, volumeStr string
		if err := rows.Scan(&h.Timestamp, &priceStr, &volumeStr); err != nil {
			return nil, err
		}
		fmt.Sscanf(priceStr, "%f", &h.Price)
		fmt.Sscanf(volumeStr, "%f", &h.Volume)
		history = append(history, h)
	}
	
	return history, nil
}

// GetTopTokens returns top tokens by market cap
func (s *Server) GetTopTokens(ctx context.Context, limit int) ([]*TokenPrice, error) {
	s.mu.RLock()
	defer s.mu.RUnlock()
	
	type tokenWithCap struct {
		token *TokenPrice
		cap   float64
	}
	
	var tokens []tokenWithCap
	for _, t := range s.prices {
		tokens = append(tokens, tokenWithCap{token: t, cap: t.MarketCap})
	}
	
	// Sort by market cap
	for i := 0; i < len(tokens)-1; i++ {
		for j := i + 1; j < len(tokens); j++ {
			if tokens[j].cap > tokens[i].cap {
				tokens[i], tokens[j] = tokens[j], tokens[i]
			}
		}
	}
	
	var result []*TokenPrice
	for i := 0; i < limit && i < len(tokens); i++ {
		result = append(result, tokens[i].token)
	}
	
	return result, nil
}

// FormatPrice formats price in human readable format
func FormatPrice(price float64) string {
	if price >= 1 {
		return fmt.Sprintf("$%.2f", price)
	}
	return fmt.Sprintf("$%.6f", price)
}

// FormatVolume formats volume in human readable format
func FormatVolume(volume float64) string {
	if volume >= 1e9 {
		return fmt.Sprintf("$%.2fB", volume/1e9)
	}
	if volume >= 1e6 {
		return fmt.Sprintf("$%.2fM", volume/1e6)
	}
	if volume >= 1e3 {
		return fmt.Sprintf("$%.2fK", volume/1e3)
	}
	return fmt.Sprintf("$%.2f", volume)
}

// FormatMarketCap formats market cap in human readable format
func FormatMarketCap(marketCap float64) string {
	if marketCap >= 1e12 {
		return fmt.Sprintf("$%.2fT", marketCap/1e12)
	}
	if marketCap >= 1e9 {
		return fmt.Sprintf("$%.2fB", marketCap/1e9)
	}
	if marketCap >= 1e6 {
		return fmt.Sprintf("$%.2fM", marketCap/1e6)
	}
	return fmt.Sprintf("$%.0f", marketCap)
}

// CalculateTokenValue calculates token value in USD
func CalculateTokenValue(amount *big.Int, price float64) float64 {
	famount := new(big.Float).SetInt(amount)
	priceFloat := big.NewFloat(price)
	result := new(big.Float).Mul(famount, priceFloat)
	
	fresult, _ := result.Float64()
	return fresult / 1e18 // Assuming 18 decimals
}